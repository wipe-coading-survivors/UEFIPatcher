use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecompressError {
    #[error("unsupported algorithm")]
    Unsupported,
    #[error("corrupted data")]
    Corrupted,
}

pub const EFI_NOT_COMPRESSED: u8 = 0;
pub const EFI_STANDARD_COMPRESSION: u8 = 1;
pub const EFI_LZMA_COMPRESSION: u8 = 2;

pub fn decompress(data: &[u8], algorithm: u8) -> Result<Vec<u8>, DecompressError> {
    match algorithm {
        EFI_NOT_COMPRESSED => Ok(data.to_vec()),
        EFI_STANDARD_COMPRESSION => decompress_tiano(data),
        EFI_LZMA_COMPRESSION => decompress_lzma(data),
        _ => Err(DecompressError::Unsupported),
    }
}

fn decompress_tiano(_data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    Err(DecompressError::Unsupported)
}

fn decompress_lzma(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    if data.len() < 13 {
        return Err(DecompressError::Corrupted);
    }
    let mut output: Vec<u8> = Vec::new();
    lzma_rs::lzma_decompress(&mut Cursor::new(data), &mut output)
        .map_err(|_| DecompressError::Corrupted)?;
    Ok(output)
}

pub fn lzma_dictionary_size(data: &[u8]) -> u32 {
    if data.len() >= 5 {
        u32::from_le_bytes([data[1], data[2], data[3], data[4]])
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompress_not_compressed() {
        let data = vec![0x01, 0x02, 0x03];
        assert_eq!(decompress(&data, EFI_NOT_COMPRESSED).unwrap(), data);
    }

    #[test]
    fn decompress_unsupported() {
        assert!(decompress(&[], 0xFF).is_err());
    }

    #[test]
    fn decompress_tiano_unsupported_in_cycle1() {
        assert!(matches!(
            decompress(&[0; 16], EFI_STANDARD_COMPRESSION),
            Err(DecompressError::Unsupported)
        ));
    }

    #[test]
    fn decompress_lzma_real_firmware_blob() {
        let section = include_bytes!("../../../tests/fixtures/lzma_guided_section.bin");
        let expected =
            include_bytes!("../../../tests/fixtures/lzma_guided_section.decompressed.bin");
        let data_offset = u16::from_le_bytes([section[20], section[21]]) as usize;
        let payload = &section[data_offset..];
        let out = decompress(payload, EFI_LZMA_COMPRESSION).expect("LZMA decode");
        assert_eq!(out.len(), expected.len());
        assert_eq!(out.as_slice(), &expected[..]);
        let dict = lzma_dictionary_size(payload);
        assert!(dict > 0);
    }
}
