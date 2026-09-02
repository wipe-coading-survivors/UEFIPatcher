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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LzmaMatchFinder {
    Bt2,
    Bt3,
    Bt4,
    Hc4,
}

impl LzmaMatchFinder {
    fn to_liblzma(self) -> liblzma::stream::MatchFinder {
        match self {
            LzmaMatchFinder::Bt2 => liblzma::stream::MatchFinder::BinaryTree2,
            LzmaMatchFinder::Bt3 => liblzma::stream::MatchFinder::BinaryTree3,
            LzmaMatchFinder::Bt4 => liblzma::stream::MatchFinder::BinaryTree4,
            LzmaMatchFinder::Hc4 => liblzma::stream::MatchFinder::HashChain4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LzmaEncodeParams {
    pub lc: u32,
    pub lp: u32,
    pub pb: u32,
    pub dict_size: u32,
    pub nice_len: u32,
    pub mf: LzmaMatchFinder,
}

impl Default for LzmaEncodeParams {
    fn default() -> Self {
        Self {
            lc: 0,
            lp: 0,
            pb: 0,
            dict_size: 0x0100_0000,
            nice_len: 273,
            mf: LzmaMatchFinder::Bt4,
        }
    }
}

pub fn lzma_props_byte(lc: u32, lp: u32, pb: u32) -> u8 {
    ((pb * 5 + lp) * 9 + lc) as u8
}

fn encode_raw_lzma1(input: &[u8], p: &LzmaEncodeParams) -> Result<Vec<u8>, CompressError> {
    use liblzma::stream::{Action, Filters, LzmaOptions, Status, Stream};
    let mut opts = LzmaOptions::new_preset(6).map_err(|_| CompressError::CompressFailed)?;
    opts.dict_size(p.dict_size)
        .literal_context_bits(p.lc)
        .literal_position_bits(p.lp)
        .position_bits(p.pb)
        .nice_len(p.nice_len)
        .match_finder(p.mf.to_liblzma())
        .mode(liblzma::stream::Mode::Normal);
    let mut filters = Filters::new();
    filters.lzma1(&opts);
    let mut enc = Stream::new_raw_encoder(&filters).map_err(|_| CompressError::CompressFailed)?;
    let mut out = Vec::new();
    loop {
        out.reserve(64 * 1024);
        let consumed = (enc.total_in() as usize).min(input.len());
        let status = enc
            .process_vec(&input[consumed..], &mut out, Action::Finish)
            .map_err(|_| CompressError::CompressFailed)?;
        if matches!(status, Status::StreamEnd) {
            break;
        }
    }
    Ok(out)
}

fn alone_stream(input: &[u8], p: &LzmaEncodeParams) -> Result<Vec<u8>, CompressError> {
    let mut out = Vec::with_capacity(13 + input.len() / 2);
    out.push(lzma_props_byte(p.lc, p.lp, p.pb));
    out.extend_from_slice(&p.dict_size.to_le_bytes());
    out.extend_from_slice(&(input.len() as u64).to_le_bytes());
    out.extend_from_slice(&encode_raw_lzma1(input, p)?);
    Ok(out)
}

fn round_trip_check(input: &[u8], stream: &[u8]) -> Result<(), CompressError> {
    let decoded =
        crate::decompress::decompress(stream, 2).map_err(|_| CompressError::RoundTripFailed)?;
    if decoded.as_slice() != input {
        tracing::warn!(size = input.len(), "lzma round-trip mismatch");
        return Err(CompressError::RoundTripFailed);
    }
    Ok(())
}

pub fn compress_lzma(input: &[u8]) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let out = alone_stream(input, &LzmaEncodeParams::default())?;
    round_trip_check(input, &out)?;
    Ok(out)
}

pub fn compress_lzma_fit(input: &[u8], budget: Option<usize>) -> Result<Vec<u8>, CompressError> {
    if input.is_empty() {
        return Err(CompressError::EmptyInput);
    }
    let mut best: Option<Vec<u8>> = None;
    for pb in 0..=2u32 {
        for lc in 0..=4u32 {
            let params = LzmaEncodeParams {
                lc,
                lp: 0,
                pb,
                ..LzmaEncodeParams::default()
            };
            let stream = alone_stream(input, &params)?;
            if let Some(b) = budget
                && stream.len() <= b
            {
                let mut padded = stream;
                padded.resize(b, 0x00);
                round_trip_check(input, &padded)?;
                return Ok(padded);
            }
            if best.as_ref().is_none_or(|s| stream.len() < s.len()) {
                best = Some(stream);
            }
        }
    }
    let stream = best.expect("sweep grid is non-empty");
    if let Some(b) = budget
        && stream.len() > b
    {
        tracing::debug!(
            size = stream.len(),
            budget = b,
            "lzma sweep: no candidate fits budget, using minimal"
        );
    }
    round_trip_check(input, &stream)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;

    const DECOMPRESSED: &[u8] =
        include_bytes!("../../../tests/fixtures/lzma_guided_section.decompressed.bin");

    #[test]
    fn compress_lzma_empty_input_errors() {
        assert!(matches!(compress_lzma(&[]), Err(CompressError::EmptyInput)));
    }

    #[test]
    fn compress_lzma_writes_alone_header_with_hw_proven_defaults() {
        let out = compress_lzma(&[0x00; 64]).unwrap();
        assert!(out.len() > 13);
        assert_eq!(out[0], 0x00);
        assert_eq!(&out[1..5], &0x0100_0000u32.to_le_bytes());
        assert_eq!(&out[5..13], &64u64.to_le_bytes());
    }

    #[test]
    fn lzma_props_byte_encoding() {
        assert_eq!(lzma_props_byte(0, 0, 0), 0x00);
        assert_eq!(lzma_props_byte(3, 0, 2), 0x5D);
        assert_eq!(lzma_props_byte(0, 0, 1), 0x2D);
        assert_eq!(lzma_props_byte(2, 0, 0), 0x02);
    }

    #[test]
    fn encode_raw_lzma1_respects_pb_param_in_props() {
        let p = LzmaEncodeParams {
            pb: 2,
            ..LzmaEncodeParams::default()
        };
        let out = alone_stream(&[0x41; 4096], &p).unwrap();
        assert_eq!(out[0], 0x5A);
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

    #[test]
    fn liblzma_raw_lzma1_custom_props_decode_with_known_size_header() {
        use liblzma::stream::{Action, Filters, LzmaOptions, MatchFinder, Mode, Status, Stream};

        let input: Vec<u8> = DECOMPRESSED.to_vec();
        let mut opts = LzmaOptions::new_preset(6).unwrap();
        opts.dict_size(0x0100_0000)
            .literal_context_bits(0)
            .literal_position_bits(0)
            .position_bits(0)
            .nice_len(273)
            .match_finder(MatchFinder::BinaryTree4)
            .mode(Mode::Normal);
        let mut filters = Filters::new();
        filters.lzma1(&opts);
        let mut enc = Stream::new_raw_encoder(&filters).unwrap();

        let mut raw = Vec::new();
        loop {
            raw.reserve(64 * 1024);
            let consumed = enc.total_in() as usize;
            let status = enc
                .process_vec(&input[consumed..], &mut raw, Action::Finish)
                .unwrap();
            if matches!(status, Status::StreamEnd) {
                break;
            }
        }
        assert!(!raw.is_empty());

        let mut alone = vec![0x00u8];
        alone.extend_from_slice(&0x0100_0000u32.to_le_bytes());
        alone.extend_from_slice(&(input.len() as u64).to_le_bytes());
        alone.extend_from_slice(&raw);
        alone.extend_from_slice(&[0u8; 16]);

        let decoded = crate::decompress::decompress(&alone, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    fn incompressible(len: usize) -> Vec<u8> {
        let mut x: u64 = 0x243F_6A88_85A3_08D3;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 32) as u8
            })
            .collect()
    }

    #[test]
    fn compress_lzma_fit_fits_budget_and_pads_with_zeros() {
        let first = compress_lzma(DECOMPRESSED).unwrap();
        let budget = first.len();
        let out = compress_lzma_fit(DECOMPRESSED, Some(budget)).unwrap();
        assert_eq!(out.len(), budget);
        assert_eq!(&out[5..13], &(DECOMPRESSED.len() as u64).to_le_bytes());
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_fit_padded_tail_decodes() {
        let first = compress_lzma(DECOMPRESSED).unwrap();
        let padded = compress_lzma_fit(DECOMPRESSED, Some(first.len() + 64)).unwrap();
        assert_eq!(padded.len(), first.len() + 64);
        let decoded = crate::decompress::decompress(&padded, 2).unwrap();
        assert_eq!(decoded.as_slice(), DECOMPRESSED);
    }

    #[test]
    fn compress_lzma_fit_without_budget_returns_minimal_candidate() {
        let data = incompressible(4096);
        let first = compress_lzma(&data).unwrap();
        let out = compress_lzma_fit(&data, None).unwrap();
        assert!(out.len() <= first.len());
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), data);
    }

    #[test]
    fn compress_lzma_fit_unreachable_budget_still_returns_stream() {
        let data = incompressible(512);
        let out = compress_lzma_fit(&data, Some(13)).unwrap();
        assert!(out.len() > 13);
        let decoded = crate::decompress::decompress(&out, 2).unwrap();
        assert_eq!(decoded.as_slice(), data);
    }

    #[test]
    fn compress_lzma_fit_prefers_pb0_for_aligned_payload() {
        let data: Vec<u8> = (0..2048u32).flat_map(|i| i.to_le_bytes()).collect();
        let out = compress_lzma_fit(&data, None).unwrap();
        assert_eq!(out[0], 0x00);
    }
}
