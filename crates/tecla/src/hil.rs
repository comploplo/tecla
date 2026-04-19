//! Hardware-in-the-loop test fixtures via Renode + Podman/Docker.
//!
//! Provides [`RenodeRunner`] for running embedded firmware in a Renode
//! simulation inside a container. Board crates use this in integration
//! tests instead of shell scripts.
//!
//! # Usage
//!
//! ```rust,ignore
//! use tecla::hil::{HilConfig, RenodeRunner};
//!
//! let runner = RenodeRunner::new(HilConfig {
//!     platform: Platform::Rp2040,
//!     firmware_elf: "target/thumbv6m-none-eabi/release/examples/hil_keyboard",
//!     timeout_secs: 10,
//! });
//! let output = runner.run();
//! assert!(output.passed(), "HIL test failed: {}", output.stderr());
//! ```

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Supported Renode target platforms.
#[derive(Debug, Clone, Copy)]
pub enum Platform {
    /// RP2040 (Cortex-M0+).
    Rp2040,
    /// nRF52840 (Cortex-M4F).
    Nrf52840,
    /// STM32F411 (Cortex-M4F).
    Stm32f4,
}

impl Platform {
    /// Returns the Renode platform description for this target.
    #[must_use]
    pub fn repl_content(self) -> &'static str {
        match self {
            Self::Rp2040 => include_str!("hil/rp2040.repl"),
            Self::Nrf52840 => include_str!("hil/nrf52840.repl"),
            Self::Stm32f4 => include_str!("hil/stm32f4.repl"),
        }
    }

    fn default_timeout(self) -> u32 {
        match self {
            Self::Rp2040 => 10,
            Self::Nrf52840 | Self::Stm32f4 => 10,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rp2040 => write!(f, "rp2040"),
            Self::Nrf52840 => write!(f, "nrf52840"),
            Self::Stm32f4 => write!(f, "stm32f4"),
        }
    }
}

/// Configuration for a HIL test run.
pub struct HilConfig {
    /// Target platform.
    pub platform: Platform,
    /// Path to the firmware ELF binary.
    pub firmware_elf: PathBuf,
    /// Simulation timeout in seconds.
    pub timeout_secs: u32,
}

impl HilConfig {
    /// Creates a config with default timeout for the platform.
    #[must_use]
    #[expect(clippy::impl_trait_in_params, reason = "ergonomic builder API")]
    pub fn new(platform: Platform, firmware_elf: impl Into<PathBuf>) -> Self {
        Self {
            timeout_secs: platform.default_timeout(),
            platform,
            firmware_elf: firmware_elf.into(),
        }
    }

    /// Sets the simulation timeout.
    #[must_use]
    pub fn timeout(mut self, secs: u32) -> Self {
        self.timeout_secs = secs;
        self
    }
}

/// Result of a HIL test run.
pub struct HilOutput {
    stdout: String,
    stderr: String,
    exit_code: Option<i32>,
}

impl HilOutput {
    /// Returns `true` if the output contains "HIL OK".
    #[must_use]
    pub fn passed(&self) -> bool {
        self.stdout.contains("HIL OK") || self.stderr.contains("HIL OK")
    }

    /// Returns `true` if the output contains a specific marker string.
    #[must_use]
    pub fn contains(&self, marker: &str) -> bool {
        self.stdout.contains(marker) || self.stderr.contains(marker)
    }

    /// Stdout from the Renode process.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Stderr from the Renode process.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Exit code from the container process.
    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

impl fmt::Display for HilOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = match self.exit_code {
            Some(c) => format!("{c}"),
            None => "none".into(),
        };
        write!(
            f,
            "exit={code}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout, self.stderr
        )
    }
}

const RENODE_IMAGE: &str = "docker.io/antmicro/renode:latest";

/// Runs a command with a hard timeout and a cleanup callback.
///
/// The command's stdout and stderr are captured. If the command does not exit
/// before the timeout, it is killed, reaped, and then `cleanup` runs.
pub fn run_with_timeout_and_cleanup<F>(
    mut command: Command,
    timeout_secs: u32,
    cleanup: F,
) -> Output
where
    F: FnOnce(),
{
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn container runtime: {e}"));
    let mut stdout = child.stdout.take().expect("child stdout not captured");
    let mut stderr = child.stderr.take().expect("child stderr not captured");

    let stdout_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        stdout
            .read_to_end(&mut buf)
            .unwrap_or_else(|e| panic!("failed to read child stdout: {e}"));
        buf
    });
    let stderr_thread = thread::spawn(move || {
        let mut buf = Vec::new();
        stderr
            .read_to_end(&mut buf)
            .unwrap_or_else(|e| panic!("failed to read child stderr: {e}"));
        buf
    });

    let deadline = Instant::now() + Duration::from_secs(timeout_secs.into());
    let status = loop {
        match child
            .try_wait()
            .unwrap_or_else(|e| panic!("failed to poll container runtime: {e}"))
        {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                break child
                    .wait()
                    .unwrap_or_else(|e| panic!("failed to reap timed-out container runtime: {e}"));
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    };

    cleanup();

    let stdout = stdout_thread
        .join()
        .unwrap_or_else(|_| panic!("failed to join stdout reader"));
    let stderr = stderr_thread
        .join()
        .unwrap_or_else(|_| panic!("failed to join stderr reader"));

    Output {
        status,
        stdout,
        stderr,
    }
}

/// Runs a Renode container with a platform-appropriate timeout and cleanup.
///
/// This keeps the timeout policy in the HIL module instead of each test.
pub fn run_renode_container<F>(command: Command, platform: Platform, cleanup: F) -> Output
where
    F: FnOnce(),
{
    let timeout_secs = platform.default_timeout().saturating_add(50);
    run_with_timeout_and_cleanup(command, timeout_secs, cleanup)
}

/// Runs firmware in a Renode simulation via Podman/Docker.
///
/// The container is created, run, and removed automatically. Platform
/// definition files and Renode scripts are generated from the [`HilConfig`]
/// and written to a temporary directory that is bind-mounted into the
/// container.
pub struct RenodeRunner {
    config: HilConfig,
}

impl RenodeRunner {
    /// Creates a new runner from the given configuration.
    #[must_use]
    pub fn new(config: HilConfig) -> Self {
        Self { config }
    }

    /// Runs the HIL test and returns the captured output.
    ///
    /// # Panics
    ///
    /// Panics if the firmware ELF does not exist or if the container runtime
    /// (podman or docker) is not found.
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "HIL runner panics on missing firmware or container runtime -- no recovery path"
    )]
    pub fn run(&self) -> HilOutput {
        struct CleanupGuard(PathBuf);
        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        let firmware_elf = self.config.firmware_elf.canonicalize().unwrap_or_else(|e| {
            panic!(
                "firmware ELF not found at {}: {e}",
                self.config.firmware_elf.display()
            );
        });

        let info = extract_elf_info(&firmware_elf, "hil_marker");
        let work_dir = self.prepare_work_dir(&firmware_elf, &info);
        let container_work = "/hil";

        let runtime = detect_container_runtime();

        let _guard = CleanupGuard(work_dir.clone());

        let cidfile = work_dir.join("renode.cid");
        let mut command = Command::new(&runtime);
        command.args([
            "run",
            "--rm",
            "--cidfile",
            cidfile.to_str().expect("cidfile path not valid UTF-8"),
            "--volume",
            &format!("{}:{container_work}:ro", work_dir.display()),
            "--volume",
            &format!("{}:{}", firmware_elf.display(), firmware_elf.display()),
            "--entrypoint",
            "/opt/renode/renode",
            RENODE_IMAGE,
            "--disable-xwt",
            "--plain",
            "-e",
            &format!("include @{container_work}/test.resc"),
        ]);

        let timeout_secs = self.config.timeout_secs.saturating_add(15);
        let output = run_with_timeout_and_cleanup(command, timeout_secs, || {
            cleanup_container(&runtime, &cidfile);
        });

        HilOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "HIL work dir setup -- panics on filesystem errors"
    )]
    fn prepare_work_dir(&self, firmware_elf: &Path, info: &ElfInfo) -> PathBuf {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("failed to find workspace root")
            .join("target");
        let dir = base.join(format!(
            "hil-{}-{}",
            self.config.platform,
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("failed to create HIL work dir");

        let repl_path = dir.join("platform.repl");
        std::fs::write(&repl_path, self.config.platform.repl_content())
            .expect("failed to write .repl");

        let resc = generate_resc(
            firmware_elf,
            "/hil/platform.repl",
            self.config.timeout_secs,
            info,
        );
        std::fs::write(dir.join("test.resc"), resc).expect("failed to write .resc");

        dir
    }
}

/// Extracted ELF information for Renode initialization.
pub struct ElfInfo {
    /// Vector table offset.
    pub vtor: u32,
    /// Initial stack pointer.
    pub sp: u32,
    /// Reset handler address (thumb bit cleared).
    pub pc: u32,
    /// Address of the `hil_marker` symbol, if found.
    pub hook_addr: Option<u32>,
}

/// Extracts vector table and marker symbol from a firmware ELF.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "HIL ELF extraction -- panics if rust-objdump missing"
)]
#[expect(
    clippy::indexing_slicing,
    reason = "parts length is checked before indexing"
)]
pub fn extract_elf_info(firmware_elf: &Path, marker: &str) -> ElfInfo {
    let vt_output = Command::new("rust-objdump")
        .args(["-s", "-j", ".vector_table"])
        .arg(firmware_elf)
        .output()
        .expect("rust-objdump not found -- install via `rustup component add llvm-tools`");

    let stdout = String::from_utf8_lossy(&vt_output.stdout);
    let mut vtor = 0u32;
    let mut sp = 0u32;
    let mut pc = 0u32;
    for line in stdout.lines() {
        let trimmed = line.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3 && u32::from_str_radix(parts[0], 16).is_ok() {
            vtor = u32::from_str_radix(parts[0], 16).unwrap_or(0);
            sp = swap_endian_u32(parts[1]);
            let pc_raw = swap_endian_u32(parts[2]);
            pc = pc_raw & !1;
            break;
        }
    }
    assert!(
        pc != 0,
        "failed to extract vector table from {}",
        firmware_elf.display()
    );

    let hook_addr = find_symbol_address(firmware_elf, marker);

    ElfInfo {
        vtor,
        sp,
        pc,
        hook_addr,
    }
}

fn find_symbol_address(firmware_elf: &Path, symbol: &str) -> Option<u32> {
    let output = Command::new("rust-nm")
        .arg("--defined-only")
        .arg(firmware_elf)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        let addr_str = fields.next()?;
        let _type_field = fields.next();
        let name = fields.next();
        if name == Some(symbol) {
            return u32::from_str_radix(addr_str, 16).ok();
        }
    }
    None
}

#[expect(
    clippy::big_endian_bytes,
    reason = "intentional endian swap for ELF vector table parsing"
)]
fn swap_endian_u32(hex: &str) -> u32 {
    let bytes = u32::from_str_radix(hex, 16).unwrap_or(0).to_be_bytes();
    u32::from_le_bytes(bytes)
}

#[expect(
    clippy::integer_division_remainder_used,
    reason = "converting seconds to mm:ss"
)]
fn generate_resc(
    firmware_elf: &Path,
    repl_path: &str,
    timeout_secs: u32,
    info: &ElfInfo,
) -> String {
    let hook_line = if let Some(addr) = info.hook_addr {
        format!("cpu AddHook {addr:#010X} \"self.Log(LogLevel.Warning, 'HIL OK')\"")
    } else {
        String::new()
    };

    format!(
        r#"mach create "hil"
machine LoadPlatformDescription @{repl_path}

sysbus LoadELF @{firmware}

cpu VectorTableOffset {vtor:#010X}
cpu SP {sp:#010X}
cpu PC {pc:#010X}

{hook_line}

start
emulation RunFor "00:{timeout_mm:02}:{timeout_ss:02}"
quit
"#,
        firmware = firmware_elf.display(),
        vtor = info.vtor,
        sp = info.sp,
        pc = info.pc,
        timeout_mm = timeout_secs / 60,
        timeout_ss = timeout_secs % 60,
    )
}

#[expect(
    clippy::panic,
    reason = "no recovery path when container runtime is missing"
)]
fn detect_container_runtime() -> String {
    if let Ok(rt) = std::env::var("TECLA_CONTAINER_RUNTIME") {
        return rt;
    }
    for candidate in ["podman", "docker"] {
        if Command::new(candidate).arg("--version").output().is_ok() {
            return (*candidate).to_owned();
        }
    }
    panic!("no container runtime found -- install podman or docker");
}

fn cleanup_container(runtime: &str, cidfile: &Path) {
    let Ok(cid) = std::fs::read_to_string(cidfile) else {
        return;
    };
    let cid = cid.trim();
    if cid.is_empty() {
        return;
    }
    let _ = Command::new(runtime).args(["rm", "-f", cid]).status();
}

/// A key in the matrix test plan: GPIO pin assignments and expected HID output.
#[derive(Debug, Clone)]
pub struct MatrixTestKey {
    /// Matrix row index.
    pub row: u8,
    /// Matrix column index.
    pub col: u8,
    /// Expected HID usage ID (0 = no output expected).
    pub expected_hid_usage: u8,
}

/// Describes the GPIO ↔ matrix ↔ keycode mapping for a keyboard under test.
///
/// Generated from QMK `info.json` (pin assignments) and `keymap.c` (keycodes).
/// Used to generate Renode Python peripherals and expected-output assertions.
#[derive(Debug, Clone)]
pub struct MatrixTestPlan {
    /// GPIO numbers for row output pins (active-low strobe).
    pub row_gpios: Vec<u32>,
    /// GPIO numbers for column input pins (active-low read with pull-up).
    pub col_gpios: Vec<u32>,
    /// Keys to test with expected HID output.
    pub keys: Vec<MatrixTestKey>,
}

impl MatrixTestPlan {
    /// Generates a Renode Python peripheral script that simulates the key matrix.
    ///
    /// The peripheral watches GPIO output register writes to detect row strobes,
    /// then returns column input values based on which key is currently "pressed".
    /// The test firmware controls which key is pressed by writing a key index to
    /// a magic register.
    #[must_use]
    pub fn generate_sio_peripheral(&self) -> String {
        let row_gpios: Vec<String> = self.row_gpios.iter().map(ToString::to_string).collect();
        let col_gpios: Vec<String> = self.col_gpios.iter().map(ToString::to_string).collect();

        format!(
            r#"# Auto-generated RP2040 SIO peripheral for matrix HIL testing.
# Simulates GPIO row-strobe / column-read for a {rows}x{cols} matrix.

ROW_GPIOS = [{row_list}]
COL_GPIOS = [{col_list}]
SPINLOCK_BASE = 0x100
SPINLOCK_COUNT = 32

gpio_out = 0xFFFFFFFF  # all pins high (idle)
pressed_row = -1
pressed_col = -1
locks = [False] * SPINLOCK_COUNT

def read_word(offset):
    global gpio_out, pressed_row, pressed_col

    # GPIO_IN at offset 0x04
    if offset == 0x04:
        result = 0xFFFFFFFF  # all high (no key pressed)
        if pressed_row >= 0 and pressed_col >= 0:
            # check if the active row's GPIO is driven low
            row_gpio = ROW_GPIOS[pressed_row] if pressed_row < len(ROW_GPIOS) else -1
            if row_gpio >= 0 and (gpio_out & (1 << row_gpio)) == 0:
                col_gpio = COL_GPIOS[pressed_col] if pressed_col < len(COL_GPIOS) else -1
                if col_gpio >= 0:
                    result &= ~(1 << col_gpio)  # pull column low
        return result

    # GPIO_OUT at offset 0x10
    if offset == 0x10:
        return gpio_out

    # Magic test control register at offset 0xF00
    # Write key_index here to "press" that key; write 0xFFFF to release all.
    if offset == 0xF00:
        return (pressed_row << 8) | (pressed_col & 0xFF) if pressed_row >= 0 else 0xFFFF

    # Spinlock range
    if SPINLOCK_BASE <= offset < SPINLOCK_BASE + SPINLOCK_COUNT * 4:
        idx = (offset - SPINLOCK_BASE) // 4
        if locks[idx]:
            return 0
        locks[idx] = True
        return 1

    return 0

def write_word(offset, value):
    global gpio_out, pressed_row, pressed_col

    # GPIO_OUT at offset 0x10
    if offset == 0x10:
        gpio_out = value
        return

    # GPIO_OUT_SET at offset 0x14
    if offset == 0x14:
        gpio_out |= value
        return

    # GPIO_OUT_CLR at offset 0x18
    if offset == 0x18:
        gpio_out &= ~value
        return

    # GPIO_OUT_XOR at offset 0x1C
    if offset == 0x1C:
        gpio_out ^= value
        return

    # Magic test control register at offset 0xF00
    if offset == 0xF00:
        if value == 0xFFFF:
            pressed_row = -1
            pressed_col = -1
        else:
            pressed_row = (value >> 8) & 0xFF
            pressed_col = value & 0xFF
        return

    # Spinlock range
    if SPINLOCK_BASE <= offset < SPINLOCK_BASE + SPINLOCK_COUNT * 4:
        idx = (offset - SPINLOCK_BASE) // 4
        locks[idx] = False

def read_byte(offset):
    return read_word(offset) & 0xFF

def write_byte(offset, value):
    write_word(offset, value)

if 'self' in dir():
    self.read_byte = read_byte
    self.write_byte = write_byte
    self.read_word = read_word
    self.write_word = write_word
"#,
            rows = self.row_gpios.len(),
            cols = self.col_gpios.len(),
            row_list = row_gpios.join(", "),
            col_list = col_gpios.join(", "),
        )
    }

    /// Generates a Renode platform `.repl` that uses a Python peripheral for
    /// the SIO instead of plain MappedMemory, enabling GPIO matrix simulation.
    #[must_use]
    pub fn generate_matrix_repl(&self, sio_script_path: &str) -> String {
        format!(
            r#"flash: Memory.MappedMemory @ sysbus 0x10000000
    size: 0x200000

sram: Memory.MappedMemory @ sysbus 0x20000000
    size: 0x42000

psm: Memory.MappedMemory @ sysbus 0x40010000
    size: 0x4000

sio: Python.PythonPeripheral @ sysbus 0xD0000000
    size: 0x1000
    initable: true
    filename: "{sio_script_path}"

nvic: IRQControllers.NVIC @ sysbus 0xe000e000
    systickFrequency: 125000000
    -> cpu@0

cpu: CPU.CortexM @ sysbus
    cpuType: "cortex-m0+"
    nvic: nvic
"#
        )
    }

    /// Generates a Renode script that loads firmware and runs it.
    /// The firmware itself iterates all keys via the magic SIO register.
    #[must_use]
    #[expect(
        clippy::integer_division_remainder_used,
        reason = "converting seconds to mm:ss"
    )]
    pub fn generate_test_resc(
        &self,
        firmware_elf: &Path,
        repl_path: &str,
        info: &ElfInfo,
        timeout_secs: u32,
    ) -> String {
        let hook_line = if let Some(addr) = info.hook_addr {
            format!("cpu AddHook {addr:#010X} \"self.Log(LogLevel.Warning, 'HIL OK')\"")
        } else {
            String::new()
        };

        format!(
            r#"mach create "hil"
machine LoadPlatformDescription @{repl_path}

sysbus LoadELF @{firmware}

cpu VectorTableOffset {vtor:#010X}
cpu SP {sp:#010X}
cpu PC {pc:#010X}

{hook_line}

start
emulation RunFor "00:{timeout_mm:02}:{timeout_ss:02}"
quit
"#,
            firmware = firmware_elf.display(),
            vtor = info.vtor,
            sp = info.sp,
            pc = info.pc,
            timeout_mm = timeout_secs / 60,
            timeout_ss = timeout_secs % 60,
        )
    }
}

/// Builds a [`MatrixTestPlan`] from parsed QMK config data.
///
/// `row_pins` and `col_pins` are GPIO pin names from `info.json` (e.g. "GP0", "B1").
/// `base_layer_keys` are the resolved keycodes from the base layer of `keymap.c`.
/// `pin_to_gpio` maps QMK pin names to RP2040 GPIO numbers.
#[must_use]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "computing row/col from flat index"
)]
pub fn build_test_plan(
    row_gpios: Vec<u32>,
    col_gpios: Vec<u32>,
    base_layer_keys: &[u8],
    cols: usize,
) -> MatrixTestPlan {
    let mut keys = Vec::new();
    for (i, &usage_id) in base_layer_keys.iter().enumerate() {
        if usage_id == 0 {
            continue;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::as_conversions,
            reason = "keyboard dimensions fit in u8"
        )]
        let row = (i / cols) as u8;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::as_conversions,
            reason = "keyboard dimensions fit in u8"
        )]
        let col = (i % cols) as u8;
        keys.push(MatrixTestKey {
            row,
            col,
            expected_hid_usage: usage_id,
        });
    }
    MatrixTestPlan {
        row_gpios,
        col_gpios,
        keys,
    }
}

/// Generates a Renode `.resc` script for the matrix keycode verification test.
///
/// The script loads firmware, hooks `report_marker` to read each `ReportEntry`
/// from SRAM and log it as `KEY:<row>,<col>,<mod>,<kc>`, hooks `hil_marker`
/// for the finish signal, then runs the simulation.
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "Renode script generation needs all these addresses"
)]
#[expect(
    clippy::integer_division_remainder_used,
    reason = "converting seconds to mm:ss"
)]
pub fn generate_matrix_test_resc(
    firmware_elf: &Path,
    repl_path: &str,
    report_marker_addr: u32,
    hil_marker_addr: u32,
    report_buf_addr: u32,
    report_count_addr: u32,
    info: &ElfInfo,
    timeout_secs: u32,
    tapdance_marker_addr: Option<u32>,
    tap_result_addr: Option<u32>,
    hold_layer_result_addr: Option<u32>,
) -> String {
    let tapdance_hook = match (tapdance_marker_addr, tap_result_addr, hold_layer_result_addr) {
        (Some(td_addr), Some(tap_addr), Some(hold_addr)) => format!(
            "cpu AddHook {td_addr:#010X} \"tap = self.Bus.ReadByte({tap_addr}); hold = self.Bus.ReadByte({hold_addr}); self.Log(LogLevel.Warning, 'TAPDANCE:' + hex(tap) + ',' + hex(hold))\"",
        ),
        _ => String::new(),
    };

    format!(
        r#"mach create "hil"
machine LoadPlatformDescription @{repl_path}

sysbus LoadELF @{firmware}

cpu VectorTableOffset {vtor:#010X}
cpu SP {sp:#010X}
cpu PC {pc:#010X}

cpu AddHook {report_marker:#010X} "idx = self.Bus.ReadDoubleWord({count_addr}); addr = {buf_addr} + (idx - 1) * 4; val = self.Bus.ReadDoubleWord(addr); self.Log(LogLevel.Warning, 'KEY:' + str(val & 0xFF) + ',' + str((val >> 8) & 0xFF) + ',' + hex((val >> 16) & 0xFF) + ',' + hex((val >> 24) & 0xFF))"
cpu AddHook {hil_marker:#010X} "self.Log(LogLevel.Warning, 'HIL OK')"
{tapdance_hook}

start
emulation RunFor "00:{timeout_mm:02}:{timeout_ss:02}"
quit
"#,
        firmware = firmware_elf.display(),
        vtor = info.vtor,
        sp = info.sp,
        pc = info.pc,
        report_marker = report_marker_addr,
        hil_marker = hil_marker_addr,
        count_addr = report_count_addr,
        buf_addr = report_buf_addr,
        timeout_mm = timeout_secs / 60,
        timeout_ss = timeout_secs % 60,
    )
}

/// Parses `TAPDANCE:tap_result,hold_result` from Renode log output.
#[must_use]
#[expect(
    clippy::indexing_slicing,
    reason = "parts length is checked before indexing"
)]
pub fn parse_tapdance_log(output: &str) -> Option<(u8, u8)> {
    for line in output.lines() {
        let Some(payload) = extract_after(line, "TAPDANCE:") else {
            continue;
        };
        let parts: Vec<&str> = payload.split(',').collect();
        if parts.len() == 2 {
            let tap = parse_hex_or_dec(parts[0])?;
            let hold = parse_hex_or_dec(parts[1])?;
            return Some((tap, hold));
        }
    }
    None
}

/// Parses `KEY:row,col,mod,kc` lines from Renode log output.
#[must_use]
#[expect(
    clippy::indexing_slicing,
    reason = "parts length is checked before indexing"
)]
pub fn parse_key_log(output: &str) -> Vec<(u8, u8, u8, u8)> {
    let mut results = Vec::new();
    for line in output.lines() {
        let Some(payload) = extract_after(line, "KEY:") else {
            continue;
        };
        let parts: Vec<&str> = payload.split(',').collect();
        if parts.len() == 4 {
            let row = parts[0].parse().ok();
            let col = parts[1].parse().ok();
            let mods = parse_hex_or_dec(parts[2]);
            let kc = parse_hex_or_dec(parts[3]);
            if let (Some(r), Some(c), Some(m), Some(k)) = (row, col, mods, kc) {
                results.push((r, c, m, k));
            }
        }
    }
    results
}

fn parse_hex_or_dec(s: &str) -> Option<u8> {
    let s = s.trim().trim_end_matches('L').trim_end_matches('l');
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16).ok()
    } else {
        u8::from_str_radix(s, 16).ok().or_else(|| s.parse().ok())
    }
}

fn extract_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let idx = line.find(marker)?;
    let rest = line.get(idx + marker.len()..)?;
    Some(rest.trim().trim_end_matches('\''))
}

/// Asserts that a HIL test run passed (output contains "HIL OK").
#[macro_export]
macro_rules! assert_hil_ok {
    ($output:expr) => {
        assert!($output.passed(), "HIL test failed.\n{}", $output,)
    };
    ($output:expr, $marker:expr) => {
        assert!(
            $output.contains($marker),
            "HIL test failed -- expected {:?}.\n{}",
            $marker,
            $output,
        )
    };
}
