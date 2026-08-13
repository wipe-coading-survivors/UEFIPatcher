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
    let new_node = parse_ffs_bytes(ffs_bytes).map_err(|_| OpsError::InvalidFfs)?;
    let parent_path = match target {
        Target::Path(p) => p.clone(),
        _ => return Err(OpsError::NotFound),
    };
    match mode {
        InsertMode::Into => {
            let parent = find_mut(root, &parent_path).ok_or(OpsError::NotFound)?;
            parent.children.push(new_node);
            mark_rebuild_to_root_by_path(root, &parent_path);
        }
        InsertMode::Before | InsertMode::After => {
            if parent_path.is_empty() {
                return Err(OpsError::InvalidParent);
            }
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
    let path = target_path(target)?;
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
    let path = target_path(target)?;
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
    let path = target_path(target)?;
    let node = find_mut(root, &path).ok_or(OpsError::NotFound)?;
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

fn target_path(target: &Target) -> Result<Vec<usize>, OpsError> {
    match target {
        Target::Path(p) => Ok(p.clone()),
        _ => Err(OpsError::NotFound),
    }
}

fn find_mut<'a>(node: &'a mut FfsNode, path: &[usize]) -> Option<&'a mut FfsNode> {
    let mut cur = node;
    for &i in path {
        cur = cur.children.get_mut(i)?;
    }
    Some(cur)
}

fn parse_ffs_bytes(data: &[u8]) -> Result<FfsNode, ParserError> {
    crate::parser::file::parse_file(data, 0, 0xFF, 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE, size_to_uint24};
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
}
