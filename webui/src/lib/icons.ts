// зеркало crates/uefi-tui/src/theme.rs (источник истины)
export const TYPE_IMAGE = 62;
export const TYPE_REGION = 63;
export const TYPE_PADDING = 64;
export const TYPE_VOLUME = 65;
export const TYPE_FILE = 66;
export const TYPE_SECTION = 67;
export const TYPE_FREESPACE = 68;
export const SECTION_COMPRESSION = 0x01;
export const SECTION_GUID_DEFINED = 0x02;
export const SECTION_PE32 = 0x10;
export const SECTION_UI = 0x15;
export const SECTION_RAW = 0x19;

export const ICON_CODEPOINTS = [
    0xeae8, 0xeb7d, 0xf016, 0xf023, 0xf031, 0xf0c7, 0xf0ca, 0xf10c,
    0xf14a, 0xf15b, 0xf1c0, 0xf1c6, 0xf1c9, 0xf1ec, 0xf2db, 0xf492,
];

export function typeIcon(type: number, subtype: number): string {
    switch (type) {
        case TYPE_IMAGE: return '\u{F2DB}';
        case TYPE_VOLUME: return '\u{F1C0}';
        case TYPE_FILE: return '\u{F15B}';
        case TYPE_SECTION:
            switch (subtype) {
                case SECTION_PE32: return '\u{F1C9}';
                case SECTION_GUID_DEFINED: return '\u{F023}';
                case SECTION_COMPRESSION: return '\u{F1C6}';
                case SECTION_UI: return '\u{F031}';
                case SECTION_RAW: return '\u{EAE8}';
                default: return '\u{F016}';
            }
        case TYPE_PADDING: return '\u{EB7D}';
        case TYPE_FREESPACE: return '\u{F10C}';
        case TYPE_REGION: return '\u{F492}';
        default: return '?';
    }
}

export function storeIcon(isNvar: boolean, type: number, subtype: number): string {
    return isNvar ? '\u{F0C7}' : typeIcon(type, subtype);
}

export function questionIcon(kind: string): string {
    switch (kind) {
        case 'one_of': return '\u{F0CA}';
        case 'checkbox': return '\u{F14A}';
        case 'numeric': return '\u{F1EC}';
        default: return '\u{F016}';
    }
}
