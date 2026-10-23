//! Board profile discovery and flash-target selection.
//!
//! The decision-making half of the `flash` binary: which board profile matches
//! a name, which USB device looks like a board in bootloader mode, what cargo
//! invocation builds a given profile. Kept in the library rather than in
//! `src/bin/flash.rs` so it is covered by the normal test run — logic that
//! only exists inside a binary is logic no test ever reaches.
//!
//! The binary keeps the parts that cannot be unit tested: spawning processes,
//! walking sysfs, and waiting for a bootloader volume to appear.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
/// How firmware reaches a board.
pub enum FlashMethod {
    /// Flash over SWD/JTAG with probe-rs.
    ProbeRs,
    /// Drag-and-drop UF2 to a mounted bootloader volume.
    Uf2,
    /// USB DFU.
    Dfu,
}

#[derive(Debug, Clone, Deserialize)]
/// A `board.toml` as written on disk.
pub struct BoardManifest {
    /// Profile name, as passed to `--tecla-config`.
    pub name: String,
    #[serde(rename = "crate")]
    /// Cargo package that builds this board.
    pub package: String,
    /// Cargo example target inside that package.
    pub example: String,
    /// Rust target triple to cross-compile for.
    pub target: String,
    /// How to get the firmware onto the board.
    pub flash: FlashMethod,
    #[serde(default)]
    /// Extra cargo features to enable.
    pub features: Vec<String>,
    #[serde(default)]
    /// Disable the package's default features when building this profile.
    pub no_default_features: bool,
    #[serde(default)]
    /// QMK keyboard family this profile stands in for, if any.
    pub qmk_family: Option<String>,
    #[serde(default)]
    /// USB manufacturer string used to auto-detect the board.
    pub vendor: Option<String>,
    #[serde(default)]
    /// `vid:pid` of the DFU bootloader, for DFU boards.
    pub dfu_vid_pid: Option<String>,
    #[serde(default)]
    /// Flash address to write to in DFU mode.
    pub dfu_address: Option<String>,
    #[serde(default)]
    /// Maximum raw firmware bytes allowed by this DFU profile.
    pub dfu_max_size: Option<u32>,
}

#[derive(Debug, Clone)]
/// A discovered board profile, ready to build and flash.
pub struct BoardProfile {
    /// Profile name, as passed to `--tecla-config`.
    pub name: String,
    /// Cargo package that builds this board.
    pub package: String,
    /// Cargo example target inside that package.
    pub example: String,
    /// Rust target triple to cross-compile for.
    pub target: String,
    /// How to get the firmware onto the board.
    pub method: FlashMethod,
    /// Extra cargo features to enable.
    pub features: Vec<String>,
    /// Disable the package's default features when building this profile.
    pub no_default_features: bool,
    /// QMK keyboard family this profile stands in for, if any.
    pub qmk_family: Option<String>,
    /// USB manufacturer string used to auto-detect the board.
    pub vendor: Option<String>,
    /// `vid:pid` of the DFU bootloader, for DFU boards.
    pub dfu_vid_pid: Option<String>,
    /// Flash address to write to in DFU mode.
    pub dfu_address: Option<String>,
    /// Maximum raw firmware bytes allowed by this DFU profile.
    pub dfu_max_size: Option<u32>,
}

/// Physical selectors forwarded to dfu-util after resolving one DFU device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DfuTarget {
    /// USB topology path, when available.
    pub path: Option<String>,
    /// USB serial number, when nonempty.
    pub serial: Option<String>,
}

/// Rejects empty images and images exceeding the profile's application region.
pub fn validate_dfu_image_size(profile: &BoardProfile, size: u64) -> Result<(), String> {
    if size == 0 {
        return Err("DFU firmware image is empty".to_string());
    }
    if let Some(maximum) = profile.dfu_max_size {
        if size > u64::from(maximum) {
            return Err(format!(
                "DFU image is {size} bytes; profile `{}` allows at most {maximum} bytes",
                profile.name
            ));
        }
    }
    Ok(())
}

fn quoted_dfu_field<'a>(line: &'a str, field: &str) -> Option<&'a str> {
    line.split_once(&format!("{field}=\""))?
        .1
        .split_once('"')
        .map(|(value, _)| value)
}

fn normalized_vid_pid(value: &str) -> String {
    value.to_ascii_lowercase().replace('/', ":")
}

fn dfu_selector_matches(target: &DfuTarget, selector: &str, vid_pid: &str) -> bool {
    if let Some(path) = selector.strip_prefix("path:") {
        return !path.is_empty() && target.path.as_deref() == Some(path);
    }
    if let Some(serial) = selector.strip_prefix("serial:") {
        return !serial.is_empty() && target.serial.as_deref() == Some(serial);
    }
    normalized_vid_pid(selector) == normalized_vid_pid(vid_pid)
        || target.path.as_deref() == Some(selector)
        || target.serial.as_deref() == Some(selector)
}

/// Selects one physical DFU device; alternate settings do not count as devices.
///
/// Selectors match an exact USB path or serial, optionally prefixed by `path:`
/// or `serial:`. A VID:PID selector still requires a unique physical device.
pub fn select_dfu_target(
    listing: &str,
    vid_pid: &str,
    selector: Option<&str>,
) -> Result<Option<DfuTarget>, String> {
    let expected = normalized_vid_pid(vid_pid);
    let mut targets = Vec::new();
    for line in listing.lines().map(str::trim) {
        let Some(rest) = line.strip_prefix("Found DFU: [") else {
            continue;
        };
        let Some((identity, _)) = rest.split_once(']') else {
            continue;
        };
        if normalized_vid_pid(identity) != expected {
            continue;
        }
        let target = DfuTarget {
            path: quoted_dfu_field(line, "path")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            serial: quoted_dfu_field(line, "serial")
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        };
        if target.path.is_none() && target.serial.is_none() {
            return Err(
                "matching DFU device has no path or serial; refusing an unqualified flash"
                    .to_string(),
            );
        }
        if selector.is_none_or(|selector| dfu_selector_matches(&target, selector, vid_pid))
            && !targets.contains(&target)
        {
            targets.push(target);
        }
    }
    match targets.len() {
        0 => Ok(None),
        1 => Ok(targets.pop()),
        _ => Err(
            "multiple matching DFU devices; pass --device path:<usb-path> or serial:<serial>"
                .to_string(),
        ),
    }
}

/// Interprets an explicitly typed selector without discovering or opening USB.
pub fn dfu_preview_target(
    selector: Option<&str>,
    vid_pid: &str,
) -> Result<Option<DfuTarget>, String> {
    let Some(selector) = selector else {
        return Ok(None);
    };
    if normalized_vid_pid(selector) == normalized_vid_pid(vid_pid) {
        return Ok(None);
    }
    if let Some(path) = selector
        .strip_prefix("path:")
        .filter(|value| !value.is_empty())
    {
        return Ok(Some(DfuTarget {
            path: Some(path.to_string()),
            serial: None,
        }));
    }
    if let Some(serial) = selector
        .strip_prefix("serial:")
        .filter(|value| !value.is_empty())
    {
        return Ok(Some(DfuTarget {
            path: None,
            serial: Some(serial.to_string()),
        }));
    }
    Err("DFU dry-run selectors require path:<usb-path> or serial:<serial>; no discovery is performed".to_string())
}

/// Builds the DFU command with the physical selectors used during discovery.
pub fn dfu_flash_args(
    profile: &BoardProfile,
    binary: &Path,
    target: Option<&DfuTarget>,
) -> Result<Vec<String>, String> {
    let vid_pid = profile
        .dfu_vid_pid
        .as_deref()
        .ok_or("DFU flashing requires `dfu_vid_pid` in board.toml")?;
    let mut args = vec!["-d".to_string(), normalized_vid_pid(vid_pid)];
    if let Some(target) = target {
        if let Some(path) = &target.path {
            args.extend(["-p".to_string(), path.clone()]);
        }
        if let Some(serial) = &target.serial {
            args.extend(["-S".to_string(), serial.clone()]);
        }
    }
    args.extend([
        "-a".to_string(),
        "0".to_string(),
        "-s".to_string(),
        profile
            .dfu_address
            .as_deref()
            .unwrap_or("0x08000000:leave")
            .to_string(),
        "-D".to_string(),
        binary.display().to_string(),
    ]);
    Ok(args)
}

#[derive(Debug, Clone)]
/// A USB device seen on the bus.
pub struct UsbDevice {
    /// sysfs node for the device.
    pub sysfs_path: PathBuf,
    /// USB bus number.
    pub busnum: String,
    /// USB device number.
    pub devnum: String,
    /// USB vendor id, lowercase hex.
    pub vendor: String,
    /// USB product id, lowercase hex.
    pub product: String,
    /// USB manufacturer string, if the device reports one.
    pub manufacturer: Option<String>,
    /// USB product string, if the device reports one.
    pub product_name: Option<String>,
}

/// Human-readable name of a flash method, as printed by `--list`.
pub fn method_name(method: FlashMethod) -> &'static str {
    match method {
        FlashMethod::ProbeRs => "probe-rs",
        FlashMethod::Uf2 => "uf2",
        FlashMethod::Dfu => "dfu",
    }
}

/// Suggests a `just flash` command for a device whose USB manufacturer
/// string matches exactly one profile's `vendor`.
///
/// Returns `None` when the match is ambiguous or the device reports no
/// manufacturer — a wrong suggestion is worse than none, since following it
/// flashes the wrong firmware.
pub fn suggest_native_flash(device: &UsbDevice, profiles: &[BoardProfile]) -> Option<String> {
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

/// Like [`suggest_native_flash`], falling back to the single UF2 profile
/// when the device is an RP2040 in bootloader mode.
pub fn suggest_usb_flash(device: &UsbDevice, profiles: &[BoardProfile]) -> Option<String> {
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

/// Builds the cargo argument list that compiles a profile.
pub fn cargo_args(profile: &BoardProfile, run: bool) -> Vec<String> {
    let mut args = vec![
        if run { "run" } else { "build" }.to_string(),
        "-p".to_string(),
        profile.package.clone(),
        "--example".to_string(),
        profile.example.clone(),
        "--target".to_string(),
        profile.target.clone(),
        "--release".to_string(),
    ];
    if profile.no_default_features {
        args.push("--no-default-features".to_string());
    }
    for feature in &profile.features {
        args.push("--features".to_string());
        args.push(feature.clone());
    }
    args
}

/// Maps a QMK `keyboard_name` to its family (the first path segment).
pub fn qmk_family(keyboard_name: &str) -> String {
    if keyboard_name.eq_ignore_ascii_case("zsa/voyager")
        || keyboard_name.eq_ignore_ascii_case("voyager")
    {
        return "zsa/voyager".to_string();
    }

    keyboard_name
        .split('/')
        .next()
        .unwrap_or(keyboard_name)
        .to_ascii_lowercase()
}

/// Returns `true` for an RP2040 in UF2 bootloader mode (VID 2e8a, PID 0003).
pub fn is_rp2_bootloader(device: &UsbDevice) -> bool {
    device.vendor.eq_ignore_ascii_case("2e8a") && device.product.eq_ignore_ascii_case("0003")
}

/// Formats a device as `manufacturer product` for display.
pub fn usb_device_name(device: &UsbDevice) -> String {
    let manufacturer = device
        .manufacturer
        .as_deref()
        .unwrap_or("unknown manufacturer");
    let product = device.product_name.as_deref().unwrap_or("unknown product");
    format!("{manufacturer} {product}")
}

/// The sysfs leaf name, used as the `--device` selector.
pub fn usb_device_id(device: &UsbDevice) -> String {
    device
        .sysfs_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// Identity of the USB device backing a block device, read from sysfs.
///
/// Populated by the binary (which does the sysfs walk); matched here.
#[derive(Debug, Clone)]
pub struct UsbSourceInfo {
    /// sysfs node for the device.
    pub sysfs_path: PathBuf,
    /// USB bus number.
    pub busnum: String,
    /// USB device number.
    pub devnum: String,
    /// USB vendor id, lowercase hex.
    pub vendor: String,
    /// USB product id, lowercase hex.
    pub product: String,
}

/// Returns `true` if `selector` identifies this device.
///
/// Accepts the sysfs name, `vid/pid`, `vid:pid`, `bus:dev`, or a bare bus or
/// device number.
pub fn usb_source_matches_selector(info: &UsbSourceInfo, selector: &str) -> bool {
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

/// Finds every `board.toml` under `root` and parses it into a profile.
///
/// Unreadable or malformed manifests are skipped rather than aborting the
/// scan, so one bad file does not hide every other board. Results are sorted
/// by name for stable output.
pub fn discover_native_profiles(root: &Path) -> Vec<BoardProfile> {
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
                no_default_features: manifest.no_default_features,
                qmk_family: manifest.qmk_family,
                vendor: manifest.vendor,
                dfu_vid_pid: manifest.dfu_vid_pid,
                dfu_address: manifest.dfu_address,
                dfu_max_size: manifest.dfu_max_size,
            });
        }
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles
}

/// Looks up a single profile by name under `root`.
///
/// The error lists the known profile names, which is the only useful thing to
/// say when someone typos one.
pub fn resolve_native_profile(root: &Path, name: &str) -> Result<BoardProfile, String> {
    discover_native_profiles(root)
        .into_iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| {
            let known = discover_native_profiles(root)
                .into_iter()
                .map(|profile| profile.name)
                .collect::<Vec<_>>()
                .join(", ");
            format!("unknown native profile `{name}`; known profiles: {known}")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn profile(name: &str, method: FlashMethod, vendor: Option<&str>) -> BoardProfile {
        BoardProfile {
            name: name.to_string(),
            package: format!("tecla-{name}"),
            example: name.to_string(),
            target: "thumbv6m-none-eabi".to_string(),
            method,
            features: Vec::new(),
            no_default_features: false,
            qmk_family: None,
            vendor: vendor.map(str::to_string),
            dfu_vid_pid: None,
            dfu_address: None,
            dfu_max_size: None,
        }
    }

    fn device(vendor: &str, product: &str, manufacturer: Option<&str>) -> UsbDevice {
        UsbDevice {
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-2"),
            busnum: "001".to_string(),
            devnum: "004".to_string(),
            vendor: vendor.to_string(),
            product: product.to_string(),
            manufacturer: manufacturer.map(str::to_string),
            product_name: Some("Keyboard".to_string()),
        }
    }

    #[test]
    fn method_names_are_the_tool_names() {
        assert_eq!(method_name(FlashMethod::ProbeRs), "probe-rs");
        assert_eq!(method_name(FlashMethod::Uf2), "uf2");
        assert_eq!(method_name(FlashMethod::Dfu), "dfu");
    }

    const STM32_DFU: &str = concat!(
        "Found DFU: [0483:df11] ver=2200, devnum=1, cfg=1, intf=0, path=\"20-4.1\", alt=1, name=\"@Option Bytes /0x1ffff800/01*016 e\", serial=\"TEST01\"\n",
        "Found DFU: [0483:df11] ver=2200, devnum=1, cfg=1, intf=0, path=\"20-4.1\", alt=0, name=\"@Internal Flash /0x08000000/064*002Kg\", serial=\"TEST01\"\n",
    );

    #[test]
    fn dfu_alternates_select_one_physical_target_and_forward_its_selectors() {
        let target = select_dfu_target(STM32_DFU, "0483:DF11", None)
            .unwrap()
            .expect("one physical device despite two alternates");
        assert_eq!(target.path.as_deref(), Some("20-4.1"));
        assert_eq!(target.serial.as_deref(), Some("TEST01"));
        for selector in [
            "20-4.1",
            "path:20-4.1",
            "TEST01",
            "serial:TEST01",
            "0483/df11",
        ] {
            assert_eq!(
                select_dfu_target(STM32_DFU, "0483:df11", Some(selector)).unwrap(),
                Some(target.clone())
            );
        }
        let mut p = profile("nos75", FlashMethod::Dfu, None);
        p.dfu_vid_pid = Some("0483:df11".to_string());
        p.dfu_address = Some("0x08000000:leave".to_string());
        let args = dfu_flash_args(&p, Path::new("nos75.bin"), Some(&target)).unwrap();
        assert_eq!(
            args,
            [
                "-d",
                "0483:df11",
                "-p",
                "20-4.1",
                "-S",
                "TEST01",
                "-a",
                "0",
                "-s",
                "0x08000000:leave",
                "-D",
                "nos75.bin"
            ]
        );
    }

    #[test]
    fn dfu_selection_rejects_ambiguity_and_cross_device_substring_matches() {
        let second = STM32_DFU
            .replace("20-4.1", "20-4.2")
            .replace("TEST01", "TEST02");
        let both = format!("{STM32_DFU}{second}");
        assert!(select_dfu_target(&both, "0483:df11", None).is_err());
        assert!(select_dfu_target(&both, "0483:df11", Some("0483:df11")).is_err());
        let target = select_dfu_target(&both, "0483:df11", Some("serial:TEST02"))
            .unwrap()
            .unwrap();
        assert_eq!(target.path.as_deref(), Some("20-4.2"));
        assert!(select_dfu_target(&both, "0483:df11", Some("TEST0"))
            .unwrap()
            .is_none());
        let unrelated = second.replace("0483:df11", "1234:5678");
        assert!(select_dfu_target(
            &format!("{STM32_DFU}{unrelated}"),
            "0483:df11",
            Some("TEST02")
        )
        .unwrap()
        .is_none());
        assert!(select_dfu_target(
            &STM32_DFU.replace("Found DFU", "Found Runtime"),
            "0483:df11",
            None
        )
        .unwrap()
        .is_none());
        assert!(select_dfu_target("Found DFU: [0483:df11] alt=0", "0483:df11", None).is_err());
    }

    #[test]
    fn dfu_preview_requires_typed_selectors_without_discovery() {
        assert!(dfu_preview_target(None, "0483:df11").unwrap().is_none());
        assert!(dfu_preview_target(Some("0483/df11"), "0483:df11")
            .unwrap()
            .is_none());
        assert_eq!(
            dfu_preview_target(Some("path:20-4.1"), "0483:df11")
                .unwrap()
                .unwrap()
                .path
                .as_deref(),
            Some("20-4.1")
        );
        assert_eq!(
            dfu_preview_target(Some("serial:TEST01"), "0483:df11")
                .unwrap()
                .unwrap()
                .serial
                .as_deref(),
            Some("TEST01")
        );
        assert!(dfu_preview_target(Some("TEST01"), "0483:df11").is_err());
        assert!(dfu_preview_target(Some("serial:"), "0483:df11").is_err());
    }

    #[test]
    fn dfu_size_limit_keeps_the_reserved_tail_out_of_the_image() {
        let mut p = profile("nos75", FlashMethod::Dfu, None);
        p.dfu_max_size = Some(122_880);
        assert!(validate_dfu_image_size(&p, 0).is_err());
        assert!(validate_dfu_image_size(&p, 1).is_ok());
        assert!(validate_dfu_image_size(&p, 122_880).is_ok());
        assert!(validate_dfu_image_size(&p, 122_881).is_err());
        p.dfu_max_size = None;
        assert!(validate_dfu_image_size(&p, 131_072).is_ok());
        assert!(validate_dfu_image_size(&p, 0).is_err());
    }

    #[test]
    fn cargo_args_build_vs_run() {
        let p = profile("planck", FlashMethod::Uf2, None);
        let build = cargo_args(&p, false);
        assert_eq!(build[0], "build");
        assert_eq!(
            build,
            [
                "build",
                "-p",
                "tecla-planck",
                "--example",
                "planck",
                "--target",
                "thumbv6m-none-eabi",
                "--release",
            ]
        );
        assert_eq!(cargo_args(&p, true)[0], "run");
    }

    #[test]
    fn qmk_family_takes_the_vendor_segment() {
        assert_eq!(qmk_family("planck/rev6"), "planck");
        assert_eq!(qmk_family("Preonic"), "preonic");
        assert_eq!(qmk_family("keebio/iris/rev2"), "keebio");
    }

    #[test]
    fn qmk_family_special_cases_the_voyager() {
        assert_eq!(qmk_family("voyager"), "zsa/voyager");
        assert_eq!(qmk_family("ZSA/Voyager"), "zsa/voyager");
    }

    #[test]
    fn rp2_bootloader_is_recognized_case_insensitively() {
        assert!(is_rp2_bootloader(&device("2e8a", "0003", None)));
        assert!(is_rp2_bootloader(&device("2E8A", "0003", None)));
        assert!(!is_rp2_bootloader(&device("2e8a", "000a", None)));
        assert!(!is_rp2_bootloader(&device("1234", "0003", None)));
    }

    #[test]
    fn usb_device_name_falls_back_when_strings_are_missing() {
        let mut d = device("2e8a", "0003", Some("Raspberry Pi"));
        assert_eq!(usb_device_name(&d), "Raspberry Pi Keyboard");
        d.manufacturer = None;
        d.product_name = None;
        assert_eq!(usb_device_name(&d), "unknown manufacturer unknown product");
    }

    #[test]
    fn usb_device_id_is_the_sysfs_leaf() {
        assert_eq!(usb_device_id(&device("2e8a", "0003", None)), "1-2");
    }

    #[test]
    fn native_suggestion_needs_exactly_one_vendor_match() {
        let profiles = [
            profile("voyager", FlashMethod::Dfu, Some("ZSA")),
            profile("planck", FlashMethod::Uf2, Some("OLKB")),
        ];
        let d = device("3297", "1977", Some("zsa"));
        assert_eq!(
            suggest_native_flash(&d, &profiles).as_deref(),
            Some("just flash --device 1-2 --tecla-config voyager")
        );

        // Two profiles claiming the same vendor is ambiguous — suggest nothing.
        let ambiguous = [
            profile("a", FlashMethod::Dfu, Some("ZSA")),
            profile("b", FlashMethod::Dfu, Some("zsa")),
        ];
        assert_eq!(suggest_native_flash(&d, &ambiguous), None);
    }

    #[test]
    fn native_suggestion_needs_a_manufacturer_string() {
        let profiles = [profile("voyager", FlashMethod::Dfu, Some("ZSA"))];
        assert_eq!(
            suggest_native_flash(&device("3297", "1977", None), &profiles),
            None
        );
        assert_eq!(
            suggest_native_flash(&device("3297", "1977", Some("  ")), &profiles),
            None
        );
    }

    #[test]
    fn usb_suggestion_falls_back_to_the_single_uf2_profile_for_a_rp2_bootloader() {
        let profiles = [
            profile("planck", FlashMethod::Uf2, None),
            profile("voyager", FlashMethod::Dfu, None),
        ];
        assert_eq!(
            suggest_usb_flash(&device("2e8a", "0003", None), &profiles).as_deref(),
            Some("just flash --tecla-config planck")
        );
        // Not a bootloader ⇒ no suggestion.
        assert_eq!(
            suggest_usb_flash(&device("2e8a", "000a", None), &profiles),
            None
        );
    }

    #[test]
    fn usb_source_selector_matches_every_documented_form() {
        let info = UsbSourceInfo {
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-2"),
            busnum: "001".to_string(),
            devnum: "004".to_string(),
            vendor: "2e8a".to_string(),
            product: "0003".to_string(),
        };
        for selector in ["1-2", "2e8a/0003", "2e8a:0003", "001:004", "001", "004"] {
            assert!(
                usb_source_matches_selector(&info, selector),
                "selector {selector:?} should match"
            );
        }
        assert!(!usb_source_matches_selector(&info, "nope"));
    }
}

/// Profile discovery against the repository's real `keyboards/tecla` tree.
///
/// These were impossible before: `discover_native_profiles` hardcoded a
/// relative path, so it only worked from the repo root and no test could point
/// it anywhere. Taking the root as a parameter makes the scan testable and
/// fixes the CLI's dependence on the current directory at the same time.
#[cfg(test)]
mod discovery_tests {
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("workspace root")
    }

    fn profile_root() -> PathBuf {
        repo_root().join("keyboards/tecla")
    }

    #[test]
    fn discovers_every_board_toml_in_the_repo() {
        let found = discover_native_profiles(&profile_root());
        let on_disk = walk_count(&profile_root());
        assert!(on_disk > 0, "no board.toml files in the repo to scan");
        assert_eq!(
            found.len(),
            on_disk,
            "every board.toml should parse; {} of {on_disk} did",
            found.len()
        );
    }

    fn walk_count(dir: &Path) -> usize {
        let mut n = 0;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.file_name() == Some(OsStr::new("board.toml")) {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn discovered_profiles_are_sorted_and_uniquely_named() {
        let profiles = discover_native_profiles(&profile_root());
        let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "profiles should come back sorted");

        let mut seen = sorted.clone();
        seen.dedup();
        assert_eq!(
            seen.len(),
            names.len(),
            "duplicate profile names: {names:?}"
        );
    }

    #[test]
    fn every_profile_names_a_real_workspace_crate_and_example() {
        // A profile pointing at a package or example that does not exist fails
        // only at flash time, on someone's desk with a board in bootloader mode.
        let root = repo_root();
        for p in discover_native_profiles(&profile_root()) {
            let crate_dir = root.join("crates").join(&p.package);
            assert!(
                crate_dir.is_dir(),
                "profile `{}` names crate `{}`, which does not exist",
                p.name,
                p.package
            );
            let example = crate_dir.join("examples").join(format!("{}.rs", p.example));
            assert!(
                example.is_file(),
                "profile `{}` names example `{}`, missing at {example:?}",
                p.name,
                p.example
            );

            let manifest: toml::Value = toml::from_str(
                &fs::read_to_string(crate_dir.join("Cargo.toml")).expect("crate manifest"),
            )
            .expect("valid Cargo.toml");
            assert_eq!(
                manifest["package"]["name"].as_str(),
                Some(p.package.as_str())
            );

            let features = manifest.get("features").and_then(toml::Value::as_table);
            for feature in &p.features {
                assert!(
                    features.is_some_and(|table| table.contains_key(feature)),
                    "profile `{}` enables undefined feature `{feature}`",
                    p.name
                );
            }
            let example_manifest = manifest
                .get("example")
                .and_then(toml::Value::as_array)
                .and_then(|examples| {
                    examples.iter().find(|example| {
                        example.get("name").and_then(toml::Value::as_str)
                            == Some(p.example.as_str())
                    })
                });
            if let Some(required) = example_manifest
                .and_then(|example| example.get("required-features"))
                .and_then(toml::Value::as_array)
            {
                let args = cargo_args(&p, false);
                let defaults = if p.no_default_features {
                    None
                } else {
                    features
                        .and_then(|table| table.get("default"))
                        .and_then(toml::Value::as_array)
                };
                for feature in required {
                    let feature = feature.as_str().expect("feature name");
                    assert!(
                        args.windows(2)
                            .any(|pair| pair[0] == "--features" && pair[1] == feature)
                            || defaults.is_some_and(|defaults| {
                                defaults
                                    .iter()
                                    .any(|enabled| enabled.as_str() == Some(feature))
                            }),
                        "profile `{}` omits required example feature `{feature}`",
                        p.name
                    );
                }
            }
        }
    }

    #[test]
    fn voyager_typing_and_music_profiles_keep_distinct_builds_on_the_same_hardware() {
        for (name, features) in [
            ("voyager", &[][..]),
            ("voyager_tonnetz", &["usb-midi"][..]),
            ("voyager_tonnetz_parallel", &["usb-midi"][..]),
            ("voyager_four_finger", &["usb-midi"][..]),
            ("voyager_accordion", &["usb-midi"][..]),
            ("voyager_harmony", &["usb-midi"][..]),
        ] {
            let p = resolve_native_profile(&profile_root(), name).expect("Voyager profile");
            assert_eq!(p.package, "tecla-stm32f3");
            assert_eq!(p.example, name);
            assert_eq!(p.features, features);
            assert_eq!(p.target, "thumbv7em-none-eabihf");
            assert_eq!(p.method, FlashMethod::Dfu);
            assert_eq!(p.qmk_family.as_deref(), Some("zsa/voyager"));
            assert_eq!(p.dfu_vid_pid.as_deref(), Some("3297:0791"));
            assert_eq!(p.dfu_address.as_deref(), Some("0x08002000:leave"));
        }
    }

    #[test]
    fn nos75_profile_uses_rom_dfu_and_reserves_configuration_flash() {
        let profile = resolve_native_profile(&profile_root(), "nos75").expect("NOS75 profile");
        assert_eq!(profile.package, "tecla-stm32f0");
        assert_eq!(profile.example, "nos75");
        assert_eq!(profile.target, "thumbv6m-none-eabi");
        assert_eq!(profile.method, FlashMethod::Dfu);
        assert_eq!(profile.dfu_vid_pid.as_deref(), Some("0483:df11"));
        assert_eq!(profile.dfu_address.as_deref(), Some("0x08000000:leave"));
        assert_eq!(profile.dfu_max_size, Some(122_880));
    }

    #[test]
    fn asterisk_music_profiles_do_not_change_the_typing_profiles_features() {
        for (name, features) in [
            ("asterisk_v1_4", &["steno-dictionary"][..]),
            ("asterisk_v1_4_keytest", &[][..]),
            ("asterisk_v1_4_midi", &["usb-midi"][..]),
            ("asterisk_v1_4_autochord", &["usb-midi"][..]),
            ("asterisk_v1_4_omnichord", &["usb-midi"][..]),
            ("asterisk_v1_4_chromatica", &["usb-midi"][..]),
            ("asterisk_v1_4_suspension", &["usb-midi"][..]),
        ] {
            let p = resolve_native_profile(&profile_root(), name).expect("Asterisk profile");
            assert_eq!(p.package, "tecla-rp2040");
            assert_eq!(p.example, name);
            assert_eq!(p.features, features);
            assert_eq!(p.target, "thumbv6m-none-eabi");
            assert_eq!(p.method, FlashMethod::Uf2);
        }
    }

    #[test]
    fn ble_profile_uses_mpsl_critical_sections_without_the_usb_defaults() {
        let ble =
            resolve_native_profile(&profile_root(), "planck-nrf52840-ble").expect("BLE profile");
        assert!(ble.no_default_features);
        let args = cargo_args(&ble, true);
        assert_eq!(args[0], "run");
        assert!(args.iter().any(|arg| arg == "--no-default-features"));
        assert!(args.windows(2).any(|pair| pair == ["--features", "ble"]));

        let usb = resolve_native_profile(&profile_root(), "planck-nrf52840").expect("USB profile");
        assert!(!usb.no_default_features);
        assert!(!cargo_args(&usb, true)
            .iter()
            .any(|arg| arg == "--no-default-features"));
    }

    #[test]
    fn a_connected_voyager_cannot_silently_select_one_of_its_layouts() {
        let profiles = discover_native_profiles(&profile_root());
        let device = UsbDevice {
            sysfs_path: PathBuf::from("/sys/bus/usb/devices/1-2"),
            busnum: "001".to_string(),
            devnum: "004".to_string(),
            vendor: "3297".to_string(),
            product: "0791".to_string(),
            manufacturer: Some("ZSA Technology Labs".to_string()),
            product_name: Some("Voyager".to_string()),
        };
        assert_eq!(suggest_native_flash(&device, &profiles), None);
        assert_eq!(suggest_usb_flash(&device, &profiles), None);

        let family = qmk_family("ZSA/Voyager");
        let names = profiles
            .iter()
            .filter(|p| p.qmk_family.as_deref() == Some(&family))
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>();
        for name in [
            "voyager",
            "voyager_tonnetz",
            "voyager_tonnetz_parallel",
            "voyager_harmony",
        ] {
            assert!(names.contains(&name));
        }
    }

    #[test]
    fn dfu_profiles_carry_the_vid_pid_the_flasher_needs() {
        for p in discover_native_profiles(&profile_root()) {
            if p.method == FlashMethod::Dfu {
                assert!(
                    p.dfu_vid_pid.is_some(),
                    "DFU profile `{}` has no dfu_vid_pid; the flasher cannot find the device",
                    p.name
                );
            }
        }
    }

    #[test]
    fn resolve_finds_a_known_profile_and_explains_an_unknown_one() {
        let root = profile_root();
        let first = discover_native_profiles(&root)
            .into_iter()
            .next()
            .expect("at least one profile");
        assert_eq!(
            resolve_native_profile(&root, &first.name)
                .expect("known")
                .name,
            first.name
        );

        let err = resolve_native_profile(&root, "no-such-board").expect_err("unknown");
        assert!(err.contains("no-such-board"));
        assert!(
            err.contains(&first.name),
            "the error should list known profiles, got: {err}"
        );
    }

    #[test]
    fn scanning_a_missing_directory_yields_nothing_instead_of_panicking() {
        assert!(discover_native_profiles(Path::new("/nonexistent/tecla")).is_empty());
    }

    #[test]
    fn cargo_args_for_every_real_profile_keep_its_package_target_and_features() {
        for p in discover_native_profiles(&profile_root()) {
            let args = cargo_args(&p, matches!(p.method, FlashMethod::ProbeRs));
            for (flag, value) in [
                ("-p", p.package.as_str()),
                ("--example", p.example.as_str()),
                ("--target", p.target.as_str()),
            ] {
                assert!(
                    args.windows(2).any(|pair| pair == [flag, value]),
                    "{}",
                    p.name
                );
            }
            assert!(args.iter().any(|arg| arg == "--release"));
            assert_eq!(
                args.iter().any(|arg| arg == "--no-default-features"),
                p.no_default_features,
                "{}",
                p.name
            );
            let enabled = args
                .windows(2)
                .filter(|pair| pair[0] == "--features")
                .map(|pair| pair[1].as_str())
                .collect::<Vec<_>>();
            assert_eq!(enabled, p.features, "{}", p.name);
        }
    }
}
