#!/usr/bin/env python3
"""uncore_policy_scan.py — сканер записей полиси бифуркации в UncoreInitPeim.

Отчёт docs/reports/2026-09-14-uncore-bif-recon.md §R2b/§B4: линейный
sweep .text (skipdata) с трекингом lea — печатает каждую
`mov byte ptr [X], imm`, резолвящуюся в семейство полей 0x248-0x251
(0x248=IOU0-s0/Port 2, 0x24c=IOU1-s0/Port 3, 0x250=IOU2-s0/Port 1,
нечные +1 = сокет 1; 0x24a/0x24b/0x24e/0x24f — соседние не-полиси
поля). Калибровка: 12 сайтов IOU0-s0 = таблица §R2b. Требует capstone
(контейнер edk2-builder); запуск через hack/disasm.sh не предусмотрен —
монтировать /hack и образ:

    podman-remote run --rm -v hack:/hack:ro -v <dir>:/work \
        uefipatcher-edk2-builder:latest python3 /hack/uncore_policy_scan.py /work/IIO-pei32.bin
"""
import sys

sys.path.insert(0, "/hack")
from uncore_disasm import Image
from capstone.x86_const import X86_OP_MEM, X86_OP_IMM, X86_OP_REG

POL = set(range(0x248, 0x252))

img = Image(sys.argv[1])
md = img.disassembler()
md.skipdata = True

for roff, end in img.code_ranges():
    last_lea = {}
    for insn in md.disasm(img.data[roff:end], img.file_off_to_va(roff) or img.image_base):
        if insn.id == 0:
            last_lea.clear()
            continue
        if insn.mnemonic == "lea" and len(insn.operands) == 2:
            dst, src = insn.operands
            if dst.type == X86_OP_REG and src.type == X86_OP_MEM:
                last_lea[dst.reg] = (src.mem.base, src.mem.disp)
        elif insn.mnemonic == "mov" and len(insn.operands) == 2:
            dst, src = insn.operands
            if dst.type == X86_OP_MEM and dst.size == 1 and src.type == X86_OP_IMM and dst.mem.index == 0:
                disp = dst.mem.disp
                if disp == 0 and dst.mem.base != 0:
                    info = last_lea.get(dst.mem.base)
                    if info:
                        disp = info[1]
                if disp in POL:
                    print(f"{insn.address:#010x}  {insn.bytes.hex():<18} {insn.mnemonic} {insn.op_str}  -> field {disp:#x} imm={src.imm}")
