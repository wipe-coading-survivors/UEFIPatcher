use crate::pi::{FileType, SectionType};

pub fn file_type_name(t: FileType) -> &'static str {
    match t {
        FileType::All => "All",
        FileType::Raw => "Raw",
        FileType::Freeform => "Freeform",
        FileType::SecurityCore => "SEC core",
        FileType::PeiCore => "PEI core",
        FileType::DxeCore => "DXE core",
        FileType::Peim => "PEI module",
        FileType::Driver => "DXE driver",
        FileType::CombinedPeimDriver => "Combined PEI/DXE",
        FileType::Application => "Application",
        FileType::Mm => "SMM module",
        FileType::FirmwareVolumeImage => "Volume image",
        FileType::CombinedMmDxe => "Combined SMM/DXE",
        FileType::MmCore => "SMM core",
        FileType::MmStandalone => "MM standalone module",
        FileType::MmCoreStandalone => "MM standalone core",
        FileType::Pad => "Pad",
    }
}

pub fn section_type_name(t: SectionType) -> &'static str {
    match t {
        SectionType::Compression => "Compressed",
        SectionType::GuidDefined => "GUID defined",
        SectionType::Disposable => "Disposable",
        SectionType::Pe32 => "PE32 image",
        SectionType::Pic => "PIC image",
        SectionType::Te => "TE image",
        SectionType::DxeDepex => "DXE dependency",
        SectionType::Version => "Version",
        SectionType::UserInterface => "UI",
        SectionType::Compatibility16 => "16-bit image",
        SectionType::FirmwareVolumeImage => "Volume image",
        SectionType::FreeformSubtypeGuid => "Freeform subtype GUID",
        SectionType::Raw => "Raw",
        SectionType::PeiDepex => "PEI dependency",
        SectionType::MmDepex => "MM dependency",
    }
}

pub fn node_type_name(code: u32) -> &'static str {
    match code {
        60 => "Root",
        61 => "Capsule",
        62 => "Image",
        63 => "Region",
        64 => "Padding",
        65 => "Volume",
        66 => "File",
        67 => "Section",
        68 => "FreeSpace",
        _ => "Unknown",
    }
}

pub fn file_type_name_or_raw(code: u8) -> String {
    match FileType::try_from(code) {
        Ok(t) => file_type_name(t).to_string(),
        Err(_) => format!("Unknown {code:02X}h"),
    }
}

pub fn section_type_name_or_raw(code: u8) -> String {
    match SectionType::try_from(code) {
        Ok(t) => section_type_name(t).to_string(),
        Err(_) => format!("Unknown {code:02X}h"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_type_known_names() {
        assert_eq!(file_type_name(FileType::Raw), "Raw");
        assert_eq!(file_type_name(FileType::Driver), "DXE driver");
        assert_eq!(file_type_name(FileType::Pad), "Pad");
        assert_eq!(file_type_name(FileType::Application), "Application");
    }

    #[test]
    fn section_type_known_names() {
        assert_eq!(section_type_name(SectionType::Pe32), "PE32 image");
        assert_eq!(section_type_name(SectionType::UserInterface), "UI");
        assert_eq!(section_type_name(SectionType::GuidDefined), "GUID defined");
        assert_eq!(section_type_name(SectionType::Raw), "Raw");
    }

    #[test]
    fn node_type_codes_match_ffstype() {
        assert_eq!(node_type_name(62), "Image");
        assert_eq!(node_type_name(65), "Volume");
        assert_eq!(node_type_name(66), "File");
        assert_eq!(node_type_name(67), "Section");
        assert_eq!(node_type_name(99), "Unknown");
    }

    #[test]
    fn fallback_for_oem_and_unknown_codes() {
        assert_eq!(file_type_name_or_raw(0xC0), "Unknown C0h");
        assert_eq!(section_type_name_or_raw(0xCC), "Unknown CCh");
        assert_eq!(file_type_name_or_raw(0x07), "DXE driver");
        assert_eq!(section_type_name_or_raw(0x15), "UI");
    }
}
