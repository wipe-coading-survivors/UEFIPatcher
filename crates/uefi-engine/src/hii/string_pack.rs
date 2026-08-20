use std::collections::HashMap;

use super::HiiError;
use crate::ops;
use crate::types::*;

pub use r_efi::hii::PACKAGE_STRINGS;

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

const PACKAGE_HEADER_LEN: usize = 4;
const STRING_INFO_OFFSET_POS: usize = 8;

pub fn add_strings(
    image: &mut Image,
    ffs_guid: Option<&Guid>,
    strings: &[String],
) -> Result<HashMap<String, u16>, HiiError> {
    let path = string_package_section_path(&image.root, ffs_guid)
        .ok_or(HiiError::StringPackageNotFound)?;
    let mut node = &mut image.root;
    for &i in &path {
        node = &mut node.children[i];
    }
    let mapping = add_strings_to_body(&mut node.body, strings);
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    Ok(mapping)
}

fn add_strings_to_body(body: &mut Vec<u8>, strings: &[String]) -> HashMap<String, u16> {
    let sibt_start = string_info_offset(body);
    let (mut next_id, mut end_pos) = scan_sibt(body, sibt_start);
    let mut mapping = HashMap::new();
    for s in strings {
        let new_id = next_id;
        next_id = next_id.wrapping_add(1);
        mapping.insert(s.clone(), new_id);
        end_pos = append_scsu_string(body, end_pos, s);
    }
    update_package_length(body);
    mapping
}

pub fn string_package_section_path(root: &FfsNode, ffs_guid: Option<&Guid>) -> Option<Vec<usize>> {
    walk_for_string_package(root, ffs_guid, None, &mut Vec::new())
}

fn walk_for_string_package(
    node: &FfsNode,
    ffs_guid: Option<&Guid>,
    owner: Option<&Guid>,
    path: &mut Vec<usize>,
) -> Option<Vec<usize>> {
    for (i, child) in node.children.iter().enumerate() {
        let child_owner = if child.node_type == FfsType::File {
            child.guid.as_ref()
        } else {
            owner
        };
        if child.node_type == FfsType::Section
            && child.children.is_empty()
            && is_string_package(&child.body)
            && ffs_guid.is_none_or(|g| owner == Some(g))
        {
            path.push(i);
            return Some(path.clone());
        }
        path.push(i);
        if let Some(found) = walk_for_string_package(child, ffs_guid, child_owner, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

pub fn is_string_package(body: &[u8]) -> bool {
    body.len() >= PACKAGE_HEADER_LEN && body[3] == PACKAGE_STRINGS
}

fn string_info_offset(body: &[u8]) -> usize {
    if body.len() >= STRING_INFO_OFFSET_POS + 4 {
        let off = u32::from_le_bytes([
            body[STRING_INFO_OFFSET_POS],
            body[STRING_INFO_OFFSET_POS + 1],
            body[STRING_INFO_OFFSET_POS + 2],
            body[STRING_INFO_OFFSET_POS + 3],
        ]) as usize;
        if off >= PACKAGE_HEADER_LEN && off < body.len() {
            return off;
        }
    }
    PACKAGE_HEADER_LEN
}

fn scan_sibt(body: &[u8], start: usize) -> (u16, usize) {
    let mut pos = start;
    let mut next_id: u16 = 1;
    while pos < body.len() {
        match body[pos] {
            SIBT_END => return (next_id, pos),
            SIBT_STRING_SCSU => {
                next_id = next_id.wrapping_add(1);
                pos = skip_scsu(body, pos + 1);
            }
            SIBT_STRING_SCSU_FONT => {
                next_id = next_id.wrapping_add(1);
                pos = skip_scsu(body, pos + 2);
            }
            SIBT_STRINGS_SCSU => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRINGS_SCSU_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_scsu(body, pos);
                }
            }
            SIBT_STRING_UCS2 => {
                next_id = next_id.wrapping_add(1);
                pos = skip_ucs2(body, pos + 1);
            }
            SIBT_STRING_UCS2_FONT => {
                next_id = next_id.wrapping_add(1);
                pos = skip_ucs2(body, pos + 2);
            }
            SIBT_STRINGS_UCS2 => {
                let (ids, p) = read_u16_count(body, pos + 1);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_STRINGS_UCS2_FONT => {
                let (ids, p) = read_u16_count(body, pos + 2);
                pos = p;
                for _ in 0..ids {
                    next_id = next_id.wrapping_add(1);
                    pos = skip_ucs2(body, pos);
                }
            }
            SIBT_DUPLICATE => {
                next_id = next_id.wrapping_add(1);
                pos += 1 + 2;
            }
            SIBT_SKIP2 => {
                let (c, p) = read_u16_count(body, pos + 1);
                next_id = next_id.wrapping_add(c);
                pos = p;
            }
            SIBT_SKIP1 => {
                let count = body.get(pos + 1).copied().unwrap_or(0);
                next_id = next_id.wrapping_add(count as u16);
                pos += 2;
            }
            _ => break,
        }
    }
    (next_id, body.len())
}

fn skip_scsu(body: &[u8], start: usize) -> usize {
    let mut p = start;
    while p < body.len() {
        if body[p] == 0 {
            return p + 1;
        }
        p += 1;
    }
    p
}

fn skip_ucs2(body: &[u8], start: usize) -> usize {
    let mut p = start;
    while p + 1 < body.len() {
        if body[p] == 0 && body[p + 1] == 0 {
            return p + 2;
        }
        p += 2;
    }
    body.len()
}

fn read_u16_count(body: &[u8], pos: usize) -> (u16, usize) {
    if pos + 2 > body.len() {
        return (0, body.len());
    }
    let c = u16::from_le_bytes([body[pos], body[pos + 1]]);
    (c, pos + 2)
}

fn append_scsu_string(body: &mut Vec<u8>, end_pos: usize, text: &str) -> usize {
    let block = build_scsu_block(text);
    body.splice(end_pos..end_pos, block.iter().copied());
    end_pos + block.len()
}

fn build_scsu_block(text: &str) -> Vec<u8> {
    let mut block = Vec::with_capacity(1 + text.len() + 1);
    block.push(SIBT_STRING_SCSU);
    block.extend_from_slice(text.as_bytes());
    block.push(0x00);
    block
}

fn update_package_length(body: &mut [u8]) {
    let len = body.len() as u32;
    body[0] = (len & 0xFF) as u8;
    body[1] = ((len >> 8) & 0xFF) as u8;
    body[2] = ((len >> 16) & 0xFF) as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_string_package(existing: &[&str]) -> Vec<u8> {
        let sibt_start = 12u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(PACKAGE_STRINGS);
        buf.extend_from_slice(&sibt_start.to_le_bytes());
        buf.extend_from_slice(&sibt_start.to_le_bytes());
        for s in existing {
            buf.push(SIBT_STRING_SCSU);
            buf.extend_from_slice(s.as_bytes());
            buf.push(0x00);
        }
        buf.push(SIBT_END);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
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

    fn make_image(pkg_body: Vec<u8>) -> Image {
        let section = mk_node(FfsType::Section, pkg_body, vec![]);
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn is_string_package_detects() {
        let pkg = make_string_package(&[]);
        assert!(is_string_package(&pkg));
        assert!(!is_string_package(&[0x00, 0x00, 0x00, 0x02]));
    }

    #[test]
    fn add_strings_to_body_assigns_sequential_ids() {
        let mut pkg = make_string_package(&["first", "second"]);
        let mapping = add_strings_to_body(&mut pkg, &["a".to_string(), "b".to_string()]);
        assert_eq!(mapping["a"], 3);
        assert_eq!(mapping["b"], 4);
    }

    #[test]
    fn add_strings_to_body_writes_scsu_block_before_end() {
        let mut pkg = make_string_package(&["first"]);
        let end_pos_before = pkg.len() - 1;
        assert_eq!(pkg[end_pos_before], SIBT_END);
        add_strings_to_body(&mut pkg, &["new".to_string()]);
        assert_eq!(pkg[end_pos_before], SIBT_STRING_SCSU);
        assert_eq!(&pkg[end_pos_before + 1..end_pos_before + 4], b"new");
        assert_eq!(pkg[end_pos_before + 4], 0x00);
        assert_eq!(*pkg.last().unwrap(), SIBT_END);
    }

    #[test]
    fn add_strings_to_body_updates_length() {
        let mut pkg = make_string_package(&[]);
        let len_before = pkg.len();
        add_strings_to_body(&mut pkg, &["hello".to_string()]);
        let stored = pkg[0] as u32 | ((pkg[1] as u32) << 8) | ((pkg[2] as u32) << 16);
        assert_eq!(stored as usize, pkg.len());
        assert!(pkg.len() > len_before);
    }

    #[test]
    fn add_strings_skips_skip_blocks_in_id_counting() {
        let mut pkg = make_string_package(&["first"]);
        let end = pkg.len() - 1;
        pkg.splice(end..end, [SIBT_SKIP2, 0x05, 0x00]);
        let mapping = add_strings_to_body(&mut pkg, &["after".to_string()]);
        assert_eq!(mapping["after"], 7);
    }

    #[test]
    fn add_strings_on_image_mutates_section_and_marks_rebuild() {
        let pkg = make_string_package(&["first", "second"]);
        let mut image = make_image(pkg);
        let mapping = add_strings(&mut image, None, &["x".to_string()]).unwrap();
        assert_eq!(mapping["x"], 3);
        let section = &image.root.children[0].children[0].children[0];
        let stored = section.body[0] as u32
            | ((section.body[1] as u32) << 8)
            | ((section.body[2] as u32) << 16);
        assert_eq!(stored as usize, section.body.len());
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
    }

    #[test]
    fn add_strings_returns_error_when_no_package() {
        let mut image = make_image(vec![0x00, 0x00, 0x00, 0x02]);
        assert!(matches!(
            add_strings(&mut image, None, &["x".to_string()]),
            Err(HiiError::StringPackageNotFound)
        ));
    }

    fn make_wrapped_image(pkg_body: Vec<u8>, file_guid: &str) -> Image {
        let inner = mk_node(FfsType::Section, pkg_body, vec![]);
        let wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        let mut file_node = mk_node(FfsType::File, vec![], vec![wrapper]);
        file_node.guid = Some(Guid::try_parse(file_guid).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file_node]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn add_strings_finds_package_inside_wrapper_section() {
        let pkg = make_string_package(&["first"]);
        let mut image = make_wrapped_image(pkg, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
        let mapping = add_strings(&mut image, None, &["x".to_string()]).unwrap();
        assert_eq!(mapping["x"], 2);
        let inner = &image.root.children[0].children[0].children[0].children[0];
        assert!(inner.body.len() > 20);
        assert_eq!(*inner.body.last().unwrap(), SIBT_END);
        assert_eq!(inner.action, Action::Rebuild);
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::Rebuild
        );
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
    }

    #[test]
    fn add_strings_guid_filter_applies_through_wrapper() {
        let pkg = make_string_package(&["first"]);
        let mut image = make_wrapped_image(pkg, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
        let other = Guid::try_parse("899407D7-92A6-4174-968F-6F0B47F86A23").unwrap();
        assert!(matches!(
            add_strings(&mut image, Some(&other), &["x".to_string()]),
            Err(HiiError::StringPackageNotFound)
        ));
        let same = Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        assert!(add_strings(&mut image, Some(&same), &["x".to_string()]).is_ok());
    }

    #[test]
    fn add_strings_refuses_pe_resource_channel() {
        let pkg = make_string_package(&["first"]);
        let g = Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let mut list = g.to_bytes().to_vec();
        let total = 20 + pkg.len() + 4;
        list.extend_from_slice(&(total as u32).to_le_bytes());
        list.extend_from_slice(&pkg);
        list.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        assert!(
            !crate::hii::pe_resource::hii_resource_ranges(&pe).is_empty(),
            "sanity: resource channel must hold the list"
        );
        let mut blob_range = crate::hii::pe_resource::hii_resource_ranges(&pe)[0];
        blob_range.1 += blob_range.0;
        let blob = &pe[blob_range.0..blob_range.1];
        let parsed = crate::hii::package_list::parse_package_list(blob).unwrap();
        assert!(
            parsed.packages.iter().any(|p| p.kind == PACKAGE_STRINGS),
            "sanity: STRING package must be inside the resource blob"
        );

        let pe_sec = mk_node(FfsType::Section, pe, vec![]);
        let mut file = mk_node(FfsType::File, vec![], vec![pe_sec]);
        file.guid = Some(g);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        assert!(matches!(
            add_strings(&mut image, None, &["x".to_string()]),
            Err(HiiError::StringPackageNotFound)
        ));
    }
}
