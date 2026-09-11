use crate::types::{Action, FfsNode, FfsType, FlashRegionKind, ParsingData, RegionParsingData};

pub const FLASH_DESCRIPTOR_SIGNATURE: u32 = 0x0FF0A55A;
pub const FLASH_DESCRIPTOR_SIZE: usize = 0x1000;
const FLMAP0_OFFSET: usize = 0x10;
const REGION_SECTION_ALIGN: usize = 0x10;
const REGION_GRANULARITY: usize = 0x1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlashRegion {
    pub kind: FlashRegionKind,
    pub offset: usize,
    pub size: usize,
}

/// Parses the Intel flash descriptor region table (FLMAP0/FLREG).
/// Returns None when `buf` has no descriptor signature at offset 0 or no BIOS region.
pub fn parse_flash_regions(buf: &[u8]) -> Option<Vec<FlashRegion>> {
    if buf.len() < FLASH_DESCRIPTOR_SIZE {
        return None;
    }
    if u32::from_le_bytes(buf[0..4].try_into().unwrap()) != FLASH_DESCRIPTOR_SIGNATURE {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor_image(region_specs: &[(usize, u16, u16)], total: usize) -> Vec<u8> {
        let mut buf = vec![0xFFu8; total];
        buf[0..4].copy_from_slice(&super::FLASH_DESCRIPTOR_SIGNATURE.to_le_bytes());
        buf[FLMAP0_OFFSET..FLMAP0_OFFSET + 4].copy_from_slice(&0x0040_0000u32.to_le_bytes());
        for &(i, base, limit) in region_specs {
            let at = 0x400 + i * 4;
            buf[at..at + 2].copy_from_slice(&base.to_le_bytes());
            buf[at + 2..at + 4].copy_from_slice(&limit.to_le_bytes());
        }
        buf
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
}
