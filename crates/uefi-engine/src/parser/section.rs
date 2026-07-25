use super::ParserError;
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
    let parsing_data = match stype {
        EFI_SECTION_GUID_DEFINED if body.len() >= 20 => {
            let guid = crate::parser::file::guid_from_bytes(&body[0..16])
                .ok_or_else(|| ParserError::InvalidHeader("guid".into()))?;
            let _data_offset = u16::from_le_bytes([body[16], body[17]]) as usize;
            let _attributes = u16::from_le_bytes([body[18], body[19]]);
            ParsingData::GuidedSection(GuidedSectionParsingData {
                guid,
                dictionary_size: 0,
            })
        }
        EFI_SECTION_COMPRESSION if body.len() >= 5 => {
            let uncomp_size = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
            let comp_type = body[4];
            ParsingData::CompressedSection(CompressedSectionParsingData {
                uncompressed_size: uncomp_size,
                compression_type: comp_type,
                algorithm: 0,
                dictionary_size: 0,
            })
        }
        _ => ParsingData::None,
    };
    let children = vec![];
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
}
