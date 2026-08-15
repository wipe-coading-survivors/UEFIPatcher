use std::io::Cursor;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("empty input")]
    EmptyInput,
    #[error("lzma compression failed")]
    CompressFailed,
    #[error("lzma round-trip check failed")]
    RoundTripFailed,
}

pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let options = lzma_rs::compress::Options {
        unpacked_size: lzma_rs::compress::UnpackedSize::WriteToHeader(Some(input.len() as u64)),
    };
    let mut out = Vec::new();
    lzma_rs::lzma_compress_with_options(&mut Cursor::new(input), &mut out, &options)
        .map_err(|_| CompressError::CompressFailed)?;
    let mut round_trip = Vec::new();
    lzma_rs::lzma_decompress(&mut Cursor::new(&out), &mut round_trip)
        .map_err(|_| CompressError::RoundTripFailed)?;
    if round_trip.as_slice() != input {
        tracing::warn!(size = input.len(), "lzma round-trip mismatch");
        return Err(CompressError::RoundTripFailed);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECOMPRESSED: &[u8] =
        include_bytes!("../../../tests/fixtures/lzma_guided_section.decompressed.bin");

    #[test]
    fn compress_lzma_empty_input_errors() {
        assert!(matches!(compress_lzma(&[]), Err(CompressError::EmptyInput)));
    }

    #[test]
    fn compress_lzma_writes_edk2_alone_header() {
        let out = compress_lzma(&[0x00; 64]).unwrap();
        assert!(out.len() > 13);
        assert_eq!(out[0], 0x5D);
        assert_eq!(&out[1..5], &[0x00, 0x00, 0x80, 0x00]);
        assert_eq!(&out[5..13], &64u64.to_le_bytes());
    }

    #[test]
    fn compress_lzma_round_trips_real_blob() {
        let out = compress_lzma(DECOMPRESSED).unwrap();
        let mut decoded = Vec::new();
        lzma_rs::lzma_decompress(&mut Cursor::new(&out), &mut decoded).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_output_decodable_by_engine() {
        let out = compress_lzma(DECOMPRESSED).unwrap();
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }
}
