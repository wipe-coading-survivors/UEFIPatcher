use std::path::PathBuf;
use uefi_engine::ffs::{calculate_checksum8, uint24_to_u32};
use uefi_engine::parser::file::guid_from_bytes;

const FFS_TYPE_DRIVER: u8 = 0x07;
const SECTION_GUIDED: u8 = 0x02;
const LZMA_CUSTOM_DECOMPRESS_GUID: &[u8] = &[
    0x98, 0x58, 0x4E, 0xEE, 0x14, 0x39, 0x59, 0x42, 0x9D, 0x6E, 0xDC, 0x7B, 0xD7, 0x94, 0x03, 0xCF,
];

#[test]
fn bif_epa_probe_artifact_invariants() {
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/bif/BifEpaProbe.ffs"),
    )
    .expect("artifact missing: run docker/edk2/build_bif_epa.sh (план Task 6)");
    assert!(bytes.len() >= 24);

    let guid = guid_from_bytes(&bytes[..16])
        .unwrap()
        .to_string()
        .to_ascii_uppercase();
    assert_eq!(guid, "B7E4A2C1-58D6-4E3F-B9A2-7C1D0E6F5A84");
    assert_eq!(bytes[0x12], FFS_TYPE_DRIVER);

    let mut hdr = bytes[..24].to_vec();
    hdr[17] = 0;
    hdr[23] = 0;
    assert_eq!(calculate_checksum8(&hdr), 0, "FFS header checksum");

    let size24 = uint24_to_u32([bytes[0x14], bytes[0x15], bytes[0x16]]) as usize;
    assert_eq!(size24, bytes.len(), "FFS size24 == file size");

    let mut off = 24usize;
    let mut guided: Option<(usize, usize)> = None;
    while off + 4 <= bytes.len() {
        let sec_size = uint24_to_u32([bytes[off], bytes[off + 1], bytes[off + 2]]) as usize;
        if sec_size < 4 {
            break;
        }
        if bytes[off + 3] == SECTION_GUIDED {
            guided = Some((off, sec_size));
        }
        off += (sec_size + 3) & !3;
    }

    let (goff, gsize) = guided.expect("no GUIDED section");
    assert_eq!(&bytes[goff + 4..goff + 20], LZMA_CUSTOM_DECOMPRESS_GUID);
    let data_offset = u16::from_le_bytes([bytes[goff + 20], bytes[goff + 21]]) as usize;
    assert_eq!(data_offset, 0x18);
    let attrs = u16::from_le_bytes([bytes[goff + 22], bytes[goff + 23]]);
    assert_eq!(attrs, 0x0001, "PROCESSING_REQUIRED");
    let blob = &bytes[goff + data_offset..goff + gsize];
    assert_eq!(blob[0], 0x5D, "LZMA props lc=3/lp=0/pb=2");
    let declared = u64::from_le_bytes(blob[5..13].try_into().unwrap()) as usize;
    assert!(
        (15000..30000).contains(&declared),
        "LZMA declared size {declared} implausible for probe payload"
    );
}
