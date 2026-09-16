#!/usr/bin/env python3
"""uncore_iou0_flags_patch.py — v22: транслятор IOU0 sel0 -> флаги «все активны».

Отчёт docs/reports/2026-09-14-uncore-bif-recon.md §P. Цепочка, вскрытая
фронтом (f):

  * селекторы IOU0/IOU1/IOU2 = cfg+0x248/0x24c/0x250 (dev2/3/1); RC-дефолт
    политики = FF, кодовые дефолты nat {0,4,1} (донор sm: {4,4,1}).
  * PcieLinkTrainingInit (F_e90e48, PE VA 0xffe90e48) пишет селектор
    БЕЗ отображения в RP+0x190 (bit3 = start_bifurcation): живой трейс
    M3 «IIO=0, IOU0=0» => 2A.0x190=0 => x4x4x4x4 — расщепление УЖЕ
    активно (даташит 6.2.86: dev2/3 field 000=x4x4x4x4, 100=x16).
  * трейс M3: D[2]:F[1] Link up as x04 Gen3 при ECAM FFFF — линки
    через sibling-функции тренируются, декод выкл. отдельно.
  * транслятор F_e97ad7 (PE VA 0xffe97ad7) пакует селектор в пер-портовые
    флаги cfg[0x8cb+sock*11+port]/cfg[0x964+...]. Ветка iou==0 (dev2,
    порты 4/5/6 = 2B/2C/2D): sel4 -> все три флага =1; дефолт-провал
    (sel0 и inval) -> нули. Селектор перегружен: в 0x190-пространстве
    0=x4x4x4x4, во флаг-пространстве 0=«нет sibling-портов» — шизофрения
    леново-сборки. Менять селектор нельзя (сломает 0x190/лейн-таблицы),
    поэтому развязка здесь: sel0 ведёт себя как sel4.

Патч 5 байт @image 0xe97bb7 (FV12 nat-uncore, несжат; image_off =
VMA - 0xff000000, см. hack/uncore_publish_patch.py):

    03 c6 88 50 04   add %esi,%eax; mov %dl,0x4(%eax)   ; дефолт: нули
    -> eb 05 90 90 90 jmp 0xffe97bbe (+nops)             ; -> ветка sel4

0x190 и лейн-таблицы (sel0={04,04,04,04}) не тронуты. Известный риск:
DXE IioInit перетранслирует селектор сам (F_15d50, та же семантика) и
может сбросить флаги — если вердикт отрицательный, следующий шаг:
патч F_15d50 через rebuild FV8 движком.

    python3 hack/uncore_iou0_flags_patch.py <base.bin> <out.bin>
    python3 hack/uncore_iou0_flags_patch.py <bin> --check
"""
import hashlib
import sys

PATCH_OFF = 0xE97BB7  # image = VMA 0xffe97bb7 - 0xff000000
OLD = bytes.fromhex("03c6885004")  # add %esi,%eax; mov %dl,0x4(%eax)
NEW = bytes.fromhex("eb05909090")  # jmp 0xffe97bbe (sel4-ветка); nop*3


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
        print(f"{args[0]}: {'IOU0-FLAGS-PATCH OK' if ok else 'NOT PATCHED'} ({cur.hex(' ')})")
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
