//! DAFSA steno dictionary lookup benchmarks.
//!
//! Frames the steno engine as a relative-regression check, not an absolute
//! MCU-cycle claim: x86 microsecond numbers translate to ARM ratios within
//! ±2×. The point is to keep `process_stroke` complexity bounded as the
//! dictionary grows.
//!
//! Three workload classes:
//!
//! - **single-stroke hit** — common path, one trie descent.
//! - **multi-stroke hit** — two- and three-stroke briefs, the expensive path
//!   in real-world Plover use.
//! - **miss** — stroke that doesn't match any child; resets and returns None.

#![allow(missing_docs, reason = "criterion bench harness")]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tecla::devices::steno::{StenoChord, StenoEngine};
use tecla_config::trie::compile_steno_dictionary;

/// Builds a synthetic DAFSA with `n` single-stroke entries plus a small
/// constellation of multi-stroke briefs. Strokes are densely packed u64 bit
/// patterns so we stress the binary-search-over-children path inside each
/// node.
fn build_dict(n_singles: u32) -> &'static [u8] {
    let mut entries: Vec<(Vec<u64>, Vec<u8>)> = Vec::with_capacity(n_singles as usize + 8);
    for i in 0..n_singles {
        // Spread strokes across the u32 chord space — each entry is a unique
        // single-stroke brief with a 4-byte translation header + payload.
        let stroke = u64::from(i.wrapping_mul(2_654_435_761)) | 1;
        let payload: Vec<u8> = vec![0x01, 0x01, 0x00, b'a' + (i as u8 % 26)];
        entries.push((vec![stroke], payload));
    }
    // Two-stroke briefs.
    for i in 0..32u32 {
        let s1 = u64::from(i) << 10 | 1;
        let s2 = u64::from(i) << 20 | 2;
        entries.push((vec![s1, s2], vec![0x01, 0x02, 0x00, b'X', b'Y']));
    }
    // Three-stroke briefs.
    for i in 0..8u32 {
        let s1 = u64::from(i) << 4 | 1;
        let s2 = u64::from(i) << 8 | 1;
        let s3 = u64::from(i) << 12 | 1;
        entries.push((vec![s1, s2, s3], vec![0x01, 0x03, 0x00, b'X', b'Y', b'Z']));
    }
    let raw = compile_steno_dictionary(&entries);
    Box::leak(raw.into_boxed_slice())
}

fn bench_single_stroke_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("steno/single_stroke_hit");
    for &n in &[64u32, 1_024, 16_384] {
        let dict = build_dict(n);
        // Pick a stroke that's known to be in the dict (entry 0).
        let stroke = StenoChord { keys: 1 };
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let mut engine = StenoEngine::new(dict);
                let _ = engine.process_stroke(black_box(stroke));
            });
        });
    }
    group.finish();
}

fn bench_multi_stroke_hit(c: &mut Criterion) {
    let dict = build_dict(1_024);
    // Mirrors the first two-stroke entry pushed in build_dict(): i=0 → s1=1, s2=2.
    let s1 = StenoChord { keys: 1 };
    let s2 = StenoChord { keys: 2 };
    c.bench_function("steno/two_stroke_hit", |b| {
        b.iter(|| {
            let mut engine = StenoEngine::new(dict);
            let _ = engine.process_stroke(black_box(s1));
            let _ = engine.process_stroke(black_box(s2));
        });
    });

    // Mirrors the first three-stroke entry pushed in build_dict(): i=0 →
    // all three strokes have value 1 (different bit layouts but same value).
    let t1 = StenoChord { keys: 1 };
    let t2 = StenoChord { keys: 1 };
    let t3 = StenoChord { keys: 1 };
    c.bench_function("steno/three_stroke_hit", |b| {
        b.iter(|| {
            let mut engine = StenoEngine::new(dict);
            let _ = engine.process_stroke(black_box(t1));
            let _ = engine.process_stroke(black_box(t2));
            let _ = engine.process_stroke(black_box(t3));
        });
    });
}

fn bench_miss(c: &mut Criterion) {
    let dict = build_dict(1_024);
    // Stroke value that won't match any child (high bit set, even-numbered).
    let miss = StenoChord {
        keys: 0xFFFF_FFFE_FFFF_FFFE,
    };
    c.bench_function("steno/miss", |b| {
        b.iter(|| {
            let mut engine = StenoEngine::new(dict);
            let _ = engine.process_stroke(black_box(miss));
        });
    });
}

criterion_group!(
    benches,
    bench_single_stroke_hit,
    bench_multi_stroke_hit,
    bench_miss
);
criterion_main!(benches);
