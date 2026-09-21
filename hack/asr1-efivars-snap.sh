#!/usr/bin/env bash
# Снапшот ВСЕХ efivars рига asr1 в refs/amibcp/asr1-map/<name>/ (карта переменных 226D2IL).
# Использование: hack/asr1-efivars-snap.sh <state-name>   например s0-baseline
# Итог: <name>/efivars/* (полный слепок) + <name>/Setup.bin (главная Setup-EC87D643, плоско)
set -euo pipefail
name="${1:?usage: $0 <state-name>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/refs/amibcp/asr1-map/$name"
[ -e "$dest" ] && { echo "already exists: $dest" >&2; exit 1; }
mkdir -p "$dest"
ssh root-asr1 'tar -C /sys/firmware/efi -cf - efivars 2>/dev/null' | tar -C "$dest" -xf -
setup="$dest/efivars/Setup-ec87d643-eba4-4bb5-a1e5-3f3e36b20da9"
[ -f "$setup" ] && cp "$setup" "$dest/Setup.bin"
echo "state: $name -> $(ls "$dest/efivars" | wc -l) vars, Setup $(wc -c < "$dest/Setup.bin") bytes"
