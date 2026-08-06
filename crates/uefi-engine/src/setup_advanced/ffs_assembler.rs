use crate::ffs::*;
use crate::types::Guid;

use super::SetupAdvancedError;

const FFS_HEADER_SIZE: usize = 24;

pub fn assemble_ffs(
    ifr_bytes: &[u8],
    string_package_bytes: &[u8],
    file_guid: &Guid,
) -> Result<Vec<u8>, SetupAdvancedError> {
    let mut body = Vec::new();
    emit_raw_section(&mut body, ifr_bytes);
    emit_raw_section(&mut body, string_package_bytes);
    let total = FFS_HEADER_SIZE + body.len();
    let mut header = vec![0u8; FFS_HEADER_SIZE];
    header[0..16].copy_from_slice(&file_guid.to_bytes());
    header[18] = EFI_FV_FILETYPE_RAW;
    header[19] = 0x00;
    let size_b = size_to_uint24(total as u32);
    header[20] = size_b[0];
    header[21] = size_b[1];
    header[22] = size_b[2];
    header[16] = calculate_checksum8(&header);
    let mut ffs = Vec::with_capacity(total);
    ffs.extend_from_slice(&header);
    ffs.extend_from_slice(&body);
    Ok(ffs)
}

fn emit_raw_section(out: &mut Vec<u8>, data: &[u8]) {
    let total = 4 + data.len();
    let size_b = size_to_uint24(total as u32);
    out.push(size_b[0]);
    out.push(size_b[1]);
    out.push(size_b[2]);
    out.push(EFI_SECTION_RAW);
    out.extend_from_slice(data);
    let aligned = (out.len() + 3) & !3;
    while out.len() < aligned {
        out.push(0x00);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn assemble_minimal_ffs() {
        let g = Guid::from_str("12345678-90AB-CDEF-1234-567890ABCDEF").unwrap();
        let ifr = vec![0x29, 0x02];
        let strpkg = vec![0x06, 0x00, 0x00, 0x00, 0x04];
        let ffs = assemble_ffs(&ifr, &strpkg, &g).unwrap();
        assert!(ffs.len() > 24);
        assert_eq!(&ffs[0..16], &g.to_bytes());
        assert_eq!(ffs[18], EFI_FV_FILETYPE_RAW);
        let sum: u8 = ffs[0..24].iter().fold(0u8, |a, &b| a.wrapping_add(b));
        assert_eq!(sum, 0);
    }

    #[test]
    fn ffs_size_matches() {
        let g = Guid::from_str("11111111-2222-3333-4444-555555555555").unwrap();
        let ifr = vec![0x01, 0x06, 0x01, 0x00, 0x01, 0x00, 0x29, 0x02];
        let ffs = assemble_ffs(&ifr, &[], &g).unwrap();
        let size = uint24_to_u32([ffs[20], ffs[21], ffs[22]]) as usize;
        assert_eq!(size, ffs.len());
    }

    #[test]
    fn assemble_ffs_round_trips_through_parser() {
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let ifr = vec![0x29, 0x02];
        let ffs = assemble_ffs(&ifr, &[], &g).unwrap();
        let node = crate::parser::file::parse_file(&ffs, 0, 0xFF, 2).unwrap();
        assert_eq!(node.guid, Some(g));
        assert_eq!(node.subtype, EFI_FV_FILETYPE_RAW);
        assert_eq!(node.header.len(), 24);
        assert_eq!(node.body.len(), ffs.len() - 24);
    }
}
