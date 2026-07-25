use super::ParserError;
use crate::ffs::*;
use crate::types::*;

pub fn parse_volume(buf: &[u8], offset: u32) -> Result<FfsNode, ParserError> {
    let off = offset as usize;
    if off + 56 > buf.len() {
        return Err(ParserError::EndOfBuffer);
    }
    let sig = u32::from_le_bytes([buf[off + 40], buf[off + 41], buf[off + 42], buf[off + 43]]);
    if sig != EFI_FVH_SIGNATURE {
        return Err(ParserError::InvalidHeader(format!(
            "bad FVH signature at {offset}"
        )));
    }
    let fv_length = u64::from_le_bytes([
        buf[off + 32],
        buf[off + 33],
        buf[off + 34],
        buf[off + 35],
        buf[off + 36],
        buf[off + 37],
        buf[off + 38],
        buf[off + 39],
    ]);
    let vol_size = fv_length as usize;
    let header_len = u16::from_le_bytes([buf[off + 48], buf[off + 49]]) as usize;
    let attributes =
        u32::from_le_bytes([buf[off + 44], buf[off + 45], buf[off + 46], buf[off + 47]]);
    if header_len < 56 || vol_size < header_len || off + vol_size > buf.len() {
        return Err(ParserError::InvalidHeader(format!(
            "bad FV geometry at {offset}: header_len={header_len}, fv_length={fv_length}"
        )));
    }
    let empty_byte = if attributes & EFI_FVB2_ERASE_POLARITY != 0 {
        0xFF
    } else {
        0x00
    };
    let header = buf[off..off + header_len].to_vec();
    let body = buf[off + header_len..off + vol_size].to_vec();
    let parsing_data = ParsingData::Volume(VolumeParsingData {
        extended_header_guid: None,
        alignment: 1u32 << (attributes & 0x1F),
        ffs_version: 2,
        empty_byte,
        revision: buf[off + 55],
    });
    Ok(FfsNode {
        guid: None,
        node_type: FfsType::Volume,
        subtype: 0,
        offset,
        header,
        body,
        tail: vec![],
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
    use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE};

    fn make_minimal_volume() -> Vec<u8> {
        let mut buf = vec![0u8; 256];
        buf[32..40].copy_from_slice(&256u64.to_le_bytes());
        buf[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        buf[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        buf[48..50].copy_from_slice(&56u16.to_le_bytes());
        buf[55] = 2;
        buf
    }

    #[test]
    fn parse_minimal_volume() {
        let buf = make_minimal_volume();
        let node = parse_volume(&buf, 0).unwrap();
        assert_eq!(node.node_type, FfsType::Volume);
        assert_eq!(node.offset, 0);
        assert_eq!(node.header.len(), 56);
        assert!(node.children.is_empty());
    }
}
