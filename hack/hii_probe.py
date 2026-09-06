#!/usr/bin/env python3
"""HII probe: где в firmware-образе лежат HII package lists.

Разведочный скрипт фазы 6 — спека
docs/superpowers/specs/2026-08-14-hii-pe-resource-extraction-design.md (§2),
факты разведки — TODO.md, раздел «Находки фазы 5 real-image validation»
(коммит ffa1a46). Не часть движка; запуск вручную:

    python3 hack/hii_probe.py <image.bin> [<image2.bin> ...]

Что делает: raw-MZ-скан образа; поиск GUIDed-LZMA-секций (все 4 GUID из
crates/uefi-engine/src/ffs.rs), распаковка (LZMA alone); рекурсивный
MZ-скан внутри распакованного (PE на любой глубине вложенности FV);
для каждого PE — обход resource directory (именованный тип L"HII"),
разбор package-list (16B list GUID + u32 total + пакеты u24 len + u8 type,
стоп PACKAGE_END=0xDF) и bare-скан FORM-пакетов ([u24 len][0x02][0x0E])
внутри тела PE (с ограничением по таблице секций).

Известные ограничения: bare-паттерн даёт ложные срабатывания на
 данных PE — глубокая валидация (parse_form_package) остается за
движком/тестами; TE-образцы ('VZ') не сканируются.
"""
import lzma, struct, sys

LZMA_GUIDS = [
    bytes.fromhex('98584eee143959429d6edc7bd79403cf'),
    bytes.fromhex('235ed80e53f23f41a03c901987b04397'),
    bytes.fromhex('ea2199bd91ed4a408b2fb4d724747c8c'),
    bytes.fromhex('bde62ad45213fb4b909aca72a6eae889'),
]

def u16(d, o): return struct.unpack_from('<H', d, o)[0]
def u32(d, o): return struct.unpack_from('<I', d, o)[0]

def guid_str(b):
    d1, d2, d3 = struct.unpack_from('<IHH', b, 0)
    return f'{d1:08X}-{d2:04X}-{d3:04X}-{b[8:10].hex().upper()}-{b[10:16].hex().upper()}'

def find_pes(data):
    out, pos = [], 0
    while True:
        i = data.find(b'MZ', pos)
        if i < 0: break
        pos = i + 1
        if i + 0x40 > len(data): continue
        e = u32(data, i + 0x3c)
        pe = i + e
        if 0 < e < 0x10000 and pe + 4 <= len(data) and data[pe:pe+4] == b'PE\x00\x00':
            out.append(i)
    return out

class PE:
    def __init__(self, data, base):
        self.data, self.base, self.ok, self.extent = data, base, False, 0
        body = data
        if body[base:base+2] != b'MZ': return
        e = u32(body, base + 0x3c)
        if body[base+e:base+e+4] != b'PE\x00\x00': return
        coff = base + e + 4
        nsec, optsz = u16(body, coff + 2), u16(body, coff + 16)
        opt = coff + 20
        if opt + 2 > len(body): return
        magic = u16(body, opt)
        if magic not in (0x10b, 0x20b): return
        ddoff = opt + (96 if magic == 0x10b else 112)
        if ddoff + 24 > len(body): return
        self.rsrc_rva, self.rsrc_size = u32(body, ddoff + 16), u32(body, ddoff + 20)
        self.sectbl, self.nsec = opt + optsz, nsec
        self.extent = opt + optsz + nsec * 40
        for s in range(nsec):
            so = self.sectbl + s * 40
            self.extent = max(self.extent, base + u32(body, so + 20) + u32(body, so + 16))
        self.ok = True

    def rva2off(self, rva):
        b = self.data
        for s in range(self.nsec):
            so = self.sectbl + s * 40
            vsize, vaddr, rawsize, rawptr = u32(b, so+8), u32(b, so+12), u32(b, so+16), u32(b, so+20)
            if vaddr <= rva < vaddr + max(vsize, rawsize):
                d = rva - vaddr
                return self.base + rawptr + d if d < rawsize else None
        return self.base + rva if rva < 0x1000 else None

    def h_resources(self):
        if not self.rsrc_rva: return []
        root = self.rva2off(self.rsrc_rva)
        if root is None: return []
        b, out = self.data, []

        def name_at(off):
            p = root + off
            if p + 2 > len(b): return None
            ln = u16(b, p)
            if p + 2 + ln * 2 > len(b): return None
            return b[p+2:p+2+ln*2].decode('utf-16-le', 'replace')

        def walk(dir_off, level, path):
            if dir_off + 16 > len(b): return
            total = u16(b, dir_off + 12) + u16(b, dir_off + 14)
            for k in range(min(total, 64)):
                eo = dir_off + 16 + k * 8
                if eo + 8 > len(b): return
                nraw, oraw = u32(b, eo), u32(b, eo + 4)
                is_dir, off = bool(oraw & 0x80000000), oraw & 0x7fffffff
                if level == 0:
                    if nraw & 0x80000000:
                        nm = name_at(nraw & 0x7fffffff)
                        if nm in ('H', 'HII') and is_dir:
                            walk(root + off, 1, ['H'])
                elif is_dir:
                    walk(root + off, level + 1, path + [str(nraw & 0x7fffffff)])
                else:
                    de = root + off
                    if de + 16 > len(b): continue
                    rva, size = u32(b, de), u32(b, de + 4)
                    fo = self.rva2off(rva)
                    if fo is not None and fo + size <= len(b):
                        out.append((fo, b[fo:fo+size]))
        walk(root, 0, [])
        return out

def parse_pkglist(blob):
    if len(blob) < 20: return None
    g, total, pkgs, pos = blob[:16], u32(blob, 16), [], 20
    while pos + 4 <= len(blob):
        plen = blob[pos] | blob[pos+1] << 8 | blob[pos+2] << 16
        ptype = blob[pos+3]
        if ptype == 0xDF:
            pkgs.append(('END', plen)); break
        if plen < 4 or pos + plen > len(blob):
            pkgs.append(('MALFORMED', ptype)); break
        info = ''
        if ptype == 0x02 and pos + 22 <= len(blob) and blob[pos+4] == 0x0E:
            info = ' formset=' + guid_str(blob[pos+6:pos+22])
        elif ptype == 0x04 and pos + 22 <= len(blob):
            hdr = u32(blob, pos + 4)
            info = f' hdr={hdr} lang@14={blob[pos+14:pos+22]!r}'
        pkgs.append((f'type=0x{ptype:02x} len={plen}{info}', plen))
        pos += plen
    return guid_str(g), total, pkgs

def scan_buffer(data, label, report, depth=0):
    n = 0
    for i in find_pes(data):
        pe = PE(data, i)
        if not pe.ok: continue
        n += 1
        hres = pe.h_resources()
        for fo, blob in hres:
            pl = parse_pkglist(blob)
            report.append(f'    {label} PE@{i:#x} .rsrc H @{fo:#x} size={len(blob)}')
            if pl:
                g, total, pkgs = pl
                report.append(f'      list_guid={g} declared_total={total} blob={len(blob)}')
                for p, _ in pkgs[:8]:
                    report.append(f'        pkg {p}')
        end = min(pe.extent, len(data))
        for o in range(i, max(end - 24, i)):
            plen = data[o] | data[o+1] << 8 | data[o+2] << 16
            if data[o+3] == 0x02 and data[o+4] == 0x0E and 24 <= plen <= end - o:
                if any(fo <= o < fo + len(b) for fo, b in hres): continue
                report.append(f'    {label} PE@{i:#x} BARE form pkg @+{o-i} len={plen} formset={guid_str(data[o+6:o+22])}')
    return n

def decompress_lzma_blobs(data, label, report, depth=1):
    if depth > 3: return 0
    n = 0
    for g in LZMA_GUIDS:
        pos = 0
        while True:
            i = data.find(g, pos)
            if i < 0: break
            pos = i + 1
            if i + 20 > len(data): continue
            doff = u16(data, i + 16)
            start = i - 4 + doff
            if start + 13 >= len(data): continue
            try:
                out = lzma.LZMADecompressor(format=lzma.FORMAT_ALONE).decompress(data[start:])
            except Exception:
                continue
            if len(out) > 0x1000:
                n += 1
                sub = f'{label}/lzma@{start:#x}'
                scan_buffer(out, sub, report)
                n += decompress_lzma_blobs(out, sub, report, depth + 1)
    return n

def probe(path):
    data = open(path, 'rb').read()
    name = path.split('/')[-1][:12]
    report = [f'== {path.split("/")[-1]} ({len(data)} bytes)']
    raw = scan_buffer(data, f'{name}:raw', report)
    lz = decompress_lzma_blobs(data, name, report)
    report.insert(1, f'   PEs scanned: raw+decompressed, lzma blobs={lz}')
    return report, raw, lz

for p in sys.argv[1:]:
    rep, raw, lz = probe(p)
    print(rep[0]); print(rep[1])
    for line in rep[2:]: print(line)
