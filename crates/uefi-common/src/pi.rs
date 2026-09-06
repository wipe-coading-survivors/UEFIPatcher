use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum SectionType {
    Compression = 0x01,
    GuidDefined = 0x02,
    Disposable = 0x03,
    Pe32 = 0x10,
    Pic = 0x11,
    Te = 0x12,
    DxeDepex = 0x13,
    Version = 0x14,
    UserInterface = 0x15,
    Compatibility16 = 0x16,
    FirmwareVolumeImage = 0x17,
    FreeformSubtypeGuid = 0x18,
    Raw = 0x19,
    PeiDepex = 0x1B,
    MmDepex = 0x1C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive)]
#[repr(u8)]
pub enum FileType {
    All = 0x00,
    Raw = 0x01,
    Freeform = 0x02,
    SecurityCore = 0x03,
    PeiCore = 0x04,
    DxeCore = 0x05,
    Peim = 0x06,
    Driver = 0x07,
    CombinedPeimDriver = 0x08,
    Application = 0x09,
    Mm = 0x0A,
    FirmwareVolumeImage = 0x0B,
    CombinedMmDxe = 0x0C,
    MmCore = 0x0D,
    MmStandalone = 0x0E,
    MmCoreStandalone = 0x0F,
    Pad = 0xF0,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
    NotCompressed = 0,
    Standard = 1,
    Lzma = 2,
}

pub const EFI_FVH_SIGNATURE: u32 = 0x4856465F;
pub const EFI_FVB2_ERASE_POLARITY: u32 = 0x00000800;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_type_known_codes() {
        assert_eq!(SectionType::Pe32 as u8, 0x10);
        assert_eq!(SectionType::UserInterface as u8, 0x15);
        assert_eq!(SectionType::Raw as u8, 0x19);
        assert_eq!(SectionType::MmDepex as u8, 0x1C);
        assert_eq!(SectionType::try_from(0x10u8).unwrap(), SectionType::Pe32);
        assert_eq!(
            SectionType::try_from(0x15u8).unwrap(),
            SectionType::UserInterface
        );
    }

    #[test]
    fn section_type_unknown_returns_err() {
        assert!(SectionType::try_from(0xCCu8).is_err());
        assert!(SectionType::try_from(0x05u8).is_err());
    }

    #[test]
    fn file_type_pad_and_oem_range() {
        assert_eq!(FileType::Pad as u8, 0xF0);
        assert_eq!(FileType::try_from(0xF0u8).unwrap(), FileType::Pad);
        assert!(FileType::try_from(0xC0u8).is_err());
    }

    #[test]
    fn compression_type_values() {
        assert_eq!(CompressionType::NotCompressed as u8, 0);
        assert_eq!(CompressionType::Standard as u8, 1);
        assert_eq!(CompressionType::Lzma as u8, 2);
    }

    #[test]
    fn fv_signature_is_fvh_ascii_le() {
        let bytes = EFI_FVH_SIGNATURE.to_le_bytes();
        assert_eq!(&bytes[..], b"_FVH");
    }

    #[test]
    fn erase_polarity_bit_value() {
        assert_eq!(EFI_FVB2_ERASE_POLARITY, 0x00000800);
    }
}
