#!/usr/bin/env python3
"""edk2 build artifacts check: FFS-инварианты + воспроизводимость сборки.

Стенд S1 (план docs/superpowers/plans/2026-09-06-serial-s1-edk2-build.md):
валидирует тройку SerialDxe/TerminalDxe/SerialConsoleGlue и серийный FV;
со вторым аргументом-каталогом сравнивает две сборки побайтово
(допустимое расхождение — только COFF TimeDateStamp). Не часть движка;
запуск вручную:

    python3 hack/edk2_build_check.py <out-dir> [<out-dir2>]
"""
import struct, sys, uuid

EXPECTED = {
    "SerialDxe.ffs": ("9A5163E7-5C29-453F-825C-837A46A81E15", 11),
    "TerminalDxe.ffs": ("9E863906-A40F-4875-977F-5B93FF237FC6", 11),
    "SerialConsoleGlue.ffs": ("1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43", 11),
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
    assert sum(hdr) & 0xFF == 0, f"{path}: header checksum"
    if not (d[0x13] & 0x40):
        assert d[17] == 0xAA, f"{path}: file checksum fixed value"
    size = u24(d, 0x14)
    assert size == len(d), f"{path}: size24 {size} != {len(d)}"
    pe32s, ui = walk_sections(d)
    assert pe32s, f"{path}: no PE32 section"
    machine, subsystem, tds_off = pe_fields(pe32s[-1][1])
    assert machine == PE_MACHINE_AMD64, f"{path}: machine {machine:#06x}"
    assert subsystem == expect_subsys, f"{path}: subsystem {subsystem}"
    assert len(d) > 1024, f"{path}: implausibly small"
    return len(d), ui, pe32s, tds_off

def main():
    if len(sys.argv) not in (2, 3):
        sys.exit("usage: edk2_build_check.py <out-dir> [<out-dir2>]")
    dir1 = sys.argv[1].rstrip("/")
    results = {}
    for name, (guid, subsys) in EXPECTED.items():
        size, ui, pe32s, tds_off = check_ffs(f"{dir1}/{name}", guid, subsys)
        results[name] = (pe32s, tds_off)
        print(f"{name}: {size} B  ui={ui!r}  machine=AMD64 subsystem={subsys}")
    fv = open(f"{dir1}/SERIAL_CONSOLE_FV.Fv", "rb").read(0x30)
    assert fv[0x28:0x2C] == b"_FVH", "FV signature"
    print(f"SERIAL_CONSOLE_FV.Fv: _FVH ok")
    if len(sys.argv) == 3:
        dir2 = sys.argv[2].rstrip("/")
        for name, (guid, subsys) in EXPECTED.items():
            b1 = bytearray(open(f"{dir1}/{name}", "rb").read())
            b2 = bytearray(open(f"{dir2}/{name}", "rb").read())
            if b1 == b2:
                print(f"{name}: reproducible (identical)")
                continue
            zero_coff_tds(b1)
            zero_coff_tds(b2)
            if b1 == b2:
                print(f"{name}: reproducible (COFF TimeDateStamp only)")
            else:
                sys.exit(f"{name}: NOT reproducible beyond TimeDateStamp")
        print("REPRODUCIBLE")
    return 0

if __name__ == "__main__":
    sys.exit(main())
