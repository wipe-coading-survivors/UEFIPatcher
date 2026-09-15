#!/usr/bin/env python3
"""uncore_bif_patch.py — вариант 2: in-place патч полиси бифуркации IOU.

План docs/superpowers/plans/2026-09-14-bif-recon.md Task V2; сайты —
docs/reports/2026-09-14-uncore-bif-recon.md §R2b/§B4: инструкции
`mov byte ptr [cfg+сокет0+0x248/0x24c/0x250], imm` (подмена AUTO=0xFF
борд-константой; IOU0=Port 2, IOU1=Port 3, IOU2=Port 1), imm {1,2,3,4}
→ 0 = x4x4x4x4. PEIM D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 несжат в FV
0xE200000, PE32-тело побайтово @ image 0xE64B08 (одна копия); FFS
attrs=0x00 → ATTRIB_CHECKSUM clear, байт file-checksum 0xAA не
пересчитывается — правка = только байты imm, размеры не меняются.
Списки сайтов сгенерированы из PEIM (hack/uncore_policy_scan.py,
lea-трекинг; калибровка: 12 сайтов IOU0 = таблица §R2b). Не часть движка:

    python3 hack/uncore_bif_patch.py <base.bin> <out.bin> [--ports=all]
    python3 hack/uncore_bif_patch.py <patched.bin> --check [--ports=all]

--ports=port2 (по умолчанию): только IOU0/Port 2 (patch1, §B4).
--ports=all: IOU0+IOU1+IOU2 сокета 0 (эксперимент владельца «все порты»).
Сокет-1 (0x249/0x24d/0x251), не-полиси поля 0x24a/0x24b/0x24e/0x24f и
страп-ветка IOU2 (VA 0xffe97848, al из MMIO) не патчатся.
"""
import hashlib
import sys

PEIM_IMG_OFF = 0xE64B08
VA_IMG_BASE = 0xFF000000

SITES_IOU0 = [
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

SITES_IOU1 = [
    (0x3229A, "c6874c02000004"),
    (0x32325, "c60004"),
    (0x324A0, "c60004"),
    (0x32643, "c6864c02000004"),
    (0x326F6, "c6864c02000004"),
    (0x3270F, "c6864c02000002"),
    (0x3272A, "c6864c02000003"),
    (0x32747, "c6864c02000002"),
    (0x3276A, "c6864c02000001"),
    (0x328F6, "c60004"),
    (0x32ABA, "c6864c02000004"),
    (0x32AE7, "c6864c02000004"),
    (0x32AFE, "c6864c02000003"),
    (0x32BF6, "c60004"),
    (0x32D5F, "c60004"),
    (0x32DB1, "c60003"),
]

SITES_IOU2 = [
    (0x3227A, "c6875002000001"),
    (0x32309, "c60001"),
    (0x32484, "c60001"),
    (0x32627, "c60001"),
    (0x328DA, "c60001"),
    (0x32AA0, "c60001"),
    (0x32BE8, "c60001"),
]

MODES = {
    "port2": SITES_IOU0,
    "all": SITES_IOU0 + SITES_IOU1 + SITES_IOU2,
}


def site_bytes(peim_off, patched):
    insn = bytearray(bytes.fromhex(dict(MODES["all"])[peim_off]))
    if patched:
        insn[-1] = 0
    return bytes(insn)


def verify(data, sites, patched):
    for peim_off, _ in sites:
        want = site_bytes(peim_off, patched)
        got = data[PEIM_IMG_OFF + peim_off:PEIM_IMG_OFF + peim_off + len(want)]
        if got != want:
            sys.exit(f"site peim={peim_off:#x} img={PEIM_IMG_OFF + peim_off:#x}: "
                     f"want {want.hex()} ({'patched' if patched else 'base'}), got {got.hex()}")


def main():
    args = sys.argv[1:]
    ports = "port2"
    if "--ports=all" in args:
        ports = "all"
        args.remove("--ports=all")
    if len(args) == 2 and args[1] == "--check":
        data = open(args[0], "rb").read()
        verify(data, MODES[ports], True)
        print(f"ok: {len(MODES[ports])} sites patched ({ports}, all imm=0), sha256 "
              f"{hashlib.sha256(data).hexdigest()}")
        return
    if len(args) != 2:
        sys.exit(__doc__)

    sites = MODES[ports]
    base = open(args[0], "rb").read()
    verify(base, sites, False)
    out = bytearray(base)
    for peim_off, _ in sites:
        img_off = PEIM_IMG_OFF + peim_off
        out[img_off + len(site_bytes(peim_off, False)) - 1] = 0

    diffs = [i for i in range(len(base)) if base[i] != out[i]]
    if len(diffs) != len(sites):
        sys.exit(f"unexpected diff count: {len(diffs)} != {len(sites)}")
    if sorted(diffs) != sorted(PEIM_IMG_OFF + off + len(bytes.fromhex(raw)) - 1
                               for off, raw in sites):
        sys.exit("diff offsets do not match site imm bytes")
    if len(out) != len(base):
        sys.exit("image size changed")

    print(f"base  sha256 {hashlib.sha256(base).hexdigest()}")
    print(f"out   sha256 {hashlib.sha256(bytes(out)).hexdigest()}")
    print(f"{'#':>2} {'peim':>8} {'image':>8} {'VA':>12}  bytes")
    for i, (peim_off, _) in enumerate(sites, 1):
        img_off = PEIM_IMG_OFF + peim_off
        va = hex(VA_IMG_BASE + img_off)
        print(f"{i:>2} {peim_off:#8x} {img_off:#8x} {va:>12}  "
              f"{site_bytes(peim_off, False).hex()} -> {site_bytes(peim_off, True).hex()}")
    with open(args[1], "wb") as f:
        f.write(out)
    print(f"ok: {len(sites)} sites patched ({ports}) -> {args[1]} ({len(out)} bytes)")


if __name__ == "__main__":
    main()
