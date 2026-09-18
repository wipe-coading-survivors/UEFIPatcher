#!/usr/bin/env python3
"""v20b: хирургический фикс ложного DL_Active в IioInit (0x121c8 tail).

v20 (NOP je @PE+0xE1BA в writer'е) оказался слишком грубым: сломал путь
тренированных портов через ветку width-масок e201 (изменился аккумулятор
ebx → UBOX+0x64c биты 8-9 + *out writer'а) — DXE вис (железо, §O6).

v20b правит сам предикат: LNKSTA@0xA2 читает 0xFFFF у скрытого порта →
(0xFFFF>>13)&1 = 1 = ложный «DL active». Код-пещера в .text-slack
(24 нулевых байта @PE+0x179C8, file==VMA): хвост 0x121c8 заменён на
jmp в пещеру, которая возвращает al = (ax & 0xE000) == 0x2000 —
«бит 13 установлен, зарезервированные биты 14-15 чисты». Скрытый порт
(FFFF) → 0 («линк неактивен», семантика донора), живой линк → 1,
прочее → 0. Тренированные порты идут по стоковому пути без изменений.

Действует на всех 4 вызывающих (e09b/e1b3/e421/ffcc — аккумуляторы и
writer), у всех семантика «порт занят/активен».

Патч (PE-оффсеты):
  sect-table .text VirtualSize: 0x17748 → 0x17760 (= SizeOfRawData; пещера
    в файловсом slack за VirtualSize НЕ мапится DXE-загрузчиком — v20b вис
    именно на прыжке в незамапленые нули; раздуваем vsize ровно до соседней
    секции: 0x280+0x17760 = 0x179E0 = vaddr следующей, SizeOfImage не меняется)
  0x12308 (10B): e9 bb 56 00 00 90 90 90 90 90   jmp 0x179C8 + nops
  0x179C8 (20B): 0f b7 04 10 movzx eax,[rax+rdx]
                 66 25 00 e0 and  ax,0xe000
                 66 3d 00 20 cmp  ax,0x2000
                 0f 94 c0    sete al
                 e9 36 a9 ff ff jmp 0x12312

Каноническая сборка — движком (node replace 8/17/1/0 --body-only на
декомпрессированном PE + image save); этот скрипт готовит пропатченный
PE для artifact import и independently проверяет байты.

Запуск: python3 hack/iioinit_dlactive_cave_patch.py <iioinit-native.pe> <out.pe>
"""
import struct
import sys

TEXT_VADDR = 0x280
TEXT_VSIZE_ORIG = 0x17748
TEXT_VSIZE_NEW = 0x17760  # == SizeOfRawData; 0x280+0x17760 == vaddr след. секции
TAIL_OFF = 0x12308
CAVE_OFF = 0x179C8
JMP1 = bytes.fromhex("e9bb560000") + b"\x90" * 5  # jmp 0x179C8
CAVE = bytes.fromhex("0fb70410662500e0663d00200f94c0e936a9ffff")


def main(src, dst):
    pe = bytearray(open(src, "rb").read())
    orig_tail = pe[TAIL_OFF : TAIL_OFF + 10]
    expect = bytes.fromhex("0fb7041066c1e80d2401")
    if orig_tail != expect:
        sys.exit(f"хвост 0x121c8 не совпал @0x{TAIL_OFF:x}: {orig_tail.hex()} != {expect.hex()}")
    cave_area = pe[CAVE_OFF : CAVE_OFF + 24]
    if cave_area != bytes(24):
        sys.exit(f"пещера @0x{CAVE_OFF:x} не нулевая: {cave_area.hex()}")
    # пересчёт rel32 на случай иных адресов (сверка с захардкоженными)
    rel1 = CAVE_OFF - (TAIL_OFF + 5)
    rel2 = TAIL_OFF + 10 - (CAVE_OFF + 20)
    assert struct.pack("<i", rel1) == JMP1[1:5], hex(rel1)
    assert struct.pack("<i", rel2) == CAVE[16:20], hex(rel2)
    pe[TAIL_OFF : TAIL_OFF + 10] = JMP1
    pe[CAVE_OFF : CAVE_OFF + len(CAVE)] = CAVE
    # раздуть VirtualSize .text: секция с vaddr 0x280 в таблице секций
    e_lfanew = struct.unpack_from("<I", pe, 0x3C)[0]
    nsec = struct.unpack_from("<H", pe, e_lfanew + 6)[0]
    optsz = struct.unpack_from("<H", pe, e_lfanew + 20)[0]
    fixed = False
    for i in range(nsec):
        o = e_lfanew + 24 + optsz + i * 40
        vsize, vaddr = struct.unpack_from("<II", pe, o + 8)
        if vaddr == TEXT_VADDR:
            if vsize != TEXT_VSIZE_ORIG:
                sys.exit(f".text vsize=0x{vsize:x}, ожидался 0x{TEXT_VSIZE_ORIG:x}")
            struct.pack_into("<I", pe, o + 8, TEXT_VSIZE_NEW)
            fixed = True
            break
    if not fixed:
        sys.exit(".text секция не найдена")
    open(dst, "wb").write(pe)
    print(f"OK: {dst} (jmp rel32=0x{rel1:x}, back rel32=0x{rel2 & 0xffffffff:x})")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
