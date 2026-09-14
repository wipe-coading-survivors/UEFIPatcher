#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
REFS="$(readlink -f "$REPO_ROOT/refs")"
IMAGE="uefipatcher-edk2-builder:latest"

if [ $# -lt 2 ]; then
    echo "usage: disasm.sh <info|strings|xref|dis|mmio> <file-in-/refs> [args...]" >&2
    exit 2
fi

exec podman-remote run --rm --security-opt label=disable \
    -v "$REPO_ROOT/hack":/hack:ro \
    -v "$REFS":/refs:ro \
    "$IMAGE" python3 /hack/uncore_disasm.py "$@"
