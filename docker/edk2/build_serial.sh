#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/../.." && pwd)"
PKG_DIR="$REPO_ROOT/docker/edk2"
EDK2_SRC="$REPO_ROOT/refs/edk2"
IMAGE="uefipatcher-edk2-builder:latest"

if [ "${EDK2_BUILDER_CONTAINER:-0}" != "1" ]; then
    OUT_DIR="${1:-$(mktemp -d /tmp/serial-s1-out.XXXXXX)}"
    mkdir -p "$OUT_DIR"
    podman-remote run --rm --security-opt label=disable \
        -v "$EDK2_SRC":/src/edk2:ro \
        -v "$PKG_DIR":/pkg:ro \
        -v "$OUT_DIR":/out \
        "$IMAGE" /pkg/build_serial.sh /out
    echo "artifacts: $OUT_DIR"
    ls -la "$OUT_DIR"
    exit 0
fi

OUT_DIR="${1:?usage: build_serial.sh <out-dir>}"
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
build -p UefiPatcherSerialPkg/UefiPatcherSerial.dsc -a X64 -t GCC -b RELEASE -n "$(nproc)"
BUILD_ROOT="$WORKSPACE/Build/UefiPatcherSerial/RELEASE_GCC"
cp "$BUILD_ROOT/FV/Ffs/9A5163E7-5C29-453F-825C-837A46A81E15SerialDxe/9A5163E7-5C29-453F-825C-837A46A81E15.ffs" "$OUT_DIR/SerialDxe.ffs"
cp "$BUILD_ROOT/FV/Ffs/9E863906-A40F-4875-977F-5B93FF237FC6TerminalDxe/9E863906-A40F-4875-977F-5B93FF237FC6.ffs" "$OUT_DIR/TerminalDxe.ffs"
cp "$BUILD_ROOT/FV/SERIAL_CONSOLE_FV.Fv" "$OUT_DIR/"
ls -la "$OUT_DIR"
