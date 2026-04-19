//! Rust-native flash coordinator for Tecla board profiles.
//!
//! The flash logic stays in Rust, but the final transport step is delegated to
//! the existing tools:
//! - `cargo run` for probe-rs-backed boards
//! - `elf2uf2-rs --deploy` for UF2 boards

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use serde::Deserialize;
use tecla_config::qmk::info_json;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum FlashMethod {
    ProbeRs,
    Uf2,
}

#[derive(Debug, Clone, Deserialize)]
struct BoardManifest {
    name: String,
    #[serde(rename = "crate")]
    package: String,
    example: String,
    target: String,
    flash: FlashMethod,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    qmk_family: Option<String>,
    #[serde(default)]
    vendor: Option<String>,
}

#[derive(Debug, Clone)]
struct BoardProfile {
    name: String,
    package: String,
    example: String,
    target: String,
    method: FlashMethod,
    features: Vec<String>,
    qmk_family: Option<String>,
    vendor: Option<String>,
}

#[derive(Debug, Clone)]
struct UsbDevice {
    sysfs_path: PathBuf,
    busnum: String,
    devnum: String,
    vendor: String,
    product: String,
    manufacturer: Option<String>,
    product_name: Option<String>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut device = None;
    let mut qmk_keyboard = None;
    let mut tecla_config = None;
    let mut list = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--list" => list = true,
            "--device" => {
                device = Some(next_value("--device", &mut args));
            }
            "--qmk-keyboard" => {
                qmk_keyboard = Some(next_value("--qmk-keyboard", &mut args));
            }
            "--tecla-config" => {
                tecla_config = Some(next_value("--tecla-config", &mut args));
            }
            "--help" | "-h" => usage_and_exit(),
            other => {
                eprintln!("unknown argument: {other}");
                usage_and_exit();
            }
        }
    }

    if list {
        print_list();
        return;
    }

    match (qmk_keyboard, tecla_config) {
        (Some(keyboard_name), None) => flash_qmk(device.as_deref(), &keyboard_name),
        (None, Some(profile_name)) => {
            let profile = resolve_native_profile(&profile_name).unwrap_or_else(|err| fail(&err));
            flash_profile(profile, device.as_deref());
        }
        (Some(_), Some(_)) => fail("use only one of --qmk-keyboard or --tecla-config"),
        (None, None) => fail("one of --qmk-keyboard or --tecla-config is required"),
    }
}

fn usage_and_exit() -> ! {
    eprintln!("usage:");
    eprintln!("  flash --list");
    eprintln!("  flash [--device <usb-device>] --qmk-keyboard <keyboard_name>");
    eprintln!("  flash [--device <usb-device>] --tecla-config <profile>");
    std::process::exit(2);
}

fn next_value(flag: &str, args: &mut impl Iterator<Item = String>) -> String {
    args.next()
        .unwrap_or_else(|| fail(&format!("missing value for {flag}")))
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn print_list() {
    let profiles = discover_native_profiles();
    let usb_devices = discover_usb_devices();
    let flashable_devices = usb_devices
        .iter()
        .filter_map(|device| suggest_usb_flash(device, &profiles).map(|hint| (device, hint)))
        .collect::<Vec<_>>();

    println!("uf2-compatible usb devices:");
    if flashable_devices.is_empty() {
        println!("  (none)");
    } else {
        for (device, hint) in flashable_devices {
            let name = usb_device_name(device);
            let device_id = usb_device_id(device);
            let path = device
                .sysfs_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?");
            let vidpid = format!("{}/{}", device.vendor, device.product);
            println!(
                "  {path:<16} {vidpid} {name} (bus {bus} dev {dev})",
                bus = device.busnum,
                dev = device.devnum,
                path = path,
                vidpid = vidpid,
            );
            println!("    device id: {device_id}");
            println!("    flash: {hint}");
        }
    }

    println!();
    println!("native profiles:");
    for profile in &profiles {
        println!(
            "  {name:<20} {package}::{example} ({target}, {method})",
            name = profile.name,
            package = profile.package,
            example = profile.example,
            target = profile.target,
            method = method_name(profile.method),
        );
    }
}

fn method_name(method: FlashMethod) -> &'static str {
    match method {
        FlashMethod::ProbeRs => "probe-rs",
        FlashMethod::Uf2 => "uf2",
    }
}

fn suggest_native_flash(device: &UsbDevice, profiles: &[BoardProfile]) -> Option<String> {
    let manufacturer = device
        .manufacturer
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if manufacturer.is_empty() {
        return None;
    }

    let matches = profiles
        .iter()
        .filter(|profile| {
            profile
                .vendor
                .as_deref()
                .is_some_and(|vendor| vendor.eq_ignore_ascii_case(&manufacturer))
        })
        .collect::<Vec<_>>();

    matches.as_slice().first().and_then(|profile| {
        if matches.len() == 1 {
            Some(format!(
                "just flash --device {} --tecla-config {}",
                usb_device_id(device),
                profile.name
            ))
        } else {
            None
        }
    })
}

fn suggest_usb_flash(device: &UsbDevice, profiles: &[BoardProfile]) -> Option<String> {
    if let Some(hint) = suggest_native_flash(device, profiles) {
        return Some(hint);
    }
    if !is_rp2_bootloader(device) {
        return None;
    }

    let matches = profiles
        .iter()
        .filter(|profile| matches!(profile.method, FlashMethod::Uf2))
        .collect::<Vec<_>>();

    matches.as_slice().first().and_then(|profile| {
        if matches.len() == 1 {
            Some(format!("just flash --tecla-config {}", profile.name))
        } else {
            None
        }
    })
}

fn discover_native_profiles() -> Vec<BoardProfile> {
    let root = Path::new("keyboards/tecla");
    let mut profiles = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name() != Some(OsStr::new("board.toml")) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(manifest) = toml::from_str::<BoardManifest>(&text) else {
                continue;
            };
            profiles.push(BoardProfile {
                name: manifest.name,
                package: manifest.package,
                example: manifest.example,
                target: manifest.target,
                method: manifest.flash,
                features: manifest.features,
                qmk_family: manifest.qmk_family,
                vendor: manifest.vendor,
            });
        }
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}

fn resolve_native_profile(name: &str) -> Result<BoardProfile, String> {
    discover_native_profiles()
        .into_iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| {
            let known = discover_native_profiles()
                .into_iter()
                .map(|profile| profile.name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("unknown native profile `{name}`; known profiles: {known}")
        })
}

fn flash_qmk(device: Option<&str>, keyboard_name: &str) -> ! {
    let root = Path::new(".vendor/qmk_firmware/keyboards");
    let Some(found) = find_qmk_keyboard(root, keyboard_name) else {
        fail(&format!(
            "could not find vendored QMK keyboard `{keyboard_name}` under {}",
            root.display()
        ));
    };
    eprintln!("found QMK keyboard at {}", found.display());

    let family = qmk_family(keyboard_name);
    let matches = discover_native_profiles()
        .into_iter()
        .filter(|profile| profile.qmk_family.as_deref() == Some(&family))
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => fail(&format!(
            "keyboard `{keyboard_name}` is present in vendored QMK, but Tecla has no qmk-backed native profile for family `{family}`"
        )),
        [profile] => flash_profile(profile.clone(), device),
        many => {
            let profiles = many
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            fail(&format!(
                "keyboard family `{family}` matches multiple native profiles: {profiles}. Use `--tecla-config <profile>` instead."
            ));
        }
    }
}

fn flash_profile(profile: BoardProfile, device: Option<&str>) -> ! {
    eprintln!(
        "flashing {name} via {package}::{example}",
        name = profile.name,
        package = profile.package,
        example = profile.example,
    );

    let mut cargo_args = cargo_args(&profile, matches!(profile.method, FlashMethod::ProbeRs));
    for feature in &profile.features {
        cargo_args.push("--features".to_string());
        cargo_args.push(feature.clone());
    }

    match profile.method {
        FlashMethod::ProbeRs => run_cargo(&cargo_args),
        FlashMethod::Uf2 => {
            let device = device
                .map(|selector| selector.to_string())
                .or_else(auto_select_uf2_device)
                .unwrap_or_else(|| {
                    fail("UF2 flashing requires a connected UF2 boot device or `--device <usb-device>`")
                });
            run_uf2(&profile, &cargo_args, &device)
        }
    }
}

fn auto_select_uf2_device() -> Option<String> {
    let devices = discover_usb_devices();
    let bootloader_devices = devices
        .iter()
        .filter(|device| is_rp2_bootloader(device))
        .collect::<Vec<_>>();

    if bootloader_devices.len() == 1 {
        let device = bootloader_devices[0];
        return Some(usb_device_id(device));
    }

    if bootloader_devices.is_empty() {
        return None;
    }

    fail("multiple RP2 boot devices are connected; pass `--device <usb-device>`");
}

fn cargo_args(profile: &BoardProfile, run: bool) -> Vec<String> {
    vec![
        if run { "run" } else { "build" }.to_string(),
        "-p".to_string(),
        profile.package.clone(),
        "--example".to_string(),
        profile.example.clone(),
        "--target".to_string(),
        profile.target.clone(),
        "--release".to_string(),
    ]
}

fn run_cargo(cargo_args: &[String]) -> ! {
    let status = run_command("cargo", cargo_args);
    exit_status("cargo", status);
}

fn run_uf2(profile: &BoardProfile, cargo_args: &[String], device: &str) -> ! {
    let status = run_command("cargo", cargo_args);
    ensure_success("cargo", status);

    let elf = build_elf_path(profile);
    eprintln!("built ELF: {}", elf.display());
    let uf2 = build_uf2_path(profile);
    let status = run_command(
        "elf2uf2-rs",
        &[elf.display().to_string(), uf2.display().to_string()],
    );
    ensure_success("elf2uf2-rs", status);

    let mount = find_uf2_mount(device)
        .or_else(|| find_uf2_block_device(device).and_then(|block| mount_uf2_block_device(&block)))
        .unwrap_or_else(|| {
            fail("could not find the UF2 boot device; put the board into boot mode first and retry")
        });

    eprintln!("found UF2 target at {}", mount.display());
    print_uf2_info(&mount);

    let dest = mount.join(
        uf2.file_name()
            .unwrap_or_else(|| OsStr::new("firmware.uf2")),
    );
    eprintln!("copying {} -> {}", uf2.display(), dest.display());
    copy_with_progress(&uf2, &dest).unwrap_or_else(|err| fail(&format!("copy failed: {err}")));
    eprintln!(
        "flash complete: {} flashed to {}",
        profile.name,
        mount.display()
    );
    std::process::exit(0);
}

fn print_uf2_info(mount: &Path) {
    let info = mount.join("INFO_UF2.TXT");
    match fs::read_to_string(&info) {
        Ok(text) => {
            eprintln!("INFO_UF2.TXT on {}:", mount.display());
            for line in text.lines() {
                eprintln!("  {line}");
            }
        }
        Err(err) => {
            eprintln!("could not read {}: {err}", info.display());
        }
    }
}

fn find_uf2_mount(selector: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
            for line in mounts.lines() {
                let mut fields = line.split_whitespace();
                let Some(source) = fields.next() else {
                    continue;
                };
                let Some(mount) = fields.next() else {
                    continue;
                };
                let mount = PathBuf::from(unescape_mount_path(mount));
                if mount.join("INFO_UF2.TXT").is_file()
                    && mount_matches_selector(&mount, source, selector)
                {
                    return Some(mount);
                }
            }
        }
    }

    for root in ["/Volumes", "/media", "/run/media", "/mnt"] {
        let root = Path::new(root);
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("INFO_UF2.TXT").is_file() && mount_matches_selector(&path, "", selector) {
                return Some(path);
            }
            if let Ok(subdirs) = fs::read_dir(&path) {
                for sub in subdirs.flatten() {
                    let subpath = sub.path();
                    if subpath.join("INFO_UF2.TXT").is_file()
                        && mount_matches_selector(&subpath, "", selector)
                    {
                        return Some(subpath);
                    }
                }
            }
        }
    }

    None
}

fn find_uf2_block_device(selector: &str) -> Option<PathBuf> {
    let root = Path::new("/sys/class/block");
    let Ok(entries) = fs::read_dir(root) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(block_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Some(info) = usb_source_info(block_name) {
            if usb_source_matches_selector(&info, selector) {
                return Some(PathBuf::from(format!("/dev/{block_name}")));
            }
        }
    }
    None
}

fn mount_uf2_block_device(block: &Path) -> Option<PathBuf> {
    let output = Command::new("udisksctl")
        .args(["mount", "-b", block.to_str()?])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("Mounted ") {
            if let Some((_, mount_part)) = rest.rsplit_once(" at ") {
                let mount = mount_part.trim_end_matches('.').trim();
                if !mount.is_empty() {
                    return Some(PathBuf::from(mount));
                }
            }
        }
    }
    None
}

fn mount_matches_selector(mount: &Path, source: &str, selector: &str) -> bool {
    let selector_path = Path::new(selector);
    if selector_path.is_absolute() {
        return mount == selector_path;
    }
    if mount
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == selector || mount.ends_with(selector))
    {
        return true;
    }
    if let Some(source_device) = source.strip_prefix("/dev/") {
        if let Some(info) = usb_source_info(source_device) {
            return usb_source_matches_selector(&info, selector);
        }
    }
    false
}

fn usb_source_matches_selector(info: &UsbSourceInfo, selector: &str) -> bool {
    selector
        == info
            .sysfs_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
        || selector == format!("{}/{}", info.vendor, info.product)
        || selector == format!("{}:{}", info.vendor, info.product)
        || selector == format!("{}:{}", info.busnum, info.devnum)
        || selector == info.busnum
        || selector == info.devnum
}

fn qmk_family(keyboard_name: &str) -> String {
    keyboard_name
        .split('/')
        .next()
        .unwrap_or(keyboard_name)
        .to_ascii_lowercase()
}

fn discover_usb_devices() -> Vec<UsbDevice> {
    let mut devices = Vec::new();
    let root = Path::new("/sys/bus/usb/devices");
    let Ok(entries) = fs::read_dir(root) else {
        return devices;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(vendor) = read_trimmed(&path.join("idVendor")) else {
            continue;
        };
        let Some(product) = read_trimmed(&path.join("idProduct")) else {
            continue;
        };
        if vendor == "1d6b" {
            continue;
        }
        let busnum = read_trimmed(&path.join("busnum")).unwrap_or_default();
        let devnum = read_trimmed(&path.join("devnum")).unwrap_or_default();
        let manufacturer = read_trimmed(&path.join("manufacturer"));
        let product_name = read_trimmed(&path.join("product"));
        devices.push(UsbDevice {
            sysfs_path: path,
            busnum,
            devnum,
            vendor,
            product,
            manufacturer,
            product_name,
        });
    }

    devices.sort_by(|a, b| a.sysfs_path.cmp(&b.sysfs_path));
    devices
}

fn is_rp2_bootloader(device: &UsbDevice) -> bool {
    device.vendor.eq_ignore_ascii_case("2e8a") && device.product.eq_ignore_ascii_case("0003")
}

fn usb_device_name(device: &UsbDevice) -> String {
    let manufacturer = device
        .manufacturer
        .as_deref()
        .unwrap_or("unknown manufacturer");
    let product = device.product_name.as_deref().unwrap_or("unknown product");
    format!("{manufacturer} {product}")
}

fn usb_device_id(device: &UsbDevice) -> String {
    device
        .sysfs_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

struct UsbSourceInfo {
    sysfs_path: PathBuf,
    busnum: String,
    devnum: String,
    vendor: String,
    product: String,
}

fn usb_source_info(block_name: &str) -> Option<UsbSourceInfo> {
    let device_link = Path::new("/sys/class/block")
        .join(block_name)
        .join("device");
    let Ok(mut path) = fs::canonicalize(device_link) else {
        return None;
    };

    loop {
        let vendor = read_trimmed(&path.join("idVendor"));
        let product = read_trimmed(&path.join("idProduct"));
        if let (Some(vendor), Some(product)) = (vendor, product) {
            return Some(UsbSourceInfo {
                sysfs_path: path.clone(),
                busnum: read_trimmed(&path.join("busnum")).unwrap_or_default(),
                devnum: read_trimmed(&path.join("devnum")).unwrap_or_default(),
                vendor,
                product,
            });
        }
        if !path.pop() {
            break;
        }
    }

    None
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn build_uf2_path(profile: &BoardProfile) -> PathBuf {
    Path::new("target")
        .join(&profile.target)
        .join("release")
        .join("examples")
        .join(format!("{}.uf2", profile.example))
}

fn copy_with_progress(src: &Path, dst: &Path) -> io::Result<u64> {
    let mut input = fs::File::open(src)?;
    let mut output = fs::File::create(dst)?;
    let total = input.metadata()?.len();
    let mut copied = 0u64;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut input, &mut buf)?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut output, &buf[..read])?;
        copied += u64::try_from(read).unwrap_or(0);
        eprint!("\rcopying UF2: {copied}/{total} bytes");
        let _ = io::stderr().flush();
    }
    eprintln!();
    output.flush()?;
    Ok(copied)
}

fn unescape_mount_path(path: &str) -> String {
    path.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn build_elf_path(profile: &BoardProfile) -> PathBuf {
    Path::new("target")
        .join(&profile.target)
        .join("release")
        .join("examples")
        .join(&profile.example)
}

fn run_command(program: &str, args: &[String]) -> ExitStatus {
    eprintln!(
        "+ {} {}",
        program,
        args.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
    );
    Command::new(program)
        .args(args)
        .status()
        .unwrap_or_else(|err| fail(&format!("failed to run {program}: {err}")))
}

fn exit_status(program: &str, status: ExitStatus) -> ! {
    ensure_success(program, status);
    std::process::exit(0);
}

fn ensure_success(program: &str, status: ExitStatus) {
    if status.success() {
        return;
    }
    if let Some(code) = status.code() {
        eprintln!("{program} exited with status code {code}");
    } else {
        eprintln!("{program} terminated by signal");
    }
    std::process::exit(1);
}

fn find_qmk_keyboard(root: &Path, keyboard_name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name() != Some(OsStr::new("info.json"))
                && path.file_name() != Some(OsStr::new("keyboard.json"))
            {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(config) = info_json::parse(&text) else {
                continue;
            };
            let Some(config_name) = config.keyboard_name.as_deref() else {
                continue;
            };
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .trim_end_matches("/info.json")
                .trim_end_matches("/keyboard.json")
                .to_string();
            if config_name == keyboard_name || rel == keyboard_name {
                return Some(path);
            }
        }
    }
    None
}
