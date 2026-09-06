use std::path::PathBuf;
use uefi_engine::ffs::{calculate_checksum8, uint24_to_u32};
use uefi_engine::parser::file::guid_from_bytes;

const FFS_TYPE_DRIVER: u8 = 0x07;
const SECTION_PE32: u8 = 0x10;
const PE_MACHINE_AMD64: u16 = 0x8664;
const PE_SUBSYSTEM_BOOT_DRIVER: u16 = 11;

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/serial")
}

struct ParsedFfs {
    guid: String,
    ffs_type: u8,
    size24: u32,
    header_checksum_zero: bool,
    pe_machine: u16,
    pe_subsystem: u16,
}

fn parse_ffs(bytes: &[u8]) -> ParsedFfs {
    assert!(bytes.len() >= 24);
    let guid = guid_from_bytes(&bytes[..16]).unwrap();
    let mut hdr = bytes[..24].to_vec();
    hdr[17] = 0;
    hdr[23] = 0;
    let mut off = 24usize;
    let mut pe32: Option<&[u8]> = None;
    while off + 4 <= bytes.len() {
        let sec_size = uint24_to_u32([bytes[off], bytes[off + 1], bytes[off + 2]]) as usize;
        if sec_size < 4 {
            break;
        }
        if bytes[off + 3] == SECTION_PE32 {
            pe32 = Some(&bytes[off + 4..off + sec_size]);
            break;
        }
        off += (sec_size + 3) & !3;
    }
    let pe = pe32.expect("no PE32 section");
    assert_eq!(&pe[..2], b"MZ");
    let lfanew = u32::from_le_bytes(pe[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&pe[lfanew..lfanew + 4], b"PE\x00\x00");
    let pe_machine = u16::from_le_bytes(pe[lfanew + 4..lfanew + 6].try_into().unwrap());
    let pe_subsystem = u16::from_le_bytes(pe[lfanew + 92..lfanew + 94].try_into().unwrap());
    ParsedFfs {
        guid: guid.to_string().to_ascii_uppercase(),
        ffs_type: bytes[0x12],
        size24: uint24_to_u32([bytes[0x14], bytes[0x15], bytes[0x16]]),
        header_checksum_zero: calculate_checksum8(&hdr) == 0,
        pe_machine,
        pe_subsystem,
    }
}

#[test]
fn serial_s1_artifacts_parse() {
    let cases = [
        ("SerialDxe.ffs", "9A5163E7-5C29-453F-825C-837A46A81E15"),
        ("TerminalDxe.ffs", "9E863906-A40F-4875-977F-5B93FF237FC6"),
        (
            "SerialConsoleGlue.ffs",
            "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43",
        ),
    ];
    for (file, expect_guid) in cases {
        let path = data_dir().join(file);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let parsed = parse_ffs(&bytes);
        assert_eq!(parsed.guid, expect_guid, "{file}");
        assert_eq!(parsed.ffs_type, FFS_TYPE_DRIVER, "{file}");
        assert_eq!(parsed.size24 as usize, bytes.len(), "{file}");
        assert!(parsed.header_checksum_zero, "{file} header checksum");
        assert_eq!(parsed.pe_machine, PE_MACHINE_AMD64, "{file}");
        assert_eq!(parsed.pe_subsystem, PE_SUBSYSTEM_BOOT_DRIVER, "{file}");
    }
}

const SECTION_GUIDED: u8 = 0x02;
const SECTION_DEPEX: u8 = 0x13;
const SECTION_UI: u8 = 0x15;
const LZMA_GUID: [u8; 16] = [
    0x98, 0x58, 0x4E, 0xEE, 0x14, 0x39, 0x59, 0x42, 0x9D, 0x6E, 0xDC, 0x7B, 0xD7, 0x94, 0x03, 0xCF,
];
const PCD_PROTOCOL_GUID_LE: [u8; 16] = [
    0xF6, 0xF0, 0xA3, 0x13, 0x4A, 0x26, 0xF0, 0x3E, 0xF2, 0xE0, 0xDE, 0xC5, 0x12, 0x34, 0x2F, 0x34,
];

#[test]
fn serial_io_ami_artifact_structure() {
    let path = data_dir().join("SerialIoAmiDxe.ffs");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(bytes.len(), 7675);

    let guid = guid_from_bytes(&bytes[..16]).unwrap();
    assert_eq!(
        guid.to_string().to_ascii_uppercase(),
        "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F"
    );
    assert_eq!(bytes[0x12], FFS_TYPE_DRIVER);
    assert_eq!(
        uint24_to_u32([bytes[0x14], bytes[0x15], bytes[0x16]]) as usize,
        bytes.len()
    );
    let mut hdr = bytes[..24].to_vec();
    hdr[17] = 0;
    hdr[23] = 0;
    assert_eq!(calculate_checksum8(&hdr), 0);

    let depex_size = uint24_to_u32([bytes[0x18], bytes[0x19], bytes[0x1A]]) as usize;
    assert_eq!(depex_size, 22);
    assert_eq!(bytes[0x1B], SECTION_DEPEX);
    let mut expect_depex = vec![0x02];
    expect_depex.extend_from_slice(&PCD_PROTOCOL_GUID_LE);
    expect_depex.push(0x08);
    assert_eq!(&bytes[0x1C..0x1C + expect_depex.len()], &expect_depex[..]);

    let guided_off = (0x18 + depex_size + 3) & !3;
    let guided_size = uint24_to_u32([
        bytes[guided_off],
        bytes[guided_off + 1],
        bytes[guided_off + 2],
    ]) as usize;
    assert_eq!(bytes[guided_off + 3], SECTION_GUIDED);
    assert_eq!(guided_off + guided_size, bytes.len());
    assert_eq!(&bytes[guided_off + 4..guided_off + 20], &LZMA_GUID[..]);
    let data_offset = u16::from_le_bytes([bytes[guided_off + 20], bytes[guided_off + 21]]) as usize;
    assert_eq!(data_offset, 24);
    let payload = &bytes[guided_off + data_offset..guided_off + guided_size];
    let inner = uefi_engine::decompress::decompress(payload, 2).expect("LZMA decode");
    assert_eq!(inner.len(), 14858);

    let mut pe32: Option<&[u8]> = None;
    let mut ui_name = String::new();
    let mut off = 0usize;
    while off + 4 <= inner.len() {
        let sec_size = uint24_to_u32([inner[off], inner[off + 1], inner[off + 2]]) as usize;
        if sec_size < 4 {
            break;
        }
        match inner[off + 3] {
            SECTION_PE32 => pe32 = Some(&inner[off + 4..off + sec_size]),
            SECTION_UI => {
                let body = &inner[off + 4..off + sec_size];
                let units: Vec<u16> = body
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|&u| u != 0)
                    .collect();
                ui_name = String::from_utf16_lossy(&units);
            }
            _ => {}
        }
        off = (off + sec_size + 3) & !3;
    }
    assert_eq!(ui_name, "SerialIo");
    let pe = pe32.expect("no PE32 in donor payload");
    assert_eq!(pe.len(), 14816);
    assert_eq!(&pe[..2], b"MZ");
    let lfanew = u32::from_le_bytes(pe[0x3C..0x40].try_into().unwrap()) as usize;
    assert_eq!(&pe[lfanew..lfanew + 4], b"PE\x00\x00");
    let machine = u16::from_le_bytes(pe[lfanew + 4..lfanew + 6].try_into().unwrap());
    assert_eq!(machine, PE_MACHINE_AMD64);
    let subsystem = u16::from_le_bytes(pe[lfanew + 92..lfanew + 94].try_into().unwrap());
    assert_eq!(subsystem, PE_SUBSYSTEM_BOOT_DRIVER);
    let timestamp = u32::from_le_bytes(pe[lfanew + 8..lfanew + 12].try_into().unwrap());
    assert_eq!(timestamp, 0);
    let entry = u32::from_le_bytes(pe[lfanew + 40..lfanew + 44].try_into().unwrap());
    assert_eq!(entry, 0xBA0);
}
