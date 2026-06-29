//! Build script — copies `memory.x` to OUT_DIR for `cortex-m-rt` linker discovery.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::copy("memory.x", out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    // The voyager example pulls its keymap + combos via `include!`, which cargo
    // does NOT track for rebuilds — so editing the config silently flashes a
    // stale binary. Watch the config files explicitly.
    println!("cargo:rerun-if-changed=../../keyboards/tecla/voyager/keymaps/default.rs");
    println!("cargo:rerun-if-changed=../../keyboards/tecla/voyager/combos/default.rs");
    println!("cargo:rerun-if-changed=build.rs");
}
