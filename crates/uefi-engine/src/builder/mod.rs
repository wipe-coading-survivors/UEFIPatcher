use self::align::{align4, align8, pad_to};
use crate::ffs::*;
use crate::types::*;
use thiserror::Error;

pub mod align;

pub const FFS_ATTRIB_CHECKSUM: u8 = 0x40;

#[derive(Debug, Error)]
pub enum BuilderError {
    #[error("size mismatch: content exceeds container capacity")]
    SizeMismatch,
    #[error("checksum failed")]
    ChecksumFailed,
}

#[tracing::instrument(level = "info", skip_all, err)]
pub fn build_image(image: &Image) -> Result<Vec<u8>, BuilderError> {
    let mut out = vec![];
    build_node(&image.root, &mut out)?;
    tracing::info!(size = out.len(), "image built");
    Ok(out)
}

fn build_node(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    match node.node_type {
        FfsType::Image | FfsType::Capsule | FfsType::Region | FfsType::Root => {
            for child in &node.children {
                build_node(child, out)?;
            }
        }
        FfsType::Volume => build_volume(node, out)?,
        FfsType::File => build_file(node, out)?,
        FfsType::Section => build_section(node, out)?,
        FfsType::Padding | FfsType::FreeSpace => out.extend_from_slice(&node.body),
    }
    Ok(())
}

fn build_volume(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove {
        return Ok(());
    }
    if node.action == Action::NoAction {
        out.extend_from_slice(&node.header);
        out.extend_from_slice(&node.body);
        out.extend_from_slice(&node.tail);
        return Ok(());
    }
    let vol_start = out.len();
    out.extend_from_slice(&node.header);
    let empty_byte = match &node.parsing_data {
        ParsingData::Volume(vd) => vd.empty_byte,
        _ => 0xFF,
    };
    for child in &node.children {
        if child.action == Action::Remove {
            continue;
        }
        let target = vol_start + align8(out.len() - vol_start);
        pad_to(out, target, empty_byte);
        build_node(child, out)?;
    }
    let old_total = vol_start + node.header.len() + node.body.len();
    if out.len() > old_total {
        return Err(BuilderError::SizeMismatch);
    }
    pad_to(out, old_total, empty_byte);
    out.extend_from_slice(&node.tail);
    Ok(())
}

fn build_file(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove {
        return Ok(());
    }
    if node.action == Action::NoAction {
        out.extend_from_slice(&node.header);
        out.extend_from_slice(&node.body);
        out.extend_from_slice(&node.tail);
        return Ok(());
    }
    let mut header = node.header.clone();
    let mut body = if node.children.is_empty() {
        node.body.clone()
    } else {
        Vec::new()
    };
    for child in &node.children {
        build_node(child, &mut body)?;
        let target = align4(body.len());
        pad_to(&mut body, target, 0x00);
    }
    let tail = node.tail.clone();
    let total = header.len() + body.len() + tail.len();
    set_ffs_size(&mut header, total);
    recompute_ffs_checksums(&mut header, &body);
    out.extend_from_slice(&header);
    out.extend_from_slice(&body);
    out.extend_from_slice(&tail);
    Ok(())
}

fn build_section(node: &FfsNode, out: &mut Vec<u8>) -> Result<(), BuilderError> {
    if node.action == Action::Remove {
        return Ok(());
    }
    if node.action == Action::NoAction || is_compressed_or_guided(node) {
        out.extend_from_slice(&node.header);
        out.extend_from_slice(&node.body);
        out.extend_from_slice(&node.tail);
        return Ok(());
    }
    let mut body = if node.children.is_empty() {
        node.body.clone()
    } else {
        Vec::new()
    };
    for child in &node.children {
        build_node(child, &mut body)?;
        let target = align4(body.len());
        pad_to(&mut body, target, 0x00);
    }
    let mut header = node.header.clone();
    let total = header.len() + body.len();
    set_section_size(&mut header, total);
    out.extend_from_slice(&header);
    out.extend_from_slice(&body);
    Ok(())
}

fn is_compressed_or_guided(node: &FfsNode) -> bool {
    node.subtype == EFI_SECTION_COMPRESSION || node.subtype == EFI_SECTION_GUID_DEFINED
}

fn set_ffs_size(header: &mut [u8], total: usize) {
    if header.len() >= 32 {
        header[24..32].copy_from_slice(&(total as u64).to_le_bytes());
        header[20] = 0xFF;
        header[21] = 0xFF;
        header[22] = 0xFF;
    } else {
        let sb = size_to_uint24(total as u32);
        header[20] = sb[0];
        header[21] = sb[1];
        header[22] = sb[2];
    }
}

fn set_section_size(header: &mut [u8], total: usize) {
    if is_large_section(header) {
        let sb = (total as u32).to_le_bytes();
        header[4] = sb[0];
        header[5] = sb[1];
        header[6] = sb[2];
        header[7] = sb[3];
    } else {
        let sb = size_to_uint24(total as u32);
        header[0] = sb[0];
        header[1] = sb[1];
        header[2] = sb[2];
    }
}

fn recompute_ffs_checksums(header: &mut [u8], body: &[u8]) {
    if header.len() < 24 {
        return;
    }
    let sum_all = header.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    let old_header_cs = header[16];
    let file_cs = header[17];
    let state = header[23];
    let without = sum_all
        .wrapping_sub(old_header_cs)
        .wrapping_sub(file_cs)
        .wrapping_sub(state);
    header[16] = 0u8.wrapping_sub(without);
    let attrs = header[19];
    if attrs & FFS_ATTRIB_CHECKSUM != 0 {
        header[17] = calculate_checksum8(body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffs::{EFI_FVH_SIGNATURE, EFI_SECTION_RAW};
    use crate::parser::image::parse_image;
    use crate::types::{
        Action, FfsNode, FfsType, Image, ImageMode, ParsingData, VolumeParsingData,
    };

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
    fn round_trip_volume() {
        let orig = make_image_with_volume();
        let img = parse_image(&orig, ImageMode::Read, "img1", "s1").unwrap();
        let rebuilt = build_image(&img).unwrap();
        assert_eq!(rebuilt, orig);
    }

    fn removed_section_image(remove: bool) -> Image {
        let section_action = if remove {
            Action::Remove
        } else {
            Action::Rebuild
        };
        let section = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: EFI_SECTION_RAW,
            offset: 0,
            header: vec![0u8; 4],
            body: vec![0xAAu8; 8],
            tail: vec![],
            children: vec![],
            action: section_action,
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
            header: vec![0u8; 24],
            body: vec![],
            tail: vec![],
            children: vec![section],
            action: Action::Rebuild,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let volume = FfsNode {
            guid: None,
            node_type: FfsType::Volume,
            subtype: 0,
            offset: 0,
            header: vec![0u8; 56],
            body: vec![0xFFu8; 128],
            tail: vec![],
            children: vec![file],
            action: Action::Rebuild,
            parsing_data: ParsingData::Volume(VolumeParsingData {
                extended_header_guid: None,
                alignment: 0,
                ffs_version: 2,
                empty_byte: 0xFF,
                revision: 2,
            }),
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
            children: vec![volume],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        Image {
            image_id: "img1".into(),
            session_id: "s1".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn build_section_skips_removed() {
        let img = removed_section_image(true);
        let out = build_image(&img).unwrap();
        assert!(
            !out.windows(8).any(|w| w.iter().all(|&b| b == 0xAA)),
            "removed section body must not appear in output"
        );
    }

    #[test]
    fn build_section_emits_rebuilt_when_not_removed() {
        let img = removed_section_image(false);
        let out = build_image(&img).unwrap();
        assert!(
            out.windows(8).any(|w| w.iter().all(|&b| b == 0xAA)),
            "rebuilt (non-removed) section body must appear in output"
        );
    }

    #[test]
    fn rebuild_file_recomputes_size_and_header_checksum() {
        let guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let mut header = vec![0u8; 24];
        header[0..16].copy_from_slice(&guid.to_bytes());
        header[18] = 0x01;
        header[19] = 0x00;
        let body = vec![0xAAu8; 16];
        let total = header.len() + body.len();
        set_ffs_size(&mut header, total);
        recompute_ffs_checksums(&mut header, &body);
        assert_eq!(
            uint24_to_u32([header[20], header[21], header[22]]),
            total as u32
        );
        let mut check = header.clone();
        check[16] = 0;
        let s = check.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        assert_eq!(s.wrapping_add(header[16]), 0);
    }

    #[test]
    fn rebuild_file_with_checksum_attribute_uses_body_checksum() {
        let guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let mut header = vec![0u8; 24];
        header[0..16].copy_from_slice(&guid.to_bytes());
        header[18] = 0x01;
        header[19] = FFS_ATTRIB_CHECKSUM;
        let body = vec![0x01u8, 0x02, 0x03];
        recompute_ffs_checksums(&mut header, &body);
        assert_eq!(header[17], calculate_checksum8(&body));
    }
}
