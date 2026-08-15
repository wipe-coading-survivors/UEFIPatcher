use crate::types::Guid;

pub use uefi_common::pi::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};

pub const EFI_SECTION_COMPRESSION: u8 = uefi_common::pi::SectionType::Compression as u8;
pub const EFI_SECTION_GUID_DEFINED: u8 = uefi_common::pi::SectionType::GuidDefined as u8;
pub const EFI_SECTION_PE32: u8 = uefi_common::pi::SectionType::Pe32 as u8;
pub const EFI_SECTION_TE: u8 = uefi_common::pi::SectionType::Te as u8;
pub const EFI_SECTION_UI: u8 = uefi_common::pi::SectionType::UserInterface as u8;
pub const EFI_SECTION_VERSION: u8 = uefi_common::pi::SectionType::Version as u8;
pub const EFI_SECTION_FV_IMAGE: u8 = uefi_common::pi::SectionType::FirmwareVolumeImage as u8;
pub const EFI_SECTION_FREEFORM_SUBTYPE_GUID: u8 =
    uefi_common::pi::SectionType::FreeformSubtypeGuid as u8;
pub const EFI_SECTION_RAW: u8 = uefi_common::pi::SectionType::Raw as u8;
pub const EFI_SECTION_DEPEX: u8 = uefi_common::pi::SectionType::MmDepex as u8;

pub const EFI_FV_FILETYPE_RAW: u8 = uefi_common::pi::FileType::Raw as u8;

pub fn tiano_guid() -> Guid {
    Guid::try_parse("a31280ad-481e-41b6-95e8-127f4c984779").unwrap()
}
pub fn lzma_guid() -> Guid {
    Guid::try_parse("ee4e5898-3914-4259-9d6e-dc7bd79403cf").unwrap()
}
pub fn lzma_hp_guid() -> Guid {
    Guid::try_parse("0ed85e23-f253-413f-a03c-901987b04397").unwrap()
}
pub fn lzma_ms_guid() -> Guid {
    Guid::try_parse("bd9921ea-ed91-404a-8b2f-b4d724747c8c").unwrap()
}
pub fn lzmaf86_guid() -> Guid {
    Guid::try_parse("d42ae6bd-1352-4bfb-909a-ca72a6eae889").unwrap()
}
pub fn crc32_guid() -> Guid {
    Guid::try_parse("fc1bcdb0-7d31-49aa-936a-a4600d9dd083").unwrap()
}

pub fn is_lzma_guid(g: &Guid) -> bool {
    let b = g.to_bytes();
    b == lzma_guid().to_bytes()
        || b == lzma_hp_guid().to_bytes()
        || b == lzma_ms_guid().to_bytes()
        || b == lzmaf86_guid().to_bytes()
}

pub fn is_recompressable_lzma_guid(g: &Guid) -> bool {
    let b = g.to_bytes();
    b == lzma_guid().to_bytes() || b == lzma_hp_guid().to_bytes() || b == lzma_ms_guid().to_bytes()
}

pub fn is_tiano_guid(g: &Guid) -> bool {
    g.to_bytes() == tiano_guid().to_bytes()
}

pub fn calculate_checksum8(data: &[u8]) -> u8 {
    let sum = data.iter().fold(0u8, |acc, &byte| acc.wrapping_add(byte));
    0u8.wrapping_sub(sum)
}

pub fn calculate_checksum16(data: &[u8]) -> u16 {
    let mut sum: u16 = 0;
    for &byte in data {
        sum = sum.wrapping_add(byte as u16);
    }
    0u16.wrapping_sub(sum)
}

pub fn size_to_uint24(size: u32) -> [u8; 3] {
    [size as u8, (size >> 8) as u8, (size >> 16) as u8]
}

pub fn uint24_to_u32(b: [u8; 3]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}

pub fn is_large_section(header: &[u8]) -> bool {
    header.len() >= 8 && header[0] == 0xFF && header[1] == 0xFF && header[2] == 0xFF
}

pub fn is_large_ffs(header: &[u8]) -> bool {
    header.len() >= 32 && header[20] == 0xFF && header[21] == 0xFF && header[22] == 0xFF
}

pub fn section_size(header: &[u8]) -> u32 {
    if is_large_section(header) {
        u32::from_le_bytes([header[4], header[5], header[6], header[7]])
    } else {
        uint24_to_u32([header[0], header[1], header[2]])
    }
}

pub fn ffs_file_size(header: &[u8]) -> u32 {
    if is_large_ffs(header) {
        if header.len() >= 32 {
            u32::from_le_bytes([header[24], header[25], header[26], header[27]])
        } else {
            0
        }
    } else {
        uint24_to_u32([header[20], header[21], header[22]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guid_to_upper_string;

    #[test]
    fn checksum8_known_vector() {
        let mut data = [0x01, 0x02, 0x03, 0x04, 0x00];
        let checksum = calculate_checksum8(&data);
        assert_eq!(checksum, 0xF6);
        if let Some(last) = data.last_mut() {
            *last = checksum;
        }
        assert_eq!(calculate_checksum8(&data), 0);
    }

    #[test]
    fn checksum16_known_vector() {
        let data = [0x01, 0x00, 0x02, 0x00];
        assert_eq!(calculate_checksum16(&data), 0xFFFD);
    }

    #[test]
    fn uint24_roundtrip() {
        let s = 0x123456u32;
        let b = size_to_uint24(s);
        assert_eq!(uint24_to_u32(b), s);
    }

    #[test]
    fn large_section_threshold() {
        let small_hdr = vec![0x00, 0x00, 0x00, 0x00];
        assert!(!is_large_section(&small_hdr));
        let mut large_hdr = vec![0xFF, 0xFF, 0xFF, 0x00];
        large_hdr.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        assert!(is_large_section(&large_hdr));
    }

    #[test]
    fn ffs_file_size_parsing() {
        let mut small_ffs = vec![0; 24];
        small_ffs[20..23].copy_from_slice(&size_to_uint24(0x000150));
        assert!(!is_large_ffs(&small_ffs));
        assert_eq!(ffs_file_size(&small_ffs), 0x000150);

        let mut large_ffs = vec![0; 32];
        large_ffs[20] = 0xFF;
        large_ffs[21] = 0xFF;
        large_ffs[22] = 0xFF;
        let large_size: u64 = 0x02000000;
        large_ffs[24..32].copy_from_slice(&large_size.to_le_bytes());
        assert!(is_large_ffs(&large_ffs));
        assert_eq!(ffs_file_size(&large_ffs), 0x02000000);
    }

    #[test]
    fn well_known_guids_match_edk2() {
        assert_eq!(
            guid_to_upper_string(&lzma_guid()),
            "EE4E5898-3914-4259-9D6E-DC7BD79403CF"
        );
        assert_eq!(
            guid_to_upper_string(&tiano_guid()),
            "A31280AD-481E-41B6-95E8-127F4C984779"
        );
        assert_eq!(
            guid_to_upper_string(&lzmaf86_guid()),
            "D42AE6BD-1352-4BFB-909A-CA72A6EAE889"
        );
        assert_eq!(
            guid_to_upper_string(&crc32_guid()),
            "FC1BCDB0-7D31-49AA-936A-A4600D9DD083"
        );
        assert!(is_lzma_guid(&lzma_guid()));
        assert!(is_lzma_guid(&lzma_hp_guid()));
        assert!(is_lzma_guid(&lzma_ms_guid()));
        assert!(is_lzma_guid(&lzmaf86_guid()));
        assert!(!is_lzma_guid(&tiano_guid()));
        assert!(is_tiano_guid(&tiano_guid()));
    }

    #[test]
    fn is_recompressable_lzma_guid_excludes_f86_and_tiano() {
        assert!(is_recompressable_lzma_guid(&lzma_guid()));
        assert!(is_recompressable_lzma_guid(&lzma_hp_guid()));
        assert!(is_recompressable_lzma_guid(&lzma_ms_guid()));
        assert!(!is_recompressable_lzma_guid(&lzmaf86_guid()));
        assert!(!is_recompressable_lzma_guid(&tiano_guid()));
    }
}
