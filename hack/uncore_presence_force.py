#!/usr/bin/env python3
"""uncore_presence_force.py — v19: presence-массивы e0b/e37 всегда 1.

Отчёт docs/reports/2026-09-14-uncore-bif-recon.md §O: пробер F_96a0f
(VMA 0xffe96a0f, nat-uncore PE) копирует дефолтную BDF-таблицу (все 44
функции, PE VA 0xffe70e68) и зондит VID@0x0 каждого RP: нет 0x8086 →
cfg[0xe37+sock*11+port]=0 и cfg[0xe0b+...]=0. У донора (sm-uncore)
VID-зонда нет вовсе. Для скрытой функции зонд читает FFFF → «порта
нет» → F_91ceb (единственный пер-портовый конфигуратор, RP+0x39c/
0x3f0) скипает порт навсегда — самосбывающийся замок, v18 его не
трогает. Патч игнорирует результат сравнения VID (5 байт):

    cmp %cx,%ax; je +5; movb $0,(%ebx); jmp +4; movb $1,-0x2c(%ebx)
      → nop ×2 (не скипать); movb $1,(%ebx); nop ×2 (провалиться в e0b=1)

Оба presence-байта пишутся 1 безусловно. PEIM D71C8BA4-… несжат,
image_off = VMA - 0xFF000000 (см. hack/uncore_publish_patch.py).

    python3 hack/uncore_presence_force.py <base.bin> <out.bin>
    python3 hack/uncore_presence_force.py <bin> --check
"""
import hashlib
import sys

PATCH_OFF = 0xE96A8A  # image = VMA 0xffe96a8a - 0xff000000 (je после cmp)
OLD = bytes.fromhex("7405c60300eb04")  # je +5; movb $0,(%ebx); jmp +4
NEW = bytes.fromhex("9090c603019090")  # nop ×2; movb $1,(%ebx); nop ×2


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
        print(f"{args[0]}: {'PRESENCE-PATCH OK' if ok else 'NOT PATCHED'} ({cur.hex(' ')})")
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
