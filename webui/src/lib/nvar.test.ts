import { describe, expect, it } from 'vitest';
import { decodeBase64, hexDump, hexN, parseNum } from './nvar';

describe('nvar helpers', () => {
    it('decodeBase64 decodes standard base64', () => {
        expect(Array.from(decodeBase64('AQID'))).toEqual([1, 2, 3]);
        expect(Array.from(decodeBase64(''))).toEqual([]);
    });

    it('hexDump renders offset + hex + ascii rows of 16', () => {
        const bytes = new Uint8Array(18);
        for (let i = 0; i < 18; i++) bytes[i] = i + 0x41; // 'A'..
        const rows = hexDump(bytes);
        expect(rows).toHaveLength(2);
        expect(rows[0].startsWith('00000000')).toBe(true);
        expect(rows[0]).toContain('41 42 43 44 45 46 47 48  49 4A 4B 4C 4D 4E 4F 50');
        expect(rows[0]).toContain('|ABCDEFGHIJKLMNOP|');
        expect(rows[1].startsWith('00000010')).toBe(true);
        expect(rows[1]).toContain('|QR|');
    });

    it('parseNum accepts decimal and 0x-hex, normalizes to decimal string', () => {
        expect(parseNum('58')).toBe('58');
        expect(parseNum('0x3A')).toBe('58');
        expect(parseNum('0x3a')).toBe('58');
        expect(parseNum('0')).toBe('0');
        expect(parseNum('0x0')).toBe('0');
    });

    it('parseNum rejects garbage', () => {
        for (const bad of ['', 'x', '0x', '0xG', '1 2', '-1', '1.5']) {
            expect(parseNum(bad)).toBeNull();
        }
    });

    it('parseNum keeps uint64 precision via BigInt', () => {
        expect(parseNum('0xFFFFFFFFFFFFFFFF')).toBe('18446744073709551615');
        expect(parseNum('18446744073709551615')).toBe('18446744073709551615');
    });

    it('hexN formats numbers and decimal strings as 0x-HEX', () => {
        expect(hexN(0x3a)).toBe('0x3A');
        expect(hexN(58)).toBe('0x3A');
        expect(hexN('58')).toBe('0x3A');
        expect(hexN('0')).toBe('0x0');
    });
});
