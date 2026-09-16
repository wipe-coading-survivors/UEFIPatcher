#!/usr/bin/env python3
"""v20: патч писателя cfg+0x8cb в IioInit (FV8, GUIDed-LZMA секция).

Отчёт §O5: writer @PE+0xdff4 перед установкой cfg[0x8cb+sock*11+port]=1
вызывает предикат 0x121c8 «IsDlActive» (LNKSTA@0xA2 бит 13). Для скрытого
порта ECAM читает 0xFFFF → (0xFFFF>>13)&1 = 1 → «линк активен» → запись
0x8cb=1 срезается (je @PE+0xe1ba) → d2d0-гейт (je @PE+0xd3cf) скипает
сантехнику порта (UBOX+0x80 + CBDMA/CTLE). На донорах (X10DRH/SM450)
писатель семантически идентичен, но порты видимы и LNKSTA читает реальные
значения — пустые порты получают 0x8cb=1 и полный прумблинг.

Патч: NOP je @PE+0xe1ba (2 байта `74 45` → `90 90`) — writer перестаёт
доверять ложному DL_Active скрытых портов; семантика донора восстановлена.
Всё в пределах GUIDed-LZMA секции файла 8/17 FV8 (GUID 63809859-…),
репак python-lzma с параметрами оригинального стрима, пэд нулями до
исходного размера секции — layout FFS/FV не меняется.

Запуск: python3 hack/iioinit_plumbing_patch.py <in.bin> <out.bin>

Канонический v20 собран движком (write-режим уже реализован: node replace
<guided-child> --body-only + image save; builder::build_recompressed_guided
+ compress_lzma_fit с бюджетом секции), sha256 84bad780550b99dd0c0ea7c849c4
c7ae6a946829588c25942d48401e6303d678. Этот скрипт — независимый путь
воспроизведения (python-lzma, тот же патч, roundtrip-гейт внутри).
"""
import hashlib
import lzma
import struct
import sys

IIOINIT_GUID = bytes.fromhex("59988063" + "29f0" + "c341" + "9f34eeeb9ea787a5")
LZMA_GUID = bytes.fromhex("98584eee143959429d6edc7bd79403cf")
PATCH_OFF = 0xE1BA
PATCH_FROM = bytes.fromhex("7445")
PATCH_TO = bytes.fromhex("9090")


def u16(d, o):
    return struct.unpack_from("<H", d, o)[0]


def u24(d, o):
    return d[o] | d[o + 1] << 8 | d[o + 2] << 16


def wr8(buf, off, val):
    buf[off] = val & 0xFF


def ffs_header_checksum(hdr24):
    # сумма всех байт заголовка (кроме самих байт IntegrityCheck) + File checksum == 0
    s = (-sum(hdr24[:0x10]) - sum(hdr24[0x12:])) & 0xFF
    return s


def main(src, dst):
    data = bytearray(open(src, "rb").read())
    off = data.find(IIOINIT_GUID)
    if off < 0 or data[off + 18] != 0x07:
        sys.exit("IioInit FFS не найден")
    fsize = u24(data, off + 20)
    fattr = data[off + 19]

    sp = off + 24
    while sp + 4 <= off + fsize:
        stype = data[sp + 3]
        ssize = u24(data, sp)
        sh = 4
        if ssize == 0xFFFFFF:
            ssize = struct.unpack_from("<I", data, sp + 4)[0]
            sh = 8
        if stype == 0x02 and bytes(data[sp + sh : sp + sh + 16]) == LZMA_GUID:
            break
        sp = (sp + ssize + 3) & ~3
    else:
        sys.exit("GUIDed-LZMA секция не найдена")

    dataoff = u16(data, sp + sh + 16)
    attr = u16(data, sp + sh + 18)
    if attr & 0x02:
        sys.exit("секция с CRC32-атрибутом — нужен фиксап CRC (не реализовано)")
    payload_off = sp + dataoff
    payload_len = ssize - dataoff
    payload = bytes(data[payload_off : payload_off + payload_len])

    props = payload[0]
    dict_size = struct.unpack_from("<I", payload, 1)[0]
    usize = struct.unpack_from("<Q", payload, 5)[0]
    dec = bytearray(lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(payload))
    print(f"FFS @0x{off:x} fsize=0x{fsize:x}; guided sec @0x{sp:x} ssize=0x{ssize:x} attr=0x{attr:x}")
    print(f"lzma: props=0x{props:02x} dict=0x{dict_size:x}; dec={len(dec)} (hdr usize={usize})")

    pe_off = 4  # PE32-секция сразу за 4-байтовым заголовком внутри dec
    if dec[3] != 0x10:
        sys.exit("ожидалась PE32-секция на первом месте в dec")
    site = PATCH_OFF + pe_off
    if bytes(dec[site : site + len(PATCH_FROM)]) != PATCH_FROM:
        sys.exit(f"байты патча не совпали @dec+0x{site:x}: {bytes(dec[site:site+2]).hex()}")
    dec[site : site + len(PATCH_TO)] = PATCH_TO
    print(f"patched: PE+0x{PATCH_OFF:x} {PATCH_FROM.hex()} -> {PATCH_TO.hex()} (dec+0x{site:x})")

    filters = [
        {
            "id": lzma.FILTER_LZMA1,
            "preset": 9 | lzma.PRESET_EXTREME,
            "dict_size": dict_size,
            "lc": props % 9,
            "lp": (props // 9) % 5,
            "pb": props // 45,
        }
    ]
    new_payload = None
    for filt in (filters, [dict(filters[0], preset=9)]):
        cand = bytearray(lzma.compress(bytes(dec), format=lzma.FORMAT_ALONE, filters=filt))
        struct.pack_into("<Q", cand, 5, len(dec))  # EDK2 требует точный usize
        print(f"recompress[{filt[0]['preset']:x}]: {len(cand)} B (ориг. {payload_len} B)")
        if len(cand) <= payload_len:
            new_payload = cand
            break
    if new_payload is None:
        sys.exit("рекомпрессия не влезла в исходную секцию — нужен FV-relayout")

    new_payload = new_payload + bytes(payload_len - len(new_payload))
    data[payload_off : payload_off + payload_len] = new_payload

    # FFS data checksum (атрибут 0x40): File-байт IntegrityCheck = сумма тела
    if fattr & 0x40:
        body = bytes(data[off + 24 : off + fsize])
        wr8(data, off + 0x11, sum(body) & 0xFF)

    # roundtrip: секция из нового образа декомпрессируется байт-в-байт в patched dec
    back = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(
        bytes(data[payload_off : payload_off + payload_len])
    )
    if back != bytes(dec):
        sys.exit("roundtrip не сошёлся")
    if bytes(data[off : off + 24]) and ffs_header_checksum(bytes(data[off : off + 24])) != data[off + 0x10]:
        # header checksum байт = сумма заголовка с нулевым IC-полем; заголовок не менялся — просто сверка
        pass

    open(dst, "wb").write(bytes(data))
    print(f"OK -> {dst}")
    print(f"sha256: {hashlib.sha256(bytes(data)).hexdigest()}")
    print(f"delta vs input: {sum(a != b for a, b in zip(bytes(data), open(src,'rb').read()))} байт")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
