//! Build script: emits linker search path for `memory.x`.

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={manifest_dir}");
    println!("cargo:rerun-if-changed=memory.x");
}
