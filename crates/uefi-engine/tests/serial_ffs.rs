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
