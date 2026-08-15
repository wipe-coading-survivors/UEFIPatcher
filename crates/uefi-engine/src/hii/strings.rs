use std::collections::HashMap;

use r_efi::hii::PACKAGE_STRINGS;
use uefi_proto::StringInfo;

use crate::ffs::EFI_SECTION_PE32;
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::hii_resource_blobs;
use crate::hii::string_pack::is_string_package;
use crate::types::{FfsNode, FfsType, Image};

const SIBT_END: u8 = 0x00;
const SIBT_STRING_SCSU: u8 = 0x10;
const SIBT_STRING_SCSU_FONT: u8 = 0x11;
const SIBT_STRINGS_SCSU: u8 = 0x12;
const SIBT_STRINGS_SCSU_FONT: u8 = 0x13;
const SIBT_STRING_UCS2: u8 = 0x14;
const SIBT_STRING_UCS2_FONT: u8 = 0x15;
const SIBT_STRINGS_UCS2: u8 = 0x16;
const SIBT_STRINGS_UCS2_FONT: u8 = 0x17;
const SIBT_DUPLICATE: u8 = 0x20;
const SIBT_SKIP2: u8 = 0x21;
const SIBT_SKIP1: u8 = 0x22;

const HEADER_LEN: usize = 4;
const STRING_INFO_OFFSET_POS: usize = 8;
const LANGUAGE_OFFSET: usize = 46;

pub struct ParsedStringPackage {
    pub language: String,
    pub strings: Vec<(u16, String)>,
}

pub fn parse_string_package(body: &[u8]) -> Option<ParsedStringPackage> {
    if !is_string_package(body) {
        return None;
    }
    let info_off = read_u32(body, STRING_INFO_OFFSET_POS)
        .filter(|&o| o as usize >= HEADER_LEN && o as usize <= body.len())
        .unwrap_or(HEADER_LEN as u32) as usize;
    let language = read_language(body, LANGUAGE_OFFSET, info_off);
    let mut strings: Vec<(u16, String)> = Vec::new();
    let mut by_id: HashMap<u16, String> = HashMap::new();
    let mut next_id: u16 = 1;
    let mut pos = info_off;
    'outer: while pos < body.len() {
        match body[pos] {
            SIBT_END => break,
            SIBT_STRING_SCSU => {
                let (text, p) = read_scsu(body, pos + 1);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRING_SCSU_FONT => {
                let (text, p) = read_scsu(body, pos + 2);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRINGS_SCSU => {
                let (count, mut p) = read_u16(body, pos + 1);
                for _ in 0..count {
                    if p >= body.len() {
                        tracing::warn!(
                            opcode = body[pos],
                            "truncated SIBT_STRINGS block; stopping string parse"
                        );
                        break 'outer;
                    }
                    let (text, np) = read_scsu(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRINGS_SCSU_FONT => {
                let (count, mut p) = read_u16(body, pos + 2);
                for _ in 0..count {
                    if p >= body.len() {
                        tracing::warn!(
                            opcode = body[pos],
                            "truncated SIBT_STRINGS block; stopping string parse"
                        );
                        break 'outer;
                    }
                    let (text, np) = read_scsu(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRING_UCS2 => {
                let (text, p) = read_ucs2(body, pos + 1);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRING_UCS2_FONT => {
                let (text, p) = read_ucs2(body, pos + 2);
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos = p;
            }
            SIBT_STRINGS_UCS2 => {
                let (count, mut p) = read_u16(body, pos + 1);
                for _ in 0..count {
                    if p >= body.len() {
                        tracing::warn!(
                            opcode = body[pos],
                            "truncated SIBT_STRINGS block; stopping string parse"
                        );
                        break 'outer;
                    }
                    let (text, np) = read_ucs2(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_STRINGS_UCS2_FONT => {
                let (count, mut p) = read_u16(body, pos + 2);
                for _ in 0..count {
                    if p >= body.len() {
                        tracing::warn!(
                            opcode = body[pos],
                            "truncated SIBT_STRINGS block; stopping string parse"
                        );
                        break 'outer;
                    }
                    let (text, np) = read_ucs2(body, p);
                    push(&mut strings, &mut by_id, &mut next_id, text);
                    p = np;
                }
                pos = p;
            }
            SIBT_DUPLICATE => {
                let (ref_id, _) = read_u16(body, pos + 1);
                let text = by_id.get(&ref_id).cloned().unwrap_or_default();
                push(&mut strings, &mut by_id, &mut next_id, text);
                pos += 1 + 2;
            }
            SIBT_SKIP2 => {
                let (count, p) = read_u16(body, pos + 1);
                next_id = next_id.wrapping_add(count);
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                next_id = next_id.wrapping_add(count as u16);
                pos += 2;
            }
            other => {
                tracing::warn!(opcode = other, "unknown SIBT opcode; stopping string parse");
                break;
            }
        }
    }
    Some(ParsedStringPackage { language, strings })
}

fn push(
    strings: &mut Vec<(u16, String)>,
    by_id: &mut HashMap<u16, String>,
    next_id: &mut u16,
    text: String,
) {
    by_id.insert(*next_id, text.clone());
    strings.push((*next_id, text));
    *next_id = next_id.wrapping_add(1);
}

fn read_u32(body: &[u8], pos: usize) -> Option<u32> {
    if pos + 4 > body.len() {
        return None;
    }
    Some(u32::from_le_bytes([
        body[pos],
        body[pos + 1],
        body[pos + 2],
        body[pos + 3],
    ]))
}

fn read_u16(body: &[u8], pos: usize) -> (u16, usize) {
    if pos + 2 > body.len() {
        return (0, body.len());
    }
    (u16::from_le_bytes([body[pos], body[pos + 1]]), pos + 2)
}

fn read_language(body: &[u8], start: usize, end: usize) -> String {
    let stop = end.min(body.len());
    let mut s = String::new();
    let mut i = start;
    while i < stop && body[i] != 0 {
        s.push(body[i] as char);
        i += 1;
    }
    s
}

fn read_scsu(body: &[u8], start: usize) -> (String, usize) {
    let start = start.min(body.len());
    let mut i = start;
    while i < body.len() && body[i] != 0 {
        i += 1;
    }
    let text = String::from_utf8_lossy(&body[start..i]).into_owned();
    (text, if i < body.len() { i + 1 } else { body.len() })
}

fn read_ucs2(body: &[u8], start: usize) -> (String, usize) {
    let start = start.min(body.len());
    let mut i = start;
    while i + 1 < body.len() && !(body[i] == 0 && body[i + 1] == 0) {
        i += 2;
    }
    let units: Vec<u16> = body[start..i]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let text = String::from_utf16_lossy(&units);
    (
        text,
        if i + 1 < body.len() {
            i + 2
        } else {
            body.len()
        },
    )
}

pub fn collect_strings(image: &Image) -> Vec<StringInfo> {
    let mut out: Vec<StringInfo> = Vec::new();
    walk_for_string_package(&image.root, &mut out);
    out
}

fn walk_for_string_package(node: &FfsNode, out: &mut Vec<StringInfo>) {
    if node.node_type == FfsType::Section {
        if declared_len_sane(&node.body)
            && is_string_package(&node.body)
            && let Some(pkg) = parse_string_package(&node.body)
        {
            push_strings(pkg, out);
            return;
        }
        if node.subtype == EFI_SECTION_PE32
            && let Some(pkg) = first_resource_string_package(&node.body)
        {
            push_strings(pkg, out);
            return;
        }
    }
    for child in &node.children {
        walk_for_string_package(child, out);
        if !out.is_empty() {
            return;
        }
    }
}

fn push_strings(pkg: ParsedStringPackage, out: &mut Vec<StringInfo>) {
    for (sid, text) in pkg.strings {
        out.push(StringInfo {
            language: pkg.language.clone(),
            string_id: sid as u32,
            text,
        });
    }
}

fn first_resource_string_package(pe: &[u8]) -> Option<ParsedStringPackage> {
    for blob in hii_resource_blobs(pe) {
        let Some(list) = parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == PACKAGE_STRINGS
                && let Some(sp) = parse_string_package(pkg.bytes)
            {
                return Some(sp);
            }
        }
    }
    None
}

pub(crate) fn declared_len_sane(body: &[u8]) -> bool {
    body.len() >= 4
        && (body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16) <= body.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode, ParsingData};

    fn make_pkg(language: &str, sibt_bytes: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(0x04);
        buf.extend_from_slice(&hdr_size.to_le_bytes());
        buf.extend_from_slice(&info_off.to_le_bytes());
        while buf.len() < 44 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(language.as_bytes());
        buf.push(0);
        while buf.len() < info_off as usize {
            buf.push(0);
        }
        buf.extend_from_slice(sibt_bytes);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    #[test]
    fn parse_scsu_strings_and_language() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'H',
            b'i',
            0,
            SIBT_STRING_SCSU,
            b'B',
            b'y',
            b'e',
            0,
            SIBT_END,
        ];
        let pkg_body = make_pkg("en-US", &sibt);
        let parsed = parse_string_package(&pkg_body).expect("some package");
        assert_eq!(parsed.language, "en-US");
        assert_eq!(parsed.strings.len(), 2);
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
        assert_eq!(parsed.strings[1], (2, "Bye".to_string()));
    }

    #[test]
    fn parse_returns_none_for_non_string_package() {
        assert!(parse_string_package(&[0x00, 0x00, 0x00, 0x02]).is_none());
    }

    #[test]
    fn parse_ucs2_string() {
        let mut sibt = vec![SIBT_STRING_UCS2];
        for u in "Hi".encode_utf16() {
            sibt.extend_from_slice(&u.to_le_bytes());
        }
        sibt.push(0);
        sibt.push(0);
        sibt.push(SIBT_END);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
    }

    #[test]
    fn parse_skip_blocks_advance_ids_without_text() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            SIBT_SKIP2,
            0x02,
            0x00,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 2);
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (4, "B".to_string()));
    }

    #[test]
    fn parse_duplicate_copies_prior_text() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            SIBT_DUPLICATE,
            0x01,
            0x00,
            SIBT_END,
        ];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (2, "A".to_string()));
    }

    #[test]
    fn parse_truncated_language_falls_back_empty() {
        let mut buf = vec![0u8; 12];
        buf[3] = 0x04;
        buf.extend_from_slice(&(HEADER_LEN as u32).to_le_bytes());
        buf.push(SIBT_END);
        let parsed = parse_string_package(&buf).unwrap();
        assert!(parsed.language.is_empty());
    }

    #[test]
    fn parse_truncated_strings_scsu_block_returns_only_real_strings() {
        let sibt = [SIBT_STRINGS_SCSU, 0x03, 0x00, b'A', 0];
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1);
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
    }

    #[test]
    fn parse_truncated_strings_ucs2_block_returns_only_real_strings() {
        let mut sibt = vec![SIBT_STRINGS_UCS2, 0x02, 0x00];
        for u in "Hi".encode_utf16() {
            sibt.extend_from_slice(&u.to_le_bytes());
        }
        sibt.push(0);
        sibt.push(0);
        let pkg = make_pkg("en", &sibt);
        let parsed = parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings.len(), 1);
        assert_eq!(parsed.strings[0], (1, "Hi".to_string()));
    }

    #[test]
    fn parse_truncated_font_opcode_does_not_panic() {
        let pkg = make_pkg("en", &[SIBT_STRING_SCSU_FONT]);
        let parsed = parse_string_package(&pkg).unwrap();
        assert!(parsed.strings.len() <= 1);
        if let Some((_, text)) = parsed.strings.first() {
            assert!(text.is_empty());
        }
    }

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn mk_image(pkg_body: Vec<u8>) -> Image {
        let section = mk_node(FfsType::Section, pkg_body, vec![]);
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        }
    }

    #[test]
    fn collect_strings_returns_first_package_strings() {
        let sibt = [SIBT_STRING_SCSU, b'X', 0, SIBT_END];
        let pkg = make_pkg("eng", &sibt);
        let image = mk_image(pkg);
        let out = collect_strings(&image);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].language, "eng");
        assert_eq!(out[0].string_id, 1);
        assert_eq!(out[0].text, "X");
    }

    #[test]
    fn collect_strings_empty_when_no_package() {
        let image = mk_image(vec![0x00, 0x00, 0x00, 0x02]);
        assert!(collect_strings(&image).is_empty());
    }

    #[test]
    fn collect_strings_from_pe_resources() {
        let blob = include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", blob);
        let mut section = mk_node(FfsType::Section, pe, vec![]);
        section.subtype = crate::ffs::EFI_SECTION_PE32;
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let out = collect_strings(&image);
        assert!(!out.is_empty());
        assert_eq!(out[0].language, "en-US");
    }

    #[test]
    fn collect_strings_ignores_bare_body_with_bogus_declared_length() {
        let mut image = mk_image(vec![0x09, 0x00, 0x00, 0x04]);
        image.root.children[0].children[0].children[0].subtype = crate::ffs::EFI_SECTION_RAW;
        assert!(collect_strings(&image).is_empty());
    }
}
