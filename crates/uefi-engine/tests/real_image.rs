use std::path::PathBuf;

use uefi_engine::ffs::{EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32};
use uefi_engine::parser::file::parse_file;
use uefi_engine::parser::section::parse_sections;
use uefi_engine::parser::volume::parse_volume;
use uefi_engine::types::{FfsNode, FfsType, ParsingData};

fn fw_path() -> PathBuf {
    if let Ok(p) = std::env::var("UEFIPATCHER_TEST_FW") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin")
}

fn load_fw() -> Vec<u8> {
    let path = fw_path();
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const FHV_SIG: [u8; 4] = *b"_FVH";

fn find_sig_offsets(data: &[u8]) -> Vec<usize> {
    let mut out = vec![];
    let mut from = 0usize;
    while let Some(i) = data[from..].windows(4).position(|w| w == FHV_SIG) {
        let abs = from + i;
        out.push(abs);
        from = abs + 1;
    }
    out
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_parses_genuine_volumes() {
    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");

    let mut ok = vec![];
    let mut err = 0usize;
    for sig_off in find_sig_offsets(&data) {
        let fv_off = sig_off.saturating_sub(40);
        if fv_off == sig_off {
            continue;
        }
        match parse_volume(&data, fv_off as u32) {
            Ok(node) => ok.push((fv_off, node)),
            Err(_) => err += 1,
        }
    }

    assert!(ok.len() >= 3, "expected >=3 genuine FVs, got {}", ok.len());

    let known_real: &[u32] = &[0x800000, 0x890000, 0xda0000];
    for &off in known_real {
        let pair = ok
            .iter()
            .find(|(o, _)| *o as u32 == off)
            .unwrap_or_else(|| panic!("genuine FV at {off:#x} did not parse"));
        let node = &pair.1;
        assert_eq!(node.node_type, FfsType::Volume);
        assert_eq!(node.offset, off);
        assert!(node.header.len() >= 56, "header too small");
        assert!(!node.body.is_empty(), "empty body");
    }

    assert!(
        err >= 5,
        "expected >=5 false-positive signatures rejected, got {err}"
    );

    eprintln!(
        "real_image: {} genuine FVs parsed, {err} false positives rejected",
        ok.len()
    );
    for (off, node) in &ok {
        eprintln!(
            "  FV @{off:#x}: header={} body={} subtype={}",
            node.header.len(),
            node.body.len(),
            node.subtype
        );
    }
}

fn scan_ffs_files(body: &[u8], erase: u8, revision: u8) -> Vec<FfsNode> {
    let mut out = vec![];
    let mut pos = 0usize;
    while pos + 24 <= body.len() {
        pos = (pos + 7) & !7;
        if pos + 24 > body.len() {
            break;
        }
        if body[pos] == erase {
            match body[pos..].iter().position(|&b| b != erase) {
                Some(rel) => {
                    pos += rel;
                    continue;
                }
                None => break,
            }
        }
        match parse_file(body, pos as u32, erase, revision) {
            Ok(node) => {
                let total = node.header.len() + node.body.len() + node.tail.len();
                if total == 0 {
                    break;
                }
                out.push(node);
                pos += total;
            }
            Err(_) => pos += 8,
        }
    }
    out
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_parses_ffs_files() {
    let data = load_fw();

    let vol = parse_volume(&data, 0x800000).expect("FV @0x800000");
    let (erase, revision) = match &vol.parsing_data {
        ParsingData::Volume(v) => (v.empty_byte, v.revision),
        _ => panic!("expected Volume parsing data"),
    };
    let files = scan_ffs_files(&vol.body, erase, revision);

    assert_eq!(
        files.len(),
        1,
        "FV @0x800000 should hold exactly 1 FFS file"
    );
    let f = &files[0];
    assert_eq!(f.node_type, FfsType::File);
    assert_eq!(f.subtype, 0x01, "RAW file type expected");
    assert_eq!(f.header.len(), 24, "FFSv2 header");
    assert_eq!(f.tail.len(), 0, "revision 2 has no tail");
    assert_eq!(
        f.header.len() + f.body.len(),
        0x3ffb8,
        "total FFS size must match header Size field"
    );

    eprintln!(
        "real_image FFS @0x800000: type={:#x} header={} body={} guid={:?}",
        f.subtype,
        f.header.len(),
        f.body.len(),
        f.guid
    );

    for fv_off in [0x890000u32, 0xda0000u32] {
        let v = parse_volume(&data, fv_off).expect("FV");
        let (erase, rev) = match &v.parsing_data {
            ParsingData::Volume(d) => (d.empty_byte, d.revision),
            _ => unreachable!(),
        };
        let n = scan_ffs_files(&v.body, erase, rev).len();
        eprintln!(
            "real_image FFS @ {fv_off:#x}: {n} straight FFS files (non-FFS body needs volume-body parser, Task 8)"
        );
    }
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_parses_sections() {
    let data = load_fw();

    let mut total = 0usize;
    let mut pe32 = 0usize;
    let mut guided = 0usize;

    for fv_off in [0x890000u32, 0xda0000u32] {
        let vol = parse_volume(&data, fv_off).expect("FV");
        let (erase, rev) = match &vol.parsing_data {
            ParsingData::Volume(d) => (d.empty_byte, d.revision),
            _ => unreachable!(),
        };
        for f in scan_ffs_files(&vol.body, erase, rev) {
            if f.body.is_empty() {
                continue;
            }
            for s in parse_sections(&f.body, 0) {
                total += 1;
                match s.subtype {
                    EFI_SECTION_PE32 => pe32 += 1,
                    EFI_SECTION_GUID_DEFINED => guided += 1,
                    _ => {}
                }
            }
        }
    }

    eprintln!(
        "real_image sections: total={total} pe32={pe32} guided={guided} (inner sections of compressed bodies need Task 7 decompression)"
    );
    assert!(total > 200, "expected many sections, got {total}");
    assert!(
        guided > 100,
        "expected many GUID_DEFINED wrappers, got {guided}"
    );
    assert!(pe32 > 30, "expected PE32 DXE modules, got {pe32}");
}
