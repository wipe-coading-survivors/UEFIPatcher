#!/usr/bin/env python3
"""uncore_disasm.py — RE-стенд PEIM'ов Grantley (цикл A бифуркации).

Спека docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md §4
(R1), план docs/superpowers/plans/2026-09-14-bif-recon.md Task 2.
Парсит PE32 (MZ/PE) и TE (VZ) образы: заголовки, ASCII-строки с VA,
xref-ы (байт-скан little-endian адреса), таргетный дизасм, скан
MMIO/PCI-CF8 доступов. Требует capstone (контейнер edk2-builder);
запуск через hack/disasm.sh. Не часть движка; запуск вручную:

    hack/disasm.sh info    /refs/fw/IIO-pei32.bin
    hack/disasm.sh strings /refs/fw/IIO-pei32.bin 8
    hack/disasm.sh xref    /refs/fw/IIO-pei32.bin 'Invalid IOUx'
    hack/disasm.sh dis     /refs/fw/IIO-pei32.bin 0x1000 0x80
    hack/disasm.sh mmio    /refs/fw/IIO-pei32.bin 0xF0000000 0xFFFFFFFF
"""
import struct
import sys

from capstone import Cs, CS_ARCH_X86, CS_MODE_32, CS_MODE_64

TE_SIG = b"VZ"
MACHINE_MODES = {0x14C: CS_MODE_32, 0x8664: CS_MODE_64}
MACHINE_NAMES = {0x14C: "I386", 0x8664: "AMD64"}


class Image:
    def __init__(self, path):
        self.path = path
        self.data = open(path, "rb").read()
        d = self.data
        self.te = d[:2] == TE_SIG
        if self.te:
            self.machine, nsec, self.subsystem, stripped = struct.unpack_from("<HHHH", d, 2)
            self.entry, self.base_of_code = struct.unpack_from("<II", d, 10)
            (self.image_base,) = struct.unpack_from("<Q", d, 18)
            sec_off, hdr_delta = 42, stripped
        else:
            assert d[:2] == b"MZ", f"{path}: no MZ"
            lfa = struct.unpack_from("<I", d, 0x3C)[0]
            assert d[lfa:lfa + 4] == b"PE\x00\x00", f"{path}: no PE"
            self.machine, nsec = struct.unpack_from("<HH", d, lfa + 4)
            self.subsystem = struct.unpack_from("<H", d, lfa + 92)[0]
            self.entry = struct.unpack_from("<I", d, lfa + 40)[0]
            if struct.unpack_from("<H", d, lfa + 24)[0] == 0x20B:
                (self.image_base,) = struct.unpack_from("<Q", d, lfa + 48)
            else:
                (self.image_base,) = struct.unpack_from("<I", d, lfa + 52)
            sec_off = lfa + 24 + struct.unpack_from("<H", d, lfa + 20)[0]
            hdr_delta = 0
        self.sections = []
        for i in range(nsec):
            s = sec_off + i * 40
            name = d[s:s + 8].rstrip(b"\x00").decode("ascii", "replace")
            vsize, va, rsize, roff = struct.unpack_from("<IIII", d, s + 8)
            self.sections.append({"name": name, "vsize": vsize, "va": va, "rsize": rsize, "roff": roff})
        self.hdr_delta = hdr_delta

    def file_off_to_va(self, off):
        for s in self.sections:
            if s["roff"] <= off < s["roff"] + s["rsize"]:
                return self.image_base + s["va"] + (off - s["roff"])
        return self.image_base + off - self.hdr_delta if off < self.hdr_delta + 0x400 else None

    def va_to_file(self, va):
        rva = va - self.image_base
        for s in self.sections:
            if s["va"] <= rva < s["va"] + max(s["vsize"], s["rsize"]):
                off = s["roff"] + (rva - s["va"])
                return off if off < len(self.data) else None
        return None

    def code_ranges(self):
        out = []
        for s in self.sections:
            if s["rsize"] and (s["name"].startswith(".text") or "X" in s["name"] or s["name"] in ("UPX0", ".cod")):
                out.append((s["roff"], s["roff"] + s["rsize"]))
        return out or [(self.sections and (self.sections[0]["roff"], self.sections[0]["roff"] + self.sections[0]["rsize"])) or (0, 0)]

    def disassembler(self):
        md = Cs(CS_ARCH_X86, MACHINE_MODES.get(self.machine, CS_MODE_32))
        md.detail = True
        return md


def parse_int(s):
    return int(s, 16) if s.lower().startswith("0x") else int(s, 0)


def cmd_info(img):
    kind = "TE" if img.te else "PE32"
    print(f"{img.path}: {kind} machine={MACHINE_NAMES.get(img.machine, hex(img.machine))} "
          f"subsystem={img.subsystem} entry={hex(img.image_base + img.entry)} base={hex(img.image_base)}")
    for s in img.sections:
        print(f"  {s['name']:<8} va={hex(img.image_base + s['va']):<12} vsize={s['vsize']:#x} raw={s['roff']:#x}+{s['rsize']:#x}")


def cmd_strings(img, min_len=4):
    min_len = int(min_len)
    runs, cur, start = [], b"", 0
    for i, b in enumerate(img.data):
        if 0x20 <= b < 0x7F:
            if not cur:
                start = i
            cur += bytes([b])
        else:
            if len(cur) >= min_len:
                runs.append((start, cur))
            cur = b""
    for off, s in runs:
        va = img.file_off_to_va(off)
        if va:
            print(f"{va:#010x} {s.decode('ascii')}")


def find_string_va(img, needle):
    target = needle.encode("ascii", "replace")
    idx = img.data.find(target)
    while idx != -1:
        va = img.file_off_to_va(idx)
        if va:
            return va
        idx = img.data.find(target, idx + 1)
    return None


def cmd_xref(img, target):
    va = parse_int(target) if target.lower().startswith("0x") else find_string_va(img, target)
    if va is None:
        sys.exit(f"xref: {target!r} not found / not section-backed")
    print(f"; target {target!r} va={va:#x}")
    refs = [va] if target.lower().startswith("0x") else range(va - 8, va + 1)
    for ref in refs:
        for width in (4, 8):
            pat = ref.to_bytes(width, "little")
            idx = img.data.find(pat)
            while idx != -1:
                site = img.file_off_to_va(idx)
                if site:
                    print(f"hit  file={idx:#x} site~{site:#x} width={width} ref={ref:#x} -{va - ref}")
                idx = img.data.find(pat, idx + 1)


def cmd_dis(img, va, length):
    off = img.va_to_file(va)
    if off is None:
        sys.exit(f"dis: {va:#x} not mapped")
    md = img.disassembler()
    for insn in md.disasm(img.data[off:off + length], va):
        print(f"{insn.address:#010x}  {insn.bytes.hex():<20} {insn.mnemonic} {insn.op_str}")


def cmd_mmio(img, lo, hi):
    md = img.disassembler()
    for roff, end in img.code_ranges():
        for insn in md.disasm(img.data[roff:end], img.file_off_to_va(roff) or img.image_base):
            hit = None
            for op in insn.operands:
                if op.type == 3 and lo <= op.mem.disp <= hi:  # X86_OP_MEM, absolute/RIP-rel disp
                    hit = f"mem={op.mem.disp:#x}"
                elif op.type == 2 and insn.mnemonic.startswith("mov") and lo <= op.imm <= hi:  # X86_OP_IMM
                    hit = f"imm={op.imm:#x}"
            if 0xCF8 <= (insn.operands[1].imm if len(insn.operands) > 1 and insn.operands[1].type == 2 else 0) <= 0xCFC:
                hit = hit or "cf8/cfc"
            if hit:
                print(f"{insn.address:#010x}  {insn.bytes.hex():<20} {insn.mnemonic} {insn.op_str}  ; {hit}")


def main():
    cmds = {"info": (cmd_info, 0), "strings": (cmd_strings, 1), "xref": (cmd_xref, 1),
            "dis": (cmd_dis, 2), "mmio": (cmd_mmio, 2)}
    if len(sys.argv) < 3 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    name, path = sys.argv[1], sys.argv[2]
    fn, _ = cmds[name]
    fn(Image(path), *[parse_int(a) if a.lower().startswith("0x") else a for a in sys.argv[3:]])


if __name__ == "__main__":
    main()
