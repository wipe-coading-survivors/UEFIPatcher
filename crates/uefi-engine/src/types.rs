pub use uguid::Guid;

pub fn guid_to_upper_string(g: &Guid) -> String {
    g.to_string().to_ascii_uppercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum FfsType {
    Root = 60,
    Capsule = 61,
    Image = 62,
    Region = 63,
    Padding = 64,
    Volume = 65,
    File = 66,
    Section = 67,
    FreeSpace = 68,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, num_enum::TryFromPrimitive)]
#[repr(u8)]
pub enum Action {
    NoAction = 50,
    Create = 51,
    Insert = 52,
    Replace = 53,
    Remove = 54,
    Rebuild = 55,
    Rebase = 56,
}

#[derive(Debug, Clone)]
pub struct VolumeParsingData {
    pub extended_header_guid: Option<Guid>,
    pub alignment: u32,
    pub ffs_version: u8,
    pub empty_byte: u8,
    pub revision: u8,
}

#[derive(Debug, Clone)]
pub struct FileParsingData {
    pub empty_byte: u8,
    pub guid: Guid,
}

#[derive(Debug, Clone)]
pub struct GuidedSectionParsingData {
    pub guid: Guid,
    pub dictionary_size: u32,
}

#[derive(Debug, Clone)]
pub struct CompressedSectionParsingData {
    pub uncompressed_size: u32,
    pub compression_type: u8,
    pub algorithm: u8,
    pub dictionary_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashRegionKind {
    Descriptor,
    Bios,
    Me,
    Gbe,
    Pdr,
    DevExpansion1,
    SecondaryBios,
    Microcode,
    Ec,
    DevExpansion2,
    Ie,
    Tgbe1,
    Tgbe2,
    Reserved1,
    Reserved2,
    Ptt,
}

impl FlashRegionKind {
    pub fn from_flreg_index(i: usize) -> Option<Self> {
        Some(match i {
            0 => Self::Descriptor,
            1 => Self::Bios,
            2 => Self::Me,
            3 => Self::Gbe,
            4 => Self::Pdr,
            5 => Self::DevExpansion1,
            6 => Self::SecondaryBios,
            7 => Self::Microcode,
            8 => Self::Ec,
            9 => Self::DevExpansion2,
            10 => Self::Ie,
            11 => Self::Tgbe1,
            12 => Self::Tgbe2,
            13 => Self::Reserved1,
            14 => Self::Reserved2,
            15 => Self::Ptt,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Descriptor => "Descriptor",
            Self::Bios => "BIOS",
            Self::Me => "ME",
            Self::Gbe => "GbE",
            Self::Pdr => "PDR",
            Self::DevExpansion1 => "Dev Expansion 1",
            Self::SecondaryBios => "Secondary BIOS",
            Self::Microcode => "Microcode",
            Self::Ec => "EC",
            Self::DevExpansion2 => "Dev Expansion 2",
            Self::Ie => "IE",
            Self::Tgbe1 => "10GbE 1",
            Self::Tgbe2 => "10GbE 2",
            Self::Reserved1 => "Reserved 1",
            Self::Reserved2 => "Reserved 2",
            Self::Ptt => "PTT",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegionParsingData {
    pub kind: FlashRegionKind,
}

#[derive(Debug, Clone)]
pub enum ParsingData {
    None,
    Volume(VolumeParsingData),
    File(FileParsingData),
    GuidedSection(GuidedSectionParsingData),
    CompressedSection(CompressedSectionParsingData),
    Region(RegionParsingData),
}

#[derive(Debug, Clone)]
pub struct FfsNode {
    pub guid: Option<Guid>,
    pub node_type: FfsType,
    pub subtype: u8,
    pub offset: u32,
    pub header: Vec<u8>,
    pub body: Vec<u8>,
    pub tail: Vec<u8>,
    pub children: Vec<FfsNode>,
    pub action: Action,
    pub parsing_data: ParsingData,
    pub fixed: bool,
    pub compressed: bool,
    pub alignment_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode {
    Read,
    Write,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub image_id: String,
    pub session_id: String,
    pub root: FfsNode,
    pub mode: ImageMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Guid(Guid),
    Path(Vec<usize>),
    GuidSection {
        guid: Guid,
        section_type: u8,
        section_index: Option<usize>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn guid_display_uppercase() {
        let g = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let s = guid_to_upper_string(&g);
        assert_eq!(s, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
    }

    #[test]
    fn guid_from_str_roundtrip() {
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let s = guid_to_upper_string(&g);
        assert_eq!(s, "5C60F367-A505-419A-859E-2A4FF6CA6FE5");
    }

    #[test]
    fn guid_invalid() {
        assert!(Guid::try_parse("not-a-guid").is_err());
    }
}
