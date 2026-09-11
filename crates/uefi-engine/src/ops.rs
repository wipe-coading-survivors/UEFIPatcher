use crate::ffs::{EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED};
use crate::parser::ParserError;
use crate::types::*;

#[derive(Debug, thiserror::Error)]
pub enum OpsError {
    #[error("not found")]
    NotFound,
    #[error("invalid parent")]
    InvalidParent,
    #[error("invalid FFS data")]
    InvalidFfs,
    #[error("mutation behind non-recompressable compression barrier")]
    MutationBehindCompression,
    #[error("flash region is read-only")]
    ImmutableRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertMode {
    Into,
    Before,
    After,
}

#[tracing::instrument(level = "debug", skip(root, ffs_bytes), fields(mode = ?mode, target = ?target), err)]
pub fn insert(
    root: &mut FfsNode,
    target: &Target,
    ffs_bytes: &[u8],
    mode: InsertMode,
) -> Result<(), OpsError> {
    let mut new_node = parse_ffs_bytes(ffs_bytes).map_err(|_| OpsError::InvalidFfs)?;
    let parent_path = match target {
        Target::Path(p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    let vol_path = match mode {
        InsertMode::Into => parent_path.clone(),
        InsertMode::Before | InsertMode::After => {
            if parent_path.is_empty() {
                return Err(OpsError::InvalidParent);
            }
            parent_path[..parent_path.len() - 1].to_vec()
        }
    };
    match mode {
        InsertMode::Into => ensure_mutable(root, &parent_path, true)?,
        InsertMode::Before | InsertMode::After => ensure_mutable(root, &parent_path, false)?,
    }
    if enclosing_volume_empty_byte(root, &vol_path) == Some(0xFF)
        && new_node.header.get(23) == Some(&0x07)
    {
        new_node.header[23] = 0xF8;
    }
    match mode {
        InsertMode::Into => {
            let parent = find_mut(root, &parent_path).ok_or(OpsError::NotFound)?;
            parent.children.push(new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::Before | InsertMode::After => {
            let idx = *parent_path.last().unwrap();
            let grandparent_path = &parent_path[..parent_path.len() - 1];
            let grandparent = find_mut(root, grandparent_path).ok_or(OpsError::NotFound)?;
            if idx > grandparent.children.len() {
                return Err(OpsError::InvalidParent);
            }
            let insert_at = if mode == InsertMode::Before {
                idx
            } else {
                idx + 1
            };
            grandparent.children.insert(insert_at, new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
    }
    tracing::debug!(path = ?parent_path, mode = ?mode, "node inserted");
    Ok(())
}

#[tracing::instrument(level = "debug", skip(root), fields(target = ?target), err)]
pub fn remove(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(root, target)?;
    ensure_mutable(root, &path, true)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    node.action = Action::Remove;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, "marked for removal");
    Ok(())
}

#[tracing::instrument(level = "debug", skip(root, data), fields(target = ?target, body_only), err)]
pub fn replace(
    root: &mut FfsNode,
    target: &Target,
    data: &[u8],
    body_only: bool,
) -> Result<(), OpsError> {
    let path = target_path(root, target)?;
    ensure_mutable(root, &path, true)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    if body_only {
        node.children.clear();
        node.body = data.to_vec();
    } else {
        let new_node = parse_ffs_bytes(data).map_err(|_| OpsError::InvalidFfs)?;
        node.header = new_node.header;
        node.body = new_node.body;
        node.tail = new_node.tail;
        node.children = new_node.children;
        node.guid = new_node.guid;
        node.node_type = new_node.node_type;
        node.subtype = new_node.subtype;
        node.parsing_data = new_node.parsing_data;
    }
    node.action = Action::Replace;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, body_only, "node replaced");
    Ok(())
}

#[tracing::instrument(level = "debug", skip(root), fields(target = ?target), err)]
pub fn rebuild(root: &mut FfsNode, target: &Target) -> Result<(), OpsError> {
    let path = target_path(root, target)?;
    ensure_mutable(root, &path, true)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
    if node.action == Action::Remove {
        return Ok(());
    }
    node.action = Action::Rebuild;
    mark_rebuild_to_root_by_path(root, &path);
    tracing::debug!(path = ?path, "marked for rebuild");
    Ok(())
}

pub fn mark_rebuild_to_root_by_path(root: &mut FfsNode, path: &[usize]) {
    if root.action == Action::NoAction {
        root.action = Action::Rebuild;
    }
    let mut node = root;
    for &i in path {
        let Some(child) = node.children.get_mut(i) else {
            break;
        };
        if child.action == Action::NoAction {
            child.action = Action::Rebuild;
        }
        node = child;
    }
}

fn ensure_mutable(root: &FfsNode, path: &[usize], include_target: bool) -> Result<(), OpsError> {
    let n = if include_target {
        path.len()
    } else {
        path.len().saturating_sub(1)
    };
    let mut node = root;
    for (depth, &i) in path.iter().enumerate() {
        let Some(child) = node.children.get(i) else {
            break;
        };
        if child.node_type == FfsType::Region {
            return Err(OpsError::ImmutableRegion);
        }
        if depth < n {
            if child.node_type == FfsType::Section && child.subtype == EFI_SECTION_COMPRESSION {
                return Err(OpsError::MutationBehindCompression);
            }
            if child.node_type == FfsType::Section
                && child.subtype == EFI_SECTION_GUID_DEFINED
                && !matches!(&child.parsing_data, ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid))
            {
                return Err(OpsError::MutationBehindCompression);
            }
        }
        node = child;
    }
    Ok(())
}

fn target_path(root: &FfsNode, target: &Target) -> Result<Vec<usize>, OpsError> {
    crate::parser::target::find_item_path(root, target).ok_or(OpsError::NotFound)
}

fn find_mut<'a>(node: &'a mut FfsNode, path: &[usize]) -> Option<&'a mut FfsNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.children.get_mut(i)?;
    }
    Some(cur)
}

fn enclosing_volume_empty_byte(root: &FfsNode, path: &[usize]) -> Option<u8> {
    let mut chain = vec![root];
    let mut node = root;
    for &i in path {
        node = node.children.get(i)?;
        chain.push(node);
    }
    for n in chain.into_iter().rev() {
        if let ParsingData::Volume(vd) = &n.parsing_data {
            return Some(vd.empty_byte);
        }
    }
    None
}

fn parse_ffs_bytes(data: &[u8]) -> Result<FfsNode, ParserError> {
    crate::parser::file::parse_file(data, 0, 0xFF, 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::{
        EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE, EFI_SECTION_COMPRESSION, size_to_uint24,
    };
    use crate::parser::image::parse_image;
    use crate::parser::target::parse_target;

    fn make_simple_image() -> Vec<u8> {
        let mut buf = vec![0xFFu8; 256];
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        buf
    }

    fn make_ffs_file() -> Vec<u8> {
        let guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let mut buf = vec![0u8; 32];
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = 0x01;
        buf[20..23].copy_from_slice(&size_to_uint24(32));
        buf[24..32].copy_from_slice(&[0xAA; 8]);
        buf
    }

    fn make_simple_image_polarity(empty: u8) -> Vec<u8> {
        let mut buf = make_simple_image();
        if empty == 0x00 {
            buf[44..48].copy_from_slice(&0u32.to_le_bytes());
        }
        buf
    }

    fn make_ffs_file_state(state: u8) -> Vec<u8> {
        let mut buf = make_ffs_file();
        buf[16] = 0xAB;
        buf[23] = state;
        buf
    }

    #[test]
    fn insert_adapts_state_byte_to_erase_polarity() {
        let buf = make_simple_image_polarity(0xFF);
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        img.root.children[0]
            .children
            .push(parse_ffs_bytes(&make_ffs_file_state(0x07)).unwrap());
        let anchor = parse_target("0/0").unwrap();
        let ffs = make_ffs_file_state(0x07);
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let new_file = &img.root.children[0].children[1];
        assert_eq!(new_file.header[23], 0xF8);
        assert_eq!(
            new_file.header[16], 0xAB,
            "header checksum must stay untouched"
        );
    }

    #[test]
    fn insert_keeps_state_byte_verbatim_in_polarity0_volume() {
        let buf = make_simple_image_polarity(0x00);
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        img.root.children[0]
            .children
            .push(parse_ffs_bytes(&make_ffs_file_state(0x07)).unwrap());
        let anchor = parse_target("0/0").unwrap();
        let ffs = make_ffs_file_state(0x07);
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let new_file = &img.root.children[0].children[1];
        assert_eq!(new_file.header[23], 0x07);
        assert_eq!(new_file.header[16], 0xAB);
    }

    #[test]
    fn rebuild_marks_node_and_cascade() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        rebuild(&mut img.root, &t).unwrap();
        assert_eq!(img.root.action, Action::Rebuild);
        assert_eq!(img.root.children[0].action, Action::Rebuild);
    }

    #[test]
    fn rebuild_after_remove_keeps_remove() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let ffs = make_ffs_file();
        let t = parse_target("0").unwrap();
        insert(&mut img.root, &t, &ffs, InsertMode::Into).unwrap();
        let file_target = parse_target("0/0").unwrap();
        remove(&mut img.root, &file_target).unwrap();
        rebuild(&mut img.root, &file_target).unwrap();
        assert_eq!(
            img.root.children[0].children[0].action,
            Action::Remove,
            "rebuild must not resurrect a removed node"
        );
    }

    #[test]
    fn rebuild_accepts_guid_section_target() {
        let buf = make_simple_image();
        let file_bytes = make_ffs_file();
        let mut file = parse_ffs_bytes(&file_bytes).unwrap();
        let guid = file.guid.unwrap();
        file.children.push(FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x19,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xAA; 8],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        });
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        img.root.children[0].children.push(file);

        let t = parse_target(&format!("{guid}:0x19:0")).unwrap();
        rebuild(&mut img.root, &t).unwrap();
        assert_eq!(img.root.action, Action::Rebuild);
        assert_eq!(img.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(
            img.root.children[0].children[0].children[0].action,
            Action::Rebuild
        );

        let t_missing = parse_target(&format!("{guid}:0x15:0")).unwrap();
        assert!(matches!(
            rebuild(&mut img.root, &t_missing),
            Err(OpsError::NotFound)
        ));
    }

    #[test]
    fn remove_marks_node_and_cascades() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        remove(&mut img.root, &t).unwrap();
        assert_eq!(img.root.children[0].action, Action::Remove);
        assert_eq!(img.root.action, Action::Rebuild);
    }

    #[test]
    fn insert_into_appends_and_marks() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        let ffs = make_ffs_file();
        insert(&mut img.root, &t, &ffs, InsertMode::Into).unwrap();
        let vol = &img.root.children[0];
        assert!(!vol.children.is_empty());
        assert_eq!(vol.children[0].node_type, FfsType::File);
        assert_eq!(vol.action, Action::Rebuild);
    }

    #[test]
    fn replace_body_only_keeps_header() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        let ffs = make_ffs_file();
        insert(&mut img.root, &t, &ffs, InsertMode::Into).unwrap();
        let new_body = vec![0x42u8; 4];
        let file_target = parse_target("0/0").unwrap();
        replace(&mut img.root, &file_target, &new_body, true).unwrap();
        let f = &img.root.children[0].children[0];
        assert_eq!(f.body, new_body);
        assert!(f.children.is_empty());
        assert_eq!(f.action, Action::Replace);
    }

    #[test]
    fn insert_invalid_ffs_errors() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        assert!(insert(&mut img.root, &t, &[0; 2], InsertMode::Into).is_err());
    }

    #[tracing_test::traced_test]
    #[test]
    fn remove_emits_debug_milestone() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Read, "i", "s").unwrap();
        let t = parse_target("0").unwrap();
        remove(&mut img.root, &t).unwrap();
        assert!(logs_contain("marked for removal"));
    }

    fn descriptor_image(region_specs: &[(usize, u16, u16)], total: usize) -> Vec<u8> {
        let mut buf = vec![0xFFu8; total];
        buf[0x10..0x14]
            .copy_from_slice(&crate::parser::region::FLASH_DESCRIPTOR_SIGNATURE.to_le_bytes());
        buf[0x14..0x18].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        for &(i, base, limit) in region_specs {
            let at = 0x400 + i * 4;
            buf[at..at + 2].copy_from_slice(&base.to_le_bytes());
            buf[at + 2..at + 4].copy_from_slice(&limit.to_le_bytes());
        }
        buf
    }

    #[test]
    fn region_nodes_immutable() {
        let buf = descriptor_image(&[(1, 4, 7), (2, 1, 3)], 0x10000);
        let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
        let me_idx = img
            .root
            .children
            .iter()
            .position(|c| {
                matches!(
                    &c.parsing_data,
                    ParsingData::Region(rd) if rd.kind == FlashRegionKind::Me
                )
            })
            .unwrap();
        let t = parse_target(&me_idx.to_string()).unwrap();
        assert!(matches!(
            remove(&mut img.root, &t),
            Err(OpsError::ImmutableRegion)
        ));
    }

    #[test]
    fn insert_before_me_region_refused() {
        let buf = descriptor_image(&[(1, 4, 7), (2, 1, 3)], 0x10000);
        let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
        let me_idx = img
            .root
            .children
            .iter()
            .position(|c| {
                matches!(
                    &c.parsing_data,
                    ParsingData::Region(rd) if rd.kind == FlashRegionKind::Me
                )
            })
            .unwrap();
        let t = parse_target(&me_idx.to_string()).unwrap();
        assert!(matches!(
            insert(&mut img.root, &t, &make_ffs_file(), InsertMode::Before),
            Err(OpsError::ImmutableRegion)
        ));
    }

    #[test]
    fn remove_behind_tiano_compression_refused() {
        let buf = make_simple_image();
        let mut img = parse_image(&buf, ImageMode::Write, "i", "s").unwrap();
        let mut file = parse_ffs_bytes(&make_ffs_file()).unwrap();
        let mut inner = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x19,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xAA; 8],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        inner.action = Action::Rebuild;
        file.children.push(FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_COMPRESSION,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xBB; 16],
            tail: vec![],
            children: vec![inner],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        });
        img.root.children[0].children.push(file);
        let t = parse_target("0/0/0/0").unwrap();
        assert!(matches!(
            remove(&mut img.root, &t),
            Err(OpsError::MutationBehindCompression)
        ));
    }
}
