use super::ParserError;
use crate::ffs::*;
use crate::types::*;

pub fn guid_to_bytes(g: &Guid) -> [u8; 16] {
    g.to_bytes()
}

pub fn guid_from_bytes(b: &[u8]) -> Option<Guid> {
    let arr: [u8; 16] = b.get(..16)?.try_into().unwrap();
    Some(Guid::from_bytes(arr))
}

pub fn parse_file(
    buf: &[u8],
    offset: u32,
    erase_polarity: u8,
    revision: u8,
) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 24 > buf.len() {
        return Err(ParserError::EndOfBuffer);
    }
    let large = is_large_ffs(&buf[off..]);
    let hdr_len = if large { 32 } else { 24 };
    if off + hdr_len > buf.len() {
        return Err(ParserError::EndOfBuffer);
    }
    let guid = guid_from_bytes(&buf[off..off + 16])
        .ok_or_else(|| ParserError::InvalidHeader("guid".into()))?;
    let ftype = buf[off + 18];
    let size = ffs_file_size(&buf[off..]);
    let total = size as usize;
    if off + total > buf.len() {
        return Err(ParserError::EndOfBuffer);
    }
    if total < hdr_len {
        return Err(ParserError::InvalidHeader("size < header".into()));
    }
    let header = buf[off..off + hdr_len].to_vec();
    let tail_len = if revision == 1 { 2 } else { 0 };
    let body_end = off + total - tail_len;
    let body = if body_end > off + hdr_len {
        buf[off + hdr_len..body_end].to_vec()
    } else {
        vec![]
    };
    let tail = if tail_len > 0 {
        buf[body_end..body_end + tail_len].to_vec()
    } else {
        vec![]
    };
    let parsing_data = ParsingData::File(FileParsingData {
        empty_byte: erase_polarity,
        guid,
    });
    Ok(FfsNode {
        guid: Some(guid),
        node_type: FfsType::File,
        subtype: ftype,
        offset,
        header,
        body,
        tail,
        children: vec![],
        action: Action::NoAction,
        parsing_data,
        fixed: false,
        compressed: false,
        alignment_bytes: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_ffs() -> Vec<u8> {
        let mut buf = vec![0u8; 48];
        let guid = Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = 0x01;
        buf[19] = 0x02;
        buf[20..23].copy_from_slice(&size_to_uint24(48));
        buf[24..48].copy_from_slice(&[0xFF; 24]);
        buf
    }

    #[test]
    fn parse_ffs_minimal() {
        let buf = make_minimal_ffs();
        let node = parse_file(&buf, 0, 0xFF, 2).unwrap();
        assert_eq!(node.node_type, FfsType::File);
        assert_eq!(node.offset, 0);
        assert_eq!(node.header.len(), 24);
        assert_eq!(node.body.len(), 24);
        assert_eq!(node.subtype, 0x01);
    }
}
