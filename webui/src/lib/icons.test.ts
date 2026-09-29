import { describe, expect, it } from 'vitest';
import { ICON_CODEPOINTS, questionIcon, storeIcon, typeIcon } from './icons';

describe('icons — зеркало uefi-tui/theme.rs', () => {
    it('codepoint set matches theme.rs (смена theme.rs = сознательная правка)', () => {
        expect([...ICON_CODEPOINTS].sort((a, b) => a - b)).toEqual([
            0xeae8, 0xeb7d, 0xf016, 0xf023, 0xf031, 0xf0c7, 0xf0ca, 0xf10c,
            0xf14a, 0xf15b, 0xf1c0, 0xf1c6, 0xf1c9, 0xf1ec, 0xf2db, 0xf492,
        ]);
    });

    it('no duplicate codepoints', () => {
        expect(new Set(ICON_CODEPOINTS).size).toBe(ICON_CODEPOINTS.length);
    });

    it('typeIcon mirrors theme.rs mapping', () => {
        expect(typeIcon(62, 0)).toBe('\u{F2DB}');
        expect(typeIcon(63, 0)).toBe('\u{F492}');
        expect(typeIcon(64, 0)).toBe('\u{EB7D}');
        expect(typeIcon(65, 0)).toBe('\u{F1C0}');
        expect(typeIcon(66, 0)).toBe('\u{F15B}');
        expect(typeIcon(68, 0)).toBe('\u{F10C}');
        expect(typeIcon(67, 0x10)).toBe('\u{F1C9}');
        expect(typeIcon(67, 0x02)).toBe('\u{F023}');
        expect(typeIcon(67, 0x01)).toBe('\u{F1C6}');
        expect(typeIcon(67, 0x15)).toBe('\u{F031}');
        expect(typeIcon(67, 0x19)).toBe('\u{EAE8}');
        expect(typeIcon(67, 0xee)).toBe('\u{F016}');
        expect(typeIcon(99, 0)).toBe('?');
    });

    it('storeIcon: nvar → floppy, иначе typeIcon', () => {
        expect(storeIcon(true, 66, 0)).toBe('\u{F0C7}');
        expect(storeIcon(false, 65, 0)).toBe('\u{F1C0}');
    });

    it('questionIcon mirrors theme.rs', () => {
        expect(questionIcon('one_of')).toBe('\u{F0CA}');
        expect(questionIcon('checkbox')).toBe('\u{F14A}');
        expect(questionIcon('numeric')).toBe('\u{F1EC}');
        expect(questionIcon('other')).toBe('\u{F016}');
    });
});
