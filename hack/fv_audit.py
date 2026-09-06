#!/usr/bin/env python3
"""FV audit: карта firmware volumes образа + свободное место.

Разведка S0.1b (спека 2026-09-05-serial-console-ladder-design.md §3):
FV-карта живого образа, свободный хвост каждого FV, носители вставки
прошлых попыток. Не часть движка; запуск вручную:

    python3 hack/fv_audit.py <image.bin> [--json]

Ограничения: raw-скан без LZMA-распаковки (вложенные в LZMA-секции
FV не видны); FV внутри спанов уже найденных FV пропускаются
(claimed-spans); FFS-цепочка читается по заголовкам без валидации
типов секций.
"""
import json, struct, sys, uuid

def u16(d, o): return struct.unpack_from('<H', d, o)[0]
def u32(d, o): return struct.unpack_from('<I', d, o)[0]
def u64(d, o): return struct.unpack_from('<Q', d, o)[0]
def g(b): return str(uuid.UUID(bytes_le=b)).upper()

def walk_ffs(data, fv_off, hdr_len, fv_len):
    files, pad, p, end = 0, 0, fv_off + hdr_len, fv_off + fv_len
    last = p
    while p + 24 <= end:
        if data[p + 0x12] == 0xFF and data[p + 0x14] == 0xFF:
            break
        size = data[p + 0x14] | (data[p + 0x15] << 8) | (data[p + 0x16] << 16)
        if size == 0xFFFFFF or size < 24:
            break
        files += 1
        if data[p + 0x12] == 0xF0:
            pad += 1
        last = p + size
        p = (last + 7) & ~7
    return files, pad, last, end - max(last, fv_off + hdr_len)

def walk_fvs(data):
    out, claimed, pos = [], [], 0
    while True:
        i = data.find(b'_FVH', pos)
        if i < 0:
            break
        pos = i + 4
        fv = i - 0x28
        if fv < 0 or any(a <= fv < b for a, b in claimed):
            continue
        length, hlen = u64(data, fv + 0x20), u16(data, fv + 0x30)
        if length == 0 or fv + length > len(data) or hlen < 0x38:
            continue
        files, pad, last, free_tail = walk_ffs(data, fv, hlen, length)
        claimed.append((fv, fv + length))
        out.append({'offset': fv, 'guid': g(data[fv + 0x10:fv + 0x20]),
                    'length': length, 'header_len': hlen, 'files': files,
                    'pad_files': pad, 'used': last - fv, 'free_tail': free_tail})
    return out

if __name__ == '__main__':
    img = open(sys.argv[1], 'rb').read()
    fvs = walk_fvs(img)
    if '--json' in sys.argv:
        print(json.dumps(fvs, indent=1))
    else:
        print('offset\tguid\tlength\thdr\tfiles\tpads\tused\tfree_tail')
        for f in sorted(fvs, key=lambda x: x['offset']):
            print(f"{f['offset']:#x}\t{f['guid']}\t{f['length']:#x}\t{f['header_len']:#x}\t{f['files']}\t{f['pad_files']}\t{f['used']:#x}\t{f['free_tail']:#x}")
