//! CLI for compiling vendored Lapwing JSON dictionaries into DAFSA bytecode.

use std::path::PathBuf;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let input_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dictionaries"));
    let output_file = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("lapwing.steno"));

    if args.next().is_some() {
        eprintln!("usage: compile_lapwing_dictionary [input_dir] [output_file]");
        std::process::exit(2);
    }

    let build = tecla_config::steno_dictionary::compile_lapwing_dictionary_dir(&input_dir)
        .unwrap_or_else(|err| {
            eprintln!(
                "failed to compile Lapwing dictionary from {}: {err}",
                input_dir.display()
            );
            std::process::exit(1);
        });

    std::fs::write(&output_file, &build.bytes).unwrap_or_else(|err| {
        eprintln!("failed to write {}: {err}", output_file.display());
        std::process::exit(1);
    });

    eprintln!(
        "compiled {} files into {} bytes; kept {} entries, skipped {} outlines and {} translations",
        build.stats.files_scanned,
        build.bytes.len(),
        build.stats.entries_kept,
        build.stats.entries_skipped_outline,
        build.stats.entries_skipped_translation,
    );
}
