#!/usr/bin/env python3
"""edk2_bif_check.py — инварианты артефакта BifEpaProbe (цикл A).

Спека docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md 5/9;
паттерн hack/edk2_build_check.py (S1). Со вторым аргументом-каталогом
сравнивает две сборки побайтово (допустимое расхождение — только COFF
TimeDateStamp). Не часть движка; запуск вручную:

    python3 hack/edk2_bif_check.py <out-dir> [<out-dir2>]
"""
import struct
import sys
import uuid

EXPECTED = {
    "BifEpaProbe.ffs": ("B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84", 11),
}
FFS_TYPE_DRIVER = 0x07
SECTION_PE32 = 0x10
SECTION_UI = 0x15
PE_MACHINE_AMD64 = 0x8664


def u24(b, o):
    return b[o] | (b[o + 1] << 8) | (b[o + 2] << 16)


def walk_sections(d):
    off, end, pe32s, ui = 24, len(d), [], None
    while off + 4 <= end:
        size, stype = u24(d, off), d[off + 3]
        if size < 4:
            break
        if stype == SECTION_PE32:
            pe32s.append((off + 4, d[off + 4:off + size]))
        elif stype == SECTION_UI:
            ui = d[off + 4:off + size].decode("utf-16-le", "ignore").rstrip("\x00")
        off += (size + 3) & ~3
    return pe32s, ui


def pe_fields(pe):
    assert pe[:2] == b"MZ", "no MZ"
    lfa = struct.unpack_from("<I", pe, 0x3C)[0]
    assert pe[lfa:lfa + 4] == b"PE\x00\x00", "no PE signature"
    machine = struct.unpack_from("<H", pe, lfa + 4)[0]
    subsystem = struct.unpack_from("<H", pe, lfa + 92)[0]
    return machine, subsystem, lfa + 8


def zero_coff_tds(b):
    pe32s, _ = walk_sections(b)
    for pe_off, pe32 in pe32s:
        machine, subsystem, tds_off = pe_fields(pe32)
        struct.pack_into("<I", b, pe_off + tds_off, 0)


def check_ffs(path, expect_guid, expect_subsys):
    d = open(path, "rb").read()
    guid = str(uuid.UUID(bytes_le=d[:16])).upper()
    assert guid == expect_guid, f"{path}: guid {guid}"
    assert d[0x12] == FFS_TYPE_DRIVER, f"{path}: type {d[0x12]:#04x}"
    hdr = bytearray(d[:24])
    hdr[17] = hdr[23] = 0
    s = 0
    for x in hdr:
        s = (s + x) & 0xFF
    assert s == 0, f"{path}: header checksum"
    assert u24(d, 0x14) == len(d), f"{path}: size24 {u24(d, 0x14)} != {len(d)}"
    pe32s, ui = walk_sections(d)
    assert pe32s, f"{path}: no PE32"
    machine, subsystem, _ = pe_fields(pe32s[0][1])
    assert machine == PE_MACHINE_AMD64, f"{path}: machine {machine:#06x}"
    assert subsystem == expect_subsys, f"{path}: subsystem {subsystem}"
    assert ui == "BifEpaProbe", f"{path}: ui {ui!r}"
    assert b"BIF-EPA:" in pe32s[0][1], f"{path}: probe markers missing"
    return d


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    out1 = sys.argv[1]
    blobs = {name: check_ffs(f"{out1}/{name}", guid, subsys) for name, (guid, subsys) in EXPECTED.items()}
    if len(sys.argv) == 3:
        out2 = sys.argv[2]
        for name, (guid, subsys) in EXPECTED.items():
            b2 = bytearray(check_ffs(f"{out2}/{name}", guid, subsys))
            b1 = bytearray(blobs[name])
            zero_coff_tds(b1)
            zero_coff_tds(b2)
            assert bytes(b1) == bytes(b2), f"{name}: builds differ beyond COFF TimeDateStamp"
    print(f"ok: {len(EXPECTED)} artifact(s)" + (", reproducible" if len(sys.argv) == 3 else ""))


if __name__ == "__main__":
    main()
