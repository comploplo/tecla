//! Expose the NOS75 linker layout and track its editable keymap.
fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::copy("memory.x", out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    for flag in ["--nmagic", "-Tlink.x", "-Tdefmt.x"] {
        println!("cargo:rustc-link-arg={flag}");
    }
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=profiles");
    println!("cargo:rerun-if-changed=build.rs");
}
