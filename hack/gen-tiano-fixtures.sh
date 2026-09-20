#!/usr/bin/env bash
# Генератор фикстур tiano-op: payload всех Compressed-секций (algo 1) образа
# разворачиваются C-оракулом. Спека tiano-op §3.
# Использование: hack/gen-tiano-fixtures.sh <образ> <outdir>
set -euo pipefail
image=$1; outdir=$2
root="$(cd "$(dirname "$0")/.." && pwd)"
sock=${UEFIPATCHER_SOCK:-/tmp/tiano-fixtures.sock}
cc -O2 -I "$root/refs/UEFITool-ai-fork/common/Tiano" -o /tmp/tiano-oracle \
   "$root/hack/tiano-oracle.c" \
   "$root/refs/UEFITool-ai-fork/common/Tiano/EfiTianoDecompress.c"
mkdir -p "$outdir"
rm -f "$sock"
export UEFIPATCHER_SOCK=$sock
"$root/target/debug/engine" >"$outdir/engine.log" 2>&1 &
engine_pid=$!
trap 'kill $engine_pid 2>/dev/null || true; rm -f "$sock"' EXIT
sleep 1
cd "$root"
cli=target/debug/uefi-cli
$cli session init --force >/dev/null
$cli image open "$image" >/dev/null
i=0
for target in $($cli node list 2>/dev/null | grep 'Section(Compressed)' | awk '{print $1}'); do
    body=$outdir/$(echo "$target" | tr / _).body
    payload=${body%.body}.in
    aid=$($cli node extract "$target" --body-only 2>/dev/null | tail -1)
    $cli artifact export "$aid" "$body" >/dev/null
    # algo-байт (5-й тела) обязан быть 1 (EFI standard) — разведка: 228/228 на C275
    if [ "$(head -c 5 "$body" | tail -c 1 | xxd -p)" != "01" ]; then
        rm -f "$body"; continue
    fi
    dd if="$body" bs=1 skip=5 of="$payload" 2>/dev/null
    if /tmp/tiano-oracle 4 <"$payload" >"${payload%.in}.expected" 2>/dev/null; then
        i=$((i+1))
    else
        rm -f "$payload" "${payload%.in}.expected"
    fi
    rm -f "$body"
done
echo "fixtures: $i -> $outdir"
