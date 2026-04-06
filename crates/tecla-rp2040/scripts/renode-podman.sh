#!/usr/bin/env bash
set -euo pipefail

IMAGE="${RENODE_IMAGE:-tecla-renode:latest}"
REPO_ROOT="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

exec podman run --rm \
    --entrypoint /opt/renode/renode \
    --volume="/tmp:/tmp" \
    --volume="${REPO_ROOT}:${REPO_ROOT}:ro" \
    "$IMAGE" --disable-xwt --plain "$@"
