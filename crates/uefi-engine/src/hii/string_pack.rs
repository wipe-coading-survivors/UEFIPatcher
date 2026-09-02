use std::collections::HashMap;

use super::HiiError;
use super::strings::declared_len_sane;
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::{
    hii_entry_locations, plan_rsrc_blob_growth, try_grow_rsrc_tail, write_length_chain,
};
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

fn push_skip(out: &mut Vec<u8>, count: u16) {
    if count <= 0xFF {
        out.push(SIBT_SKIP1);
        out.push(count as u8);
    } else {
        out.push(SIBT_SKIP2);
        out.extend_from_slice(&count.to_le_bytes());
    }
}

fn push_string(out: &mut Vec<u8>, text: &str) {
    out.push(SIBT_STRING_SCSU);
    out.extend_from_slice(text.as_bytes());
    out.push(0x00);
}

fn block_id_count(body: &[u8], pos: usize) -> usize {
    match body[pos] {
        SIBT_STRING_SCSU
        | SIBT_STRING_SCSU_FONT
        | SIBT_STRING_UCS2
        | SIBT_STRING_UCS2_FONT
        | SIBT_DUPLICATE => 1,
        SIBT_STRINGS_SCSU | SIBT_STRINGS_UCS2 => usize::from(read_u16_count(body, pos + 1).0),
        SIBT_STRINGS_SCSU_FONT | SIBT_STRINGS_UCS2_FONT => {
            usize::from(read_u16_count(body, pos + 2).0)
        }
        SIBT_SKIP2 => usize::from(read_u16_count(body, pos + 1).0),
        SIBT_SKIP1 => body.get(pos + 1).copied().unwrap_or(0) as usize,
        _ => 0,
    }
}

fn strings_block_extent(body: &[u8], pos: usize, count_off: usize, ucs2: bool) -> usize {
    if pos + count_off + 2 > body.len() {
        return body.len();
    }
    let count = u16::from_le_bytes([body[pos + count_off], body[pos + count_off + 1]]);
    let mut p = pos + count_off + 2;
    for _ in 0..count {
        p = if ucs2 {
            skip_ucs2(body, p)
        } else {
            skip_scsu(body, p)
        };
    }
    p
}

fn block_end(body: &[u8], pos: usize) -> Option<usize> {
    let end = match body[pos] {
        SIBT_STRING_SCSU => skip_scsu(body, pos + 1),
        SIBT_STRING_SCSU_FONT => skip_scsu(body, pos + 2),
        SIBT_STRING_UCS2 => skip_ucs2(body, pos + 1),
        SIBT_STRING_UCS2_FONT => skip_ucs2(body, pos + 2),
        SIBT_DUPLICATE | SIBT_SKIP2 => (pos + 3).min(body.len()),
        SIBT_SKIP1 => (pos + 2).min(body.len()),
        SIBT_STRINGS_SCSU => strings_block_extent(body, pos, 1, false),
        SIBT_STRINGS_SCSU_FONT => strings_block_extent(body, pos, 2, false),
        SIBT_STRINGS_UCS2 => strings_block_extent(body, pos, 1, true),
        SIBT_STRINGS_UCS2_FONT => strings_block_extent(body, pos, 2, true),
        SIBT_END => pos,
        _ => return None,
    };
    Some(end)
}

pub(crate) fn insert_strings_at_ids(
    body: &mut Vec<u8>,
    entries: &[(u16, &str)],
) -> Result<(), crate::hii::HiiError> {
    let mut pending: Vec<(u16, &str)> = entries.iter().map(|(i, t)| (*i, *t)).collect();
    pending.sort_by_key(|(id, _)| *id);
    let mut pi = 0usize;
    let info_off = string_info_offset(body);
    let mut result: Vec<u8> = Vec::with_capacity(body.len() + 16);
    result.extend_from_slice(&body[..info_off]);
    let mut next_id: u16 = 1;
    let mut pos = info_off;

    while pos < body.len() && body[pos] != SIBT_END {
        let Some(end) = block_end(body, pos) else {
            if pending.get(pi).is_some() {
                return Err(crate::hii::HiiError::SibtBlockUnsupported(body[pos]));
            }
            break;
        };
        let count = block_id_count(body, pos) as u16;
        let span_end = next_id.wrapping_add(count);
        if body[pos] == SIBT_SKIP1 || body[pos] == SIBT_SKIP2 {
            let mut cursor = next_id;
            while pending
                .get(pi)
                .is_some_and(|(id, _)| *id >= cursor && *id < span_end)
            {
                let (id, text) = pending[pi];
                if id > cursor {
                    push_skip(&mut result, id - cursor);
                }
                push_string(&mut result, text);
                pi += 1;
                cursor = id.wrapping_add(1);
            }
            if span_end > cursor {
                push_skip(&mut result, span_end - cursor);
            }
        } else {
            if pending
                .get(pi)
                .is_some_and(|(id, _)| *id >= next_id && *id < span_end)
            {
                let (id, _) = pending[pi];
                return Err(crate::hii::HiiError::IdOccupied(id));
            }
            result.extend_from_slice(&body[pos..end]);
        }
        next_id = span_end;
        pos = end;
    }

    while let Some(&(id, text)) = pending.get(pi) {
        if id < next_id {
            return Err(crate::hii::HiiError::IdOccupied(id));
        }
        if id > next_id {
            push_skip(&mut result, id - next_id);
        }
        push_string(&mut result, text);
        pi += 1;
        next_id = id.wrapping_add(1);
    }

    result.extend_from_slice(&body[pos.min(body.len())..]);
    *body = result;
    update_package_length(body);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddStringsToResourceError {
    NotFound,
    GrowthUnsupported,
}

pub fn add_strings_to_resource(
    pe: &mut Vec<u8>,
    strings: &[String],
) -> Result<HashMap<String, u16>, AddStringsToResourceError> {
    use AddStringsToResourceError::{GrowthUnsupported, NotFound};
    let (_, blob_off, blob_len) = hii_entry_locations(pe).first().copied().ok_or(NotFound)?;
    let blob_end = blob_off.checked_add(blob_len).ok_or(NotFound)?;
    let blob = pe.get(blob_off..blob_end).ok_or(NotFound)?;
    let parsed = parse_package_list(blob).ok_or(NotFound)?;
    let idx = parsed
        .packages
        .iter()
        .position(|p| p.kind == PACKAGE_STRINGS && is_string_package(p.bytes))
        .ok_or(NotFound)?;
    let prefix: usize = parsed.packages[..idx].iter().map(|p| p.bytes.len()).sum();
    let old_len = parsed.packages[idx].bytes.len();
    let mut grown = blob[20 + prefix..20 + prefix + old_len].to_vec();
    let mapping = add_strings_to_body(&mut grown, strings);
    let delta = grown.len() - old_len;
    let sum: usize = parsed.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len.checked_add(delta).ok_or(GrowthUnsupported)?;
    let new_total = 20u64 + sum as u64 + delta as u64 + 4;
    if new_total > u32::MAX as u64 || new_blob_len > u32::MAX as usize {
        tracing::debug!("string package growth overflows list lengths");
        return Err(GrowthUnsupported);
    }
    let pkg_off = blob_off + 20 + prefix;
    let plan = plan_rsrc_blob_growth(pe, delta).ok_or(GrowthUnsupported)?;
    if plan.grow > 0 && !try_grow_rsrc_tail(pe, plan.grow) {
        return Err(GrowthUnsupported);
    }
    pe.copy_within(pkg_off + old_len..blob_end, pkg_off + grown.len());
    pe[pkg_off..pkg_off + grown.len()].copy_from_slice(&grown);
    write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        new_blob_len as u32,
        new_total as u32,
    );
    Ok(mapping)
}

pub(crate) fn insert_strings_at_ids_in_resource(
    pe: &mut Vec<u8>,
    language: &str,
    entries: &[(u16, &str)],
) -> Result<(), crate::hii::HiiError> {
    use crate::hii::HiiError;
    let (_, blob_off, blob_len) = hii_entry_locations(pe)
        .first()
        .copied()
        .ok_or(HiiError::StringPackageNotFound)?;
    let blob_end = blob_off
        .checked_add(blob_len)
        .ok_or(HiiError::StringPackageNotFound)?;
    let blob = pe
        .get(blob_off..blob_end)
        .ok_or(HiiError::StringPackageNotFound)?;
    let parsed = parse_package_list(blob).ok_or(HiiError::StringPackageNotFound)?;
    let idx = parsed
        .packages
        .iter()
        .position(|p| {
            p.kind == PACKAGE_STRINGS
                && is_string_package(p.bytes)
                && crate::hii::strings::parse_string_package(p.bytes)
                    .is_some_and(|sp| sp.language == language)
        })
        .ok_or(HiiError::StringPackageNotFound)?;
    let prefix: usize = parsed.packages[..idx].iter().map(|p| p.bytes.len()).sum();
    let old_len = parsed.packages[idx].bytes.len();
    let mut grown = blob[20 + prefix..20 + prefix + old_len].to_vec();
    insert_strings_at_ids(&mut grown, entries)?;
    let delta = grown.len() as i64 - old_len as i64;
    let sum: usize = parsed.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len as i64 + delta;
    let new_total = 20i64 + sum as i64 + delta + 4;
    if !(0..=u32::MAX as i64).contains(&new_blob_len) || !(0..=u32::MAX as i64).contains(&new_total)
    {
        tracing::debug!("string package resize overflows list lengths");
        return Err(HiiError::PeGrowthUnsupported);
    }
    let pkg_off = blob_off + 20 + prefix;
    let plan =
        plan_rsrc_blob_growth(pe, delta.max(0) as usize).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !try_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    pe.copy_within(pkg_off + old_len..blob_end, pkg_off + grown.len());
    pe[pkg_off..pkg_off + grown.len()].copy_from_slice(&grown);
    write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        new_blob_len as u32,
        new_total as u32,
    );
    Ok(())
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
            && declared_len_sane(&child.body)
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

pub(crate) fn pe_resource_has_string_package(pe: &[u8]) -> bool {
    let Some((_, blob_off, blob_len)) = hii_entry_locations(pe).first().copied() else {
        return false;
    };
    let Some(end) = blob_off.checked_add(blob_len) else {
        return false;
    };
    let Some(blob) = pe.get(blob_off..end) else {
        return false;
    };
    parse_package_list(blob).is_some_and(|list| {
        list.packages
            .iter()
            .any(|p| p.kind == PACKAGE_STRINGS && is_string_package(p.bytes))
    })
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

    fn make_sppkg(language: &str, sibt: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(PACKAGE_STRINGS);
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
        buf.extend_from_slice(sibt);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn pkg_len(body: &[u8]) -> usize {
        body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16
    }

    #[test]
    fn insert_at_id_cuts_skip2_block() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            SIBT_SKIP2,
            0x03,
            0x00,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![
                (1, "A".to_string()),
                (3, "X".to_string()),
                (5, "B".to_string()),
            ]
        );
        assert_eq!(pkg_len(&pkg), pkg.len());
    }

    #[test]
    fn insert_at_id_splits_skip_into_size_classes() {
        let sibt = [SIBT_STRING_SCSU, b'A', 0, SIBT_SKIP2, 0x50, 0x01, SIBT_END];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X")]).unwrap();
        let info = string_info_offset(&pkg);
        assert_eq!(&pkg[info + 3..info + 5], &[SIBT_SKIP1, 0x01]);
        assert_eq!(&pkg[info + 5..info + 8], &[SIBT_STRING_SCSU, b'X', 0]);
        let after = info + 8;
        assert_eq!(pkg[after], SIBT_SKIP2);
        assert_eq!(
            u16::from_le_bytes([pkg[after + 1], pkg[after + 2]]),
            0x150 - 2
        );
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(parsed.strings[0], (1, "A".to_string()));
        assert_eq!(parsed.strings[1], (3, "X".to_string()));
    }

    #[test]
    fn insert_at_id_rejects_occupied_id_without_mutation() {
        let mut pkg = make_string_package(&["A", "B"]);
        let snapshot = pkg.clone();
        let err = insert_strings_at_ids(&mut pkg, &[(2, "X")]).unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::IdOccupied(2)));
        assert_eq!(pkg, snapshot);
    }

    #[test]
    fn insert_at_id_appends_with_gap_before_end() {
        let mut pkg = make_string_package(&["A"]);
        insert_strings_at_ids(&mut pkg, &[(4, "Z")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![(1, "A".to_string()), (4, "Z".to_string())]
        );
    }

    #[test]
    fn insert_at_id_multiple_trailing_entries_after_gap_keep_exact_ids() {
        let mut pkg = make_string_package(&["A"]);
        insert_strings_at_ids(&mut pkg, &[(4, "X"), (7, "Y")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![
                (1, "A".to_string()),
                (4, "X".to_string()),
                (7, "Y".to_string()),
            ]
        );
    }

    #[test]
    fn insert_at_id_multiple_entries_in_one_walk() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            SIBT_SKIP2,
            0x04,
            0x00,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        insert_strings_at_ids(&mut pkg, &[(3, "X"), (5, "Y")]).unwrap();
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![
                (1, "A".to_string()),
                (3, "X".to_string()),
                (5, "Y".to_string()),
                (6, "B".to_string()),
            ]
        );
    }

    #[test]
    fn insert_at_id_errors_on_unknown_sibt_block_with_pending() {
        let sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            0x31,
            0xAA,
            0xBB,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &sibt);
        let snapshot = pkg.clone();
        let err = insert_strings_at_ids(&mut pkg, &[(3, "X")]).unwrap_err();
        assert!(matches!(
            err,
            crate::hii::HiiError::SibtBlockUnsupported(0x31)
        ));
        assert_eq!(pkg, snapshot);
    }

    #[test]
    fn insert_at_id_unknown_block_without_pending_is_output_neutral() {
        let verbatim_sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            0x31,
            0xAA,
            0xBB,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &verbatim_sibt);
        let snapshot = pkg.clone();
        insert_strings_at_ids(&mut pkg, &[]).unwrap();
        assert_eq!(pkg, snapshot);

        let skip_sibt = [
            SIBT_STRING_SCSU,
            b'A',
            0,
            SIBT_SKIP2,
            0x02,
            0x00,
            0x31,
            0xAA,
            0xBB,
            SIBT_STRING_SCSU,
            b'B',
            0,
            SIBT_END,
        ];
        let mut pkg = make_sppkg("en", &skip_sibt);
        let tail = pkg[pkg.len() - 7..].to_vec();
        insert_strings_at_ids(&mut pkg, &[(2, "X")]).unwrap();
        assert!(pkg.ends_with(&tail));
        let parsed = crate::hii::strings::parse_string_package(&pkg).unwrap();
        assert_eq!(
            parsed.strings,
            vec![(1, "A".to_string()), (2, "X".to_string())]
        );
    }

    #[test]
    fn insert_at_id_rejects_duplicate_request_ids() {
        let mut pkg = make_string_package(&["A"]);
        let err = insert_strings_at_ids(&mut pkg, &[(2, "X"), (2, "Y")]).unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::IdOccupied(2)));
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
    fn add_strings_skips_leaf_with_bogus_declared_length() {
        let pkg = make_string_package(&["first"]);
        let pkg_len = pkg.len();
        let bogus = [0x09u8, 0x00, 0x00, 0x04];
        let bad_file = mk_node(
            FfsType::File,
            vec![],
            vec![mk_node(FfsType::Section, bogus.to_vec(), vec![])],
        );
        let mut real_file = mk_node(
            FfsType::File,
            vec![],
            vec![mk_node(FfsType::Section, pkg, vec![])],
        );
        real_file.guid = Some(Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![bad_file, real_file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };

        let mapping = add_strings(&mut image, None, &["x".to_string()]).unwrap();
        assert_eq!(mapping["x"], 2);
        let bad = &image.root.children[0].children[0].children[0];
        assert_eq!(bad.action, Action::NoAction);
        assert_eq!(bad.body, bogus.to_vec());
        let real = &image.root.children[0].children[1].children[0];
        assert_eq!(real.action, Action::Rebuild);
        assert!(real.body.len() > pkg_len);
    }

    fn pkg(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            kind,
        ];
        b.extend_from_slice(payload);
        b
    }

    fn res_list(guid: &Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    fn le_u32(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
    }

    #[test]
    fn add_strings_to_resource_grows_mid_list() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(
            r_efi::hii::PACKAGE_FORMS,
            &[0x0Eu8, 0x17, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x11],
        );
        let string = make_string_package(&["first", "second"]);
        let blob = res_list(&g, &[&form, &string]);
        assert_eq!(blob.len() % 16, 0);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let len_before = pe.len();
        let file_align = le_u32(&pe, 0x7c) as usize;
        let (entry_off, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(&pe)[0];
        let strings = vec!["alpha".to_string(), "beta".to_string()];
        let delta = strings.iter().map(|s| s.len() + 2).sum::<usize>();
        let mapping = add_strings_to_resource(&mut pe, &strings).unwrap();
        assert_eq!(mapping["alpha"], 3);
        assert_eq!(mapping["beta"], 4);
        assert_eq!(pe.len() - len_before, delta.next_multiple_of(file_align));
        assert_eq!(pe.len(), (le_u32(&pe, 0x15c) + le_u32(&pe, 0x158)) as usize);
        assert_eq!(
            crate::hii::pe_resource::hii_resource_ranges(&pe),
            [(blob_off, blob_len + delta)]
        );
        assert_eq!(le_u32(&pe, entry_off + 4), (blob_len + delta) as u32);
        let new_blob = &pe[blob_off..blob_off + blob_len + delta];
        let parsed = crate::hii::package_list::parse_package_list(new_blob).unwrap();
        assert_eq!(
            parsed.packages.iter().map(|p| p.kind).collect::<Vec<_>>(),
            [r_efi::hii::PACKAGE_FORMS, PACKAGE_STRINGS]
        );
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        let new_string = parsed.packages[1].bytes;
        let stored =
            new_string[0] as usize | (new_string[1] as usize) << 8 | (new_string[2] as usize) << 16;
        assert_eq!(stored, new_string.len());
        assert_eq!(new_string.len(), string.len() + delta);
        assert_eq!(
            le_u32(new_blob, 16),
            (20 + form.len() + string.len() + delta + 4) as u32
        );
        assert_eq!(
            &new_blob[new_blob.len() - 4..],
            &[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]
        );
    }

    #[test]
    fn add_strings_to_resource_preserves_old_ids_and_shifts_following_packages() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = make_string_package(&["first", "second"]);
        let form2 = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xBB, 0xCC]);
        let blob = res_list(&g, &[&form, &string, &form2]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let (_, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(&pe)[0];
        let delta = "alpha".len() + 2;
        let mapping = add_strings_to_resource(&mut pe, &["alpha".to_string()]).unwrap();
        assert_eq!(mapping["alpha"], 3);
        let new_blob = &pe[blob_off..blob_off + blob_len + delta];
        let parsed = crate::hii::package_list::parse_package_list(new_blob).unwrap();
        assert_eq!(
            parsed.packages.iter().map(|p| p.kind).collect::<Vec<_>>(),
            [
                r_efi::hii::PACKAGE_FORMS,
                PACKAGE_STRINGS,
                r_efi::hii::PACKAGE_FORMS
            ]
        );
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        assert_eq!(parsed.packages[2].bytes, &form2[..]);
        let sp = crate::hii::strings::parse_string_package(parsed.packages[1].bytes).unwrap();
        assert_eq!(
            sp.strings,
            vec![
                (1, "first".to_string()),
                (2, "second".to_string()),
                (3, "alpha".to_string())
            ]
        );
        assert_eq!(
            &new_blob[new_blob.len() - 4..],
            &[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]
        );
    }

    #[test]
    fn add_strings_to_resource_without_hii_resource_is_not_found() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let string = make_string_package(&["first"]);
        let blob = res_list(&g, &[&string]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("REGISTRY", &blob);
        let snapshot = pe.clone();
        assert!(matches!(
            add_strings_to_resource(&mut pe, &["x".to_string()]),
            Err(AddStringsToResourceError::NotFound)
        ));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn add_strings_to_resource_without_string_package_is_not_found() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let blob = res_list(&g, &[&form]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let snapshot = pe.clone();
        assert!(matches!(
            add_strings_to_resource(&mut pe, &["x".to_string()]),
            Err(AddStringsToResourceError::NotFound)
        ));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn add_strings_to_resource_refuses_cert_blocked_growth() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = make_string_package(&["first"]);
        let blob = res_list(&g, &[&form, &string]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let snapshot = pe.clone();
        let strings = ["alpha", "beta", "gamma", "delta"]
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert!(matches!(
            add_strings_to_resource(&mut pe, &strings),
            Err(AddStringsToResourceError::GrowthUnsupported)
        ));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn add_strings_to_resource_consumes_slack_without_growing_file() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = make_string_package(&["first"]);
        let blob = res_list(&g, &[&form, &string]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        assert!(crate::hii::pe_resource::try_grow_rsrc_tail(&mut pe, 1));
        let len_after_pregrow = pe.len();
        let (entry_off, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(&pe)[0];
        let delta = "alpha".len() + 2;
        assert!(add_strings_to_resource(&mut pe, &["alpha".to_string()]).is_ok());
        assert_eq!(pe.len(), len_after_pregrow);
        assert_eq!(le_u32(&pe, entry_off + 4), (blob_len + delta) as u32);
        let new_blob = &pe[blob_off..blob_off + blob_len + delta];
        let parsed = crate::hii::package_list::parse_package_list(new_blob).unwrap();
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(
            le_u32(new_blob, 16),
            (20 + form.len() + string.len() + delta + 4) as u32
        );
        let sp = crate::hii::strings::parse_string_package(parsed.packages[1].bytes).unwrap();
        assert_eq!(sp.strings.len(), 2);
    }

    #[test]
    fn add_strings_to_resource_with_empty_slice_is_noop() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = make_string_package(&["first"]);
        let blob = res_list(&g, &[&form, &string]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let snapshot = pe.clone();
        let mapping = add_strings_to_resource(&mut pe, &[]).unwrap();
        assert!(mapping.is_empty());
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn add_strings_to_resource_refuses_overlay_tail() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = make_string_package(&["first"]);
        let blob = res_list(&g, &[&form, &string]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        pe.extend_from_slice(&[0xEEu8; 8]);
        let snapshot = pe.clone();
        assert!(matches!(
            add_strings_to_resource(&mut pe, &["x".to_string()]),
            Err(AddStringsToResourceError::GrowthUnsupported)
        ));
        assert_eq!(pe, snapshot);
    }

    fn two_language_blob() -> (Vec<u8>, Guid) {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let display_sibt = [SIBT_STRING_SCSU, b'H', b'i', 0, SIBT_END];
        let token_sibt = [SIBT_STRING_SCSU, b'P', b'R', b'C', 0, SIBT_END];
        let display = make_sppkg("en-US", &display_sibt);
        let token = make_sppkg("x-UEFI-AMI", &token_sibt);
        (res_list(&g, &[&form, &display, &token]), g)
    }

    fn resource_pkgs(pe: &[u8]) -> Vec<crate::hii::strings::ParsedStringPackage> {
        let (_, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(pe)[0];
        let list = crate::hii::package_list::parse_package_list(&pe[blob_off..blob_off + blob_len])
            .unwrap();
        list.packages
            .iter()
            .filter(|p| p.kind == PACKAGE_STRINGS)
            .filter_map(|p| crate::hii::strings::parse_string_package(p.bytes))
            .collect()
    }

    #[test]
    fn insert_in_resource_targets_language_package_only() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI", &[(3, "UPG0001")]).unwrap();
        let pkgs = resource_pkgs(&pe);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].language, "en-US");
        assert_eq!(pkgs[0].strings, vec![(1, "Hi".to_string())]);
        assert_eq!(pkgs[1].language, "x-UEFI-AMI");
        assert_eq!(
            pkgs[1].strings,
            vec![(1, "PRC".to_string()), (3, "UPG0001".to_string())]
        );
    }

    #[test]
    fn insert_in_resource_missing_language_errors() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let err = insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI-2", &[(3, "UPG0001")])
            .unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::StringPackageNotFound));
    }

    #[test]
    fn insert_in_resource_refuses_cert_blocked_growth() {
        let (blob, _) = two_language_blob();
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let snapshot = pe.clone();
        let err = insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI", &[(3, "UPG0001")])
            .unwrap_err();
        assert!(matches!(err, crate::hii::HiiError::PeGrowthUnsupported));
        assert_eq!(pe, snapshot);
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

    #[test]
    fn insert_strings_at_ids_in_resource_survives_shrinking_reencode() {
        let g = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();
        let mut sibt = vec![SIBT_STRING_SCSU, b'A', 0x00];
        for _ in 0..20 {
            sibt.extend_from_slice(&[SIBT_SKIP2, 0x01, 0x00]);
        }
        sibt.extend_from_slice(&[SIBT_STRING_SCSU, b'B', 0x00, SIBT_END]);
        let token = make_sppkg("x-UEFI-AMI", &sibt);
        let form = pkg(r_efi::hii::PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let blob = res_list(&g, &[&form, &token]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let pe_len = pe.len();
        let (entry_off, blob_off, blob_len) = crate::hii::pe_resource::hii_entry_locations(&pe)[0];

        insert_strings_at_ids_in_resource(&mut pe, "x-UEFI-AMI", &[(10, "X")]).unwrap();

        assert_eq!(pe.len(), pe_len, "shrink must keep PE file length");
        let new_blob_len = blob_len - 19;
        assert_eq!(
            crate::hii::pe_resource::hii_resource_ranges(&pe),
            [(blob_off, new_blob_len)]
        );
        assert_eq!(le_u32(&pe, entry_off + 4), new_blob_len as u32);
        let new_blob = &pe[blob_off..blob_off + new_blob_len];
        let parsed = crate::hii::package_list::parse_package_list(new_blob).unwrap();
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        let sp = crate::hii::strings::parse_string_package(parsed.packages[1].bytes).unwrap();
        assert_eq!(
            sp.strings,
            vec![
                (1, "A".to_string()),
                (10, "X".to_string()),
                (22, "B".to_string())
            ]
        );
        assert_eq!(
            le_u32(new_blob, 16),
            (20 + form.len() + parsed.packages[1].bytes.len() + 4) as u32
        );
        assert_eq!(
            &new_blob[new_blob.len() - 4..],
            &[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]
        );
    }
}
