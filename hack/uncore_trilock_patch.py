#!/usr/bin/env python3
"""uncore_trilock_patch.py — v21: третий замок cfg[0x448+port]==2 нейтрализован.

Отчёт docs/reports/2026-09-14-uncore-bif-recon.md §O8: после publish-форса
(v18, RP+0xA0.6 всем портам @0xe918aa) и presence-форса (v19, e0b/e37=1
@0xe96a8a) конфигуратор F_91ceb (VMA 0xffe91ceb, nat-uncore PE; пишет
RP+0x39c/0x3f0 — единственный кандидат декод-включателя) всё ещё скипает
порт по третьему гейту: `F_96403(...)==1 && cfg[0x448+sock*11+port]==2 →
je ffe9212d (хвост цикла)`. RC-дефолт массива 0x448 — все 00 (константа
политики, PE VA 0xffe70ec0+0x448); живые значения строит мост
LoadHob/NVRAMPei (FV12 12/16, 12/45; переменные Setup/IntelSetup GUID
EC87D643-…) при сборке NVRAM-записи политики. Патч меняет иммедиат
cmp $2 → cmp $0xff на трёх сайтах (сравнение никогда не истинно):

  0xe91d76: 80 ba 48 04 00 00 02  cmpb $0x2,0x448(%edx)        — F_91ceb
  0xe94c65: 80 bc 30 48 04 00 00 02  cmpb $0x2,0x448(%eax,%esi) — F_94c5x
  0xe9645f: 3c 02                 cmp $0x2,%al                 — F_9642c

Семантика значения 2 = «политика: порт выключен» (F_9642c: 0→override-
таблица, 1→true, 2→false). PEIM D71C8BA4-… несжат, image_off =
VMA - 0xff000000 (см. hack/uncore_publish_patch.py). База патча — v20c
(уже содержит v17+v18+v19+v20c-cave), дельта ровно 3 байта.

    python3 hack/uncore_trilock_patch.py <base.bin> <out.bin>
    python3 hack/uncore_trilock_patch.py <bin> --check
"""
import hashlib
import sys

# (image offset, imm-байт внутри инструкции, VMA начала инструкции)
PATCHES = [
    (0xE91D7C, 0xFFE91D76),  # F_91ceb gate1
    (0xE94C6C, 0xFFE94C65),  # gate2 (F_94c5x, вызывается из F_948c3)
    (0xE96460, 0xFFE9645F),  # F_9642c gate3 (предикат F_937f2/F_93ca8)
]
OLD_IMM = 0x02
NEW_IMM = 0xFF


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--check"]
    check = "--check" in sys.argv
    if len(args) != (1 if check else 2):
        print(__doc__)
        return 2
    img = bytearray(open(args[0], "rb").read())
    if check:
        ok = all(img[off] == NEW_IMM for off, _ in PATCHES)
        detail = " ".join(f"{off:#x}={img[off]:02x}" for off, _ in PATCHES)
        print(f"{args[0]}: {'TRILOCK-PATCH OK' if ok else 'NOT PATCHED'} ({detail})")
        return 0 if ok else 1
    for off, vma in PATCHES:
        if img[off] != OLD_IMM:
            print(f"unexpected imm @ {off:#x} (VMA {vma:#x}): {img[off]:02x}")
            return 1
        img[off] = NEW_IMM
    open(args[1], "wb").write(bytes(img))
    print(f"patched {args[1]}: " + ", ".join(f"{off:#x} 02->ff" for off, _ in PATCHES))
    print("sha256:", hashlib.sha256(bytes(img)).hexdigest())
    return 0


if __name__ == "__main__":
    sys.exit(main())
