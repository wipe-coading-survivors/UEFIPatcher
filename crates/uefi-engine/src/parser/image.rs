use super::ParserError;
use super::file::parse_file;
use super::section::parse_sections;
use super::volume::parse_volume;
use crate::ffs::*;
use crate::types::*;
use uefi_proto::Node;

const FVH_SCAN_STEP: usize = 16;
const FFS_ALIGN: usize = 8;

#[tracing::instrument(level = "info", skip(buf), fields(size = buf.len()), err)]
pub fn parse_image(
    buf: &[u8],
    mode: ImageMode,
    image_id: &str,
    session_id: &str,
) -> Result<Image, ParserError> {
    let mut children = vec![];
    let mut last_end = 0usize;
    if let Some(mut regions) = super::region::parse_flash_regions(buf) {
        regions.sort_by_key(|r| r.offset);
        for r in &regions {
            if r.offset > last_end {
                children.push(make_padding_node(buf, last_end, r.offset));
            }
            match r.kind {
                FlashRegionKind::Bios => {
                    scan_volumes(
                        buf,
                        r.offset..r.offset + r.size,
                        &mut children,
                        &mut last_end,
                    );
                }
                _ => {
                    children.push(super::region::make_region_node(
                        buf, r.kind, r.offset, r.size,
                    ));
                    last_end = r.offset + r.size;
                }
            }
        }
    } else {
        scan_volumes(buf, 0..buf.len(), &mut children, &mut last_end);
    }
    if buf.len() > last_end {
        children.push(make_padding_node(buf, last_end, buf.len()));
    }
    let img = Image {
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
    };
    tracing::info!(
        volumes = img.root.children.len(),
        files = count_files(&img.root),
        "image parsed"
    );
    Ok(img)
}

fn scan_volumes(
    buf: &[u8],
    window: std::ops::Range<usize>,
    children: &mut Vec<FfsNode>,
    last_end: &mut usize,
) {
    let mut off = window.start;
    while off + 44 <= window.end {
        let sig = u32::from_le_bytes([buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43]]);
        if sig != EFI_FVH_SIGNATURE {
            off += FVH_SCAN_STEP;
            continue;
        }
        match parse_firmware_volume(buf, off) {
            Some(vol) => {
                let vol_size = vol.header.len() + vol.body.len();
                if vol_size == 0 {
                    off += FVH_SCAN_STEP;
                    continue;
                }
                if off > *last_end {
                    children.push(make_padding_node(buf, *last_end, off));
                }
                children.push(vol);
                *last_end = off + vol_size;
                off += vol_size;
            }
            None => {
                off += FVH_SCAN_STEP;
            }
        }
    }
}

pub(crate) fn parse_firmware_volume(buf: &[u8], off: usize) -> Option<FfsNode> {
    let mut vol = match parse_volume(buf, off as u32) {
        Ok(v) => v,
        Err(e) => {
            tracing::debug!("not a volume at {off:#x}: {e}");
            return None;
        }
    };
    let vol_size = vol.header.len() + vol.body.len();
    let (erase, rev) = match &vol.parsing_data {
        ParsingData::Volume(vd) => (vd.empty_byte, vd.revision),
        _ => (0xFF, 2),
    };
    let header_len = vol.header.len();
    let body_start = off + header_len;
    let body_end = off + vol_size;
    vol.children = parse_volume_files(&buf[body_start..body_end], body_start, erase, rev);
    Some(vol)
}

fn count_files(node: &FfsNode) -> usize {
    let mut n = match node.node_type {
        FfsType::File => 1,
        _ => 0,
    };
    for child in &node.children {
        n += count_files(child);
    }
    n
}

fn make_padding_node(buf: &[u8], start: usize, end: usize) -> FfsNode {
    FfsNode {
        guid: None,
        node_type: FfsType::Padding,
        subtype: 0,
        offset: start as u32,
        header: vec![],
        body: buf[start..end].to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::None,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    }
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
        FfsType::File => find_lifted_name(&node.children, 0).unwrap_or_default(),
        FfsType::Region => match &node.parsing_data {
            ParsingData::Region(rd) => format!("{} region", rd.kind.label()),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn find_lifted_name(children: &[FfsNode], depth: usize) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    for child in children {
        if child.node_type == FfsType::Section
            && (child.subtype == EFI_SECTION_UI || child.subtype == EFI_SECTION_VERSION)
        {
            return Some(decode_utf16le_body(&child.body));
        }
    }
    for child in children {
        if child.node_type == FfsType::Section
            && (child.subtype == EFI_SECTION_COMPRESSION
                || child.subtype == EFI_SECTION_GUID_DEFINED)
            && let Some(name) = find_lifted_name(&child.children, depth + 1)
        {
            return Some(name);
        }
    }
    None
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

    fn guided_with_ui_child() -> FfsNode {
        let ui = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_UI,
            offset: 0,
            header: vec![0; 4],
            body: encode_utf16le_null("DeepSetup"),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_GUID_DEFINED,
            offset: 0,
            header: vec![0; 4],
            body: vec![],
            tail: vec![],
            children: vec![ui],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    #[test]
    fn node_name_lifts_ui_through_guided_wrapper() {
        let mut file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![0; 24],
            body: vec![],
            tail: vec![],
            children: vec![guided_with_ui_child()],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        assert_eq!(node_name(&file), "DeepSetup");
        let mut wrapper = guided_with_ui_child();
        wrapper.subtype = EFI_SECTION_COMPRESSION;
        file.children = vec![wrapper];
        assert_eq!(node_name(&file), "DeepSetup");
    }

    #[test]
    fn node_name_lift_stops_at_depth_limit() {
        let mut node = guided_with_ui_child();
        for _ in 0..10 {
            let mut w = guided_with_ui_child();
            w.children = vec![node];
            node = w;
        }
        let file = FfsNode {
            guid: None,
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![0; 24],
            body: vec![],
            tail: vec![],
            children: vec![node],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        assert_eq!(node_name(&file), "");
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

    #[test]
    fn parse_image_descriptor_path_regions_and_padding() {
        let mut buf = vec![0xFFu8; 0x10000];
        buf[0..4].copy_from_slice(&0x0FF0_A55Au32.to_le_bytes());
        buf[0x10..0x14].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        buf[0x400 + 1 * 4..0x400 + 1 * 4 + 2].copy_from_slice(&1u16.to_le_bytes());
        buf[0x400 + 1 * 4 + 2..0x400 + 1 * 4 + 4].copy_from_slice(&3u16.to_le_bytes());
        buf[0x400 + 2 * 4..0x400 + 2 * 4 + 2].copy_from_slice(&4u16.to_le_bytes());
        buf[0x400 + 2 * 4 + 2..0x400 + 2 * 4 + 4].copy_from_slice(&15u16.to_le_bytes());
        buf[0x1000..0x1100].copy_from_slice(&make_image_with_volume());
        let img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let kinds: Vec<&str> = img
            .root
            .children
            .iter()
            .filter(|c| c.node_type == FfsType::Region)
            .map(|c| match &c.parsing_data {
                ParsingData::Region(rd) => rd.kind.label(),
                _ => "",
            })
            .collect();
        assert!(kinds.contains(&"Descriptor"));
        assert!(kinds.contains(&"ME"));
        let bios_volumes = img
            .root
            .children
            .iter()
            .filter(|c| c.node_type == FfsType::Volume)
            .count();
        assert!(bios_volumes >= 1, "BIOS window scanned for FVs");
        let rebuilt = crate::builder::build_image(&img).unwrap();
        assert_eq!(rebuilt, buf, "descriptor round-trip byte-identical");
    }

    #[test]
    fn parse_image_captures_leading_gap_as_padding() {
        use crate::builder::build_image;
        let fv = make_image_with_volume();
        let mut buf = vec![0xAAu8; 64];
        buf.extend_from_slice(&fv);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 2, "expected Padding + Volume");
        let pad = &img.root.children[0];
        assert_eq!(pad.node_type, FfsType::Padding);
        assert_eq!(pad.offset, 0);
        assert_eq!(pad.body.len(), 64);
        assert_eq!(&pad.body, &[0xAAu8; 64]);
        let vol = &img.root.children[1];
        assert_eq!(vol.node_type, FfsType::Volume);
        assert_eq!(vol.offset, 64);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(
            rebuilt, buf,
            "leading gap round-trip must be byte-identical"
        );
    }

    #[test]
    fn parse_image_captures_gap_between_volumes_as_padding() {
        use crate::builder::build_image;
        let fv1 = make_image_with_volume();
        let fv2 = make_image_with_volume();
        let mut buf = fv1.clone();
        buf.extend_from_slice(&[0xBBu8; 32]);
        buf.extend_from_slice(&fv2);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(
            img.root.children.len(),
            3,
            "expected Volume + Padding + Volume"
        );
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[0].offset, 0);
        assert_eq!(img.root.children[1].node_type, FfsType::Padding);
        assert_eq!(img.root.children[1].offset, 256);
        assert_eq!(img.root.children[1].body.len(), 32);
        assert_eq!(img.root.children[2].node_type, FfsType::Volume);
        assert_eq!(img.root.children[2].offset, 288);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(
            rebuilt, buf,
            "inter-volume gap round-trip must be byte-identical"
        );
    }

    #[test]
    fn parse_image_captures_trailing_gap_as_padding() {
        use crate::builder::build_image;
        let fv = make_image_with_volume();
        let mut buf = fv;
        buf.extend_from_slice(&[0xCCu8; 64]);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(
            img.root.children.len(),
            2,
            "expected Volume + trailing Padding"
        );
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[1].node_type, FfsType::Padding);
        assert_eq!(img.root.children[1].offset, 256);
        assert_eq!(img.root.children[1].body.len(), 64);
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(
            rebuilt, buf,
            "trailing gap round-trip must be byte-identical"
        );
    }

    #[test]
    fn parse_image_no_padding_when_fv_fills_buffer() {
        let buf = make_image_with_volume();
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 1, "no gaps expected");
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
    }

    #[test]
    fn parse_image_all_padding_buffer_yields_single_padding_node() {
        let buf = vec![0x77u8; 200];
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(img.root.children.len(), 1);
        assert_eq!(img.root.children[0].node_type, FfsType::Padding);
        assert_eq!(img.root.children[0].offset, 0);
        assert_eq!(img.root.children[0].body.len(), 200);
    }

    #[test]
    fn parse_image_back_to_back_volumes_insert_no_padding() {
        let fv1 = make_image_with_volume();
        let fv2 = make_image_with_volume();
        let mut buf = fv1.clone();
        buf.extend_from_slice(&fv2);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        assert_eq!(
            img.root.children.len(),
            2,
            "two adjacent volumes, no padding"
        );
        assert_eq!(img.root.children[0].node_type, FfsType::Volume);
        assert_eq!(img.root.children[1].node_type, FfsType::Volume);
        let first = &img.root.children[0];
        let first_total = first.header.len() + first.body.len() + first.tail.len();
        assert_eq!(
            img.root.children[1].offset as usize,
            first.offset as usize + first_total,
            "child[1] must immediately follow child[0] with zero gap"
        );
    }

    #[test]
    fn parse_image_padding_node_fields() {
        let fv = make_image_with_volume();
        let mut buf = vec![0xDDu8; 48];
        buf.extend_from_slice(&fv);
        buf.extend_from_slice(&[0xEEu8; 16]);
        let img = parse_image(&buf, ImageMode::Read, "img1", "s1").unwrap();
        let lead = &img.root.children[0];
        assert_eq!(lead.node_type, FfsType::Padding);
        assert_eq!(lead.guid, None);
        assert_eq!(lead.subtype, 0);
        assert_eq!(lead.offset, 0);
        assert!(lead.header.is_empty());
        assert_eq!(lead.body.len(), 48);
        assert!(lead.tail.is_empty());
        assert!(lead.children.is_empty());
        assert_eq!(lead.action, Action::NoAction);
        assert!(matches!(lead.parsing_data, ParsingData::None));
        let trail = &img.root.children[2];
        assert_eq!(trail.node_type, FfsType::Padding);
        assert_eq!(trail.offset, 304);
        assert_eq!(trail.body.len(), 16);
    }
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    use crate::types::ImageMode;

    #[tracing_test::traced_test]
    #[test]
    fn parse_image_emits_milestone() {
        let buf = vec![0xFFu8; 4096];
        let _ = parse_image(&buf, ImageMode::Read, "img-logger", "sess-logger");
        assert!(logs_contain("image parsed"));
        assert!(logs_contain("img-logger"));
    }
}
