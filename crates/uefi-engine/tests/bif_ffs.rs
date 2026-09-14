use std::path::PathBuf;
use uefi_engine::ffs::{calculate_checksum8, uint24_to_u32};
use uefi_engine::parser::file::guid_from_bytes;

const FFS_TYPE_DRIVER: u8 = 0x07;
const SECTION_PE32: u8 = 0x10;
const SECTION_UI: u8 = 0x15;
const PE_MACHINE_AMD64: u16 = 0x8664;
const PE_SUBSYSTEM_BOOT_DRIVER: u16 = 11;

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
    let mut pe32: Option<&[u8]> = None;
    let mut ui_name = String::new();
    while off + 4 <= bytes.len() {
        let sec_size = uint24_to_u32([bytes[off], bytes[off + 1], bytes[off + 2]]) as usize;
        if sec_size < 4 {
            break;
        }
        match bytes[off + 3] {
            SECTION_PE32 => pe32 = Some(&bytes[off + 4..off + sec_size]),
            SECTION_UI => {
                let raw = &bytes[off + 4..off + sec_size];
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                ui_name = String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .to_string();
            }
            _ => {}
        }
        off += (sec_size + 3) & !3;
    }

    let pe = pe32.expect("no PE32 section");
    assert_eq!(&pe[..2], b"MZ");
    let lfanew = u32::from_le_bytes(pe[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&pe[lfanew..lfanew + 4], b"PE\x00\x00");
    let pe_machine = u16::from_le_bytes(pe[lfanew + 4..lfanew + 6].try_into().unwrap());
    let pe_subsystem = u16::from_le_bytes(pe[lfanew + 92..lfanew + 94].try_into().unwrap());
    assert_eq!(pe_machine, PE_MACHINE_AMD64);
    assert_eq!(pe_subsystem, PE_SUBSYSTEM_BOOT_DRIVER);
    assert_eq!(ui_name, "BifEpaProbe");
    let marker = b"BIF-EPA:".as_slice();
    assert!(
        pe.windows(marker.len()).any(|w| w == marker),
        "probe markers missing in PE32"
    );
}
