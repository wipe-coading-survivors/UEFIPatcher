use crate::types::{
    Action, FfsNode, FfsType, FlashRegionKind, FptParsingData, ParsingData, RegionParsingData,
};

pub const FLASH_DESCRIPTOR_SIGNATURE: u32 = 0x0FF0A55A;
pub const FLASH_DESCRIPTOR_SIZE: usize = 0x1000;
const DESCRIPTOR_VECTOR_SIZE: usize = 0x10;
const FLMAP0_OFFSET: usize = DESCRIPTOR_VECTOR_SIZE + 4;
const REGION_SECTION_ALIGN: usize = 0x10;
const REGION_GRANULARITY: usize = 0x1000;

pub const FPT_SIGNATURE: u32 = 0x5450_4624;
const FPT_MAX_ENTRIES: u32 = 0x100;
const FPT_HEADER_LEN: usize = 0x20;
const FPT_ENTRY_LEN: usize = 0x20;
const ME_ROM_BYPASS_VECTOR_SIZE: usize = 0x10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashRegion {
    pub kind: FlashRegionKind,
    pub offset: usize,
    pub size: usize,
}

/// Parses the Intel flash descriptor region table (FLMAP0/FLREG). The
/// signature follows the 16-byte reserved vector (0xFF on x86), and FLMAP0
/// follows the signature; section bases are absolute. Returns None when
/// `buf` has no descriptor signature at 0x10 or no BIOS region.
/// Ref: UEFITool-ai-fork/common/descriptor.h:22-26, ffsparser.cpp:303-334.
pub fn parse_flash_regions(buf: &[u8]) -> Option<Vec<FlashRegion>> {
    if buf.len() < FLASH_DESCRIPTOR_SIZE {
        return None;
    }
    if u32::from_le_bytes(
        buf[DESCRIPTOR_VECTOR_SIZE..DESCRIPTOR_VECTOR_SIZE + 4]
            .try_into()
            .unwrap(),
    ) != FLASH_DESCRIPTOR_SIGNATURE
    {
        return None;
    }
    let flmap0 = u32::from_le_bytes(buf[FLMAP0_OFFSET..FLMAP0_OFFSET + 4].try_into().unwrap());
    let region_base = ((flmap0 >> 16) & 0xFF) as usize * REGION_SECTION_ALIGN;
    if region_base + 64 > FLASH_DESCRIPTOR_SIZE {
        return None;
    }
    let mut out = vec![FlashRegion {
        kind: FlashRegionKind::Descriptor,
        offset: 0,
        size: FLASH_DESCRIPTOR_SIZE,
    }];
    for i in 1..=15usize {
        let base = u16::from_le_bytes(
            buf[region_base + i * 4..region_base + i * 4 + 2]
                .try_into()
                .unwrap(),
        );
        let limit = u16::from_le_bytes(
            buf[region_base + i * 4 + 2..region_base + i * 4 + 4]
                .try_into()
                .unwrap(),
        );
        if limit == 0 || (base == 0xFFFF && limit == 0xFFFF) {
            continue;
        }
        let Some(kind) = FlashRegionKind::from_flreg_index(i) else {
            continue;
        };
        let offset = base as usize * REGION_GRANULARITY;
        let size = (limit as usize + 1 - base as usize) * REGION_GRANULARITY;
        if offset + size > buf.len() {
            tracing::warn!(kind = kind.label(), "region outside image, skipped");
            continue;
        }
        out.push(FlashRegion { kind, offset, size });
    }
    if !out.iter().any(|r| r.kind == FlashRegionKind::Bios) {
        tracing::warn!("descriptor has no BIOS region, ignoring descriptor");
        return None;
    }
    Some(out)
}

/// Makes a read-only Region node covering `buf[offset..offset+size]` verbatim.
pub fn make_region_node(buf: &[u8], kind: FlashRegionKind, offset: usize, size: usize) -> FfsNode {
    FfsNode {
        guid: None,
        node_type: FfsType::Region,
        subtype: 0,
        offset: offset as u32,
        header: vec![],
        body: buf[offset..offset + size].to_vec(),
        tail: vec![],
        children: vec![],
        action: Action::NoAction,
        parsing_data: ParsingData::Region(RegionParsingData { kind }),
        fixed: true,
        compressed: false,
        alignment_bytes: vec![],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FptPartition {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

/// Parses the ME $FPT partition table at offset 0 or behind the ROM bypass
/// vector. Entries always start after the fixed 0x20-byte header — the
/// HeaderLength byte is informational and counts the bypass vector on real
/// images (0x30 = 0x10 + 0x20), so it must not shift the walk. Returns None
/// on missing signature, NumEntries >= 0x100 or entries beyond the buffer.
/// Ref: UEFITool-ai-fork/common/meparser.cpp:151-176, me.h:38-87.
pub fn parse_fpt(body: &[u8]) -> Option<Vec<FptPartition>> {
    let base =
        if body.len() >= 4 && u32::from_le_bytes(body[0..4].try_into().unwrap()) == FPT_SIGNATURE {
            0
        } else if body.len() >= ME_ROM_BYPASS_VECTOR_SIZE + 4
            && u32::from_le_bytes(
                body[ME_ROM_BYPASS_VECTOR_SIZE..ME_ROM_BYPASS_VECTOR_SIZE + 4]
                    .try_into()
                    .unwrap(),
            ) == FPT_SIGNATURE
        {
            ME_ROM_BYPASS_VECTOR_SIZE
        } else {
            return None;
        };
    let hdr = &body[base..];
    let num = u32::from_le_bytes(hdr[4..8].try_into().unwrap());
    if num >= FPT_MAX_ENTRIES || base + FPT_HEADER_LEN + num as usize * FPT_ENTRY_LEN > body.len() {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..num as usize {
        let e = &hdr[FPT_HEADER_LEN + i * FPT_ENTRY_LEN..];
        let name: String = e[0..4]
            .iter()
            .map(|&b| b as char)
            .filter(|c| c.is_ascii_graphic())
            .collect();
        let offset = u32::from_le_bytes(e[8..12].try_into().unwrap());
        let size = u32::from_le_bytes(e[12..16].try_into().unwrap());
        out.push(FptPartition { name, offset, size });
    }
    Some(out)
}

/// Makes fixed read-only Region children for ME $FPT partitions; partition
/// bodies are slices of the parent region body and are never serialized by
/// the builder. Returns empty when the region has no valid $FPT.
pub fn fpt_children(body: &[u8], region_offset: usize, region_size: usize) -> Vec<FfsNode> {
    let Some(parts) = parse_fpt(body) else {
        tracing::warn!(
            region_offset,
            "ME region has no $FPT, no partition children"
        );
        return vec![];
    };
    parts
        .iter()
        .filter(|p| p.size > 0 && (p.offset as usize) < region_size)
        .map(|p| {
            let end = (p.offset as usize + p.size as usize).min(region_size);
            FfsNode {
                guid: None,
                node_type: FfsType::Region,
                subtype: 0,
                offset: (region_offset + p.offset as usize) as u32,
                header: vec![],
                body: body[p.offset as usize..end].to_vec(),
                tail: vec![],
                children: vec![],
                action: Action::NoAction,
                parsing_data: ParsingData::FptPartition(FptParsingData {
                    name: p.name.clone(),
                }),
                fixed: true,
                compressed: false,
                alignment_bytes: vec![],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::image::parse_image;
    use crate::types::ImageMode;

    fn descriptor_image(region_specs: &[(usize, u16, u16)], total: usize) -> Vec<u8> {
        let mut buf = vec![0xFFu8; total];
        buf[0x10..0x14].copy_from_slice(&super::FLASH_DESCRIPTOR_SIGNATURE.to_le_bytes());
        buf[FLMAP0_OFFSET..FLMAP0_OFFSET + 4].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        for &(i, base, limit) in region_specs {
            let at = 0x400 + i * 4;
            buf[at..at + 2].copy_from_slice(&base.to_le_bytes());
            buf[at + 2..at + 4].copy_from_slice(&limit.to_le_bytes());
        }
        buf
    }

    #[test]
    fn signature_behind_reserved_vector_is_canonical() {
        let buf = descriptor_image(&[(1, 1, 15)], 0x10000);
        assert!(parse_flash_regions(&buf).is_some());
        let mut shifted = buf.clone();
        shifted[0x10..0x14].fill(0xFF);
        assert!(parse_flash_regions(&shifted).is_none(), "no sig at 0x10");
    }

    #[test]
    fn parse_flash_regions_layout() {
        let buf = descriptor_image(&[(1, 4, 7), (2, 1, 3), (3, 8, 8)], 0x10000);
        let rs = parse_flash_regions(&buf).unwrap();
        assert_eq!(rs.len(), 4);
        assert_eq!(
            rs[0],
            FlashRegion {
                kind: FlashRegionKind::Descriptor,
                offset: 0,
                size: 0x1000
            }
        );
        assert_eq!(
            rs[1],
            FlashRegion {
                kind: FlashRegionKind::Bios,
                offset: 0x4000,
                size: 0x4000
            }
        );
        assert_eq!(
            rs[2],
            FlashRegion {
                kind: FlashRegionKind::Me,
                offset: 0x1000,
                size: 0x3000
            }
        );
        assert_eq!(
            rs[3],
            FlashRegion {
                kind: FlashRegionKind::Gbe,
                offset: 0x8000,
                size: 0x1000
            }
        );
    }

    #[test]
    fn no_signature_returns_none() {
        assert!(parse_flash_regions(&[0u8; 0x2000]).is_none());
        assert!(parse_flash_regions(&[0xFFu8; 0x2000]).is_none());
    }

    #[test]
    fn region_outside_image_skipped() {
        let buf = descriptor_image(&[(1, 0, 3), (2, 0xF000, 0xFFFF)], 0x10000);
        let rs = parse_flash_regions(&buf).unwrap();
        assert!(rs.iter().all(|r| r.kind != FlashRegionKind::Me));
    }

    fn me_body_with_fpt() -> Vec<u8> {
        let mut body = vec![0u8; 0x2000];
        body[0..4].copy_from_slice(&0x5450_4624u32.to_le_bytes());
        body[4..8].copy_from_slice(&2u32.to_le_bytes());
        body[8] = 0x10;
        body[10] = 0x20;
        body[0x20..0x24].copy_from_slice(b"FTPR");
        body[0x28..0x2C].copy_from_slice(&0x0000u32.to_le_bytes());
        body[0x2C..0x30].copy_from_slice(&0x0800u32.to_le_bytes());
        body[0x40..0x44].copy_from_slice(b"NFTP");
        body[0x48..0x4C].copy_from_slice(&0x1000u32.to_le_bytes());
        body[0x4C..0x50].copy_from_slice(&0x0400u32.to_le_bytes());
        body
    }

    #[test]
    fn parse_fpt_entries() {
        let parts = parse_fpt(&me_body_with_fpt()).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "FTPR");
        assert_eq!(parts[0].offset, 0);
        assert_eq!(parts[0].size, 0x800);
        assert_eq!(parts[1].name, "NFTP");
        assert_eq!(parts[1].offset, 0x1000);
    }

    #[test]
    fn parse_fpt_header_length_byte_is_informational() {
        let mut body = me_body_with_fpt();
        body[10] = 0x30;
        let parts = parse_fpt(&body).unwrap();
        assert_eq!(parts[0].name, "FTPR", "entries stay at 0x20");
        assert_eq!(parts[1].name, "NFTP");
    }

    #[test]
    fn parse_fpt_bypass_vector_offset() {
        let mut body = me_body_with_fpt();
        body.copy_within(0..0x40, 0x10);
        body[0..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_fpt(&body).is_some());
    }

    #[test]
    fn parse_fpt_graceful_on_garbage() {
        assert!(parse_fpt(&[0xFFu8; 0x2000]).is_none());
        let mut body = me_body_with_fpt();
        body[4..8].copy_from_slice(&1000u32.to_le_bytes());
        assert!(parse_fpt(&body).is_none());
    }

    #[test]
    fn me_region_gets_fpt_children() {
        let children = fpt_children(&me_body_with_fpt(), 0, 0x2000);
        assert_eq!(children.len(), 2);
        assert!(children.iter().all(|c| c.node_type == FfsType::Region));
        assert!(
            children
                .iter()
                .all(|c| matches!(&c.parsing_data, ParsingData::FptPartition(_)))
        );
    }

    #[test]
    fn fpt_partition_inside_me_immutable() {
        let mut buf = descriptor_image(&[(1, 4, 7), (2, 1, 3)], 0x10000);
        let me = me_body_with_fpt();
        buf[0x1000..0x1000 + me.len()].copy_from_slice(&me);
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
        assert!(
            !img.root.children[me_idx].children.is_empty(),
            "ME has $FPT children"
        );
        let inside = format!("{me_idx}/0");
        assert!(matches!(
            crate::ops::remove(
                &mut img.root,
                &crate::parser::target::parse_target(&inside).unwrap()
            ),
            Err(crate::ops::OpsError::ImmutableRegion)
        ));
    }
}
