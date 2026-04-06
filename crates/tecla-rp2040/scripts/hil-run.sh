#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel)"
CRATE_DIR="$REPO_ROOT/crates/tecla-rp2040"
IMAGE="${RENODE_IMAGE:-tecla-renode:latest}"

if ! podman image exists "$IMAGE" 2>/dev/null; then
    echo "Building Renode container image..."
    podman build -t "$IMAGE" -f "$SCRIPT_DIR/Containerfile" "$SCRIPT_DIR"
fi

echo "Building HIL firmware..."
cargo build \
    --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p tecla-rp2040 \
    --example hil_keyboard \
    --features tecla-rp2040/hil \
    --target thumbv6m-none-eabi \
    --release

FIRMWARE_ELF="$REPO_ROOT/target/thumbv6m-none-eabi/release/examples/hil_keyboard"
REPL_PATH="$CRATE_DIR/platforms/rp2040-minimal.repl"

vtable=$(rust-objdump -s -j .vector_table "$FIRMWARE_ELF" 2>/dev/null | grep '^ 1' | head -1)
VTOR=0x$(echo "$vtable" | awk '{print $1}')
SP=$(echo "$vtable" | awk '{
    printf "0x%s%s%s%s\n", substr($2,7,2), substr($2,5,2), substr($2,3,2), substr($2,1,2)
}')
reset_raw=$(echo "$vtable" | awk '{
    printf "0x%s%s%s%s\n", substr($3,7,2), substr($3,5,2), substr($3,3,2), substr($3,1,2)
}')
PC=$(printf "0x%08X" $(( $(printf '%d' "$reset_raw") & ~1 )))

disasm=$(rust-objdump -d "$FIRMWARE_ELF" 2>/dev/null)
HOOK_ADDR=$(echo "$disasm" | awk '
    /movs\tr0, #0x18/ { addr = $1 }
    /beab/ { if (addr) { print "0x" substr(addr, 1, length(addr)-1); exit } }
')

echo "  VTOR=$VTOR SP=$SP PC=$PC HOOK=$HOOK_ADDR"

RESC_TPL="$CRATE_DIR/platforms/planck.resc.tpl"
RESC_RENDERED="$CRATE_DIR/planck-hil.resc"
trap 'rm -f "$RESC_RENDERED"' EXIT

sed \
    -e "s|{{FIRMWARE_ELF}}|${FIRMWARE_ELF}|g" \
    -e "s|{{REPL_PATH}}|${REPL_PATH}|g" \
    -e "s|{{VTOR}}|${VTOR}|g" \
    -e "s|{{SP}}|${SP}|g" \
    -e "s|{{PC}}|${PC}|g" \
    -e "s|{{HOOK_ADDR}}|${HOOK_ADDR}|g" \
    "$RESC_TPL" > "$RESC_RENDERED"

echo "Running HIL test in Renode..."
output=$(timeout --kill-after=5 30 "$SCRIPT_DIR/renode-podman.sh" -e "include @${RESC_RENDERED}" 2>&1) || true

if echo "$output" | grep -q "HIL OK"; then
    echo "HIL test passed."
    exit 0
else
    echo "HIL test FAILED. Renode output:"
    echo "$output"
    exit 1
fi
