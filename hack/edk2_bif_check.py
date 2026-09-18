#!/usr/bin/env python3
"""edk2_bif_check.py — инварианты артефакта BifEpaProbe (цикл A).

Спека docs/superpowers/specs/2026-09-14-bif-driver-recon-design.md 5/9;
паттерн hack/edk2_build_check.py (S1). Раскладка — нативная
(вердикты EPA-1/1b/1c: сырая PE32-секция вешает стык PEI→DXE):
depex-секция + GUIDED EE4E5898 (LZMA_CUSTOM_DECOMPRESS) с вложенными
PE32+UI. Со вторым аргументом-каталогом сравнивает две сборки:
байт-в-байт, при расхождении — нормализованно (декомпресс, обнуление
COFF TimeDateStamp, детерминированный рекомпресс).
Не часть движка; запуск вручную:

    python3 hack/edk2_bif_check.py <out-dir> [<out-dir2>]
"""
import lzma
import struct
import sys
import uuid

EXPECTED = {
    "BifEpaProbe.ffs": ("B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84", 11),
}
FFS_TYPE_DRIVER = 0x07
SECTION_GUIDED = 0x02
SECTION_PE32 = 0x10
SECTION_UI = 0x15
LZMA_GUID = uuid.UUID("EE4E5898-3914-4259-9D6E-DC7BD79403CF").bytes_le
PE_MACHINE_AMD64 = 0x8664
LZMA_FILTERS = [{"id": lzma.FILTER_LZMA1, "preset": 9, "dict_size": 1 << 24}]


def u24(b, o):
    return b[o] | (b[o + 1] << 8) | (b[o + 2] << 16)


def walk_sections(d, start=24):
    off, end, pe32s, ui = start, len(d), [], None
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


def unwrap_guided(d):
    off, end, inner = 24, len(d), None
    while off + 4 <= end:
        size, stype = u24(d, off), d[off + 3]
        if size < 4:
            break
        if stype == SECTION_GUIDED:
            assert bytes(d[off + 4:off + 20]) == LZMA_GUID, "guided guid"
            doff = struct.unpack_from("<H", d, off + 20)[0]
            assert struct.unpack_from("<H", d, off + 22)[0] == 0x0001, "guided attrs"
            assert inner is None, "multiple guided"
            inner = lzma.decompress(bytes(d[off + doff:off + size]), format=lzma.FORMAT_ALONE)
        off += (size + 3) & ~3
    assert inner is not None, "no guided section"
    return inner


def pe_fields(pe):
    assert pe[:2] == b"MZ", "no MZ"
    lfa = struct.unpack_from("<I", pe, 0x3C)[0]
    assert pe[lfa:lfa + 4] == b"PE\x00\x00", "no PE signature"
    machine = struct.unpack_from("<H", pe, lfa + 4)[0]
    subsystem = struct.unpack_from("<H", pe, lfa + 92)[0]
    return machine, subsystem, lfa + 8


def normalized(d):
    inner = bytearray(unwrap_guided(d))
    pe32s, _ = walk_sections(bytes(inner), start=0)
    for pe_off, pe32 in pe32s:
        _, _, tds_off = pe_fields(pe32)
        struct.pack_into("<I", inner, pe_off + tds_off, 0)
    return lzma.compress(bytes(inner), format=lzma.FORMAT_ALONE, filters=LZMA_FILTERS)


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
    pe32s, ui = walk_sections(unwrap_guided(d), start=0)
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
    note = ""
    if len(sys.argv) == 3:
        out2 = sys.argv[2]
        for name, (guid, subsys) in EXPECTED.items():
            b2 = check_ffs(f"{out2}/{name}", guid, subsys)
            if blobs[name] != b2:
                assert normalized(blobs[name]) == normalized(b2), f"{name}: builds differ beyond COFF TimeDateStamp"
                note = " (modulo COFF TimeDateStamp)"
    print(f"ok: {len(EXPECTED)} artifact(s)" + (f", reproducible{note}" if len(sys.argv) == 3 else ""))


if __name__ == "__main__":
    main()
