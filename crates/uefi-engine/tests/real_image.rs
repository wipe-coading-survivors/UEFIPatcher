use std::path::PathBuf;

use uefi_engine::parser::volume::parse_volume;
use uefi_engine::types::FfsType;

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
