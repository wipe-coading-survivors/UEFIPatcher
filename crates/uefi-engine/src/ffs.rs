use crate::types::Guid;

pub const EFI_SECTION_COMPRESSION: u8 = 0x01;
pub const EFI_SECTION_GUID_DEFINED: u8 = 0x02;
pub const EFI_SECTION_PE32: u8 = 0x10;
pub const EFI_SECTION_TE: u8 = 0x12;
pub const EFI_SECTION_UI: u8 = 0x15;
pub const EFI_SECTION_VERSION: u8 = 0x16;
pub const EFI_SECTION_FV_IMAGE: u8 = 0x17;
pub const EFI_SECTION_FREEFORM_SUBTYPE_GUID: u8 = 0x18;
pub const EFI_SECTION_RAW: u8 = 0x19;
pub const EFI_SECTION_DEPEX: u8 = 0x1C;

pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;
pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;

pub fn tiano_guid() -> Guid {
    Guid::try_parse("a31280ad-0411-42b8-aa09-c484a2906fdc").unwrap()
}
pub fn lzma_guid() -> Guid {
    Guid::try_parse("ee4e5ace-8c72-4ae3-8bfc-e1f3c1a08c14").unwrap()
}
pub fn lzmaf86_guid() -> Guid {
    Guid::try_parse("d42ae6bd-1352-4b12-95a0-c1d41df29e0c").unwrap()
}
pub fn crc32_guid() -> Guid {
    Guid::try_parse("fcdefeee-3598-4908-b337-78f59f8f1a8e").unwrap()
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
}
