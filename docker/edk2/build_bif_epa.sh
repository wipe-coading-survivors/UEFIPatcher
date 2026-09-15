#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/../.." && pwd)"
PKG_DIR="$REPO_ROOT/docker/edk2"
EDK2_SRC="$REPO_ROOT/refs/edk2"
IMAGE="uefipatcher-edk2-builder:latest"
MMR_JSON="$REPO_ROOT/docs/reports/2026-09-14-uncore-bif-mmr.json"
HDR_OUT="$PKG_DIR/UefiPatcherBifPkg/BifEpaProbe/EpaConfig.h"

OUT_DIR=""
EPA_MODE="read"
EPA_RESET="warm"
EPA_PAUSE_MS=3000
while [ $# -gt 0 ]; do
    case "$1" in
        --epa-mode=*) EPA_MODE="${1#*=}" ;;
        --reset=*) EPA_RESET="${1#*=}" ;;
        --pause-ms=*) EPA_PAUSE_MS="${1#*=}" ;;
        *) OUT_DIR="$1" ;;
    esac
    shift
done
[ -n "$OUT_DIR" ] || { echo "usage: build_bif_epa.sh <out-dir> [--epa-mode=read|write] [--reset=warm|cold] [--pause-ms=N]" >&2; exit 2; }
case "$EPA_MODE" in read|write) ;; *) echo "--epa-mode: read|write" >&2; exit 2 ;; esac
case "$EPA_RESET" in warm|cold) ;; *) echo "--reset: warm|cold" >&2; exit 2 ;; esac

if [ "${EDK2_BUILDER_CONTAINER:-0}" != "1" ]; then
    python3 "$REPO_ROOT/hack/gen_mmr_table.py" --mmr "$MMR_JSON" \
        --mode "$EPA_MODE" --reset "$EPA_RESET" --pause-ms "$EPA_PAUSE_MS" --out "$HDR_OUT"
    mkdir -p "$OUT_DIR"
    podman-remote run --rm --security-opt label=disable \
        -v "$EDK2_SRC":/src/edk2:ro \
        -v "$PKG_DIR":/pkg:ro \
        -v "$OUT_DIR":/out \
        "$IMAGE" /pkg/build_bif_epa.sh /out
    echo "artifacts: $OUT_DIR"
    ls -la "$OUT_DIR"
    exit 0
fi

: "${OUT_DIR:?}"
rm -rf /work/edk2
mkdir -p /work/edk2
git -C /src/edk2 archive --format=tar HEAD | tar -xf - -C /work/edk2
mkdir -p /work/edk2/MdePkg/Library/MipiSysTLib/mipisyst/library/include \
         /work/edk2/MdeModulePkg/Library/BrotliCustomDecompressLib/brotli/c/include
cd /work/edk2
make -C BaseTools -j"$(nproc)" APPLICATIONS='GenFfs GenFv GenSec GenFw'
export WORKSPACE="$PWD"
export PACKAGES_PATH="$PWD:/pkg"
set +u
. ./edksetup.sh BaseTools
set -u
build -p UefiPatcherBifPkg/UefiPatcherBif.dsc -a X64 -t GCCNOLTO -b RELEASE -n "$(nproc)"
BUILD_ROOT="$WORKSPACE/Build/UefiPatcherBif/RELEASE_GCCNOLTO"
cp "$BUILD_ROOT/FV/Ffs/B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84BifEpaProbe/B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84.ffs" "$OUT_DIR/BifEpaProbe.ffs"
python3 - "$OUT_DIR/BifEpaProbe.ffs" <<'PY'
import struct, sys
p = sys.argv[1]
d = bytearray(open(p, 'rb').read())
off = 24
while off + 4 <= len(d):
    sz = d[off] | d[off+1] << 8 | d[off+2] << 16
    if d[off+3] == 0x10:
        pe = off + 4
        lfa = struct.unpack_from('<I', d, pe + 0x3C)[0]
        assert struct.unpack_from('<H', d, pe + lfa + 22)[0] == 0x002E, 'unexpected characteristics'
        struct.pack_into('<H', d, pe + lfa + 22, 0x2022)
        break
    off += (sz + 3) & ~3
open(p, 'wb').write(bytes(d))
PY
ls -la "$OUT_DIR"
