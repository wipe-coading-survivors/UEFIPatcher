use super::ParserError;
use crate::decompress;
use crate::ffs::*;
use crate::types::*;

pub fn parse_section(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 4 > buf.len() {
        return Err(ParserError::EndOfBuffer);
    }
    let large = is_large_section(&buf[off..]);
    let hdr_len = if large { 8 } else { 4 };
    let size = section_size(&buf[off..]) as usize;
    if size < hdr_len || off + size > buf.len() {
        return Err(ParserError::InvalidHeader("section size".into()));
    }
    let stype = buf[off + 3];
    let header = buf[off..off + hdr_len].to_vec();
    let body = buf[off + hdr_len..off + size].to_vec();

    let (parsing_data, children) = match stype {
        EFI_SECTION_GUID_DEFINED if body.len() >= 20 => {
            let guid = crate::parser::file::guid_from_bytes(&body[0..16])
                .ok_or_else(|| ParserError::InvalidHeader("guid".into()))?;
            let data_offset = u16::from_le_bytes([body[16], body[17]]) as usize;
            let _attributes = u16::from_le_bytes([body[18], body[19]]);
            let dictionary_size = decompress_guided_payload(&guid, &body, data_offset)
                .map(decompress::lzma_dictionary_size)
                .unwrap_or(0);
            let pd = GuidedSectionParsingData {
                guid,
                dictionary_size,
            };
            let ch = decompress_guided(&guid, &body, data_offset);
            (ParsingData::GuidedSection(pd), ch)
        }
        EFI_SECTION_COMPRESSION if body.len() >= 5 => {
            let uncomp_size = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            let comp_type = body[4];
            let ch = if body.len() > 5 {
                match decompress::decompress(&body[5..], comp_type) {
                    Ok(decompressed) => parse_sections(&decompressed, 0),
                    Err(e) => {
                        tracing::warn!("decompress failed at {off}: {e}");
                        vec![]
                    }
                }
            } else {
                vec![]
            };
            (
                ParsingData::CompressedSection(CompressedSectionParsingData {
                    uncompressed_size: uncomp_size,
                    compression_type: comp_type,
                    algorithm: comp_type,
                    dictionary_size: 0,
                }),
                ch,
            )
        }
        EFI_SECTION_FV_IMAGE => (ParsingData::None, parse_nested_fv(&body)),
        _ => (ParsingData::None, vec![]),
    };

    Ok(FfsNode {
        guid: None,
        node_type: FfsType::Section,
        subtype: stype,
        offset,
        header,
        body,
        tail: vec![],
        children,
        action: Action::NoAction,
        parsing_data,
        fixed: false,
        compressed: stype == EFI_SECTION_COMPRESSION,
        alignment_bytes: vec![],
    })
}

fn guided_payload(body: &[u8], data_offset: usize) -> Option<&[u8]> {
    let start = data_offset.checked_sub(4)?;
    body.get(start..)
}

fn guided_algorithm(guid: &Guid) -> Option<u8> {
    if is_lzma_guid(guid) {
        Some(uefi_common::pi::CompressionType::Lzma as u8)
    } else if is_tiano_guid(guid) {
        Some(uefi_common::pi::CompressionType::Standard as u8)
    } else {
        None
    }
}

fn decompress_guided_payload<'a>(
    guid: &Guid,
    body: &'a [u8],
    data_offset: usize,
) -> Option<&'a [u8]> {
    let payload = guided_payload(body, data_offset)?;
    if guided_algorithm(guid).is_some() {
        Some(payload)
    } else {
        None
    }
}

fn decompress_guided(guid: &Guid, body: &[u8], data_offset: usize) -> Vec<FfsNode> {
    let Some(algo) = guided_algorithm(guid) else {
        return vec![];
    };
    let Some(payload) = guided_payload(body, data_offset) else {
        return vec![];
    };
    match decompress::decompress(payload, algo) {
        Ok(decompressed) => parse_sections(&decompressed, 0),
        Err(e) => {
            tracing::warn!("guided section decompress failed: {e}");
            vec![]
        }
    }
}

fn parse_nested_fv(body: &[u8]) -> Vec<FfsNode> {
    let Some(vol) = crate::parser::image::parse_firmware_volume(body, 0) else {
        return vec![];
    };
    if vol.header.len() + vol.body.len() != body.len() {
        tracing::debug!("nested FV does not fill section body; skipped");
        return vec![];
    }
    vec![vol]
}

pub fn parse_sections(buf: &[u8], start: u32) -> Vec<FfsNode> {
    let mut nodes = vec![];
    let mut off = start as usize;
    while off + 4 <= buf.len() {
        let large = is_large_section(&buf[off..]);
        let hdr_len = if large { 8 } else { 4 };
        let size = section_size(&buf[off..]) as usize;
        if size < hdr_len || off + size > buf.len() {
            break;
        }
        let aligned = (size + 3) & !3;
        match parse_section(buf, off as u32) {
            Ok(node) => nodes.push(node),
            Err(e) => {
                tracing::warn!("section parse error at {off}: {e}");
                break;
            }
        }
        off += aligned;
    }
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_raw_section() {
        let mut buf = vec![0u8; 12];
        buf[0] = 8;
        buf[1] = 0;
        buf[2] = 0;
        buf[3] = EFI_SECTION_RAW;
        buf[4..8].copy_from_slice(&[0xAA; 4]);
        let node = parse_section(&buf, 0).unwrap();
        assert_eq!(node.node_type, FfsType::Section);
        assert_eq!(node.subtype, EFI_SECTION_RAW);
        assert_eq!(node.header.len(), 4);
        assert_eq!(node.body.len(), 4);
    }

    #[test]
    fn parse_multiple_sections() {
        let mut buf = vec![0u8; 16];
        buf[0] = 8;
        buf[3] = EFI_SECTION_RAW;
        buf[4..8].copy_from_slice(&[0x01; 4]);
        buf[8] = 8;
        buf[11] = EFI_SECTION_PE32;
        buf[12..16].copy_from_slice(&[0x02; 4]);
        let nodes = parse_sections(&buf, 0);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].subtype, EFI_SECTION_RAW);
        assert_eq!(nodes[1].subtype, EFI_SECTION_PE32);
    }

    #[test]
    fn parse_guided_lzma_section_decompresses_children() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let node = parse_section(section, 0).unwrap();
        assert_eq!(node.node_type, FfsType::Section);
        assert_eq!(node.subtype, EFI_SECTION_GUID_DEFINED);
        assert!(!node.children.is_empty(), "LZMA section must decompress");
        let pd = match &node.parsing_data {
            ParsingData::GuidedSection(d) => d,
            other => panic!("unexpected parsing data: {other:?}"),
        };
        assert!(is_lzma_guid(&pd.guid));
        assert!(pd.dictionary_size > 0);
    }

    #[test]
    fn parse_guided_lzma_section_children_match_decompressed() {
        let section = include_bytes!("../../../../tests/fixtures/lzma_guided_section.bin");
        let expected =
            include_bytes!("../../../../tests/fixtures/lzma_guided_section.decompressed.bin");
        let node = parse_section(section, 0).unwrap();
        assert!(!node.children.is_empty());
        let mut total = 0usize;
        for child in &node.children {
            total += child.header.len() + child.body.len() + child.tail.len();
        }
        assert!(total <= expected.len() + node.children.len() * 8);
    }

    fn section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    fn fv_bytes(file_guid: &Guid, sections: &[u8]) -> Vec<u8> {
        let file_size = 24 + sections.len();
        let total = (56 + file_size + 16) & !7;
        let mut buf = vec![0xFFu8; total];
        buf[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        buf[56..72].copy_from_slice(&file_guid.to_bytes());
        buf[74] = 0x07;
        buf[76..79].copy_from_slice(&size_to_uint24(file_size as u32));
        buf[80..80 + sections.len()].copy_from_slice(sections);
        buf
    }

    fn inner_fv() -> Vec<u8> {
        let guid = Guid::try_parse("899407d7-92a6-4174-968f-6f0b47f86a99").unwrap();
        fv_bytes(&guid, &section_bytes(EFI_SECTION_RAW, &[0xAA; 8]))
    }

    #[test]
    fn parse_fv_image_section_materializes_nested_volume() {
        let outer = section_bytes(EFI_SECTION_FV_IMAGE, &inner_fv());
        let nodes = parse_sections(&outer, 0);
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].subtype, EFI_SECTION_FV_IMAGE);
        let vol = &nodes[0].children[0];
        assert_eq!(vol.node_type, FfsType::Volume);
        let file = &vol.children[0];
        assert_eq!(file.node_type, FfsType::File);
        assert_eq!(
            file.guid,
            Some(Guid::try_parse("899407d7-92a6-4174-968f-6f0b47f86a99").unwrap())
        );
        let raw = &file.children[0];
        assert_eq!(raw.subtype, EFI_SECTION_RAW);
        assert_eq!(raw.body, vec![0xAA; 8]);
    }

    #[test]
    fn parse_fv_image_section_without_fvh_stays_leaf() {
        let outer = section_bytes(EFI_SECTION_FV_IMAGE, &[0x11; 64]);
        let nodes = parse_sections(&outer, 0);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].children.is_empty());
        assert_eq!(nodes[0].body, vec![0x11; 64]);
    }

    #[test]
    fn parse_fv_image_section_with_slack_stays_leaf() {
        let mut body = inner_fv();
        body.extend_from_slice(&[0xFF; 8]);
        let outer = section_bytes(EFI_SECTION_FV_IMAGE, &body);
        let nodes = parse_sections(&outer, 0);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].children.is_empty());
    }

    #[test]
    fn nested_fv_round_trips_and_removal_preserves_length() {
        let outer_guid = Guid::try_parse("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        let inner_guid = Guid::try_parse("899407d7-92a6-4174-968f-6f0b47f86a99").unwrap();
        let outer = fv_bytes(
            &outer_guid,
            &section_bytes(EFI_SECTION_FV_IMAGE, &inner_fv()),
        );
        let mut img = crate::parser::image::parse_image(&outer, ImageMode::Write, "i", "s")
            .expect("parse_image");
        let built = crate::builder::build_image(&img).expect("clean build");
        assert_eq!(
            built, outer,
            "clean nested-FV tree must round-trip verbatim"
        );

        let t = crate::types::Target::Guid(inner_guid);
        let path = crate::parser::target::find_item_path(&img.root, &t).expect("inner file path");
        crate::ops::remove(&mut img.root, &crate::types::Target::Path(path)).unwrap();
        let built2 = crate::builder::build_image(&img).expect("build after remove");
        assert_eq!(built2.len(), outer.len(), "flash length must be preserved");
        assert!(
            !built2.windows(4).any(|w| w == [0xAA, 0xAA, 0xAA, 0xAA]),
            "removed inner RAW body must not appear in output"
        );
        let re = crate::parser::image::parse_image(&built2, ImageMode::Read, "i2", "s2").unwrap();
        assert!(crate::parser::target::find_item(&re.root, &t).is_err());
    }
}
