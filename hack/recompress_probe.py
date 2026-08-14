#!/usr/bin/env python3
"""Recompression probe: свободное место в FV и размеры LZMA-секций.

Разведочный скрипт фазы рекомпрессии (issue IV, вариант «Полный»). Спека
планируется по findings docs/plans/2026-08-14-предпосылки-для-рекомпрессии.md.
Не часть движка; запуск вручную:

    python3 hack/recompress_probe.py <image.bin>

Что делает:
- находит все FV (_FVH), считает used/free (walk FFS-файлов до erase-байтов);
- сканирует образ на guided-LZMA GUID (все 4 из crates/uefi-engine/src/ffs.rs
  + Tiano для полноты), для каждого: смещение, FV-принадлежность, payload len,
  decompressed len, dict/props из alone-заголовка;
- для файла <fv>/<idx> (по умолчанию 1/13 — кейс issue IV) ищет его guided
  LZMA-секцию и дампит decompressed-буфер в /tmp/hii_recompress_<fv>_<idx>.bin
  для замера exact-коэффициента lzma-rs literal-энкодером (probe B).

Известные ограничения: сканирует только верхний уровень образа (без
nested-LZMA рекурсии); file index = порядок следования в FV (совпадает с
дочерним индексом движка при отсутствии gap-артефактов).
"""
import lzma, struct, sys

GUIDS = {
    'LZMA': bytes.fromhex('98584eee143959429d6edc7bd79403cf'),
    'LZMA_HP': bytes.fromhex('235ed80e53f23f41a03c901987b04397'),
    'LZMA_MS': bytes.fromhex('ea2199bd91ed4a408b2fb4d724747c8c'),
    'LZMAF86': bytes.fromhex('bde62ad45213fb4b909aca72a6eae889'),
    'TIANO': bytes.fromhex('ad8012a31e48b64195e8127f4c984779'),
}

def u16(d, o): return struct.unpack_from('<H', d, o)[0]
def u24(d, o): return d[o] | d[o+1] << 8 | d[o+2] << 16
def u32(d, o): return struct.unpack_from('<I', d, o)[0]
def u64(d, o): return struct.unpack_from('<Q', d, o)[0]

def find_fvs(data):
    out, pos = [], 0
    while True:
        i = data.find(b'_FVH', pos)
        if i < 0: break
        pos = i + 1
        fv = i - 0x28
        if fv < 0: continue
        total = u64(data, fv + 0x20)
        if total == 0 or fv + total > len(data): continue
        out.append((fv, total, u16(data, fv + 0x30)))
    return out

def fv_files(data, fv, hdr_len):
    files, pos = [], fv + ((hdr_len + 7) & ~7)
    end = fv + u64(data, fv + 0x20)
    while pos + 24 <= end:
        if data[pos:pos+24] == b'\xff' * 24: break
        size = u24(data, pos + 20)
        ftype = data[pos + 18]
        if size == 0xFFFFFF or size < 24: break
        files.append((pos, size, ftype))
        pos += (size + 7) & ~7
    return files, pos, end

def scan_guided(data, fvs):
    hits = []
    for name, g in GUIDS.items():
        pos = 0
        while True:
            i = data.find(g, pos)
            if i < 0: break
            pos = i + 1
            if i + 22 > len(data): continue
            doff = u16(data, i + 16)
            p = i + doff - 4 if doff >= 4 else None
            if p is None or p + 13 > len(data): continue
            sec = i - 4
            sec_size = u24(data, sec) if sec >= 0 else 0
            if sec_size < 24 or sec + sec_size > len(data) + 4: continue
            payload_len = sec + sec_size - p
            props, dict_sz = data[p], u32(data, p + 1)
            dec_len, ok = None, False
            try:
                out = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(data[p:])
                dec_len, ok = len(out), True
            except Exception:
                pass
            fv_name = next((f'FV{idx}[{fv:#x}+{total:#x}]' for idx, (fv, total, _)
                            in enumerate(fvs) if fv <= i < fv + total), 'outside-FV')
            hits.append((name, i, fv_name, payload_len, dec_len, props, dict_sz, ok))
    return sorted(hits, key=lambda h: h[1])

def main(path, dump_target=('1', '13')):
    data = open(path, 'rb').read()
    print(f'== {path.split("/")[-1]} ({len(data)} bytes)')
    fvs = find_fvs(data)
    for idx, (fv, total, hdr) in enumerate(fvs):
        files, used_end, end = fv_files(data, fv, hdr)
        free = end - used_end
        pad = sum(1 for _, _, t in files if t == 0xF0)
        print(f'FV{idx} @{fv:#x} total={total:#x} hdr={hdr:#x} files={len(files)} '
              f'(pad-files={pad}) used_end={used_end:#x} free={free} ({free/1024:.1f} KiB)')
    print('-- guided sections (top level):')
    for name, off, fv_name, plen, dec, props, dsz, ok in scan_guided(data, fvs):
        dec_s = f'{dec}' if ok else 'DECODE-FAIL'
        print(f'  {name:8} @{off:#x} in {fv_name}: payload={plen} dec={dec_s} '
              f'props={props:#04x} dict={dsz:#x}')
    if dump_target:
        fv_idx, file_idx = int(dump_target[0]), int(dump_target[1])
        fv, total, hdr = fvs[fv_idx]
        files, _, _ = fv_files(data, fv, hdr)
        fpos, fsize, _ = files[file_idx]
        print(f'-- dump target {fv_idx}/{file_idx}: file @{fpos:#x} size={fsize}')
        for name, off, fv_name, plen, dec, props, dsz, ok in scan_guided(data[fpos:fpos+fsize], []):
            p = fpos + off + u16(data, fpos + off + 16) - 4
            out = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(data[p:])
            dst = f'/tmp/recompress_{fv_idx}_{file_idx}.bin'
            open(dst, 'wb').write(out)
            print(f'   {name} @{off:#x} payload={plen} dec={len(out)} -> {dst}')

if __name__ == '__main__':
    img = sys.argv[1] if len(sys.argv) > 1 else 'refs/fw/HNX99TF_200525_original_E5C88C6F.bin'
    tgt = sys.argv[2].split('/') if len(sys.argv) > 2 else ('1', '13')
    main(img, tgt)
