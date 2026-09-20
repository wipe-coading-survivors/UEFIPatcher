use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecompressError {
    #[error("unsupported algorithm")]
    Unsupported,
    #[error("corrupted data")]
    Corrupted,
}

pub use uefi_common::pi::CompressionType;

pub fn decompress(data: &[u8], algorithm: u8) -> Result<Vec<u8>, DecompressError> {
    match algorithm {
        0 => Ok(data.to_vec()),
        1 => decompress_tiano(data),
        2 => decompress_lzma(data),
        _ => Err(DecompressError::Unsupported),
    }
}

/// Algo-1 payload: сначала EFI 1.1 (pbit=4), при ошибке — Tiano (pbit=5),
/// как в UEFITool ( ref: utility.cpp:243 — оба варианта, выбор по успеху).
/// Не разрешает «оба декодируются» preparse'ом ( ref: ffsparser.cpp:3292):
/// Efi приоритетен, корпус C275 целиком pbit=4.
fn decompress_tiano(data: &[u8]) -> Result<Vec<u8>, DecompressError> {
    crate::tiano::decompress(data, crate::tiano::Pbit::Efi)
        .or_else(|_| crate::tiano::decompress(data, crate::tiano::Pbit::Tiano))
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
        assert_eq!(decompress(&data, 0).unwrap(), data);
    }

    #[test]
    fn decompress_unsupported() {
        assert!(decompress(&[], 0xFF).is_err());
    }

    #[test]
    fn decompress_tiano_empty_stream_ok() {
        // comp_size=0, orig_size=0: успех с пустым выходом, не Unsupported
        assert_eq!(decompress(&[0; 16], 1).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decompress_tiano_real_section() {
        let data = include_bytes!("../tests/fixtures/tiano/c275-min.in");
        let expected = include_bytes!("../tests/fixtures/tiano/c275-min.expected");
        let out = decompress(data, 1).expect("tiano decode");
        assert_eq!(out.as_slice(), &expected[..]);
    }

    #[test]
    fn decompress_tiano_variant_fallback() {
        let data = include_bytes!("../tests/fixtures/tiano/226d2-min.in");
        let expected = include_bytes!("../tests/fixtures/tiano/226d2-min.expected");
        let out = decompress(data, 1).expect("tiano-variant (pbit 5) section decodes via fallback");
        assert_eq!(out.as_slice(), &expected[..]);
    }

    #[test]
    fn decompress_lzma_real_firmware_blob() {
        let section = include_bytes!("../../../tests/fixtures/lzma_guided_section.bin");
        let expected =
            include_bytes!("../../../tests/fixtures/lzma_guided_section.decompressed.bin");
        let data_offset = u16::from_le_bytes([section[20], section[21]]) as usize;
        let payload = &section[data_offset..];
        let out = decompress(payload, 2).expect("LZMA decode");
        assert_eq!(out.len(), expected.len());
        assert_eq!(out.as_slice(), &expected[..]);
        let dict = lzma_dictionary_size(payload);
        assert!(dict > 0);
    }
}
