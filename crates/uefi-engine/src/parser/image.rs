use super::ParserError;
use super::file::parse_file;
use super::section::parse_sections;
use super::volume::parse_volume;
use crate::ffs::*;
use crate::types::*;
use uefi_proto::Node;

const FVH_SCAN_STEP: usize = 16;
const FFS_ALIGN: usize = 8;

pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
    let mut children = vec![];
    let mut off = 0usize;
    while off + 44 <= buf.len() {
        let sig = u32::from_le_bytes([buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43]]);
        if sig != EFI_FVH_SIGNATURE {
            off += FVH_SCAN_STEP;
            continue;
        }
        match parse_volume(buf, off as u32) {
            Ok(vol) => {
                let vol_size = vol.header.len() + vol.body.len();
                if vol_size == 0 {
                    off += FVH_SCAN_STEP;
                    continue;
                }
                let (erase, rev) = match &vol.parsing_data {
                    ParsingData::Volume(vd) => (vd.empty_byte, vd.revision),
                    _ => (0xFF, 2),
                };
                let mut vol_with_files = vol.clone();
                let header_len = vol.header.len();
                let body_start = off + header_len;
                let body_end = off + vol_size;
                vol_with_files.children =
                    parse_volume_files(&buf[body_start..body_end], body_start, erase, rev);
                children.push(vol_with_files);
                off += vol_size;
            }
            Err(e) => {
                tracing::debug!("not a volume at {off:#x}: {e}");
                off += FVH_SCAN_STEP;
            }
        }
    }
    Ok(Image {
        image_id: image_id.into(),
        session_id: session_id.into(),
        root: FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        },
        mode,
    })
}

fn parse_volume_files(body: &[u8], body_start: usize, erase: u8, rev: u8) -> Vec<FfsNode> {
    let mut files = vec![];
    let mut foff = 0usize;
    while foff + 24 <= body.len() {
        foff = (foff + (FFS_ALIGN - 1)) & !(FFS_ALIGN - 1);
        if foff + 24 > body.len() {
            break;
        }
        if body[foff..foff + 24].iter().all(|&b| b == erase) {
            break;
        }
        match parse_file(body, foff as u32, erase, rev) {
            Ok(mut file_node) => {
                let total = file_node.header.len() + file_node.body.len() + file_node.tail.len();
                file_node.offset = (body_start + foff) as u32;
                file_node.children = parse_sections(&file_node.body, 0);
                files.push(file_node);
                if total == 0 {
                    break;
                }
                foff += total;
            }
            Err(e) => {
                tracing::warn!("file parse error at {}: {e}", body_start + foff);
                break;
            }
        }
    }
    files
}

pub fn list_items(root: &FfsNode, filter: Option<&str>) -> Vec<Node> {
    let mut items = vec![];
    list_recursive(root, "", &mut items, filter);
    items
}

fn list_recursive(node: &FfsNode, path: &str, items: &mut Vec<Node>, filter: Option<&str>) {
    let name = node_name(node);
    if filter.is_none_or(|f| name.contains(f) || path.contains(f)) {
        items.push(Node {
            path: path.to_string(),
            r#type: node.node_type as u32,
            subtype: node.subtype as u32,
            guid: node
                .guid
                .map(|g| crate::guid_to_upper_string(&g))
                .unwrap_or_default(),
            offset: node.offset as u64,
            size: (node.header.len() + node.body.len() + node.tail.len()) as u64,
            name,
            action: node.action as u32,
        });
    }
    for (i, child) in node.children.iter().enumerate() {
        let p = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}/{i}")
        };
        list_recursive(child, &p, items, filter);
    }
}

pub fn search(
    root: &FfsNode,
    query: &str,
    modes: &[uefi_common::search::SearchMode],
    limit: usize,
) -> Vec<Node> {
    let mut out = vec![];
    if modes.is_empty() {
        return out;
    }
    let effective_limit = if limit == 0 { usize::MAX } else { limit };
    let mut path = String::new();
    search_recursive(root, &mut path, query, modes, effective_limit, &mut out);
    out
}

fn search_recursive(
    node: &FfsNode,
    path: &mut String,
    query: &str,
    modes: &[uefi_common::search::SearchMode],
    limit: usize,
    out: &mut Vec<Node>,
) {
    if out.len() >= limit {
        return;
    }
    if node.node_type == FfsType::Section && section_matches(node, query, modes) {
        let name = node_name(node);
        out.push(Node {
            path: path.clone(),
            r#type: node.node_type as u32,
            subtype: node.subtype as u32,
            guid: node
                .guid
                .map(|g| crate::guid_to_upper_string(&g))
                .unwrap_or_default(),
            offset: node.offset as u64,
            size: (node.header.len() + node.body.len() + node.tail.len()) as u64,
            name,
            action: node.action as u32,
        });
        if out.len() >= limit {
            return;
        }
    }
    let saved_len = path.len();
    for (i, child) in node.children.iter().enumerate() {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&i.to_string());
        search_recursive(child, path, query, modes, limit, out);
        path.truncate(saved_len);
        if out.len() >= limit {
            return;
        }
    }
}

fn section_matches(node: &FfsNode, query: &str, modes: &[uefi_common::search::SearchMode]) -> bool {
    let name = node_name(node);
    for &m in modes {
        let hit = match m {
            uefi_common::search::SearchMode::Name => uefi_common::search::match_name(&name, query),
            uefi_common::search::SearchMode::Utf8 => {
                uefi_common::search::find_utf8(&node.body, query.as_bytes())
            }
            uefi_common::search::SearchMode::Utf16Le => {
                uefi_common::search::find_utf16le(&node.body, query)
            }
            uefi_common::search::SearchMode::Bytes => uefi_common::search::parse_hex_pattern(query)
                .map(|p| uefi_common::search::find_utf8(&node.body, &p))
                .unwrap_or(false),
        };
        if hit {
            return true;
        }
    }
    false
}

fn node_name(node: &FfsNode) -> String {
    match node.node_type {
        FfsType::Section
            if node.subtype == EFI_SECTION_UI || node.subtype == EFI_SECTION_VERSION =>
        {
            decode_utf16le_body(&node.body)
        }
        FfsType::File => {
            for child in &node.children {
                if child.node_type == FfsType::Section && child.subtype == EFI_SECTION_UI {
                    return decode_utf16le_body(&child.body);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn decode_utf16le_body(body: &[u8]) -> String {
    let text: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&text)
        .trim_end_matches('\u{0}')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::{EFI_FVH_SIGNATURE, EFI_SECTION_RAW, EFI_SECTION_UI};
    use crate::types::{Action, FfsNode, FfsType, ParsingData};

    fn make_image_with_volume() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        buf
    }

    #[test]
    fn parse_image_finds_volume() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.node_type, FfsType::Image);
        assert!(!img.root.children.is_empty());
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[0].offset, 0);
    }

    #[test]
    fn list_items_root_and_volume() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        let items = list_items(&img.root, None);
        assert!(items.len() >= 2);
        let root_item = &items[0];
        assert_eq!(root_item.path, "");
        assert_eq!(root_item.r#type, FfsType::Image as u32);
        assert_eq!(items[1].path, "0");
        assert_eq!(items[1].r#type, FfsType::Volume as u32);
    }

    #[test]
    fn parse_image_empty_buffer() {
        let img = parse_image(&[], ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.node_type, FfsType::Image);
        assert!(img.root.children.is_empty());
    }

    #[test]
    fn node_name_lifts_ui_for_ffs_file() {
        let ui_section = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_UI,
            offset: 0,
            header: vec![0; 4],
            body: encode_utf16le_null("Setup"),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![0; 24],
            body: vec![],
            tail: vec![],
            children: vec![ui_section],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        assert_eq!(node_name(&file), "Setup");
    }

    #[test]
    fn node_name_empty_for_ffs_without_ui_child() {
        let raw_section = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0; 4],
            body: vec![0xAA; 4],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x01,
            offset: 0,
            header: vec![0; 24],
            body: vec![],
            tail: vec![],
            children: vec![raw_section],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        assert_eq!(node_name(&file), "");
    }

    fn encode_utf16le_null(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|u| u.to_le_bytes())
            .collect()
    }

    #[test]
    fn search_finds_section_by_name_mode() {
        let ui_section = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_UI,
            offset: 100,
            header: vec![0; 4],
            body: encode_utf16le_null("Setup"),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![ui_section],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let res = search(&root, "set", &[uefi_common::search::SearchMode::Name], 100);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].subtype, EFI_SECTION_UI as u32);
        assert_eq!(res[0].name, "Setup");
    }

    #[test]
    fn search_finds_section_by_utf8_body() {
        let raw_section = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0; 4],
            body: b"Hello World".to_vec(),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![raw_section],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let res = search(
            &root,
            "World",
            &[uefi_common::search::SearchMode::Utf8],
            100,
        );
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn search_limit_truncates_results() {
        let sections: Vec<FfsNode> = (0..5)
            .map(|_| FfsNode {
                guid: None,
                node_type: FfsType::Section,
                subtype: EFI_SECTION_RAW,
                offset: 0,
                header: vec![0; 4],
                body: b"needle".to_vec(),
                tail: vec![],
                children: vec![],
                action: Action::NoAction,
                parsing_data: ParsingData::None,
                fixed: false,
                compressed: false,
                alignment_bytes: vec![],
            })
            .collect();
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: sections,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let res = search(&root, "needle", &[uefi_common::search::SearchMode::Utf8], 3);
        assert_eq!(res.len(), 3);
    }

    #[test]
    fn search_limit_zero_returns_all() {
        let sections: Vec<FfsNode> = (0..3)
            .map(|_| FfsNode {
                guid: None,
                node_type: FfsType::Section,
                subtype: EFI_SECTION_RAW,
                offset: 0,
                header: vec![0; 4],
                body: b"needle".to_vec(),
                tail: vec![],
                children: vec![],
                action: Action::NoAction,
                parsing_data: ParsingData::None,
                fixed: false,
                compressed: false,
                alignment_bytes: vec![],
            })
            .collect();
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: sections,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let res = search(&root, "needle", &[uefi_common::search::SearchMode::Utf8], 0);
        assert_eq!(res.len(), 3, "limit=0 must mean no limit");
    }

    #[test]
    fn search_skips_non_section_nodes() {
        let file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x01,
            offset: 0,
            header: vec![0; 24],
            body: b"needle".to_vec(),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let root = FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![file],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let res = search(
            &root,
            "needle",
            &[uefi_common::search::SearchMode::Utf8],
            100,
        );
        assert_eq!(res.len(), 0, "search must skip File nodes");
    }
}
