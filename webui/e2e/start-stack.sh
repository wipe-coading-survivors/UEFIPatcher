#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
E2E_DIR=/tmp/uefipatcher-e2e
rm -rf "$E2E_DIR"
mkdir -p "$E2E_DIR"
rm -f "$E2E_DIR/engine.sock"
UEFIPATCHER_DATA="$E2E_DIR" UEFIPATCHER_SOCK="$E2E_DIR/engine.sock" ../target/debug/engine &
echo $! > "$E2E_DIR/engine.pid"
for _ in $(seq 1 150); do [ -S "$E2E_DIR/engine.sock" ] && break; sleep 0.2; done
[ -S "$E2E_DIR/engine.sock" ]
exec env UEFIPATCHER_SOCK="$E2E_DIR/engine.sock" \
    UEFIPATCHER_GATEWAY_LISTEN=127.0.0.1:8123 \
    UEFIPATCHER_WEBUI_DIR="$PWD/build" \
    ../target/debug/uefi-gateway
