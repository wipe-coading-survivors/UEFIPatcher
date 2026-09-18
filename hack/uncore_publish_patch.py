#!/usr/bin/env python3
"""uncore_publish_patch.py — v18: безусловная публикация функций IIO.

Отчёт docs/reports/2026-09-14-uncore-bif-recon.md §N: F_9182d
(VMA 0xffe9182d) делает 16-бит RMW RP+0xA0 (NTB-раскладка 0x1B0)
`and 0xffaf | (flag?0x40:0)` per порт; флаг = cfg[0x2bc+sock*11+port]
из policy-префикса (PPI 2AB86EF5-ECB5-4134-B556-3854CA1FE1B4, NVRAMPei).
RC-дефолт (PE VA 0xffe70ec0): +0x2bc все 01 = публиковать все 44;
OEM NVRAM-политика прячет 2B/2C/2D (живое: видимые 1A/2A/3A имеют
0xa0.w=0x0c40 — бит 0x40 стоит). Патч игнорирует флаг:

    movzbl 0x8(%ebp),%ecx; neg %ecx; sbb %ecx,%ecx; and $0x40,%ecx
      → mov $0x40,%ecx; nop ×6

PEIM D71C8BA4-4AF2-4D0D-B1BA-F2409F0C20D3 несжат, image_off =
VMA - 0xFF000000 (см. hack/uncore_bif_patch.py — та же база).
Не часть движка:

    python3 hack/uncore_publish_patch.py <base.bin> <out.bin>
    python3 hack/uncore_publish_patch.py <bin> --check
"""
import hashlib
import sys

PATCH_OFF = 0xE918AA  # image = VMA 0xffe918aa - 0xff000000
OLD = bytes.fromhex("0fb64d08f7d91bc983e140")
NEW = bytes.fromhex("b9400000009090909090") + bytes.fromhex("90")


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--check"]
    check = "--check" in sys.argv
    if len(args) != (1 if check else 2):
        print(__doc__)
        return 2
    img = bytearray(open(args[0], "rb").read())
    cur = bytes(img[PATCH_OFF : PATCH_OFF + len(OLD)])
    if check:
        ok = cur == NEW
        print(f"{args[0]}: {'PUBLISH-PATCH OK' if ok else 'NOT PATCHED'} ({cur.hex(' ')})")
        return 0 if ok else 1
    if cur != OLD:
        print(f"unexpected bytes @ {PATCH_OFF:#x}: {cur.hex(' ')}")
        return 1
    img[PATCH_OFF : PATCH_OFF + len(OLD)] = NEW
    open(args[1], "wb").write(bytes(img))
    print(f"patched {args[1]}: {PATCH_OFF:#x} {OLD.hex(' ')} -> {NEW.hex(' ')}")
    print("sha256:", hashlib.sha256(bytes(img)).hexdigest())
    return 0


if __name__ == "__main__":
    sys.exit(main())
