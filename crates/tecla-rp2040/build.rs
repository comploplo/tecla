//! Build script: emits linker search path for `memory.x`.

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    if std::env::var_os("CARGO_FEATURE_STENO_DICTIONARY").is_some() {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let dict_dir = std::path::Path::new(&manifest_dir)
            .join("..")
            .join("..")
            .join("dictionaries");
        let out_file = std::path::Path::new(&out_dir).join("lapwing_dict.rs");

        println!("cargo:rerun-if-changed={}", dict_dir.display());
        if let Ok(entries) = std::fs::read_dir(&dict_dir) {
            for entry in entries.flatten() {
                println!("cargo:rerun-if-changed={}", entry.path().display());
            }
        }

        let build = tecla_config::steno_dictionary::compile_lapwing_dictionary_dir(&dict_dir)
            .unwrap_or_else(|err| {
                panic!(
                    "failed to compile Lapwing dictionary from {}: {err}",
                    dict_dir.display()
                );
            });

        let mut source = String::from("static LAPWING_DICT: &[u8] = &[\n");
        for chunk in build.bytes.chunks(12) {
            source.push_str("    ");
            for byte in chunk {
                use std::fmt::Write as _;
                let _ = write!(source, "0x{byte:02X}, ");
            }
            source.push('\n');
        }
        source.push_str("];\n");

        std::fs::write(&out_file, source).unwrap_or_else(|err| {
            panic!("failed to write {}: {err}", out_file.display());
        });
    }

    if std::env::var_os("CARGO_FEATURE_CORE1_AUTOCORRECT").is_some() {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let out_file = std::path::Path::new(&out_dir).join("autocorrect_demo_dict.rs");

        // Small demo dictionary — enough to show the core1 split correcting
        // common typos. Swap in a full SCOWL trie for production.
        let words: &[&str] = &[
            "the", "and", "you", "that", "this", "have", "with", "for", "not", "are", "from",
            "they", "what", "when", "your", "would", "there", "their", "about", "which", "because",
            "really", "should",
        ];
        let entries: Vec<(String, u8)> = words.iter().map(|w| ((*w).to_string(), 0u8)).collect();
        let trie = tecla_config::trie::compile_trie(&entries);

        let mut source = String::from("pub static AUTOCORRECT_DICT: &[u8] = &[\n");
        for chunk in trie.chunks(12) {
            source.push_str("    ");
            for byte in chunk {
                use std::fmt::Write as _;
                let _ = write!(source, "0x{byte:02X}, ");
            }
            source.push('\n');
        }
        source.push_str("];\n");

        std::fs::write(&out_file, source).unwrap_or_else(|err| {
            panic!("failed to write {}: {err}", out_file.display());
        });
    }

    println!("cargo:rustc-link-search={manifest_dir}");
    println!("cargo:rerun-if-changed=memory.x");
}
