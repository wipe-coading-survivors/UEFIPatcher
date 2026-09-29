import { readFileSync, writeFileSync } from 'node:fs';
import subsetFont from 'subset-font';
import { unzipSync } from 'fflate';

const PINNED = 'https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/NerdFontsSymbolsOnly.zip';
// синхронизируется с src/lib/icons.ts ICON_CODEPOINTS
const CODEPOINTS = [
    0xeae8, 0xeb7d, 0xf016, 0xf023, 0xf031, 0xf0c7, 0xf0ca, 0xf10c,
    0xf14a, 0xf15b, 0xf1c0, 0xf1c6, 0xf1c9, 0xf1ec, 0xf2db, 0xf492,
];

async function loadTtf() {
    const arg = process.argv[2];
    if (arg) return readFileSync(arg);
    const res = await fetch(PINNED);
    if (!res.ok) throw new Error(`download ${PINNED}: HTTP ${res.status}`);
    const files = unzipSync(Buffer.from(await res.arrayBuffer()));
    const name = Object.keys(files).find((f) => f.endsWith('.ttf'));
    if (!name) throw new Error('no .ttf inside NerdFontsSymbolsOnly.zip');
    return Buffer.from(files[name]);
}

const ttf = await loadTtf();
const text = CODEPOINTS.map((c) => String.fromCodePoint(c)).join('');
const woff2 = await subsetFont(ttf, text, { targetFormat: 'woff2' });
writeFileSync(new URL('../static/icons.woff2', import.meta.url), woff2);
console.log(`icons.woff2: ${woff2.length} bytes, ${CODEPOINTS.length} glyphs`);
