default: test

# Run tests across all workspace crates
test:
    cargo nextest run --workspace

# Check every feature combination compiles and passes tests
# Requires: cargo install cargo-hack cargo-nextest
hack:
    cargo hack --workspace --feature-powerset nextest run

# Clippy across all crates and feature combinations
# Run this before implementing any module to verify clean baseline
lint:
    cargo hack --workspace --feature-powerset clippy --all-targets

# Format all crates
fmt:
    cargo fmt --all

# Check formatting without writing (used by pre-commit and CI)
fmt-check:
    cargo fmt --all -- --check

# Coverage — tecla core must be 100%, all other crates 90%+
# Requires: cargo install cargo-llvm-cov
cov:
    cargo llvm-cov --workspace --features tecla/testing --open

# Coverage enforcement — CI gate, fails build if thresholds not met
cov-core:
    cargo llvm-cov -p tecla --features testing \
        --fail-under-lines 100 \
        --lcov --output-path lcov-core.info

cov-all:
    cargo llvm-cov --workspace --features tecla/testing \
        --fail-under-lines 90 \
        --lcov --output-path lcov.info

# Run everything — same gates as CI
ci:
    just fmt-check
    just lint
    just hack
    just cov-core
    just cov-all
