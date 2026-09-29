export interface VarRow {
    name: string;
    guid: string;
    offset: string;
    size: number;
    attributes: number;
    depth: number;
    data: string;
}

export function decodeBase64(b64: string): Uint8Array {
    if (!b64) return new Uint8Array(0);
    const raw = atob(b64);
    const out = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i);
    return out;
}

export function hexDump(bytes: Uint8Array): string[] {
    const rows: string[] = [];
    for (let off = 0; off < bytes.length; off += 16) {
        const chunk = Array.from(bytes.slice(off, off + 16));
        const hexPart = chunk
            .map((b) => b.toString(16).toUpperCase().padStart(2, '0'))
            .join(' ');
        const padded = chunk.length <= 8
            ? hexPart
            : `${hexPart.slice(0, 23)}  ${hexPart.slice(24)}`;
        const ascii = chunk
            .map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : '.'))
            .join('');
        rows.push(
            `${off.toString(16).toUpperCase().padStart(8, '0')}  ${padded.padEnd(
                16 * 3 + 2,
                ' ',
            )} |${ascii}|`,
        );
    }
    return rows;
}

export function parseNum(s: string): string | null {
    const t = s.trim();
    if (/^\d+$/.test(t)) return t;
    if (/^0x[0-9a-f]+$/i.test(t)) return BigInt(t).toString(10);
    return null;
}

export function hexN(n: string | number): string {
    const v = typeof n === 'number' ? BigInt(Math.trunc(n)) : BigInt(n);
    return `0x${v.toString(16).toUpperCase()}`;
}
