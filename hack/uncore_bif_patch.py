#!/usr/bin/env python3
"""uncore_bif_patch.py — вариант 2: in-place патч полиси IOU0/Port 2 (x4x4x4x4).

План docs/superpowers/plans/2026-09-14-bif-recon.md Task V2; сайты —
docs/reports/2026-09-14-uncore-bif-recon.md §R2b: 12 инструкций записи
байта полиси [cfg+сокет0+0x248] (board-ветки), imm {1,2,3,4} → 0.
PEIM D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 несжат в FV 0xE200000,
PE32-тело побайтово @ image 0xE64B08 (одна копия); FFS attrs=0x00 →
ATTRIB_CHECKSUM clear, байт file-checksum 0xAA не пересчитывается —
правка = ровно 12 байт, размеры не меняются. Не часть движка:

    python3 hack/uncore_bif_patch.py <base.bin> <out.bin>   # патч
    python3 hack/uncore_bif_patch.py <patched.bin> --check  # верификация
"""
import hashlib
import sys

PEIM_IMG_OFF = 0xE64B08
VA_IMG_BASE = 0xFF000000

SITES = [
    (0x3228A, "c6874802000004"),
    (0x32317, "c60004"),
    (0x32492, "c60003"),
    (0x32633, "c6864802000003"),
    (0x32665, "c6864802000002"),
    (0x326DA, "c6864802000004"),
    (0x328E8, "c60004"),
    (0x32AAE, "c60003"),
    (0x32C2C, "c60104"),
    (0x32C3A, "c60101"),
    (0x32D51, "c60004"),
    (0x32DA4, "c60004"),
]


def site_bytes(peim_off, patched):
    insn = bytearray(bytes.fromhex(dict(SITES)[peim_off]))
    if patched:
        insn[-1] = 0
    return bytes(insn)


def verify(data, patched):
    for peim_off, _ in SITES:
        want = site_bytes(peim_off, patched)
        got = data[PEIM_IMG_OFF + peim_off:PEIM_IMG_OFF + peim_off + len(want)]
        if got != want:
            sys.exit(f"site peim={peim_off:#x} img={PEIM_IMG_OFF + peim_off:#x}: "
                     f"want {want.hex()} ({'patched' if patched else 'base'}), got {got.hex()}")


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    if sys.argv[2] == "--check":
        data = open(sys.argv[1], "rb").read()
        verify(data, True)
        print(f"ok: {len(SITES)} sites patched (all imm=0), sha256 "
              f"{hashlib.sha256(data).hexdigest()}")
        return

    base = open(sys.argv[1], "rb").read()
    verify(base, False)
    out = bytearray(base)
    for peim_off, _ in SITES:
        img_off = PEIM_IMG_OFF + peim_off
        out[img_off + len(site_bytes(peim_off, False)) - 1] = 0

    diffs = [i for i in range(len(base)) if base[i] != out[i]]
    if len(diffs) != len(SITES):
        sys.exit(f"unexpected diff count: {len(diffs)} != {len(SITES)}")
    if sorted(diffs) != sorted(PEIM_IMG_OFF + off + len(bytes.fromhex(raw)) - 1
                               for off, raw in SITES):
        sys.exit("diff offsets do not match site imm bytes")
    if len(out) != len(base):
        sys.exit("image size changed")

    print(f"base  sha256 {hashlib.sha256(base).hexdigest()}")
    print(f"out   sha256 {hashlib.sha256(bytes(out)).hexdigest()}")
    print(f"{'#':>2} {'peim':>8} {'image':>8} {'VA':>12}  bytes")
    for i, (peim_off, _) in enumerate(SITES, 1):
        img_off = PEIM_IMG_OFF + peim_off
        va = hex(VA_IMG_BASE + img_off)
        print(f"{i:>2} {peim_off:#8x} {img_off:#8x} {va:>12}  "
              f"{site_bytes(peim_off, False).hex()} -> {site_bytes(peim_off, True).hex()}")
    with open(sys.argv[2], "wb") as f:
        f.write(out)
    print(f"ok: {len(SITES)} sites patched -> {sys.argv[2]} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
