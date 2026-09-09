use std::path::PathBuf;

use uefi_engine::ffs::{EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, is_lzma_guid};
use uefi_engine::parser::file::parse_file;
use uefi_engine::parser::image::{list_items, parse_image, search};
use uefi_engine::parser::section::parse_sections;
use uefi_engine::parser::target::{find_item, parse_target};
use uefi_engine::parser::volume::parse_volume;
use uefi_engine::types::{Action, FfsNode, FfsType, Guid, Image, ImageMode, ParsingData};

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

fn find_guided_ui_target(
    node: &FfsNode,
    path: &mut Vec<usize>,
    out: &mut Option<(Vec<usize>, String)>,
) {
    if out.is_some() {
        return;
    }
    if node.node_type == FfsType::Section
        && node.subtype == EFI_SECTION_GUID_DEFINED
        && matches!(
            &node.parsing_data,
            ParsingData::GuidedSection(d)
                if uefi_engine::ffs::is_recompressable_lzma_guid(&d.guid)
        )
    {
        for (i, child) in node.children.iter().enumerate() {
            if child.node_type == FfsType::Section
                && child.subtype == uefi_engine::ffs::EFI_SECTION_UI
            {
                let units: Vec<u16> = child
                    .body
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let name = String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .to_string();
                let mut ui_path = path.clone();
                ui_path.push(i);
                *out = Some((ui_path, name));
                return;
            }
        }
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        find_guided_ui_target(child, path, out);
        path.pop();
        if out.is_some() {
            return;
        }
    }
}

fn collect_ui_names(node: &FfsNode, out: &mut Vec<String>) {
    if node.node_type == FfsType::Section && node.subtype == uefi_engine::ffs::EFI_SECTION_UI {
        let units: Vec<u16> = node
            .body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        out.push(
            String::from_utf16_lossy(&units)
                .trim_end_matches('\0')
                .to_string(),
        );
    }
    for child in &node.children {
        collect_ui_names(child, out);
    }
}

fn collect_ui_owners(node: &FfsNode, owner: Option<Guid>, out: &mut Vec<(String, Guid)>) {
    let owner = if node.node_type == FfsType::File {
        node.guid
    } else {
        owner
    };
    if node.node_type == FfsType::Section
        && node.subtype == uefi_engine::ffs::EFI_SECTION_UI
        && let Some(g) = owner
    {
        let units: Vec<u16> = node
            .body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let name = String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string();
        out.push((name, g));
    }
    for child in &node.children {
        collect_ui_owners(child, owner, out);
    }
}

fn first_hii_blob_has_string_package(pe: &[u8]) -> bool {
    let blobs = uefi_engine::hii::pe_resource::hii_resource_blobs(pe);
    let Some(blob) = blobs.first() else {
        return false;
    };
    uefi_engine::hii::package_list::parse_package_list(blob).is_some_and(|list| {
        list.packages
            .iter()
            .any(|p| p.kind == uefi_engine::hii::string_pack::PACKAGE_STRINGS)
    })
}

fn find_hii_string_pe32_path(
    node: &FfsNode,
    owner: Option<Guid>,
    path: &mut Vec<usize>,
    out: &mut Option<Vec<usize>>,
) {
    if out.is_some() {
        return;
    }
    let owner = if node.node_type == FfsType::File {
        node.guid
    } else {
        owner
    };
    if node.node_type == FfsType::Section
        && node.subtype == EFI_SECTION_PE32
        && node.children.is_empty()
        && owner.is_some()
        && first_hii_blob_has_string_package(&node.body)
    {
        *out = Some(path.clone());
        return;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        find_hii_string_pe32_path(child, owner, path, out);
        path.pop();
        if out.is_some() {
            return;
        }
    }
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

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_decompresses_lzma_sections() {
    let data = load_fw();

    let mut lzma_total = 0usize;
    let mut lzma_decompressed = 0usize;
    let mut inner_pe32 = 0usize;
    let mut inner_total = 0usize;

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
                if s.subtype != EFI_SECTION_GUID_DEFINED {
                    continue;
                }
                let guid = match &s.parsing_data {
                    ParsingData::GuidedSection(d) => &d.guid,
                    _ => continue,
                };
                if !is_lzma_guid(guid) {
                    continue;
                }
                lzma_total += 1;
                if s.children.is_empty() {
                    continue;
                }
                lzma_decompressed += 1;
                for child in &s.children {
                    inner_total += 1;
                    if child.subtype == EFI_SECTION_PE32 {
                        inner_pe32 += 1;
                    }
                }
            }
        }
    }

    eprintln!(
        "real_image LZMA: guided_lzma={lzma_total} decompressed_ok={lzma_decompressed} inner_sections={inner_total} inner_pe32={inner_pe32}"
    );
    assert!(
        lzma_total > 100,
        "expected many LZMA GUIDed sections, got {lzma_total}"
    );
    assert_eq!(
        lzma_decompressed, lzma_total,
        "every LZMA GUIDed section must decompress"
    );
    assert!(
        inner_pe32 > 30,
        "expected PE32 DXE modules inside decompressed bodies, got {inner_pe32}"
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_parse_image_full() {
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    assert_eq!(img.root.node_type, FfsType::Image);
    assert!(
        img.root.children.len() >= 3,
        "expected >=3 top-level volumes, got {}",
        img.root.children.len()
    );

    for &off in &[0x800000u32, 0x890000u32, 0xda0000u32] {
        let vol = img
            .root
            .children
            .iter()
            .find(|c| c.offset == off)
            .unwrap_or_else(|| panic!("parse_image did not locate FV @{off:#x}"));
        assert_eq!(vol.node_type, FfsType::Volume);
        assert!(vol.header.len() >= 56, "FV @{off:#x}: header too small");
        assert!(!vol.body.is_empty(), "FV @{off:#x}: empty body");
    }

    let main = img
        .root
        .children
        .iter()
        .find(|c| c.offset == 0x890000)
        .expect("main FV");
    assert!(
        main.children.len() > 100,
        "main FV should hold many FFS files, got {}",
        main.children.len()
    );
    assert!(
        main.children.iter().all(|f| f.node_type == FfsType::File),
        "all volume children must be FFS files"
    );

    let mut guided = 0usize;
    let mut decompressed = 0usize;
    for f in &main.children {
        for s in &f.children {
            if s.subtype == EFI_SECTION_GUID_DEFINED {
                guided += 1;
                if !s.children.is_empty() {
                    decompressed += 1;
                }
            }
        }
    }
    assert!(guided > 100, "main FV guided sections: {guided}");
    assert_eq!(
        decompressed, guided,
        "all guided sections in main FV must decompress"
    );

    let items = list_items(&img.root, None);
    assert!(
        items.len() > 1000,
        "expected many list items, got {}",
        items.len()
    );
    assert!(
        items
            .iter()
            .any(|i| i.r#type == FfsType::File as u32 && i.name == "PeiCore"),
        "expected an FFS file named 'PeiCore' (UI section lifted), got names: {:?}",
        items
            .iter()
            .filter(|i| i.r#type == FfsType::File as u32 && !i.name.is_empty())
            .map(|i| &i.name)
            .take(10)
            .collect::<Vec<_>>()
    );

    eprintln!(
        "real_image parse_image: {} top-level volumes, main FV files={}, guided={}, decompressed={}, items={}, named_files={}",
        img.root.children.len(),
        main.children.len(),
        guided,
        decompressed,
        items.len(),
        items
            .iter()
            .filter(|i| i.r#type == FfsType::File as u32 && !i.name.is_empty())
            .count(),
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_target_and_find_item() {
    use uefi_engine::guid_to_upper_string;
    use uefi_engine::types::{ImageMode, Target};

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let main = img
        .root
        .children
        .iter()
        .find(|c| c.offset == 0x890000)
        .expect("main FV");

    let file = main
        .children
        .iter()
        .find(|f| f.guid.is_some() && f.children.iter().any(|s| s.subtype == EFI_SECTION_PE32))
        .expect("file with a PE32 section");
    let file_guid = file.guid.unwrap();
    let guid_str = guid_to_upper_string(&file_guid);
    let expected_offset = file.offset;

    let t_guid = parse_target(&guid_str).expect("parse guid target");
    assert!(matches!(t_guid, Target::Guid(_)));
    let found = find_item(&img.root, &t_guid).expect("find by guid");
    assert_eq!(found.node_type, FfsType::File);
    assert_eq!(found.offset, expected_offset);

    let t_sec = parse_target(&format!("{guid_str}:0x10")).expect("parse guid:type target");
    match &t_sec {
        Target::GuidSection { section_type, .. } => assert_eq!(*section_type, 0x10),
        other => panic!("expected GuidSection, got {other:?}"),
    }
    let section = find_item(&img.root, &t_sec).expect("find section by type");
    assert_eq!(section.subtype, EFI_SECTION_PE32);

    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.node_type == FfsType::Volume)
        .expect("at least one Volume child");
    let t_path = parse_target(&format!("{vol_idx}/0")).expect("parse path target");
    assert!(matches!(t_path, Target::Path(_)));
    let first_file = find_item(&img.root, &t_path).expect("find by path");
    assert_eq!(first_file.node_type, FfsType::File);

    assert!(parse_target("not-a-target").is_err());

    eprintln!(
        "real_image target: guid={guid_str} file_offset={expected_offset:#x} pe32_ok path_ok"
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_builder_round_trip() {
    use uefi_engine::builder::build_image;
    use uefi_engine::types::ImageMode;

    let data = load_fw();

    for fv_off in [0x800000u64, 0x890000u64, 0xda0000u64] {
        let fvlen = u64::from_le_bytes(
            data[fv_off as usize + 32..fv_off as usize + 40]
                .try_into()
                .unwrap(),
        );
        let slice = &data[fv_off as usize..fv_off as usize + fvlen as usize];

        let img = parse_image(slice, ImageMode::Read, "img1", "s1").expect("parse_image");
        assert!(!img.root.children.is_empty(), "FV @{fv_off:#x} not parsed");
        let rebuilt = build_image(&img).expect("build_image");
        assert_eq!(
            rebuilt,
            slice,
            "FV @{fv_off:#x} round-trip mismatch (rebuilt {} != orig {})",
            rebuilt.len(),
            slice.len()
        );
    }

    let main_len = u64::from_le_bytes(data[0x890000 + 32..0x890000 + 40].try_into().unwrap());
    let main_slice = &data[0x890000..0x890000 + main_len as usize];
    let img = parse_image(main_slice, ImageMode::Read, "img1", "s1").unwrap();
    let vol = &img.root.children[0];
    eprintln!(
        "real_image round-trip: main FV files={}, sections rebuilt verbatim (incl. {} guided LZMA)",
        vol.children.len(),
        vol.children
            .iter()
            .map(|f| f
                .children
                .iter()
                .filter(|s| s.subtype == EFI_SECTION_GUID_DEFINED)
                .count())
            .sum::<usize>()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_remove_last_file() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::remove;
    use uefi_engine::types::{ImageMode, Target};

    let data = load_fw();
    let main_len = u64::from_le_bytes(data[0x890000 + 32..0x890000 + 40].try_into().unwrap());
    let slice = data[0x890000..0x890000 + main_len as usize].to_vec();

    let mut img = parse_image(&slice, ImageMode::Read, "img1", "s1").unwrap();
    let vol = &img.root.children[0];
    let original_count = vol.children.len();
    assert!(original_count > 1);

    let last_idx = original_count - 1;
    let removed_guid = vol.children[last_idx].guid;
    let target = Target::Path(vec![0, last_idx]);
    remove(&mut img.root, &target).unwrap();
    assert_eq!(
        img.root.children[0].children[last_idx].action,
        Action::Remove
    );
    assert_eq!(img.root.action, Action::Rebuild);

    let rebuilt = build_image(&img).expect("build after remove");
    assert_eq!(
        rebuilt.len(),
        slice.len(),
        "volume length must be preserved (size absorbed into free space)"
    );

    let re_img = parse_image(&rebuilt, ImageMode::Read, "img1", "s1").unwrap();
    let new_count = re_img.root.children[0].children.len();
    assert_eq!(
        new_count,
        original_count - 1,
        "re-parsed volume must hold one fewer file"
    );

    if let Some(g) = removed_guid {
        let still_present = re_img.root.children[0]
            .children
            .iter()
            .any(|f| f.guid == Some(g));
        assert!(
            !still_present,
            "removed file must not be present after rebuild"
        );
    }

    let stable = build_image(&re_img).expect("stable rebuild");
    assert_eq!(stable, rebuilt, "rebuild of re-parsed tree must be stable");

    eprintln!(
        "real_image ops: removed last file of {original_count}, rebuilt={} re-parsed files={new_count}, length preserved",
        rebuilt.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_search_finds_setup_by_name() {
    use uefi_common::search::SearchMode;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let matches = search(&img.root, "setup", &[SearchMode::Name], 50);
    assert!(
        matches
            .iter()
            .any(|m| m.name == "Setup" && m.r#type == FfsType::Section as u32),
        "expected to find UI section 'Setup' by name; got {} matches: {:?}",
        matches.len(),
        matches.iter().map(|m| &m.name).take(5).collect::<Vec<_>>(),
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_search_finds_utf8_string_in_pe32() {
    use uefi_common::search::SearchMode;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let matches = search(&img.root, ".reloc", &[SearchMode::Utf8], 50);
    assert!(
        matches.len() >= 5,
        "expected multiple PE32 sections containing '.reloc' string, got {}",
        matches.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_full_flash_round_trip() {
    use uefi_engine::builder::build_image;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");

    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let rebuilt = build_image(&img).expect("build_image");

    assert_eq!(
        rebuilt.len(),
        data.len(),
        "full-flash round-trip: size mismatch (rebuilt {} != orig {})",
        rebuilt.len(),
        data.len()
    );
    assert_eq!(rebuilt, data, "full-flash round-trip: byte mismatch");
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_full_flash_repatch_stability() {
    use uefi_engine::builder::build_image;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let built1 = build_image(&img).expect("build_image");
    assert_eq!(built1, data, "first build must equal original");

    let img2 = parse_image(&built1, ImageMode::Read, "img1", "s1").expect("re-parse");
    let built2 = build_image(&img2).expect("second build_image");

    assert_eq!(
        built2, built1,
        "re-patch instability: second build differs from first"
    );
    assert_eq!(
        built2, data,
        "re-patch instability: second build differs from original"
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_forms_and_strings() {
    use std::collections::HashSet;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::strings::collect_strings;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    assert!(!forms.is_empty(), "expected forms in real image");
    let formsets: HashSet<&str> = forms.iter().map(|f| f.formset_guid.as_str()).collect();
    assert!(
        formsets.len() >= 2,
        "expected >=2 formsets (Setup + Platform/IntelRCSetup), got {formsets:?}"
    );
    assert!(
        forms.iter().all(|f| f.form_id.contains(':')),
        "every form_id must be a GUID-section target"
    );

    let strings = collect_strings(&img);
    assert!(!strings.is_empty(), "expected strings in real image");
    let languages: HashSet<&str> = strings.iter().map(|s| s.language.as_str()).collect();
    assert!(
        languages.iter().any(|l| l.to_lowercase().starts_with("en")),
        "expected primary language ~English (en/en-US/eng), got {languages:?}"
    );
    assert!(
        languages.len() >= 2,
        "expected >=2 string-package languages under full traversal, got {languages:?}"
    );
    assert!(
        strings
            .iter()
            .any(|s| s.language.starts_with("x-UEFI-AMI") && !s.text.is_empty()),
        "expected non-empty x-UEFI-AMI token package alongside en-US display package, got {languages:?}"
    );

    eprintln!(
        "real_image hii: {} forms across {} formsets {:?}; {} strings across {} languages {:?}",
        forms.len(),
        formsets.len(),
        formsets,
        strings.len(),
        languages.len(),
        languages
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_form_visibility_round_trip() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::set_item_visibility;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    assert!(!forms.is_empty());
    let hidden = forms
        .iter()
        .find(|f| !f.visible)
        .expect("real image has suppressed forms");
    let form_id = hidden.form_id.clone();
    let form_id_ifr = hidden.form_id_ifr;
    let item_id = format!("{form_id}#{form_id_ifr}");

    let t = parse_target(&form_id).expect("form_id parses as target");
    let node = find_item(&img.root, &t).expect("form_id resolves in tree");
    assert_eq!(
        node.node_type,
        FfsType::Section,
        "form target must resolve to a Section"
    );
    assert_eq!(
        node.subtype, 0x10,
        "HNX99TF HII targets are PE32 sections behind LZMA"
    );

    set_item_visibility(&mut img, &item_id, true)
        .expect("unsuppress specific form inside PE 'HII' resource behind recompressable LZMA");

    let built = build_image(&img).expect("build_image after PE-resource unsuppress");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    assert_eq!(
        &built[..0x890000],
        &data[..0x890000],
        "bytes before FV1 must be untouched"
    );
    assert_eq!(
        &built[0xd60000..],
        &data[0xd60000..],
        "bytes after FV1 must be untouched"
    );

    let re = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let forms2 = collect_forms(&re);
    let again = forms2
        .iter()
        .find(|f| f.form_id == form_id && f.form_id_ifr == form_id_ifr)
        .expect("patched form survives rebuild");
    assert!(again.visible, "form must be visible after round-trip");
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_formset_add_pe_resource_grows_reloc_tail() {
    use uefi_engine::builder::build_image;
    use uefi_engine::guid_to_upper_string;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::formset_add::add_setup_formset;
    use uefi_engine::hii::pe_resource::try_grow_rsrc_tail;
    use uefi_engine::hii::schema;
    use uefi_engine::types::ImageMode;

    const FORMSET_GUID: &str = "6A7C8B9D-4E5F-4A61-B2C3-9ABCDEF01234";
    const FORM_ID: u16 = 0x7A11;
    const FORM_TITLE: &str = "PATCHER ACCEPTANCE";

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == 0x890000)
        .expect("main FV @0x890000");

    let mut hii_owner = None;
    let mut walk_path = vec![vol_idx];
    find_hii_string_pe32_path(
        &img.root.children[vol_idx],
        None,
        &mut walk_path,
        &mut hii_owner,
    );
    let pe_path = hii_owner.expect("FV1 PE32 'HII' resource with a STRING package");
    let target_guid = img.root.children[vol_idx].children[pe_path[1]]
        .guid
        .unwrap();
    let target_prefix = guid_to_upper_string(&target_guid);

    let mut ui_owners = vec![];
    collect_ui_owners(&img.root, None, &mut ui_owners);
    let find_ui = |needle: &str| {
        ui_owners
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(needle))
            .map(|(_, g)| *g)
    };
    let setupdata_guid = find_ui("AMITSESetupData").expect("AMITSESetupData UI-named file");
    let amitse_guid = find_ui("AMITSE").expect("AMITSE UI-named file");
    let bogus_ami = Guid::try_parse("00000000-0000-0000-0000-00000000DEAD").unwrap();

    let mut node = &img.root;
    for &i in &pe_path {
        node = &node.children[i];
    }
    assert_eq!(node.node_type, FfsType::Section);
    assert_eq!(node.subtype, EFI_SECTION_PE32);
    let pe_snapshot = node.body.clone();
    let mut pe_probe = node.body.clone();
    assert!(
        try_grow_rsrc_tail(&mut pe_probe, 16),
        "reloc-aware growth must accept HNX99TF HII PE geometry (.reloc trails .rsrc, raw ends at EOF)"
    );
    assert!(
        pe_probe.len() > pe_snapshot.len(),
        "growth must extend the PE by the file-align-rounded delta"
    );

    let mk_schema = |sd: Guid, am: Guid| schema::FormSetSchema {
        formset_guid: FORMSET_GUID.into(),
        title: "PATCHER ACCEPTANCE FORMSET".into(),
        help: "PATCHER ACCEPTANCE HELP".into(),
        class_guids: vec![],
        varstores: vec![],
        default_stores: vec![],
        forms: vec![schema::FormSchema {
            id: FORM_ID,
            title: FORM_TITLE.into(),
            items: vec![schema::ItemSchema::Text(schema::TextItem {
                prompt: "PATCHER PROMPT".into(),
                help: "PATCHER ITEM HELP".into(),
                text_two: "PATCHER TEXT TWO".into(),
            })],
        }],
        setupdata_guid: Some(guid_to_upper_string(&sd)),
        amitse_guid: Some(guid_to_upper_string(&am)),
    };

    let err = add_setup_formset(
        &mut img,
        &mk_schema(bogus_ami, bogus_ami),
        Some(&target_guid),
    )
    .map(|_| ());
    assert!(
        matches!(err, Err(uefi_engine::hii::HiiError::AmiFilesNotFound)),
        "AMI pre-check must fire before any mutation, got {err:?}"
    );

    let mut node = &img.root;
    for &i in &pe_path {
        node = &node.children[i];
    }
    assert_eq!(node.body, pe_snapshot, "PE body must stay byte-identical");
    assert_all_no_action(&img.root);

    let res = add_setup_formset(
        &mut img,
        &mk_schema(setupdata_guid, amitse_guid),
        Some(&target_guid),
    )
    .expect("add_setup_formset must succeed on the .reloc-trailing HII PE");
    assert_eq!(res.new_ffs_guid, Guid::try_parse(FORMSET_GUID).unwrap());
    assert_eq!(res.inserted_form_ids, vec![FORM_ID]);

    let mut node = &img.root;
    for &i in &pe_path {
        node = &node.children[i];
    }
    assert!(
        node.body.len() > pe_snapshot.len(),
        "PE resource body must grow to hold the new strings and formset"
    );

    let built = build_image(&img).expect("build_image after formset add");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    assert_eq!(
        &built[..0x890000],
        &data[..0x890000],
        "bytes before FV1 must be untouched"
    );
    assert_eq!(
        &built[0xd60000..],
        &data[0xd60000..],
        "bytes after FV1 must be untouched"
    );

    let re = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let forms = collect_forms(&re);
    let added = forms
        .iter()
        .find(|f| f.formset_guid == FORMSET_GUID)
        .expect("added formset must be visible after round-trip");
    assert_eq!(added.title, FORM_TITLE);
    assert_eq!(added.form_id_ifr, u32::from(FORM_ID));

    eprintln!(
        "real_image formset-add growth: target={target_prefix} (ui-backed PE32 'HII' file, .reloc after .rsrc), setupdata={} (AMITSESetupData), amitse={} (AMITSE), formset={FORMSET_GUID} title={FORM_TITLE}",
        guid_to_upper_string(&setupdata_guid),
        guid_to_upper_string(&amitse_guid)
    );
}

fn find_file_node(node: &FfsNode, guid: Guid) -> Option<&FfsNode> {
    for c in &node.children {
        if c.node_type == FfsType::File && c.guid == Some(guid) {
            return Some(c);
        }
        if let Some(found) = find_file_node(c, guid) {
            return Some(found);
        }
    }
    None
}

fn find_first_leaf(node: &FfsNode, pred: impl Fn(&FfsNode) -> bool + Copy) -> Option<&FfsNode> {
    for c in &node.children {
        if c.children.is_empty() && pred(c) {
            return Some(c);
        }
        if let Some(found) = find_first_leaf(c, pred) {
            return Some(found);
        }
    }
    None
}

fn collect_rebuilt_file_regions(node: &FfsNode, out: &mut Vec<(usize, usize)>) {
    if node.node_type == FfsType::File && node.action == Action::Rebuild {
        let start = node.offset as usize;
        out.push((
            start,
            start + node.header.len() + node.body.len() + node.tail.len(),
        ));
    }
    for c in &node.children {
        collect_rebuilt_file_regions(c, out);
    }
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_formset_add_ami_records_in_built_bytes() {
    use uefi_engine::builder::build_image;
    use uefi_engine::guid_to_upper_string;
    use uefi_engine::hii::ami_patcher::{
        AMI_DEFAULT_ACCESS_LEVEL, AMI_RECORD_SIZE, QuestionAmiRecord, make_ami_record,
    };
    use uefi_engine::hii::formset_add::add_setup_formset;
    use uefi_engine::hii::schema;
    use uefi_engine::types::ImageMode;

    const FORMSET_GUID: &str = "5E7D8C9E-4F5A-4B62-C3D4-9BCDEF012345";
    const FORM_ID: u16 = 0x7A21;
    const QUESTION_ID: u16 = 0x7F71;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();

    let mut ui_owners = vec![];
    collect_ui_owners(&img.root, None, &mut ui_owners);
    let find_ui = |needle: &str| {
        ui_owners
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(needle))
            .map(|(_, g)| *g)
            .unwrap()
    };
    let setupdata_guid = find_ui("AMITSESetupData");
    let amitse_guid = find_ui("AMITSE");
    let setup_module_guid = Guid::try_parse("899407D7-99FE-43D8-9A21-79EC328CAC21").unwrap();

    let orig_pe32 = find_first_leaf(find_file_node(&img.root, amitse_guid).unwrap(), |n| {
        n.subtype == EFI_SECTION_PE32
    })
    .unwrap()
    .body
    .clone();
    let orig_pfs = find_first_leaf(find_file_node(&img.root, setupdata_guid).unwrap(), |n| {
        n.body.windows(4).any(|w| w == b"$SPF")
    })
    .unwrap()
    .body
    .clone();

    let sc = schema::FormSetSchema {
        formset_guid: FORMSET_GUID.into(),
        title: "AMI PATCH OP FORMSET".into(),
        help: "AMI PATCH OP HELP".into(),
        class_guids: vec![],
        varstores: vec![],
        default_stores: vec![],
        forms: vec![schema::FormSchema {
            id: FORM_ID,
            title: "AMI PATCH OP FORM".into(),
            items: vec![schema::ItemSchema::Numeric(schema::NumericItem {
                prompt: "AMI PATCH OP PROMPT".into(),
                help: "AMI PATCH OP ITEM HELP".into(),
                question_id: QUESTION_ID,
                var_store_id: 0x7F00,
                var_offset: 0,
                size: 1,
                min: 0,
                max: 255,
                step: 1,
                display: schema::DisplayMode::UintDec,
                defaults: schema::Defaults::default(),
            })],
        }],
        setupdata_guid: Some(guid_to_upper_string(&setupdata_guid)),
        amitse_guid: Some(guid_to_upper_string(&amitse_guid)),
    };
    let res = add_setup_formset(&mut img, &sc, Some(&setup_module_guid))
        .expect("add_setup_formset with AMI guids must succeed on HNX99TF");
    assert_eq!(res.inserted_form_ids, vec![FORM_ID]);

    let mut allowed = vec![];
    collect_rebuilt_file_regions(&img.root, &mut allowed);
    assert!(
        allowed.len() >= 3,
        "expected the HII module plus both AMI files to be rebuilt, got {allowed:?}"
    );

    let built = build_image(&img).expect("build_image after formset add");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    eprintln!("allowed regions: {allowed:?}");
    let mut outside = vec![];
    for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
        if a != b && !allowed.iter().any(|(s, e)| i >= *s && i < *e) {
            outside.push(i);
        }
    }
    if !outside.is_empty() {
        let mut ranges = vec![];
        let mut s0 = outside[0];
        let mut p0 = outside[0];
        for &i in &outside[1..] {
            if i == p0 + 1 {
                p0 = i;
            } else {
                ranges.push((s0, p0));
                s0 = i;
                p0 = i;
            }
        }
        ranges.push((s0, p0));
        eprintln!("outside diff ranges: {ranges:?}");
        panic!(
            "{} bytes changed outside rebuilt AMI/HII files",
            outside.len()
        );
    }

    let record = make_ami_record(&QuestionAmiRecord {
        question_id: QUESTION_ID,
        page_id: None,
        access_level: AMI_DEFAULT_ACCESS_LEVEL,
        failsafe: 0,
        optimal: 0,
    });

    let re = parse_image(&built, ImageMode::Read, "img2", "s2").unwrap();
    let pfs = find_first_leaf(find_file_node(&re.root, setupdata_guid).unwrap(), |n| {
        n.body.windows(4).any(|w| w == b"$SPF")
    })
    .unwrap();
    assert_eq!(pfs.body.len(), orig_pfs.len() + AMI_RECORD_SIZE);
    assert_eq!(
        &pfs.body[pfs.body.len() - AMI_RECORD_SIZE..],
        record.as_slice()
    );

    let pe32 = find_first_leaf(find_file_node(&re.root, amitse_guid).unwrap(), |n| {
        n.subtype == EFI_SECTION_PE32
    })
    .unwrap();
    assert_eq!(pe32.body.len(), orig_pe32.len() + 2);
    let entries = FORM_ID.to_le_bytes();
    let diverge = pe32
        .body
        .iter()
        .zip(orig_pe32.iter())
        .position(|(a, b)| a != b)
        .unwrap_or(orig_pe32.len());
    assert_eq!(
        &pe32.body[diverge..diverge + 2],
        &entries,
        "AMITSE PE32 must carry the form-id entry at the splice position"
    );
    assert_eq!(&pe32.body[..diverge], &orig_pe32[..diverge]);
    assert_eq!(&pe32.body[diverge + 2..], &orig_pe32[diverge..]);

    eprintln!(
        "real_image ami-patch-op: setupdata={} (records appended into $SPF), amitse={} (form-id splice @pe+{:#x}), rebuilt files={allowed:?}",
        guid_to_upper_string(&setupdata_guid),
        guid_to_upper_string(&amitse_guid),
        diverge
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_form_add_into_setup_formset() {
    use std::collections::BTreeSet;
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::form_add::add_form;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::schema;
    use uefi_engine::types::ImageMode;

    const SETUP_FILE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const SETUP_FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";
    const FORM_TITLE: &str = "PATCHER ADDED FORM";
    const NUM_PROMPT: &str = "PATCHER ADDED PROMPT";
    const NUM_HELP: &str = "PATCHER ADDED HELP";
    const VARSTORE_ID: u16 = 0x7F00;
    const QUESTION_ID: u16 = 0x7F01;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    let setup: Vec<_> = forms
        .iter()
        .filter(|f| f.formset_guid == SETUP_FORMSET_GUID)
        .collect();
    assert!(!setup.is_empty(), "Setup formset must exist in HNX99TF");
    let targets: BTreeSet<&str> = setup.iter().map(|f| f.form_id.as_str()).collect();
    assert_eq!(
        targets.len(),
        1,
        "Setup formset must live in a single section target, got {targets:?}"
    );
    let item_id = targets.iter().next().unwrap().to_string();
    let mut target_parts = item_id.split(':');
    let target_file_guid = target_parts.next().unwrap_or_default();
    let target_section_type = target_parts.next().unwrap_or_default();
    assert_eq!(
        target_file_guid.to_ascii_uppercase(),
        SETUP_FILE_GUID,
        "Setup formset must live in the Setup file, got {item_id}"
    );
    assert_eq!(
        target_section_type, "0x10",
        "Setup HII channel must be a PE32 section, got {item_id}"
    );

    let new_form_id =
        u16::try_from(setup.iter().map(|f| f.form_id_ifr).max().unwrap() + 1).unwrap();
    assert!(
        setup
            .iter()
            .all(|f| u32::from(new_form_id) != f.form_id_ifr),
        "new form id must not collide with existing Setup forms"
    );

    let sc = schema::FormSetSchema {
        formset_guid: SETUP_FORMSET_GUID.into(),
        title: FORM_TITLE.into(),
        help: NUM_HELP.into(),
        class_guids: vec![],
        varstores: vec![schema::VarStoreSchema {
            id: VARSTORE_ID,
            guid: "89ABCDEF-0123-4DEF-8ABC-0123456789AB".into(),
            size: 64,
            name: "PatcherVar".into(),
            var_type: schema::VarStoreType::Buffer,
        }],
        default_stores: vec![],
        forms: vec![schema::FormSchema {
            id: new_form_id,
            title: FORM_TITLE.into(),
            items: vec![schema::ItemSchema::Numeric(schema::NumericItem {
                prompt: NUM_PROMPT.into(),
                help: NUM_HELP.into(),
                question_id: QUESTION_ID,
                var_store_id: VARSTORE_ID,
                var_offset: 0,
                size: 1,
                min: 0,
                max: 255,
                step: 1,
                display: schema::DisplayMode::UintDec,
                defaults: schema::Defaults::default(),
            })],
        }],
        setupdata_guid: None,
        amitse_guid: None,
    };

    let res = add_form(&mut img, &item_id, &sc).expect("add_form into live Setup formset");
    assert_eq!(res.inserted_form_ids, vec![new_form_id]);
    assert_eq!(
        res.string_ids.len(),
        4,
        "varstore name + form title + prompt + help strings must be appended"
    );
    assert!(res.string_ids.contains_key(FORM_TITLE));

    let built = build_image(&img).expect("build_image after form add");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    assert_eq!(
        &built[..0x890000],
        &data[..0x890000],
        "bytes before FV1 must be untouched"
    );
    assert_eq!(
        &built[0xd60000..],
        &data[0xd60000..],
        "bytes after FV1 must be untouched"
    );

    let re = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let forms2 = collect_forms(&re);
    let added: Vec<_> = forms2
        .iter()
        .filter(|f| f.formset_guid == SETUP_FORMSET_GUID && f.form_id_ifr == u32::from(new_form_id))
        .collect();
    assert_eq!(
        added.len(),
        1,
        "new form must appear exactly once under the Setup formset"
    );
    assert_eq!(added[0].form_id, item_id);
    assert_eq!(
        added[0].title, FORM_TITLE,
        "title must resolve from the appended strings"
    );
    assert!(added[0].visible, "added form must not be suppressed");

    eprintln!(
        "real_image form-add: setup target={item_id} formset={SETUP_FORMSET_GUID} new_form_id={new_form_id:#06x} title='{FORM_TITLE}'"
    );
}

fn assert_all_no_action(node: &FfsNode) {
    assert_eq!(node.action, Action::NoAction);
    for child in &node.children {
        assert_all_no_action(child);
    }
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_recompress_remove_ui_inside_lzma() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::remove;
    use uefi_engine::types::{ImageMode, Target};

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let mut found = None;
    let mut path = vec![];
    find_guided_ui_target(&img.root, &mut path, &mut found);
    let (ui_path, ui_name) = found.expect("guided LZMA section with a UI child");
    assert!(!ui_name.is_empty());

    let mut before_names = vec![];
    collect_ui_names(&img.root, &mut before_names);
    let occurrences_before = before_names.iter().filter(|n| **n == ui_name).count();
    assert!(occurrences_before >= 1);

    remove(&mut img.root, &Target::Path(ui_path)).unwrap();

    let built = build_image(&img).expect("build_image after remove inside LZMA");
    assert_eq!(
        built.len(),
        data.len(),
        "total flash length must be preserved"
    );
    assert_eq!(
        &built[..0x890000],
        &data[..0x890000],
        "bytes before FV1 must be untouched"
    );
    assert_eq!(
        &built[0xd60000..],
        &data[0xd60000..],
        "bytes after FV1 must be untouched"
    );

    let re_img = parse_image(&built, ImageMode::Read, "img1", "s1").expect("re-parse");
    let mut after_names = vec![];
    collect_ui_names(&re_img.root, &mut after_names);
    let occurrences_after = after_names.iter().filter(|n| **n == ui_name).count();
    assert_eq!(
        occurrences_after,
        occurrences_before - 1,
        "removed UI section '{ui_name}' must be materialized as absent"
    );
}

const SETUP_MODULE_GUID: &str = "abbce13d-e25a-4d9f-a1f9-2f7710786892";
const HIDDEN_FORM_ID: u16 = 901;

#[ignore]
#[test]
fn real_image_string_list_full_traversal() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let strings = uefi_engine::hii::strings::collect_strings(&img);
    assert!(
        strings.len() > 5000,
        "expected ~5.7k strings, got {}",
        strings.len()
    );
    assert!(strings.iter().any(|s| s.language == "en-US"));
    assert!(strings.iter().any(|s| s.language == "x-UEFI-AMI"));
    let prc = strings
        .iter()
        .filter(|s| s.language == "x-UEFI-AMI")
        .count();
    assert!(prc > 100);
}

fn setup_pe32_node_path(img: &Image) -> Vec<usize> {
    let target =
        uefi_engine::parser::target::parse_target(&format!("{SETUP_MODULE_GUID}:0x10:0")).unwrap();
    uefi_engine::parser::target::find_item_path(&img.root, &target).expect("setup PE32 node")
}

fn file_extent(img: &Image, path: &[usize]) -> (usize, usize) {
    let mut node = &img.root;
    let mut vol = &img.root;
    let mut file_idx = 0usize;
    for &i in path {
        let child = &node.children[i];
        if child.node_type == FfsType::File {
            vol = node;
            file_idx = i;
        }
        node = child;
    }
    let file_start = vol.children[file_idx].offset as usize;
    let file_end = vol.children[file_idx + 1].offset as usize;
    (file_start, file_end)
}

fn guided_body_len(img: &Image, path: &[usize]) -> usize {
    let mut node = &img.root;
    let mut guided: Option<&FfsNode> = None;
    for &i in path {
        node = &node.children[i];
        if node.node_type == FfsType::Section
            && node.subtype == uefi_engine::ffs::EFI_SECTION_GUID_DEFINED
        {
            guided = Some(node);
        }
    }
    guided
        .expect("guided LZMA ancestor of the PE32 section")
        .body
        .len()
}

#[ignore]
#[test]
fn real_image_unhide_rebuild_keeps_layout() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let pe_path = setup_pe32_node_path(&img);
    let (file_start, file_end) = file_extent(&img, &pe_path);
    let guided_before = guided_body_len(&img, &pe_path);

    uefi_engine::hii::set_item_visibility(
        &mut img,
        &format!("{SETUP_MODULE_GUID}:0x10:0#{HIDDEN_FORM_ID}"),
        true,
    )
    .expect("unhide");
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len());
    assert_eq!(&built[..file_start], &data[..file_start]);
    assert_eq!(&built[file_end..], &data[file_end..]);

    let rebuilt = parse_image(&built, ImageMode::Read, "img1", "s1").expect("re-parse");
    let new_path = setup_pe32_node_path(&rebuilt);
    let (_, new_file_end) = file_extent(&rebuilt, &new_path);
    assert_eq!(new_file_end, file_end);
    assert_eq!(guided_body_len(&rebuilt, &new_path), guided_before);
}

const PCI_SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
const E12_FLIP_BYTES: [(usize, u8, u8); 3] = [(0x67A, 1, 2), (0xDD1, 1, 0xFF), (0xDD2, 0, 0xFF)];

fn node_at_path<'a>(img: &'a Image, path: &[usize]) -> &'a FfsNode {
    let mut node = &img.root;
    for &i in path {
        node = &node.children[i];
    }
    node
}

fn module_pe32_node_path(img: &Image, guid: &str) -> Vec<usize> {
    let target = uefi_engine::parser::target::parse_target(&format!("{guid}:0x10:0")).unwrap();
    uefi_engine::parser::target::find_item_path(&img.root, &target).expect("module PE32 node")
}

fn module_form_package(img: &Image, path: &[usize]) -> Vec<u8> {
    let pe = &node_at_path(img, path).body;
    for (off, len) in uefi_engine::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = uefi_engine::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS {
                return pkg.bytes.to_vec();
            }
        }
    }
    panic!("FORM package not found in module .rsrc HII blob");
}

#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
#[test]
fn real_image_hii_unlock_matches_e12() {
    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let pe_path = module_pe32_node_path(&img, PCI_SETUP_MODULE_GUID);
    let (file_start, file_end) = file_extent(&img, &pe_path);
    let pkg_before = module_form_package(&img, &pe_path);

    let form_item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029");
    let question_item = format!("{form_item}:0x3B");

    let form_gates = uefi_engine::hii::gates_list(&img, &form_item).expect("gates_list form");
    assert_eq!(
        form_gates.len(),
        1,
        "form 10029 is gated only by the REF suppress in form 10002"
    );
    assert_eq!(form_gates[0].gate_kind, "suppress");
    assert_eq!(form_gates[0].wraps, "ref");
    assert_eq!(form_gates[0].host_form_id, 10002);
    assert_eq!(form_gates[0].expression, "1 == 1");
    assert!(form_gates[0].flippable, "flip plan: {}", form_gates[0].flip);

    let question_gates =
        uefi_engine::hii::gates_list(&img, &question_item).expect("gates_list question");
    assert_eq!(
        question_gates.len(),
        1,
        "Above 4G Decoding is gated only by its personal grayout"
    );
    assert_eq!(question_gates[0].gate_kind, "grayout");
    assert_eq!(question_gates[0].wraps, "question");
    assert_eq!(question_gates[0].expression, "0x009A == 0x0001");
    assert!(question_gates[0].flippable);

    uefi_engine::hii::unlock(&mut img, &form_item).expect("unlock page");
    uefi_engine::hii::unlock(&mut img, &question_item).expect("unlock question");
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len(), "total flash length preserved");
    assert_eq!(
        &built[..file_start],
        &data[..file_start],
        "bytes before the Setup file untouched"
    );
    assert_eq!(
        &built[file_end..],
        &data[file_end..],
        "bytes after the Setup file untouched (slot-fit)"
    );

    let rebuilt = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let new_path = module_pe32_node_path(&rebuilt, PCI_SETUP_MODULE_GUID);
    let pkg_after = module_form_package(&rebuilt, &new_path);
    assert_eq!(
        pkg_after.len(),
        pkg_before.len(),
        "unlock is length-preserving"
    );
    let diff: Vec<(usize, u8, u8)> = pkg_before
        .iter()
        .zip(pkg_after.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| (i, *a, *b))
        .collect();
    assert_eq!(
        diff,
        E12_FLIP_BYTES.to_vec(),
        "engine unlock must reproduce the hardware-validated E12 dataflip byte-for-byte"
    );

    let (_, new_file_end) = file_extent(&rebuilt, &new_path);
    assert_eq!(
        new_file_end, file_end,
        "slot-fit keeps the Setup file extent"
    );

    let gates_after = uefi_engine::hii::gates_list(&rebuilt, &form_item).unwrap();
    assert_eq!(gates_after[0].expression, "1 == 2");
    assert!(
        !gates_after[0].flippable,
        "already-false gate offers no flip"
    );
    let qgates_after = uefi_engine::hii::gates_list(&rebuilt, &question_item).unwrap();
    assert_eq!(qgates_after[0].expression, "0x009A == 0xFFFF");
    assert!(
        !qgates_after[0].flippable,
        "already-unreachable value offers no flip"
    );

    let forms = uefi_engine::hii::forms::collect_forms(&rebuilt);
    assert!(
        forms.iter().any(|f| f.form_id_ifr == 10029),
        "form 10029 stays discoverable after unlock"
    );
}

#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
#[test]
fn real_image_hii_question_info_4g() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Read, "img1", "s1").expect("parse_image");
    let item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029:0x3B");
    let q = uefi_engine::hii::question_info(&img, &item).expect("question_info");
    assert_eq!(q.form_id, 10029);
    assert_eq!(q.question_id, 0x3B);
    assert_eq!(q.kind, "one_of");
    assert_eq!(q.var_store_id, 1);
    let vs = q.varstore.expect("varstore declared");
    assert_eq!(vs.id, 1);
    assert_eq!(vs.size, 0x72);
    assert_eq!(vs.name, "Setup");
    assert_eq!(vs.guid, "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9");
    assert_eq!(q.var_offset, 0x3A);
    assert_eq!(q.width, 1);
    assert_eq!(q.options.len(), 2);
    assert_eq!(q.options[0].value, 0);
    assert_eq!(q.options[0].flags, 0x30);
    assert_eq!(q.options[1].value, 1);
    assert_eq!(q.options[1].flags, 0x00);
    assert!(q.defaults.is_empty());
}

fn decompressed_diff(orig: &[u8], built: &[u8], sec_off: usize) -> Vec<(usize, u8, u8)> {
    let data_offset = u16::from_le_bytes([orig[sec_off + 20], orig[sec_off + 21]]) as usize;
    let size = (orig[sec_off] as usize)
        | ((orig[sec_off + 1] as usize) << 8)
        | ((orig[sec_off + 2] as usize) << 16);
    let lzma = uefi_common::pi::CompressionType::Lzma as u8;
    let old =
        uefi_engine::decompress::decompress(&orig[sec_off + data_offset..sec_off + size], lzma)
            .expect("decompress orig stream");
    let new =
        uefi_engine::decompress::decompress(&built[sec_off + data_offset..sec_off + size], lzma)
            .expect("decompress built stream");
    assert_eq!(
        old.len(),
        new.len(),
        "decompressed stream length must be preserved"
    );
    old.iter()
        .zip(new.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| (i, *a, *b))
        .collect()
}

#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
#[test]
fn real_image_hii_set_value_matches_e14() {
    let data = load_fw();
    assert_eq!(data[0x8000C2], 0, "fixture must be the E14-original image");
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");
    let item = format!("{PCI_SETUP_MODULE_GUID}:0x10:0#10029:0x3B");
    let outcome = uefi_engine::hii::set_value(&mut img, &item, 1).expect("set_value");
    assert_eq!(outcome.stores.len(), 2, "FV0 raw copy + FV2 LZMA copy");
    assert_eq!(outcome.applied.len(), 2);
    let built = uefi_engine::builder::build_image(&img).expect("build_image");

    assert_eq!(built.len(), data.len(), "total flash length preserved");
    assert_eq!(built[0x8000C2], 1, "FV0 StdDefaults 4G byte 0 -> 1 (E14)");
    let diff: Vec<usize> = (0..data.len()).filter(|&i| data[i] != built[i]).collect();
    let sec_extent = 0xa77d40..0xa77d40 + 0x366;
    assert!(
        diff.iter()
            .all(|&i| i == 0x8000C2 || sec_extent.contains(&i)),
        "flash diff must stay inside the two StdDefaults store slots"
    );

    let lzma_diff = decompressed_diff(&data, &built, 0xa77d40);
    assert_eq!(lzma_diff, vec![(0x66, 0, 1)]);

    let rebuilt = parse_image(&built, ImageMode::Read, "img2", "s2").expect("re-parse");
    let q = uefi_engine::hii::question_info(&rebuilt, &item).expect("info after set");
    assert_eq!(q.var_offset, 0x3A);
    let built2 = uefi_engine::builder::build_image(&rebuilt).expect("build2");
    assert_eq!(built2.len(), built.len());
}

fn find_file_bytes(img: &Image, guid: &str) -> Vec<u8> {
    let g = Guid::try_parse(guid).unwrap();
    let node = find_file_node(&img.root, g).unwrap_or_else(|| panic!("file {guid} in tree"));
    let mut out = node.header.clone();
    out.extend_from_slice(&node.body);
    out.extend_from_slice(&node.tail);
    out
}

fn find_spf_leaf_body(img: &Image, guid: &str) -> Vec<u8> {
    let g = Guid::try_parse(guid).unwrap();
    find_first_leaf(find_file_node(&img.root, g).unwrap(), |n| {
        n.body.windows(4).any(|w| w == b"$SPF")
    })
    .unwrap_or_else(|| panic!("$SPF leaf under {guid}"))
    .body
    .clone()
}

fn find_file_range(data: &[u8], guid: &str) -> std::ops::Range<usize> {
    let needle = Guid::try_parse(guid).unwrap().to_bytes();
    let mut from = 0usize;
    while let Some(rel) = data[from..].windows(16).position(|w| w == needle) {
        let start = from + rel;
        let size = (data[start + 20] as usize)
            | ((data[start + 21] as usize) << 8)
            | ((data[start + 22] as usize) << 16);
        if size >= 24 && start + size <= data.len() {
            return start..start + size;
        }
        from = start + 1;
    }
    panic!("no FFS occurrence of {guid} with a consistent header size");
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_form_hijack_built_bytes() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::form_hijack;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::schema;
    use uefi_engine::hii::spf;

    const ITEM: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029";
    const AMITSE_GUID: &str = "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let mut img = parse_image(&data, ImageMode::Write, "hijack", "s").unwrap();

    let sc = schema::parse_hijack_schema(
        r#"{"questions": [
            {"question_id": 59, "prompt": "PATCHER 4G QUESTION", "help": "PATCHER 4G HELP"}]}"#,
    )
    .unwrap();

    let amitse_before = find_file_bytes(&img, AMITSE_GUID);
    let sd_guid = Guid::try_parse(SETUPDATA_GUID).unwrap();
    let sd_spf_before = find_spf_leaf_body(&img, SETUPDATA_GUID);
    let controls_before = spf::scan_string_controls(&sd_spf_before);
    let matched_before: Vec<_> = controls_before
        .iter()
        .filter(|c| c.string_id == 420)
        .collect();
    assert_eq!(
        matched_before.len(),
        1,
        "exactly one $SPF string-control must carry the q59 help-id 420"
    );
    let title_before = collect_forms(&img)
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .map(|f| f.title.clone())
        .expect("form 10029 before hijack");
    let pe_path = module_pe32_node_path(&img, PCI_SETUP_MODULE_GUID);
    let pkg_before = module_form_package(&img, &pe_path);

    let res = form_hijack::hijack_form(&mut img, ITEM, &sc, Some(&sd_guid)).unwrap();
    assert_eq!(
        res.unlock_flips,
        vec![
            "pkg+0x67a: 01 -> 02".to_string(),
            "pkg+0xdd1: 01 00 -> ff ff".to_string(),
        ],
        "hijack auto-unlock must plan exactly the E12 REF-suppress + question-grayout flips"
    );
    assert_eq!(
        res.help_controls.len(),
        1,
        "the 4G question must have its $SPF help control rewritten"
    );
    assert_eq!(res.help_controls[0].question_id, 59);
    assert_eq!(res.help_controls[0].old_string_id, 420);
    let new_help_id = *res.string_ids.get("PATCHER 4G HELP").unwrap();
    assert_eq!(res.help_controls[0].new_string_id, new_help_id);
    assert_eq!(
        res.help_controls[0].offset, matched_before[0].offset,
        "the patched control must be the one that carried help-id 420"
    );
    assert_eq!(
        res.help_records.len(),
        1,
        "the 4G question must have its $SPF record s14 rewritten"
    );
    assert_eq!(res.help_records[0].question_id, 59);
    assert_eq!(res.help_records[0].old_string_id, 420);
    assert_eq!(res.help_records[0].new_string_id, new_help_id);
    assert!(res.form_ifr_end > res.form_ifr_start);

    let built = build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    let re = parse_image(&built, ImageMode::Read, "re", "s").unwrap();
    assert_eq!(
        find_file_bytes(&re, AMITSE_GUID),
        amitse_before,
        "AMITSE file must stay byte-identical"
    );

    let new_pe_path = module_pe32_node_path(&re, PCI_SETUP_MODULE_GUID);
    let pkg_after = module_form_package(&re, &new_pe_path);
    assert_eq!(
        pkg_after.len(),
        pkg_before.len(),
        "hijack + auto-unlock is length-preserving"
    );
    let diff: Vec<(usize, u8, u8)> = pkg_before
        .iter()
        .zip(pkg_after.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| (i, *a, *b))
        .collect();
    for (off, from, to) in E12_FLIP_BYTES {
        assert!(
            diff.contains(&(off, from, to)),
            "hijack built bytes must include the E12 flip at pkg+{off:#x}"
        );
    }

    let pfs_body = find_spf_leaf_body(&re, SETUPDATA_GUID);
    assert_eq!(
        pfs_body.len(),
        sd_spf_before.len(),
        "$SPF payload length invariant"
    );
    let controls_after = spf::scan_string_controls(&pfs_body);
    assert_eq!(
        controls_after.len(),
        controls_before.len(),
        "$SPF string-control count invariant"
    );
    let patched: Vec<_> = controls_after
        .iter()
        .filter(|c| c.offset == res.help_controls[0].offset)
        .collect();
    assert_eq!(patched.len(), 1);
    assert_eq!(
        patched[0].string_id, new_help_id,
        "the q59 help control must now carry the appended help string id"
    );
    assert!(controls_after.iter().all(|c| c.string_id != 420));
    let recs = spf::scan_question_records(&pfs_body);
    assert!(
        recs.iter().any(|r| r.question_id == 59),
        "4G record stays registered in built $SPF"
    );
    assert!(recs.len() >= 380, "record array must stay intact");
    let q59_after: Vec<_> = recs
        .iter()
        .filter(|r| {
            r.question_id == 59
                && r.ifr_offset >= res.form_ifr_start
                && r.ifr_offset < res.form_ifr_end
        })
        .collect();
    assert_eq!(
        q59_after.len(),
        1,
        "exactly one q59 record in the form span"
    );
    assert_eq!(q59_after[0].offset, res.help_records[0].record_offset);
    assert_eq!(
        u16::from_le_bytes([
            pfs_body[q59_after[0].offset + spf::SPF_RECORD_HELP_ID],
            pfs_body[q59_after[0].offset + spf::SPF_RECORD_HELP_ID + 1],
        ]),
        new_help_id,
        "the q59 record s14 must carry the appended help id after build"
    );

    let forms = collect_forms(&re);
    let f = forms
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .expect("hijacked form");
    assert_eq!(f.title, title_before, "form title must stay untouched");

    let (setup_slot, sd_slot) = (
        find_file_range(&data, "899407D7-99FE-43D8-9A21-79EC328CAC21"),
        find_file_range(&data, SETUPDATA_GUID),
    );
    for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
        if a != b {
            assert!(
                setup_slot.contains(&i) || sd_slot.contains(&i),
                "byte {i:#x} changed outside the two AMI-adjacent slots"
            );
        }
    }

    eprintln!(
        "real_image hijack: form=10029 q=0x3B ifr[{:#x}..{:#x}) strings={} setup_slot={setup_slot:?} sd_slot={sd_slot:?}",
        res.form_ifr_start,
        res.form_ifr_end,
        res.string_ids.len()
    );
}

#[test]
#[ignore = "needs real image at ../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin"]
fn real_image_hijack_v2_scenario_a_unlocks_victim_only() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::form_hijack;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::gates::{self, GateTarget};
    use uefi_engine::hii::schema;
    use uefi_engine::hii::spf;

    const ITEM: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "hij-a", "s").unwrap();

    let title_before = collect_forms(&img)
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .map(|f| f.title.clone())
        .expect("form 10029 before hijack");
    let pe_path = module_pe32_node_path(&img, PCI_SETUP_MODULE_GUID);
    let pkg_before = module_form_package(&img, &pe_path);
    let span_before = form_hijack::locate_form(&pkg_before, 10029).expect("form 10029 span");
    let title_ifr_before = pkg_before[span_before.form_op + 4..span_before.form_op + 6].to_vec();
    let q59_off = form_hijack::locate_questions(&pkg_before, 10029)
        .into_iter()
        .find(|&(_, qid)| qid == 59)
        .map(|(off, _)| off)
        .expect("q59 statement offset");

    let sc = schema::parse_hijack_schema(
        r#"{"questions": [
            {"question_id": 59, "prompt": "UEFIPatcher E27 A", "help": "UEFIPatcher E27 A help"}]}"#,
    )
    .unwrap();
    let sd_guid = Guid::try_parse(SETUPDATA_GUID).unwrap();
    let res = form_hijack::hijack_form(&mut img, ITEM, &sc, Some(&sd_guid)).unwrap();
    assert_eq!(
        res.unlock_flips,
        vec![
            "pkg+0x67a: 01 -> 02".to_string(),
            "pkg+0xdd1: 01 00 -> ff ff".to_string(),
        ],
        "hijack alone must unlock exactly the victim gates: hub REF suppress + q59 grayout"
    );
    assert_eq!(res.help_controls.len(), 1);
    assert_eq!(res.help_controls[0].question_id, 59);
    assert_eq!(res.help_controls[0].old_string_id, 420);
    let new_help_id = *res.string_ids.get("UEFIPatcher E27 A help").unwrap();
    assert_eq!(res.help_controls[0].new_string_id, new_help_id);
    assert_eq!(res.help_records.len(), 1);
    assert_eq!(res.help_records[0].question_id, 59);
    assert_eq!(res.help_records[0].old_string_id, 420);
    assert_eq!(res.help_records[0].new_string_id, new_help_id);

    let built = build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    let re = parse_image(&built, ImageMode::Read, "re-a", "s").unwrap();
    let forms = collect_forms(&re);
    let f10029 = forms
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .expect("form 10029 after build");
    assert_eq!(f10029.title, title_before, "form title must stay untouched");

    let new_pe_path = module_pe32_node_path(&re, PCI_SETUP_MODULE_GUID);
    let pkg_after = module_form_package(&re, &new_pe_path);
    assert_eq!(
        pkg_after.len(),
        pkg_before.len(),
        "hijack + auto-unlock is length-preserving"
    );
    let span_after = form_hijack::locate_form(&pkg_after, 10029).expect("form 10029 span after");
    assert_eq!(
        &pkg_after[span_after.form_op + 4..span_after.form_op + 6],
        title_ifr_before.as_slice(),
        "form title IFR bytes must stay untouched"
    );
    let prompt_id = u16::from_le_bytes([pkg_after[q59_off + 2], pkg_after[q59_off + 3]]);
    let help_id = u16::from_le_bytes([pkg_after[q59_off + 4], pkg_after[q59_off + 5]]);
    assert_eq!(prompt_id, *res.string_ids.get("UEFIPatcher E27 A").unwrap());
    assert_eq!(help_id, new_help_id);

    let mut allowed: Vec<usize> = vec![0x67A, 0xDD1, 0xDD2];
    allowed.extend(q59_off + 2..q59_off + 6);
    let diff: Vec<usize> = (0..pkg_before.len())
        .filter(|&i| pkg_before[i] != pkg_after[i])
        .collect();
    assert!(
        diff.iter().all(|&i| allowed.contains(&i)),
        "pkg diff {diff:#x?} must confine to the two victim flips and the q59 id slot"
    );
    for off in [0x67A, 0xDD1, 0xDD2] {
        assert!(
            diff.contains(&off),
            "E12 victim flip at pkg+{off:#x} must be present"
        );
    }

    for target in [
        GateTarget {
            form_id: 10029,
            question_id: None,
        },
        GateTarget {
            form_id: 10029,
            question_id: Some(59),
        },
    ] {
        let found = gates::find_gates(&pkg_after, &target);
        let flips = gates::plan_gates_skip_unlocked(&pkg_after, &found).unwrap();
        assert!(
            flips.is_empty(),
            "no further flips must be plannable for {target:?}"
        );
    }

    let spf_body = find_spf_leaf_body(&re, SETUPDATA_GUID);
    let controls_after = spf::scan_string_controls(&spf_body);
    assert!(
        controls_after.iter().all(|c| c.string_id != 420),
        "the $SPF control that carried help-id 420 must be repointed"
    );
    let patched: Vec<_> = controls_after
        .iter()
        .filter(|c| c.offset == res.help_controls[0].offset)
        .collect();
    assert_eq!(patched.len(), 1);
    assert_eq!(patched[0].string_id, new_help_id);

    let recs_after = spf::scan_question_records(&spf_body);
    let q59_rec: Vec<_> = recs_after
        .iter()
        .filter(|r| {
            r.question_id == 59
                && r.ifr_offset >= res.form_ifr_start
                && r.ifr_offset < res.form_ifr_end
        })
        .collect();
    assert_eq!(q59_rec.len(), 1, "scenario A: exactly one q59 record");
    assert_eq!(q59_rec[0].offset, res.help_records[0].record_offset);
    assert_eq!(
        u16::from_le_bytes([
            spf_body[q59_rec[0].offset + spf::SPF_RECORD_HELP_ID],
            spf_body[q59_rec[0].offset + spf::SPF_RECORD_HELP_ID + 1],
        ]),
        new_help_id,
        "scenario A: the q59 record s14 must carry the appended help id after build"
    );

    let (setup_slot, sd_slot) = (
        find_file_range(&data, PCI_SETUP_MODULE_GUID),
        find_file_range(&data, SETUPDATA_GUID),
    );
    for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
        if a != b {
            assert!(
                setup_slot.contains(&i) || sd_slot.contains(&i),
                "byte {i:#x} changed outside the two AMI-adjacent slots"
            );
        }
    }

    eprintln!(
        "scenario A: flips={} q59_ifr={q59_off:#x} help {} -> {new_help_id}",
        res.unlock_flips.len(),
        res.help_controls[0].old_string_id
    );
}

#[test]
#[ignore = "needs real image at ../../../refs/fw/HNX99TF_200525_original_E5C88C6F.bin"]
fn real_image_hijack_v2_scenario_b_full_page_matches_e26_content() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::HiiError;
    use uefi_engine::hii::form_hijack;
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::gates::{self, GateTarget};
    use uefi_engine::hii::schema;
    use uefi_engine::hii::spf;

    const ITEM: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "hij-b", "s").unwrap();

    let title_before = collect_forms(&img)
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .map(|f| f.title.clone())
        .expect("form 10029 before unlock");
    let pe_path = module_pe32_node_path(&img, PCI_SETUP_MODULE_GUID);
    let pkg_before = module_form_package(&img, &pe_path);
    let q59_off = form_hijack::locate_questions(&pkg_before, 10029)
        .into_iter()
        .find(|&(_, qid)| qid == 59)
        .map(|(off, _)| off)
        .expect("q59 statement offset");

    let form_out = uefi_engine::hii::unlock(&mut img, ITEM).unwrap();
    assert_eq!(
        form_out.applied,
        vec!["pkg+0x8fae: 01 -> 02".to_string()],
        "form-level unlock flips only the hub REF EqConst, no cascade to question gates (TODO.md:1972)"
    );

    let mut question_flips = Vec::new();
    for qid in [54u16, 55, 56, 57, 58, 59, 60] {
        let out = uefi_engine::hii::unlock(&mut img, &format!("{ITEM}:{qid}")).unwrap();
        assert_eq!(
            out.applied.len(),
            1,
            "question {qid} must flip exactly its own EqIdVal gate"
        );
        question_flips.push(out.applied[0].clone());
    }
    assert_eq!(
        question_flips,
        vec![
            "pkg+0x95da: 01 00 -> ff ff".to_string(),
            "pkg+0x962f: 01 00 -> ff ff".to_string(),
            "pkg+0x9684: 01 00 -> ff ff".to_string(),
            "pkg+0x96af: 01 00 -> ff ff".to_string(),
            "pkg+0x96da: 01 00 -> ff ff".to_string(),
            "pkg+0x9705: 01 00 -> ff ff".to_string(),
            "pkg+0x9730: 01 00 -> ff ff".to_string(),
        ],
        "every unlockable question flips its EQ(0x9A,1) gate to FFFF (E25 class); q59 must be pre-unlocked for the idempotence proof (TODO.md:1988)"
    );

    let q61_err = uefi_engine::hii::unlock(&mut img, &format!("{ITEM}:61"))
        .expect_err("q61 unlock must refuse its compound suppress gate (TODO.md:1981)");
    assert!(matches!(q61_err, HiiError::GateExpressionUnsupported(_)));

    let sc = schema::parse_hijack_schema(
        r#"{"questions": [
            {"question_id": 59, "prompt": "UEFIPatcher E27 B", "help": "UEFIPatcher E27 B help"}]}"#,
    )
    .unwrap();
    let sd_guid = Guid::try_parse(SETUPDATA_GUID).unwrap();
    let res = form_hijack::hijack_form(&mut img, ITEM, &sc, Some(&sd_guid)).unwrap();
    assert!(
        res.unlock_flips.is_empty(),
        "hijack's own unlock planning must yield zero flips on the already-unlocked page"
    );
    assert_eq!(res.help_controls.len(), 1);
    assert_eq!(res.help_controls[0].question_id, 59);
    assert_eq!(res.help_controls[0].old_string_id, 420);
    let new_help_id = *res.string_ids.get("UEFIPatcher E27 B help").unwrap();
    assert_eq!(res.help_controls[0].new_string_id, new_help_id);
    assert_eq!(res.help_records.len(), 1);
    assert_eq!(res.help_records[0].question_id, 59);
    assert_eq!(res.help_records[0].old_string_id, 420);
    assert_eq!(res.help_records[0].new_string_id, new_help_id);

    let built = build_image(&img).unwrap();
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    let re = parse_image(&built, ImageMode::Read, "re-b", "s").unwrap();
    let forms = collect_forms(&re);
    let f10029 = forms
        .iter()
        .find(|f| f.form_id_ifr == 10029)
        .expect("form 10029 after build");
    assert_eq!(f10029.title, title_before, "form title must stay untouched");

    let new_pe_path = module_pe32_node_path(&re, PCI_SETUP_MODULE_GUID);
    let pkg_after = module_form_package(&re, &new_pe_path);
    assert_eq!(
        pkg_after.len(),
        pkg_before.len(),
        "unlock + hijack is length-preserving"
    );

    let eq_id_val_bases = [0xCA6usize, 0xCFB, 0xD50, 0xD7B, 0xDA6, 0xDD1, 0xDFC];
    let mut allowed: Vec<usize> = vec![0x67A];
    let mut expected: Vec<usize> = vec![0x67A];
    for base in eq_id_val_bases {
        allowed.extend(base..base + 2);
        expected.extend(base..base + 2);
    }
    allowed.extend(q59_off + 2..q59_off + 6);
    let diff: Vec<usize> = (0..pkg_before.len())
        .filter(|&i| pkg_before[i] != pkg_after[i])
        .collect();
    assert!(
        diff.iter().all(|&i| allowed.contains(&i)),
        "pkg diff {diff:#x?} must confine to the 8 unlock gates and the q59 id slot"
    );
    for off in &expected {
        assert!(
            diff.contains(off),
            "unlock byte pkg+{off:#x} must be present"
        );
    }

    let prompt_id = u16::from_le_bytes([pkg_after[q59_off + 2], pkg_after[q59_off + 3]]);
    let help_id = u16::from_le_bytes([pkg_after[q59_off + 4], pkg_after[q59_off + 5]]);
    assert_eq!(prompt_id, *res.string_ids.get("UEFIPatcher E27 B").unwrap());
    assert_eq!(help_id, new_help_id);

    let mut targets = vec![GateTarget {
        form_id: 10029,
        question_id: None,
    }];
    targets.extend((54u16..=60).map(|qid| GateTarget {
        form_id: 10029,
        question_id: Some(qid),
    }));
    for target in &targets {
        let found = gates::find_gates(&pkg_after, target);
        let flips = gates::plan_gates_skip_unlocked(&pkg_after, &found).unwrap();
        assert!(
            flips.is_empty(),
            "the page must be fully unlocked for {target:?}"
        );
    }
    let q61_gates = gates::find_gates(
        &pkg_after,
        &GateTarget {
            form_id: 10029,
            question_id: Some(61),
        },
    );
    assert_eq!(
        q61_gates.len(),
        2,
        "q61 keeps its EqIdVal gate plus the non-flippable compound suppress (TODO.md:1981)"
    );
    assert!(
        gates::plan_gates_skip_unlocked(&pkg_after, &q61_gates).is_err(),
        "q61's compound suppress stays a non-plannable gate (TODO.md:1981)"
    );

    let spf_body = find_spf_leaf_body(&re, SETUPDATA_GUID);
    let controls_after = spf::scan_string_controls(&spf_body);
    assert!(
        controls_after.iter().all(|c| c.string_id != 420),
        "the $SPF control that carried help-id 420 must be repointed"
    );
    let patched: Vec<_> = controls_after
        .iter()
        .filter(|c| c.offset == res.help_controls[0].offset)
        .collect();
    assert_eq!(patched.len(), 1);
    assert_eq!(patched[0].string_id, new_help_id);

    let recs_after = spf::scan_question_records(&spf_body);
    let q59_rec: Vec<_> = recs_after
        .iter()
        .filter(|r| {
            r.question_id == 59
                && r.ifr_offset >= res.form_ifr_start
                && r.ifr_offset < res.form_ifr_end
        })
        .collect();
    assert_eq!(q59_rec.len(), 1, "scenario B: exactly one q59 record");
    assert_eq!(q59_rec[0].offset, res.help_records[0].record_offset);
    assert_eq!(
        u16::from_le_bytes([
            spf_body[q59_rec[0].offset + spf::SPF_RECORD_HELP_ID],
            spf_body[q59_rec[0].offset + spf::SPF_RECORD_HELP_ID + 1],
        ]),
        new_help_id,
        "scenario B: the q59 record s14 must carry the appended help id after build"
    );

    let (setup_slot, sd_slot) = (
        find_file_range(&data, PCI_SETUP_MODULE_GUID),
        find_file_range(&data, SETUPDATA_GUID),
    );
    for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
        if a != b {
            assert!(
                setup_slot.contains(&i) || sd_slot.contains(&i),
                "byte {i:#x} changed outside the two AMI-adjacent slots"
            );
        }
    }

    eprintln!(
        "scenario B: form=1 flips, 7 question flips, q61 refused, hijack=0 flips, q59 ids {prompt_id}/{help_id}"
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s2() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");

    let data = load_fw();
    let serial: Vec<&str> = vec!["SerialDxe.ffs", "TerminalDxe.ffs", "SerialConsoleGlue.ffs"];

    let mut img = parse_image(&data, ImageMode::Read, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }

    let rebuilt = build_image(&img).expect("build after insert");

    assert_eq!(rebuilt.len(), data.len(), "image size must be preserved");
    let mut regions = 0usize;
    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(rebuilt.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
            regions += 1;
        }
    }
    assert!(regions > 0, "insert must change bytes");
    assert_eq!(
        first_diff, FIRST_SLOT,
        "first changed byte must be first free slot"
    );
    let expected_span = 32_848 + 4 + 65_596 + 41_072;
    assert!(
        last_diff < FIRST_SLOT + expected_span,
        "changes must stay inside {expected_span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if rebuilt[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(
        non_tail_changes, 0,
        "all changed bytes must lie over 0xFF free tail"
    );

    let re_img = parse_image(&rebuilt, ImageMode::Read, "img1", "s1").unwrap();
    let vol = &re_img.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        tail,
        vec![
            "9A5163E7-5C29-453F-825C-837A46A81E15".to_string(),
            "9E863906-A40F-4875-977F-5B93FF237FC6".to_string(),
            "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43".to_string(),
        ],
        "inserted files must sit in S1 order at chain end"
    );
    let fv_attrs = u32::from_le_bytes(
        rebuilt[MAIN_FV_OFF + 0x2C..MAIN_FV_OFF + 0x30]
            .try_into()
            .unwrap(),
    );
    assert_eq!((fv_attrs >> 11) & 1, 1, "FV1 erase polarity must be 1");
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
    }
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }

    let stable = build_image(&re_img).expect("stable rebuild");
    assert_eq!(stable, rebuilt, "rebuild of re-parsed tree must be stable");
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s3() {
    use std::collections::{HashMap, HashSet};
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::schema::parse_question_add_schema;
    use uefi_engine::hii::spf;
    use uefi_engine::hii::{add_question, question_info, spf_record_resolves};
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
    const S2_WINDOW: (usize, usize) = (0xB63B18, 0xB85C18);
    const FORM_ID: u16 = 10019;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = vec!["SerialDxe.ffs", "TerminalDxe.ffs", "SerialConsoleGlue.ffs"];

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let s2_built = build_image(&img).expect("build after S2 insert");
    assert_eq!(s2_built.len(), data.len(), "total flash length preserved");

    let mut img2 = parse_image(&s2_built, ImageMode::Write, "img2", "s2").unwrap();

    let base_spf = find_spf_leaf_body(&img2, SETUPDATA_GUID);
    let base = spf::container_start(&base_spf).expect("$SPF container");
    let base_records = spf::scan_question_records(&base_spf);
    assert_eq!(base_records.len(), 389, "live scanner-visible record count");
    let base_pkg = module_form_package(&img2, &module_pe32_node_path(&img2, SETUP_MODULE_GUID));

    let r_base: Vec<spf::SpfQuestionRecord> = base_records
        .iter()
        .copied()
        .filter(|r| spf_record_resolves(&base_pkg, r.question_id, r.ifr_offset))
        .collect();
    assert_eq!(
        r_base.len(),
        32,
        "|R_base| is the measured-stable Setup-resolving partition of the 389"
    );
    let r_base_offsets: HashSet<usize> = r_base.iter().map(|r| r.offset).collect();

    let u32_at = |body: &[u8], at: usize| u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
    let base_page_count = u32_at(&base_spf, base + spf::SPF_PAGE_COUNT_OFFSET) as usize;
    let form_slot = (0..base_page_count)
        .find(|&slot| {
            let off = u32_at(&base_spf, base + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot) as usize;
            off != 0
                && u16::from_le_bytes([base_spf[base + off + 0xA], base_spf[base + off + 0xB]])
                    == FORM_ID
        })
        .expect("page slot for form 10019");
    let base_page_off =
        u32_at(&base_spf, base + spf::SPF_PAGE_TABLE_OFFSET + 4 * form_slot) as usize;
    let base_page_cnt = u32_at(&base_spf, base + base_page_off + spf::SPF_PAGE_CNT_OFFSET) as usize;
    assert_eq!(base_page_cnt, 6, "page 10019 control count before adds");
    let base_page_span = 0x20 + 4 * base_page_cnt;
    let base_page_bytes =
        base_spf[base + base_page_off..base + base_page_off + base_page_span].to_vec();

    let max_low = base_records
        .iter()
        .map(|r| u32_at(&base_spf, r.offset + spf::SPF_RECORD_COUNTER_OFFSET) & 0xFFFF)
        .max()
        .unwrap();

    let schema_path = std::path::Path::new(SERIAL_DIR).join("s3_questions.json");
    let schema_json = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
    let list = parse_question_add_schema(&schema_json).expect("fixture schema parses");
    assert_eq!(list.questions.len(), 2);

    let item = format!("{SETUP_MODULE_GUID}:0x10:0#{FORM_ID}");
    let mut results = Vec::new();
    for (i, q) in list.questions.iter().enumerate() {
        assert_eq!(q.form_id, FORM_ID);
        let res = add_question(&mut img2, &item, q)
            .unwrap_or_else(|e| panic!("add_question #{}: {e:?}", i + 1));
        assert_eq!(res.question_id, q.question_id);
        results.push(res);
    }
    let s3_built = build_image(&img2).expect("build after question adds");
    assert_eq!(s3_built.len(), data.len(), "total flash length preserved");

    let setup_slot = find_file_range(&s2_built, SETUP_MODULE_GUID);
    let sd_slot = find_file_range(&s2_built, SETUPDATA_GUID);
    let zones = [
        S2_WINDOW,
        (setup_slot.start, setup_slot.end),
        (sd_slot.start, sd_slot.end),
    ];
    let mut outside = Vec::new();
    for (i, (a, b)) in s2_built.iter().zip(s3_built.iter()).enumerate() {
        if a != b && !zones.iter().any(|(s, e)| i >= *s && i < *e) {
            outside.push(i);
        }
    }
    assert!(
        outside.is_empty(),
        "{} bytes changed outside the S2 window + Setup/SetupData slots, first={outside:#x?}",
        outside.len()
    );

    let re = parse_image(&s3_built, ImageMode::Read, "re", "s3").unwrap();

    for (q, want_opts) in [(&list.questions[0], 5usize), (&list.questions[1], 4usize)] {
        let qi = question_info(&re, &format!("{item}:{}", q.question_id))
            .unwrap_or_else(|e| panic!("question_info q{}: {e:?}", q.question_id));
        assert_eq!(qi.form_id, u32::from(FORM_ID));
        assert_eq!(qi.question_id, u32::from(q.question_id));
        assert_eq!(qi.kind, "one_of");
        assert_eq!(qi.var_store_id, u32::from(q.var_store_id));
        assert_eq!(qi.var_offset, u32::from(q.var_offset));
        assert_eq!(qi.width, u32::from(q.size));
        assert_eq!(qi.options.len(), want_opts);
        for (i, o) in q.options.iter().enumerate() {
            assert_eq!(qi.options[i].value, o.value);
            let want_flags = u32::from(if o.default.is_some() {
                r_efi::hii::IFR_OPTION_DEFAULT
            } else {
                0
            });
            assert_eq!(
                qi.options[i].flags, want_flags,
                "option {i} default marker must round-trip"
            );
        }
        assert_eq!(qi.defaults.len(), 1, "optimized default must be emitted");
        assert_eq!(
            qi.defaults[0].value, 0,
            "default value 0 (115200 / VT-UTF8)"
        );
    }

    {
        let strings = uefi_engine::hii::strings::collect_strings(&re);
        let pe = &node_at_path(&re, &module_pe32_node_path(&re, SETUP_MODULE_GUID)).body;
        for res in &results {
            for (text, id) in &res.string_ids {
                assert!(
                    strings
                        .iter()
                        .any(|s| s.string_id == u32::from(*id) && &s.text == text),
                    "string id {id} must resolve to '{text}' verbatim"
                );
                assert!(
                    pe.windows(text.len()).any(|w| w == text.as_bytes()),
                    "'{text}' must be present verbatim in the Setup module string package"
                );
            }
        }
    }

    let final_spf = find_spf_leaf_body(&re, SETUPDATA_GUID);
    let fbase = spf::container_start(&final_spf).expect("$SPF survives adds");
    let final_records = spf::scan_question_records(&final_spf);
    assert_eq!(final_records.len(), base_records.len() + 2);
    let container_len_field = (spf::SPF_HEADER_REGION_OFFSETS..spf::SPF_PAGE_COUNT_OFFSET)
        .step_by(4)
        .map(|p| u32_at(&final_spf, fbase + p))
        .max()
        .expect("header zone u32 fields");
    assert_eq!(
        container_len_field as usize,
        final_spf.len() - fbase,
        "container-length header field (max u32 in header zone, the bump_container_length rule) must equal the post-append container length"
    );
    let final_by_offset: HashMap<usize, spf::SpfQuestionRecord> = final_records
        .iter()
        .copied()
        .map(|r| (r.offset, r))
        .collect();
    let final_pkg = module_form_package(&re, &module_pe32_node_path(&re, SETUP_MODULE_GUID));

    let rec_offs: Vec<usize> = results.iter().map(|r| r.spf_record_offset).collect();
    let new_recs: Vec<spf::SpfQuestionRecord> = rec_offs
        .iter()
        .map(|&off| {
            *final_by_offset.get(&(fbase + off)).unwrap_or_else(|| {
                panic!("appended record at container+{off:#x} must be a valid scanned F8 record")
            })
        })
        .collect();
    let (insert1, insert2) = (new_recs[0].ifr_offset, new_recs[1].ifr_offset);
    let delta1 = insert2 - insert1;
    let delta2 = (final_pkg.len() - base_pkg.len()) as u32 - delta1;
    assert_eq!(new_recs[0].question_id, 512);
    assert_eq!(new_recs[1].question_id, 513);
    assert!(
        spf_record_resolves(&final_pkg, 512, insert1),
        "q512 must resolve at its exact splice position {insert1:#x}"
    );
    assert!(
        spf_record_resolves(&final_pkg, 513, insert2),
        "q513 must resolve at its exact splice position {insert2:#x}"
    );

    let mut shifted = 0usize;
    for r in &base_records {
        let fr = final_by_offset
            .get(&r.offset)
            .expect("append-only container keeps record offsets stable");
        if r_base_offsets.contains(&r.offset) {
            let mid = r.ifr_offset + u32::from(r.ifr_offset >= insert1) * delta1;
            let expected = mid + u32::from(mid >= insert2) * delta2;
            assert_eq!(
                fr.ifr_offset, expected,
                "Setup record qid={:#x} must carry the compensated ifr",
                r.question_id
            );
            assert!(
                spf_record_resolves(&final_pkg, r.question_id, expected),
                "Setup record qid={:#x} must still resolve post-rebuild",
                r.question_id
            );
            shifted += usize::from(expected != r.ifr_offset);
        } else {
            assert!(
                final_spf[r.offset..r.offset + spf::SPF_RECORD_SIZE]
                    == base_spf[r.offset..r.offset + spf::SPF_RECORD_SIZE],
                "foreign record qid={:#x} at {:#x} must stay byte-identical",
                r.question_id,
                r.offset
            );
            assert_eq!(fr.ifr_offset, r.ifr_offset);
        }
    }

    for (i, (res, &off)) in results.iter().zip(rec_offs.iter()).enumerate() {
        let at = fbase + off;
        let counter = u32_at(&final_spf, at + spf::SPF_RECORD_COUNTER_OFFSET);
        assert_eq!(
            counter,
            (0x0001u32 << 16) | (1 + max_low + i as u32),
            "counter rule (0x0001 << 16) | (1 + max low)"
        );
        let help_id = res.string_ids.get(&list.questions[i].help).unwrap();
        let prompt_id = res.string_ids.get(&list.questions[i].prompt).unwrap();
        assert_eq!(
            u16::from_le_bytes([
                final_spf[at + spf::SPF_RECORD_HELP_ID],
                final_spf[at + spf::SPF_RECORD_HELP_ID + 1]
            ]),
            *help_id,
            "record s14 must carry the appended help id"
        );
        assert_eq!(
            u16::from_le_bytes([
                final_spf[at + spf::SPF_RECORD_PROMPT_ID_OFFSET],
                final_spf[at + spf::SPF_RECORD_PROMPT_ID_OFFSET + 1],
            ]),
            *prompt_id,
            "record s30 must carry the appended prompt id"
        );
        assert_eq!(final_spf[at + spf::SPF_RECORD_FAILSAFE], 0);
        assert_eq!(final_spf[at + spf::SPF_RECORD_OPTIMAL], 0);
    }

    let fpage_count = u32_at(&final_spf, fbase + spf::SPF_PAGE_COUNT_OFFSET) as usize;
    assert_eq!(
        fpage_count, base_page_count,
        "page count must stay unchanged"
    );
    let clone_off = u32_at(
        &final_spf,
        fbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * form_slot,
    ) as usize;
    assert_ne!(
        clone_off, base_page_off,
        "slot must be repointed to the clone"
    );
    assert_eq!(
        u16::from_le_bytes([
            final_spf[fbase + clone_off + 0xA],
            final_spf[fbase + clone_off + 0xB]
        ]),
        FORM_ID,
        "clone keeps form 10019"
    );
    let clone_cnt = u32_at(&final_spf, fbase + clone_off + spf::SPF_PAGE_CNT_OFFSET) as usize;
    assert_eq!(clone_cnt, base_page_cnt + 2, "cnt 6 -> 8");
    assert_eq!(
        &final_spf[fbase + clone_off..fbase + clone_off + 0x1C],
        &base_page_bytes[..0x1C],
        "clone header up to cnt must copy the base page verbatim"
    );
    let clone_entry = |i: usize| {
        u32_at(
            &final_spf,
            fbase + clone_off + spf::SPF_PAGE_LIST_OFFSET + 4 * i,
        )
    };
    for i in 0..base_page_cnt {
        assert_eq!(
            clone_entry(i),
            u32_at(
                &base_spf,
                base + base_page_off + spf::SPF_PAGE_LIST_OFFSET + 4 * i
            ),
            "clone entries[0..6] must be byte-identical to the base page"
        );
    }
    for (i, (want_ptr, want_qid)) in [(6usize, (rec_offs[0], 512u32)), (7, (rec_offs[1], 513))] {
        assert_eq!(
            clone_entry(i) as usize,
            want_ptr,
            "new entry must equal spf_record_offset of the added question"
        );
        let rec = final_by_offset
            .get(&(fbase + want_ptr))
            .unwrap_or_else(|| panic!("entry {i} must resolve to a valid scanned F8 record"));
        assert_eq!(u32::from(rec.question_id), want_qid);
    }
    assert_eq!(
        &final_spf[fbase + base_page_off..fbase + base_page_off + base_page_span],
        &base_page_bytes[..],
        "orphaned original page must stay intact"
    );

    let vol = &re.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    for (i, f) in vol.children[files_before..].iter().enumerate() {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        let path = std::path::Path::new(SERIAL_DIR).join(serial[i]);
        let ffs = std::fs::read(&path).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
    }

    let re2 = parse_image(&s3_built, ImageMode::Read, "re2", "s3b").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, s3_built, "save -> open -> save must be byte-stable");

    eprintln!(
        "real_image s3: r_base={} shifted={} foreign={} delta1={delta1:#x} delta2={delta2:#x} inserts={insert1:#x}/{insert2:#x} clone=container+{clone_off:#x} cnt={clone_cnt}",
        r_base.len(),
        shifted,
        base_records.len() - r_base.len(),
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s4() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::schema::parse_question_add_schema;
    use uefi_engine::hii::{add_question, question_info};
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
    const FORM_ID: u16 = 10019;
    const SERIAL_DXE_GUID: &str = "9A5163E7-5C29-453F-825C-837A46A81E15";
    const PCD_PROTOCOL_GUID_LE: [u8; 16] = [
        0xF6, 0xF0, 0xA3, 0x13, 0x4A, 0x26, 0xF0, 0x3E, 0xF2, 0xE0, 0xDE, 0xC5, 0x12, 0x34, 0x2F,
        0x34,
    ];

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = vec![
        "SerialIoAmiDxe.ffs",
        "TerminalDxe.ffs",
        "SerialConsoleGlue.ffs",
    ];
    let expect_tail = [
        "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F",
        "9E863906-A40F-4875-977F-5B93FF237FC6",
        "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43",
    ];

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let s4_triple = build_image(&img).expect("build after S4 inserts");
    assert_eq!(s4_triple.len(), data.len(), "total flash length preserved");

    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(s4_triple.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
        }
    }
    assert_eq!(
        first_diff, FIRST_SLOT,
        "first changed byte must be first free slot"
    );
    let span = 7_680 + 65_600 + 41_072;
    assert!(
        last_diff < FIRST_SLOT + span,
        "changes must stay inside {span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if s4_triple[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(
        non_tail_changes, 0,
        "all changed bytes must lie over 0xFF free tail"
    );

    let mut img2 = parse_image(&s4_triple, ImageMode::Write, "img2", "s4").unwrap();

    let schema_path = std::path::Path::new(SERIAL_DIR).join("s3_questions.json");
    let schema_json = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
    let list = parse_question_add_schema(&schema_json).expect("fixture schema parses");
    assert_eq!(list.questions.len(), 2);

    let item = format!("{SETUP_MODULE_GUID}:0x10:0#{FORM_ID}");
    for (i, q) in list.questions.iter().enumerate() {
        assert_eq!(q.form_id, FORM_ID);
        let res = add_question(&mut img2, &item, q)
            .unwrap_or_else(|e| panic!("add_question #{}: {e:?}", i + 1));
        assert_eq!(res.question_id, q.question_id);
    }
    let s4_built = build_image(&img2).expect("build after question adds");
    assert_eq!(s4_built.len(), data.len(), "total flash length preserved");

    let setup_slot = find_file_range(&s4_triple, SETUP_MODULE_GUID);
    let sd_slot = find_file_range(&s4_triple, SETUPDATA_GUID);
    let zones = [
        (FIRST_SLOT, FIRST_SLOT + span),
        (setup_slot.start, setup_slot.end),
        (sd_slot.start, sd_slot.end),
    ];
    let mut outside = Vec::new();
    for (i, (a, b)) in s4_triple.iter().zip(s4_built.iter()).enumerate() {
        if a != b && !zones.iter().any(|(s, e)| i >= *s && i < *e) {
            outside.push(i);
        }
    }
    assert!(
        outside.is_empty(),
        "{} bytes changed outside the insert window + Setup/SetupData slots, first={outside:#x?}",
        outside.len()
    );

    let re = parse_image(&s4_built, ImageMode::Read, "re", "s4").unwrap();
    let vol = &re.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(tail, expect_tail, "donor swap order at chain end");
    assert!(
        !vol.children.iter().any(|f| matches!(&f.guid, Some(g)
            if g.to_string().to_ascii_uppercase() == SERIAL_DXE_GUID)),
        "SerialDxe 9A5163E7 must be absent from the S4 candidate (producer replaced, not added)"
    );
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }
    for (i, f) in vol.children[files_before..].iter().enumerate() {
        let path = std::path::Path::new(SERIAL_DIR).join(serial[i]);
        let ffs = std::fs::read(&path).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
    }

    let donor = &vol.children[files_before];
    assert_eq!(
        donor.children[0].subtype, 0x13,
        "donor first section is DEPEX"
    );
    assert_eq!(donor.body[3], 0x13);
    assert_eq!(donor.body[4], 0x02, "DEPEX starts with PUSH");
    assert_eq!(&donor.body[5..21], &PCD_PROTOCOL_GUID_LE[..]);
    assert_eq!(donor.body[21], 0x08, "DEPEX ends with END");
    assert_eq!(donor.body[0x16], 0x00);
    assert_eq!(donor.body[0x17], 0x00, "2-byte pad after DEPEX");
    assert_eq!(donor.body[0x1B], 0x02, "second section is GUID_DEFINED");
    let guided = &donor.children[1];
    assert_eq!(guided.subtype, 0x02);
    let ui = guided
        .children
        .iter()
        .find(|c| c.subtype == 0x15)
        .expect("UI section inside donor guided payload");
    let ui_text: String = String::from_utf16_lossy(
        &ui.body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect::<Vec<u16>>(),
    );
    assert_eq!(ui_text, "SerialIo", "donor UI name");

    for (q, want_opts) in [(&list.questions[0], 5usize), (&list.questions[1], 4usize)] {
        let qi = question_info(&re, &format!("{item}:{}", q.question_id))
            .unwrap_or_else(|e| panic!("question_info q{}: {e:?}", q.question_id));
        assert_eq!(qi.form_id, u32::from(FORM_ID));
        assert_eq!(qi.question_id, u32::from(q.question_id));
        assert_eq!(qi.kind, "one_of");
        assert_eq!(qi.var_store_id, u32::from(q.var_store_id));
        assert_eq!(qi.var_offset, u32::from(q.var_offset));
        assert_eq!(qi.options.len(), want_opts);
        assert_eq!(qi.defaults.len(), 1);
        assert_eq!(qi.defaults[0].value, 0, "default 0 (115200 / VT-UTF8)");
    }

    let re2 = parse_image(&s4_built, ImageMode::Read, "re2", "s4b").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, s4_built, "save -> open -> save must be byte-stable");

    eprintln!(
        "real_image s4: files {files_before} -> {} span={span:#x} last_diff={last_diff:#x}",
        vol.children.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s4a() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::schema::parse_question_add_schema;
    use uefi_engine::hii::{add_question, question_info};
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
    const FORM_ID: u16 = 10019;
    const SERIAL_DXE_GUID: &str = "9A5163E7-5C29-453F-825C-837A46A81E15";
    const TERMINAL_DXE_GUID: &str = "9E863906-A40F-4875-977F-5B93FF237FC6";

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = vec![
        "SerialIoAmiDxe.ffs",
        "TermSrcAmiDxe.ffs",
        "SerialConsoleGlueV3.ffs",
    ];
    let expect_tail = [
        "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F",
        "54891A9E-763E-4377-8841-8D5C90D88CDE",
        "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43",
    ];

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let s4a_triple = build_image(&img).expect("build after S4a inserts");
    assert_eq!(s4a_triple.len(), data.len(), "total flash length preserved");

    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(s4a_triple.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
        }
    }
    assert_eq!(
        first_diff, FIRST_SLOT,
        "first changed byte must be first free slot"
    );
    let span = 7_680 + 13_368 + 41_072;
    assert!(
        last_diff < FIRST_SLOT + span,
        "changes must stay inside {span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if s4a_triple[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(
        non_tail_changes, 0,
        "all changed bytes must lie over 0xFF free tail"
    );

    let mut img2 = parse_image(&s4a_triple, ImageMode::Write, "img2", "s4a").unwrap();

    let schema_path = std::path::Path::new(SERIAL_DIR).join("s3_questions.json");
    let schema_json = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
    let list = parse_question_add_schema(&schema_json).expect("fixture schema parses");
    assert_eq!(list.questions.len(), 2);

    let item = format!("{SETUP_MODULE_GUID}:0x10:0#{FORM_ID}");
    for (i, q) in list.questions.iter().enumerate() {
        assert_eq!(q.form_id, FORM_ID);
        let res = add_question(&mut img2, &item, q)
            .unwrap_or_else(|e| panic!("add_question #{}: {e:?}", i + 1));
        assert_eq!(res.question_id, q.question_id);
    }
    let s4a_built = build_image(&img2).expect("build after question adds");
    assert_eq!(s4a_built.len(), data.len(), "total flash length preserved");

    let setup_slot = find_file_range(&s4a_triple, SETUP_MODULE_GUID);
    let sd_slot = find_file_range(&s4a_triple, SETUPDATA_GUID);
    let zones = [
        (FIRST_SLOT, FIRST_SLOT + span),
        (setup_slot.start, setup_slot.end),
        (sd_slot.start, sd_slot.end),
    ];
    let mut outside = Vec::new();
    for (i, (a, b)) in s4a_triple.iter().zip(s4a_built.iter()).enumerate() {
        if a != b && !zones.iter().any(|(s, e)| i >= *s && i < *e) {
            outside.push(i);
        }
    }
    assert!(
        outside.is_empty(),
        "{} bytes changed outside the insert window + Setup/SetupData slots, first={outside:#x?}",
        outside.len()
    );

    let re = parse_image(&s4a_built, ImageMode::Read, "re", "s4a").unwrap();
    let vol = &re.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(tail, expect_tail, "TermSrc swap order at chain end");
    assert!(
        !vol.children.iter().any(|f| matches!(&f.guid, Some(g)
            if g.to_string().to_ascii_uppercase() == SERIAL_DXE_GUID)),
        "SerialDxe 9A5163E7 must be absent from the S4a candidate (producer replaced, not added)"
    );
    assert!(
        !vol.children.iter().any(|f| matches!(&f.guid, Some(g)
            if g.to_string().to_ascii_uppercase() == TERMINAL_DXE_GUID)),
        "TerminalDxe 9E863906 must be absent from the S4a candidate (terminal consumer swapped for TermSrc)"
    );
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }
    for (i, f) in vol.children[files_before..].iter().enumerate() {
        let path = std::path::Path::new(SERIAL_DIR).join(serial[i]);
        let ffs = std::fs::read(&path).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
    }

    let termsrc = &vol.children[files_before + 1];
    assert_eq!(
        termsrc.body[3], 0x02,
        "TermSrc has no DEPEX section: first section is GUID_DEFINED"
    );
    let guided = &termsrc.children[0];
    assert_eq!(guided.subtype, 0x02);
    let ui = guided
        .children
        .iter()
        .find(|c| c.subtype == 0x15)
        .expect("UI section inside TermSrc guided payload");
    let ui_text: String = String::from_utf16_lossy(
        &ui.body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect::<Vec<u16>>(),
    );
    assert_eq!(ui_text, "TerminalSrc", "TermSrc donor UI name");

    for (q, want_opts) in [(&list.questions[0], 5usize), (&list.questions[1], 4usize)] {
        let qi = question_info(&re, &format!("{item}:{}", q.question_id))
            .unwrap_or_else(|e| panic!("question_info q{}: {e:?}", q.question_id));
        assert_eq!(qi.form_id, u32::from(FORM_ID));
        assert_eq!(qi.question_id, u32::from(q.question_id));
        assert_eq!(qi.kind, "one_of");
        assert_eq!(qi.var_store_id, u32::from(q.var_store_id));
        assert_eq!(qi.var_offset, u32::from(q.var_offset));
        assert_eq!(qi.options.len(), want_opts);
        assert_eq!(qi.defaults.len(), 1);
        assert_eq!(qi.defaults[0].value, 0, "default 0 (115200 / VT-UTF8)");
    }

    let re2 = parse_image(&s4a_built, ImageMode::Read, "re2", "s4ab").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, s4a_built, "save -> open -> save must be byte-stable");

    eprintln!(
        "real_image s4a: files {files_before} -> {} span={span:#x} last_diff={last_diff:#x}",
        vol.children.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s4b() {
    use uefi_engine::builder::build_image;
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
    const SERIAL_DXE_GUID: &str = "9A5163E7-5C29-453F-825C-837A46A81E15";
    const TERMINAL_DXE_GUID: &str = "9E863906-A40F-4875-977F-5B93FF237FC6";
    const GLUE_GUID: &str = "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43";

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = vec!["SerialIoAmiDxe.ffs", "TermSrcAmiDxe.ffs"];
    let expect_tail = [
        "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F",
        "54891A9E-763E-4377-8841-8D5C90D88CDE",
    ];

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let e34 = build_image(&img).expect("build after E34 inserts");
    assert_eq!(e34.len(), data.len(), "total flash length preserved");

    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(e34.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
        }
    }
    assert_eq!(
        first_diff, FIRST_SLOT,
        "first changed byte must be first free slot"
    );
    let span = 7_680 + 13_368;
    assert!(
        last_diff < FIRST_SLOT + span,
        "changes must stay inside {span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if e34[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(
        non_tail_changes, 0,
        "all changed bytes must lie over 0xFF free tail"
    );

    let re = parse_image(&e34, ImageMode::Read, "re", "s4b").unwrap();
    let vol = &re.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 2);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(tail, expect_tail, "E34 pair order at chain end");
    for absent in [SERIAL_DXE_GUID, TERMINAL_DXE_GUID, GLUE_GUID] {
        assert!(
            !vol.children.iter().any(|f| matches!(&f.guid, Some(g)
                if g.to_string().to_ascii_uppercase() == absent)),
            "{absent} must be absent from the E34 candidate (no-glue discriminator)"
        );
    }
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }
    for (i, f) in vol.children[files_before..].iter().enumerate() {
        let path = std::path::Path::new(SERIAL_DIR).join(serial[i]);
        let ffs = std::fs::read(&path).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
    }

    let termsrc = &vol.children[files_before + 1];
    assert_eq!(
        termsrc.body[3], 0x02,
        "TermSrc has no DEPEX section: first section is GUID_DEFINED"
    );
    assert_eq!(termsrc.children[0].subtype, 0x02);
    let ui = termsrc.children[0]
        .children
        .iter()
        .find(|c| c.subtype == 0x15)
        .expect("UI section inside TermSrc guided payload");
    let ui_text: String = String::from_utf16_lossy(
        &ui.body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect::<Vec<u16>>(),
    );
    assert_eq!(ui_text, "TerminalSrc", "TermSrc donor UI name");

    let re2 = parse_image(&e34, ImageMode::Read, "re2", "s4bb").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, e34, "save -> open -> save must be byte-stable");

    eprintln!(
        "real_image s4b: files {files_before} -> {} span={span:#x} last_diff={last_diff:#x}",
        vol.children.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_s4c() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::schema::parse_question_add_schema;
    use uefi_engine::hii::{add_question, question_info};
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = 0x890000;
    const FIRST_SLOT: usize = 0xB63B18;
    const SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";
    const SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
    const FORM_ID: u16 = 10019;
    const SERIAL_DXE_GUID: &str = "9A5163E7-5C29-453F-825C-837A46A81E15";

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = vec![
        "SerialIoAmiDxe.ffs",
        "TerminalDxe.ffs",
        "SerialConsoleGlueV3.ffs",
    ];
    let expect_tail = [
        "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F",
        "9E863906-A40F-4875-977F-5B93FF237FC6",
        "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43",
    ];

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let e35_triple = build_image(&img).expect("build after E35 inserts");
    assert_eq!(e35_triple.len(), data.len(), "total flash length preserved");

    let mut first_diff = usize::MAX;
    let mut last_diff = 0usize;
    for (i, (a, b)) in data.iter().zip(e35_triple.iter()).enumerate() {
        if a != b {
            first_diff = first_diff.min(i);
            last_diff = i;
        }
    }
    assert_eq!(
        first_diff, FIRST_SLOT,
        "first changed byte must be first free slot"
    );
    let span = 7_680 + 65_600 + 41_072;
    assert!(
        last_diff < FIRST_SLOT + span,
        "changes must stay inside {span}-byte span: last_diff={last_diff:#x}"
    );
    let mut non_tail_changes = 0usize;
    for i in FIRST_SLOT..=last_diff {
        if e35_triple[i] != data[i] && data[i] != 0xFF {
            non_tail_changes += 1;
        }
    }
    assert_eq!(
        non_tail_changes, 0,
        "all changed bytes must lie over 0xFF free tail"
    );

    let mut img2 = parse_image(&e35_triple, ImageMode::Write, "img2", "s4c").unwrap();

    let schema_path = std::path::Path::new(SERIAL_DIR).join("s3_questions.json");
    let schema_json = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", schema_path.display()));
    let list = parse_question_add_schema(&schema_json).expect("fixture schema parses");
    assert_eq!(list.questions.len(), 2);

    let item = format!("{SETUP_MODULE_GUID}:0x10:0#{FORM_ID}");
    for (i, q) in list.questions.iter().enumerate() {
        assert_eq!(q.form_id, FORM_ID);
        let res = add_question(&mut img2, &item, q)
            .unwrap_or_else(|e| panic!("add_question #{}: {e:?}", i + 1));
        assert_eq!(res.question_id, q.question_id);
    }
    let e35_built = build_image(&img2).expect("build after question adds");
    assert_eq!(e35_built.len(), data.len(), "total flash length preserved");

    let setup_slot = find_file_range(&e35_triple, SETUP_MODULE_GUID);
    let sd_slot = find_file_range(&e35_triple, SETUPDATA_GUID);
    let zones = [
        (FIRST_SLOT, FIRST_SLOT + span),
        (setup_slot.start, setup_slot.end),
        (sd_slot.start, sd_slot.end),
    ];
    let mut outside = Vec::new();
    for (i, (a, b)) in e35_triple.iter().zip(e35_built.iter()).enumerate() {
        if a != b && !zones.iter().any(|(s, e)| i >= *s && i < *e) {
            outside.push(i);
        }
    }
    assert!(
        outside.is_empty(),
        "{} bytes changed outside the insert window + Setup/SetupData slots, first={outside:#x?}",
        outside.len()
    );

    let re = parse_image(&e35_built, ImageMode::Read, "re", "s4c").unwrap();
    let vol = &re.root.children[vol_idx];
    assert_eq!(vol.children.len(), files_before + 3);
    let tail: Vec<String> = vol.children[files_before..]
        .iter()
        .map(|f| {
            f.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(tail, expect_tail, "E35 chain order at end");
    assert!(
        !vol.children.iter().any(|f| matches!(&f.guid, Some(g)
            if g.to_string().to_ascii_uppercase() == SERIAL_DXE_GUID)),
        "SerialDxe 9A5163E7 must be absent from the E35 candidate (producer replaced, not added)"
    );
    for f in &vol.children[files_before..] {
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned in FV, got @{:#x}",
            f.offset
        );
    }
    for (i, f) in vol.children[files_before..].iter().enumerate() {
        let path = std::path::Path::new(SERIAL_DIR).join(serial[i]);
        let ffs = std::fs::read(&path).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
    }

    for (q, want_opts) in [(&list.questions[0], 5usize), (&list.questions[1], 4usize)] {
        let qi = question_info(&re, &format!("{item}:{}", q.question_id))
            .unwrap_or_else(|e| panic!("question_info q{}: {e:?}", q.question_id));
        assert_eq!(qi.form_id, u32::from(FORM_ID));
        assert_eq!(qi.question_id, u32::from(q.question_id));
        assert_eq!(qi.kind, "one_of");
        assert_eq!(qi.var_store_id, u32::from(q.var_store_id));
        assert_eq!(qi.var_offset, u32::from(q.var_offset));
        assert_eq!(qi.options.len(), want_opts);
        assert_eq!(qi.defaults.len(), 1);
        assert_eq!(qi.defaults[0].value, 0, "default 0 (115200 / VT-UTF8)");
    }

    let re2 = parse_image(&e35_built, ImageMode::Read, "re2", "s4cb").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, e35_built, "save -> open -> save must be byte-stable");

    eprintln!(
        "real_image s4c: files {files_before} -> {} span={span:#x} last_diff={last_diff:#x}",
        vol.children.len()
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_add_question_discovers_nested_setupdata() {
    let data = load_fw();
    let img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
    assert!(
        matches!(
            uefi_engine::hii::ami_patcher::pfs_payload_path(&img, None),
            Err(uefi_engine::hii::HiiError::AmiFilesNotFound)
        ),
        "legacy discovery must keep missing the live grandchild UI (documented defect)"
    );
    let path = uefi_engine::hii::ami_patcher::discover_pfs_payload_path(&img)
        .expect("deep discovery must resolve the live SetupData");
    assert_eq!(path, vec![3, 208, 0, 0]);
    let mut node = &img.root;
    for &i in &path {
        node = &node.children[i];
    }
    assert!(
        node.body.windows(4).any(|w| w == b"$SPF"),
        "resolved node must carry the $SPF container"
    );

    let mut live = parse_image(&data, ImageMode::Write, "probe", "s").unwrap();
    let schema = uefi_engine::hii::schema::QuestionAddSchema {
        form_id: 10019,
        prompt: "probe".into(),
        help: "probe help".into(),
        question_id: 512,
        var_store_id: 1,
        var_offset: u16::MAX,
        size: 1,
        options: vec![uefi_engine::hii::schema::QuestionAddOption {
            text: "off".into(),
            value: 0,
            default: None,
        }],
        defaults: None,
    };
    let err = uefi_engine::hii::add_question(&mut live, "3/28/1/0#10019", &schema)
        .expect_err("out-of-bounds var_offset must be rejected");
    eprintln!("live add_question probe error: {err}");
    assert!(
        matches!(err, uefi_engine::hii::HiiError::InvalidSchema(_)),
        "add_question must pass discovery and reach varstore validation on live geometry, got {err:?}"
    );
}

fn spf_body_of(img: &Image) -> Vec<u8> {
    let path = uefi_engine::hii::ami_patcher::discover_pfs_payload_path(img).unwrap();
    let mut node = &img.root;
    for &i in &path {
        node = &node.children[i];
    }
    node.body.clone()
}

fn setup_forms_package(img: &Image) -> Vec<u8> {
    let t = parse_target("3/28/1/0").unwrap();
    let node = find_item(&img.root, &t).unwrap();
    for blob in uefi_engine::hii::pe_resource::hii_resource_blobs(&node.body) {
        if let Some(list) = uefi_engine::hii::package_list::parse_package_list(blob) {
            for p in &list.packages {
                if uefi_engine::hii::ifr::is_form_package(p.bytes) {
                    return p.bytes.to_vec();
                }
            }
        }
    }
    panic!("no forms package in Setup PE 3/28/1/0");
}

fn live_question_schema(qid: u16, voff: u16) -> uefi_engine::hii::schema::QuestionAddSchema {
    uefi_engine::hii::schema::QuestionAddSchema {
        form_id: 10019,
        prompt: format!("S3 probe {qid}"),
        help: format!("S3 probe help {qid}"),
        question_id: qid,
        var_store_id: 1,
        var_offset: voff,
        size: 1,
        options: vec![
            uefi_engine::hii::schema::QuestionAddOption {
                text: "Off".into(),
                value: 0,
                default: None,
            },
            uefi_engine::hii::schema::QuestionAddOption {
                text: "On".into(),
                value: 1,
                default: Some(uefi_engine::hii::schema::DefaultClass::Optimized),
            },
        ],
        defaults: None,
    }
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_add_question_selective_ifr_fixup() {
    let data = load_fw();
    let base = parse_image(&data, ImageMode::Read, "base", "s").unwrap();
    let pkg_base = setup_forms_package(&base);
    let spf_base = spf_body_of(&base);
    let base_records = uefi_engine::hii::spf::scan_question_records(&spf_base);
    let r_base: Vec<_> = base_records
        .iter()
        .filter(|r| uefi_engine::hii::spf_record_resolves(&pkg_base, r.question_id, r.ifr_offset))
        .collect();
    let foreign_base: Vec<_> = base_records
        .iter()
        .filter(|r| !uefi_engine::hii::spf_record_resolves(&pkg_base, r.question_id, r.ifr_offset))
        .collect();
    eprintln!(
        "selective fixup: {} base records, {} setup-resolving, {} foreign",
        base_records.len(),
        r_base.len(),
        foreign_base.len()
    );
    assert_eq!(
        r_base.len(),
        32,
        "engine predicate: (qid, ifr) resolves at a question opcode of the Setup forms package"
    );
    assert_eq!(foreign_base.len(), 357);

    let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
    uefi_engine::hii::add_question(&mut img, "3/28/1/0#10019", &live_question_schema(512, 93))
        .expect("first live add_question");
    let pkg_after1 = setup_forms_package(&img);
    let delta1 = pkg_after1.len() - pkg_base.len();
    uefi_engine::hii::add_question(&mut img, "3/28/1/0#10019", &live_question_schema(513, 94))
        .expect("second live add_question");
    let pkg_after2 = setup_forms_package(&img);
    let delta2 = pkg_after2.len() - pkg_after1.len();

    let spf_final = spf_body_of(&img);
    let final_records = uefi_engine::hii::spf::scan_question_records(&spf_final);
    assert_eq!(final_records.len(), base_records.len() + 2);
    let rec512 = final_records
        .iter()
        .find(|r| r.question_id == 512)
        .expect("record for qid 512");
    let rec513 = final_records
        .iter()
        .find(|r| r.question_id == 513)
        .expect("record for qid 513");
    assert!(
        uefi_engine::hii::spf_record_resolves(&pkg_after2, 512, rec512.ifr_offset),
        "new record 512 must resolve at its splice position"
    );
    assert!(
        uefi_engine::hii::spf_record_resolves(&pkg_after2, 513, rec513.ifr_offset),
        "new record 513 must resolve at its splice position"
    );
    let insert1 = rec512.ifr_offset;
    assert_eq!(rec513.ifr_offset, insert1 + delta1 as u32);
    let mut shifted = 0;
    for r in &r_base {
        let fr = final_records
            .iter()
            .find(|x| x.offset == r.offset)
            .expect("base record offset must be stable");
        let expected = r.ifr_offset + u32::from(r.ifr_offset >= insert1) * (delta1 + delta2) as u32;
        assert_eq!(
            fr.ifr_offset, expected,
            "setup record qid {:#x} must shift by the deltas",
            r.question_id
        );
        assert!(
            uefi_engine::hii::spf_record_resolves(&pkg_after2, fr.question_id, fr.ifr_offset),
            "setup record qid {:#x} must still resolve after the splice",
            r.question_id
        );
        shifted += usize::from(r.ifr_offset >= insert1);
    }
    for r in &foreign_base {
        let fr = final_records
            .iter()
            .find(|x| x.offset == r.offset)
            .expect("foreign record offset must be stable");
        assert_eq!(
            fr.ifr_offset, r.ifr_offset,
            "foreign record qid {:#x} must keep its base ifr",
            r.question_id
        );
        assert_eq!(
            &spf_final[r.offset..r.offset + uefi_engine::hii::spf::SPF_RECORD_SIZE],
            &spf_base[r.offset..r.offset + uefi_engine::hii::spf::SPF_RECORD_SIZE],
            "foreign record qid {:#x} must stay byte-identical",
            r.question_id
        );
    }
    eprintln!(
        "selective fixup: {shifted}/{} setup records shifted, {} foreign untouched, deltas {delta1}+{delta2}",
        r_base.len(),
        foreign_base.len()
    );
}

const NP_SETUP_MODULE_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
const NP_SETUPDATA_GUID: &str = "FE612B72-203C-47B1-8560-A66D946EB371";
const NP_SETUP_FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";
const NP_PARENT_FORM_ID: u16 = 10019;
const NP_NEW_FORM_ID: u16 = 10101;
const NP_NEW_VARSTORE_ID: u16 = 31;
const NP_PAGE_TITLE: &str = "UEFIPatcher Serial Settings";
const NP_MAIN_FV_OFF: usize = 0x890000;
const NP_FIRST_SLOT: usize = 0xB63B18;
const NP_SERIAL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/serial");
const NP_SERIAL_FILES: [&str; 3] = [
    "SerialIoAmiDxe.ffs",
    "TerminalDxe.ffs",
    "SerialConsoleGlue.ffs",
];
const NP_EXPECT_TAIL: [&str; 3] = [
    "97C81E5D-8FA0-486A-AAEA-0EFDF090FE4F",
    "9E863906-A40F-4875-977F-5B93FF237FC6",
    "1EF3A7C2-9B64-4D58-8A31-5C0E9F2B7D43",
];

fn np_pkg_u16(pkg: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([pkg[off], pkg[off + 1]])
}

fn np_spf_u32(body: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(body[at..at + 4].try_into().unwrap())
}

fn np_spf_u16(body: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([body[at], body[at + 1]])
}

fn np_find_ref_op(pkg: &[u8], form_id: u16, qid: u16) -> Option<usize> {
    let span = uefi_engine::hii::form_hijack::locate_form(pkg, form_id)?;
    let mut i = span.form_op;
    while i + 2 <= span.next_form_op {
        let len = (pkg[i + 1] & 0x7F) as usize;
        if len < 2 {
            return None;
        }
        if pkg[i] == r_efi::hii::IFR_REF_OP && np_pkg_u16(pkg, i + 6) == qid {
            return Some(i);
        }
        i += len;
    }
    None
}

fn np_smoke_schema() -> uefi_engine::hii::schema::QuestionAddSchema {
    uefi_engine::hii::schema::QuestionAddSchema {
        form_id: NP_NEW_FORM_ID,
        prompt: "np smoke".into(),
        help: "np smoke help".into(),
        question_id: 602,
        var_store_id: NP_NEW_VARSTORE_ID,
        var_offset: 0x2,
        size: 1,
        options: vec![
            uefi_engine::hii::schema::QuestionAddOption {
                text: "A".into(),
                value: 0,
                default: None,
            },
            uefi_engine::hii::schema::QuestionAddOption {
                text: "B".into(),
                value: 1,
                default: None,
            },
        ],
        defaults: None,
    }
}

struct NpAssembly {
    data: Vec<u8>,
    built: Vec<u8>,
    spf_pre_page: Vec<u8>,
    spf_pre_refs: Vec<u8>,
    pkg_pre_refs: Vec<u8>,
    page: Option<uefi_engine::hii::AddPageResult>,
    form_string_ids: std::collections::HashMap<String, u16>,
    ref_string_ids: std::collections::HashMap<String, u16>,
    negative_smoke: Result<(), uefi_engine::hii::HiiError>,
    positive_smoke: Option<Result<(), uefi_engine::hii::HiiError>>,
}

fn np_assemble(with_page: bool) -> NpAssembly {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::add_page;
    use uefi_engine::hii::add_question;
    use uefi_engine::hii::add_ref;
    use uefi_engine::hii::check_question_add;
    use uefi_engine::hii::form_add::add_form;
    use uefi_engine::hii::schema::{parse_question_add_schema, parse_schema};
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    const MAIN_FV_OFF: usize = NP_MAIN_FV_OFF;

    let data = load_fw();
    assert_eq!(data.len(), 0x0100_0000, "16 MiB image expected");
    let serial: Vec<&str> = NP_SERIAL_FILES.to_vec();

    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == MAIN_FV_OFF as u32)
        .expect("main FV @0x890000");
    let files_before = img.root.children[vol_idx].children.len();
    assert!(files_before > 100, "main FV files: {files_before}");

    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in &serial {
        let path = std::path::Path::new(NP_SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let e35_triple = build_image(&img).expect("build after E35 triple inserts");
    assert_eq!(e35_triple.len(), data.len(), "total flash length preserved");

    let mut img2 = parse_image(&e35_triple, ImageMode::Write, "img2", "np").unwrap();
    let s3_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("s3_questions.json"))
            .unwrap();
    let s3_list = parse_question_add_schema(&s3_json).expect("s3 fixture parses");
    let item_10019 = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_PARENT_FORM_ID}");
    for q in &s3_list.questions {
        let res = add_question(&mut img2, &item_10019, q)
            .unwrap_or_else(|e| panic!("add_question q{}: {e:?}", q.question_id));
        assert_eq!(res.question_id, q.question_id);
    }
    let e35 = build_image(&img2).expect("build after q512/q513 adds");
    assert_eq!(e35.len(), data.len(), "total flash length preserved");

    let mut img3 = parse_image(&e35, ImageMode::Write, "img3", "np").unwrap();
    let spf_e35 = find_spf_leaf_body(&img3, NP_SETUPDATA_GUID);
    let e35_base = uefi_engine::hii::spf::container_start(&spf_e35).expect("$SPF in E35");
    assert_eq!(
        np_spf_u32(
            &spf_e35,
            e35_base + uefi_engine::hii::spf::SPF_PAGE_COUNT_OFFSET
        ),
        188,
        "E35 candidate must keep the live page count"
    );
    assert_eq!(
        uefi_engine::hii::spf::scan_question_records(&spf_e35).len(),
        391,
        "E35 = live 389 + q512/q513"
    );

    let form_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_form.json")).unwrap();
    let form_schema = parse_schema(&form_json).expect("np_form fixture parses");
    assert_eq!(
        form_schema.varstores.len(),
        1,
        "the added form must declare exactly the new varstore"
    );
    assert_eq!(
        form_schema.varstores[0].id, NP_NEW_VARSTORE_ID,
        "stock LIVE declares varstore ids up to 20; 31 is verified free"
    );
    assert_eq!(form_schema.varstores[0].name, "UefiPatcherSetup");
    assert_eq!(form_schema.varstores[0].size, 0x10);
    assert_eq!(form_schema.forms.len(), 1);
    assert_eq!(form_schema.forms[0].id, NP_NEW_FORM_ID);
    let setup_item = format!("{NP_SETUP_MODULE_GUID}:0x10:0");
    let form_res = add_form(&mut img3, &setup_item, &form_schema).expect("form add 10101");
    assert_eq!(form_res.inserted_form_ids, vec![NP_NEW_FORM_ID]);

    let pe_path = module_pe32_node_path(&img3, NP_SETUP_MODULE_GUID);
    let pkg_pre_refs = module_form_package(&img3, &pe_path);
    let item_10101 = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_NEW_FORM_ID}");
    let negative_smoke = check_question_add(&img3, &item_10101, &[np_smoke_schema()]);

    let spf_pre_page = find_spf_leaf_body(&img3, NP_SETUPDATA_GUID);
    let (page, positive_smoke, spf_pre_refs) = if with_page {
        let res = add_page(
            &mut img3,
            &item_10019,
            &uefi_engine::hii::schema::PageAddSchema {
                form_id: NP_NEW_FORM_ID,
                title: NP_PAGE_TITLE.into(),
            },
        )
        .expect("page add 10101");
        let spf_after = find_spf_leaf_body(&img3, NP_SETUPDATA_GUID);
        let smoke = check_question_add(&img3, &item_10101, &[np_smoke_schema()]);
        (Some(res), Some(smoke), spf_after)
    } else {
        (None, None, spf_pre_page.clone())
    };

    let ref_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_ref.json")).unwrap();
    let ref_list = parse_question_add_schema(&ref_json).expect("np_ref fixture parses");
    assert!(ref_list.questions.is_empty());
    assert_eq!(ref_list.refs.len(), 1);
    let ref_schema = &ref_list.refs[0];
    assert_eq!(ref_schema.form_id, NP_NEW_FORM_ID);
    assert_eq!(ref_schema.question_id, 528);
    let ref_res = add_ref(&mut img3, &item_10019, ref_schema).expect("ref add q0x210");
    assert_eq!(ref_res.question_id, 528);

    let built = build_image(&img3).expect("build final candidate");
    assert_eq!(built.len(), data.len(), "total flash length preserved");

    NpAssembly {
        data,
        built,
        spf_pre_page,
        spf_pre_refs,
        pkg_pre_refs,
        page,
        form_string_ids: form_res.string_ids,
        ref_string_ids: ref_res.string_ids,
        negative_smoke,
        positive_smoke,
    }
}

fn np_fv1_files(img: &Image) -> Vec<&FfsNode> {
    img.root
        .children
        .iter()
        .find(|c| c.offset == NP_MAIN_FV_OFF as u32)
        .expect("main FV @0x890000")
        .children
        .iter()
        .collect()
}

fn np_file_bytes(node: &FfsNode) -> Vec<u8> {
    let mut out = node.header.clone();
    out.extend_from_slice(&node.body);
    out.extend_from_slice(&node.tail);
    out
}

fn np_guid_of(node: &FfsNode) -> String {
    node.guid
        .map(|g| g.to_string().to_ascii_uppercase())
        .unwrap_or_default()
}

fn np_assert_fv1_layout(final_img: &Image, data: &[u8]) {
    let live_img = parse_image(data, ImageMode::Read, "live", "np").unwrap();
    let live = np_fv1_files(&live_img);
    let files = np_fv1_files(final_img);
    assert_eq!(
        files.len(),
        live.len() + NP_SERIAL_FILES.len(),
        "FV1 must hold the live stock files plus the serial triple"
    );

    for (i, (l, f)) in live.iter().zip(files.iter()).enumerate() {
        assert_eq!(
            np_guid_of(f),
            np_guid_of(l),
            "stock file order/GUIDs must be preserved at index {i}"
        );
    }

    let mut shift: Option<isize> = None;
    let mut shifted = 0usize;
    for (i, (l, f)) in live.iter().zip(files.iter()).enumerate() {
        let d = f.offset as isize - l.offset as isize;
        assert!(
            d >= 0,
            "stock file {i} must not move backwards ({} -> {})",
            l.offset,
            f.offset
        );
        if d != 0 {
            if let Some(want) = shift {
                assert_eq!(
                    d, want,
                    "all shifted FV1 files must move by one constant delta (file {i})"
                );
            } else {
                shift = Some(d);
            }
            assert_eq!(
                f.offset as usize,
                l.offset as usize + d as usize,
                "shifted file {i} must sit exactly at live offset + delta"
            );
            shifted += 1;
        }
    }
    let delta = shift.expect("at least the files after Setup must shift by the growth delta");
    assert!(delta > 0);
    assert_eq!(
        delta % 8,
        0,
        "FV1 files are 8-aligned, the push must be too"
    );
    eprintln!(
        "np fv1 layout: {shifted}/{} stock files shifted by constant delta {delta}",
        live.len()
    );

    for (i, (l, f)) in live.iter().zip(files.iter()).enumerate() {
        let guid = np_guid_of(f);
        if guid == NP_SETUP_MODULE_GUID || guid == NP_SETUPDATA_GUID {
            continue;
        }
        assert_eq!(
            np_file_bytes(f),
            np_file_bytes(l),
            "stock file {i} ({guid}) must stay byte-identical to LIVE"
        );
    }

    let tail = &files[live.len()..];
    for (i, f) in tail.iter().enumerate() {
        assert_eq!(
            np_guid_of(f),
            NP_EXPECT_TAIL[i],
            "serial triple order at chain end"
        );
        assert_eq!(
            f.header[23], 0xF8,
            "inserted file state must be polarity-1 valid"
        );
        assert_eq!(
            f.offset % 8,
            0,
            "FFS file must be 8-aligned, got @{:#x}",
            f.offset
        );
        assert!(
            f.offset as usize >= NP_FIRST_SLOT,
            "triple must sit in the FV1 free tail, got @{:#x}",
            f.offset
        );
        let ffs =
            std::fs::read(std::path::Path::new(NP_SERIAL_DIR).join(NP_SERIAL_FILES[i])).unwrap();
        assert_eq!(
            f.body,
            ffs[f.header.len()..f.header.len() + f.body.len()],
            "inserted body must be byte-identical to the fixture file"
        );
        let extent = f.offset as usize..f.offset as usize + f.header.len() + f.body.len();
        assert!(
            data[extent].iter().all(|&b| b == 0xFF),
            "triple file {i} must lie over 0xFF free tail in LIVE"
        );
    }
}

fn np_assert_ref_stage(asm: &NpAssembly, expected_page_count: u32) {
    use std::collections::HashMap;
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::spf;
    use uefi_engine::hii::spf_record_resolves;

    assert_ne!(asm.built, asm.data, "candidate must differ from LIVE");

    let re = parse_image(&asm.built, ImageMode::Read, "re", "np").unwrap();
    np_assert_fv1_layout(&re, &asm.data);
    let final_pkg = module_form_package(&re, &module_pe32_node_path(&re, NP_SETUP_MODULE_GUID));

    let delta = final_pkg.len() - asm.pkg_pre_refs.len();
    assert_eq!(delta, 15, "ref add must splice exactly the REF op");
    let ref_off =
        np_find_ref_op(&final_pkg, NP_PARENT_FORM_ID, 528).expect("REF q0x210 inside form 10019");
    assert_eq!(final_pkg[ref_off + 1] & 0x7F, 15, "REF op length");
    assert_eq!(
        np_pkg_u16(&final_pkg, ref_off + 13),
        NP_NEW_FORM_ID,
        "REF FormId@+13 must target 0x2775"
    );
    assert_eq!(np_pkg_u16(&final_pkg, ref_off + 6), 528, "REF qid@+6");
    assert_eq!(
        np_pkg_u16(&final_pkg, ref_off + 8),
        0,
        "REF var store id@+8 (stock: 0)"
    );
    assert_eq!(
        np_pkg_u16(&final_pkg, ref_off + 10),
        0xFFFF,
        "REF var offset@+10 carries the stock no-storage sentinel (all 31 stock REFs: 0xFFFF)"
    );
    assert_eq!(
        np_pkg_u16(&final_pkg, ref_off + 2),
        *asm.ref_string_ids.get("UEFIPatcher Setup").unwrap(),
        "REF prompt@+2"
    );
    assert_eq!(
        np_pkg_u16(&final_pkg, ref_off + 4),
        *asm.ref_string_ids
            .get("UEFIPatcher serial console settings")
            .unwrap(),
        "REF help@+4"
    );
    assert_eq!(
        final_pkg[ref_off + 15],
        r_efi::hii::IFR_END_OP,
        "REF must sit directly before the form END op"
    );

    let scope_balance = |pkg: &[u8]| {
        let mut bal = 0i32;
        let mut i = 4;
        while i + 2 <= pkg.len() {
            let len = (pkg[i + 1] & 0x7F) as usize;
            if len < 2 {
                break;
            }
            if pkg[i] == r_efi::hii::IFR_END_OP {
                bal -= 1;
            } else if pkg[i + 1] & 0x80 != 0 {
                bal += 1;
            }
            i += len;
        }
        bal
    };
    assert_eq!(
        scope_balance(&final_pkg),
        scope_balance(&asm.pkg_pre_refs),
        "the REF splice must not change the IFR scope balance (round-8 root cause: an own END after REF unbalanced the stack and crashed TSE)"
    );

    assert!(
        uefi_engine::hii::form_hijack::locate_form(&final_pkg, NP_NEW_FORM_ID).is_some(),
        "form 0x2775 must be present in the forms package"
    );
    let forms = uefi_engine::hii::forms::collect_forms(&re);
    let added: Vec<_> = forms
        .iter()
        .filter(|f| {
            f.formset_guid == NP_SETUP_FORMSET_GUID && f.form_id_ifr == u32::from(NP_NEW_FORM_ID)
        })
        .collect();
    assert_eq!(added.len(), 1, "form 10101 must appear exactly once");
    assert_eq!(
        added[0].title, NP_PAGE_TITLE,
        "form title string must resolve"
    );
    assert!(added[0].visible, "added form must not be suppressed");

    let item_10101 = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_NEW_FORM_ID}");
    for (qid, voff, prompt) in [
        (600u16, 0x0u32, "Serial Console"),
        (601u16, 0x1u32, "Verbose Boot"),
    ] {
        let qi = uefi_engine::hii::question_info(&re, &format!("{item_10101}:{qid}"))
            .unwrap_or_else(|e| panic!("question_info q{qid}: {e:?}"));
        assert_eq!(qi.form_id, u32::from(NP_NEW_FORM_ID));
        assert_eq!(qi.question_id, u32::from(qid));
        assert_eq!(qi.kind, "one_of");
        assert_eq!(qi.var_store_id, u32::from(NP_NEW_VARSTORE_ID));
        assert_eq!(qi.var_offset, voff);
        assert_eq!(qi.width, 1);
        assert_eq!(qi.options.len(), 2, "{prompt} options");
        assert_eq!(qi.options[0].value, 0);
        assert_eq!(qi.options[1].value, 1);
        assert_eq!(qi.options[0].flags, 0);
        assert_eq!(qi.options[1].flags, 0);
        assert_eq!(qi.defaults.len(), 1, "optimized default must round-trip");
        assert_eq!(qi.defaults[0].value, 0);
    }

    {
        let strings = uefi_engine::hii::strings::collect_strings(&re);
        let pe = &node_at_path(&re, &module_pe32_node_path(&re, NP_SETUP_MODULE_GUID)).body;
        for ids in [&asm.form_string_ids, &asm.ref_string_ids] {
            for (text, id) in ids {
                assert!(
                    strings
                        .iter()
                        .any(|s| s.string_id == u32::from(*id) && &s.text == text),
                    "string id {id} must resolve to '{text}' verbatim"
                );
                assert!(
                    pe.windows(text.len()).any(|w| w == text.as_bytes()),
                    "'{text}' must be present verbatim in the Setup module string package"
                );
            }
        }
    }

    let final_spf = find_spf_leaf_body(&re, NP_SETUPDATA_GUID);
    let fbase = spf::container_start(&final_spf).expect("$SPF survives the ref add");
    let pbase = spf::container_start(&asm.spf_pre_refs).expect("$SPF baseline");
    assert_eq!(fbase, pbase, "container start must stay put");
    assert_eq!(
        final_spf.len(),
        asm.spf_pre_refs.len(),
        "form add + ref add must not grow the $SPF container"
    );
    assert_eq!(
        np_spf_u32(&final_spf, fbase + spf::SPF_PAGE_COUNT_OFFSET),
        expected_page_count,
        "page count"
    );
    let container_len_field = (spf::SPF_HEADER_REGION_OFFSETS..spf::SPF_PAGE_COUNT_OFFSET)
        .step_by(4)
        .map(|p| np_spf_u32(&final_spf, fbase + p))
        .max()
        .expect("header zone u32 fields");
    assert_eq!(
        container_len_field as usize,
        final_spf.len() - fbase,
        "container-length header field must equal the container length"
    );

    let page_count = expected_page_count as usize;
    for slot in 0..page_count {
        assert_eq!(
            np_spf_u32(&final_spf, fbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot),
            np_spf_u32(
                &asm.spf_pre_refs,
                pbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot
            ),
            "page table slot {slot} must stay untouched by the ref add"
        );
    }

    let base_records = spf::scan_question_records(&asm.spf_pre_refs);
    let final_records = spf::scan_question_records(&final_spf);
    assert_eq!(
        final_records.len(),
        base_records.len(),
        "ref add must not append question records"
    );
    assert_eq!(
        spf::scan_string_controls(&final_spf).len(),
        spf::scan_string_controls(&asm.spf_pre_refs).len(),
        "ref add must not append string controls"
    );
    assert!(
        !final_records.iter().any(|r| r.question_id == 528),
        "the REF must not gain a $SPF record"
    );
    let final_by_offset: HashMap<usize, spf::SpfQuestionRecord> = final_records
        .iter()
        .copied()
        .map(|r| (r.offset, r))
        .collect();
    let insert_at = ref_off as u32;
    let mut shifted = 0usize;
    for r in &base_records {
        let fr = final_by_offset
            .get(&r.offset)
            .expect("append-only stage keeps record offsets stable");
        let resolves = spf_record_resolves(&asm.pkg_pre_refs, r.question_id, r.ifr_offset);
        if resolves && r.ifr_offset >= insert_at {
            assert_eq!(
                fr.ifr_offset,
                r.ifr_offset + delta as u32,
                "Setup record qid={:#x} must shift by the ref delta",
                r.question_id
            );
            assert!(
                spf_record_resolves(&final_pkg, r.question_id, fr.ifr_offset),
                "Setup record qid={:#x} must still resolve post-splice",
                r.question_id
            );
            shifted += 1;
        } else {
            assert_eq!(
                fr.ifr_offset, r.ifr_offset,
                "record qid={:#x} must keep its ifr offset",
                r.question_id
            );
        }
    }
    let expected_shifted = base_records
        .iter()
        .filter(|r| {
            spf_record_resolves(&asm.pkg_pre_refs, r.question_id, r.ifr_offset)
                && r.ifr_offset >= insert_at
        })
        .count();
    assert_eq!(shifted, expected_shifted);
    assert!(
        base_records.iter().all(|r| {
            !spf_record_resolves(&asm.pkg_pre_refs, r.question_id, r.ifr_offset)
                || r.ifr_offset < insert_at
                || final_by_offset[&r.offset].ifr_offset == r.ifr_offset + delta as u32
        }),
        "every resolving record above the ref insertion must shift by delta, others must not"
    );
    let shifted_ifr_fields: Vec<std::ops::Range<usize>> = base_records
        .iter()
        .filter(|r| {
            spf_record_resolves(&asm.pkg_pre_refs, r.question_id, r.ifr_offset)
                && r.ifr_offset >= insert_at
        })
        .map(|r| {
            (r.offset + spf::SPF_RECORD_IFR_OFFSET)..(r.offset + spf::SPF_RECORD_IFR_OFFSET + 4)
        })
        .collect();
    for (i, (a, b)) in asm.spf_pre_refs.iter().zip(final_spf.iter()).enumerate() {
        if !shifted_ifr_fields.iter().any(|rng| rng.contains(&i)) {
            assert_eq!(
                a, b,
                "byte {i:#x} outside shifted ifr fields must stay identical across the ref add"
            );
        }
    }

    if let Some(page) = &asm.page {
        assert_eq!(
            np_spf_u32(
                &final_spf,
                fbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * page.slot
            ),
            page.page_offset as u32,
            "registered slot must survive the rebuild"
        );
        let skeleton = fbase + page.page_offset;
        assert_eq!(
            np_spf_u16(&final_spf, skeleton + spf::SPF_PAGE_FORM_ID_OFFSET),
            NP_NEW_FORM_ID,
            "skeleton form id must survive the rebuild"
        );
        assert_eq!(
            np_spf_u32(&final_spf, skeleton + spf::SPF_PAGE_CNT_OFFSET),
            0,
            "skeleton cnt must stay zero"
        );
    }

    let re2 = parse_image(&asm.built, ImageMode::Read, "re2", "npb").unwrap();
    let again = build_image(&re2).expect("stable rebuild");
    assert_eq!(again, asm.built, "save -> open -> save must be byte-stable");

    eprintln!(
        "np ref stage: delta={delta:#x} ref@{ref_off:#x} records={} shifted={shifted} container={:#x}",
        final_records.len(),
        final_spf.len() - fbase
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_np1() {
    let asm = np_assemble(false);
    assert!(asm.page.is_none(), "np1 candidate must not register a page");
    assert!(
        matches!(
            asm.negative_smoke,
            Err(uefi_engine::hii::HiiError::NotFound)
        ),
        "without a registered page check_question_add must stop at plan_spf_append (NotFound), got {:?}",
        asm.negative_smoke
    );
    assert!(asm.positive_smoke.is_none());
    np_assert_ref_stage(&asm, 188);
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_np2() {
    use uefi_engine::hii::spf;

    let asm = np_assemble(true);
    let page = asm
        .page
        .as_ref()
        .expect("np2 candidate must register the page");

    assert!(
        matches!(
            asm.negative_smoke,
            Err(uefi_engine::hii::HiiError::NotFound)
        ),
        "before page add the same call must return NotFound, got {:?}",
        asm.negative_smoke
    );
    let positive = asm
        .positive_smoke
        .as_ref()
        .expect("np2 smoke after page add");
    assert!(
        positive.is_ok(),
        "in-bounds throwaway (q0x25A, vs 2 @0x2) must pass the whole preflight once the page resolves, got {:?}",
        positive
    );

    let before = &asm.spf_pre_page;
    let after = &asm.spf_pre_refs;
    let bbase = spf::container_start(before).expect("$SPF before page add");
    let abase = spf::container_start(after).expect("$SPF after page add");
    assert_eq!(bbase, abase);
    assert_eq!(
        np_spf_u32(before, bbase + spf::SPF_PAGE_COUNT_OFFSET),
        188,
        "page count before registration"
    );
    assert_eq!(
        page.slot, 188,
        "registration slot = count read before the bump"
    );
    assert_eq!(
        np_spf_u32(after, abase + spf::SPF_PAGE_COUNT_OFFSET),
        189,
        "page count 188 -> 189"
    );
    assert_eq!(
        np_spf_u32(after, abase + spf::SPF_PAGE_TABLE_OFFSET + 4 * 188),
        page.page_offset as u32,
        "slot[188] must hold the skeleton offset"
    );
    assert_eq!(
        page.page_offset,
        before.len() - bbase,
        "skeleton must be appended at the container end"
    );
    assert_eq!(
        after.len(),
        before.len() + 0x20,
        "container grows exactly 0x20"
    );

    let parent_slot = (0..188).find(|&slot| {
        let off = np_spf_u32(before, bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot) as usize;
        off != 0
            && np_spf_u16(before, bbase + off + spf::SPF_PAGE_FORM_ID_OFFSET) == NP_PARENT_FORM_ID
    });
    assert_eq!(parent_slot, Some(8), "parent page slot per plan");
    let parent_off = np_spf_u32(before, bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * 8) as usize;

    let skeleton = abase + page.page_offset;
    assert!(
        after[skeleton..skeleton + 8].iter().all(|&b| b == 0),
        "skeleton must carry the 8-zero prefix"
    );
    assert_eq!(
        after[skeleton + spf::SPF_PAGE_MARKER_OFFSET],
        before[bbase + parent_off + spf::SPF_PAGE_MARKER_OFFSET],
        "marker must be cloned from the parent page"
    );
    assert_eq!(
        np_spf_u16(after, skeleton + spf::SPF_PAGE_FORM_ID_OFFSET),
        NP_NEW_FORM_ID,
        "skeleton fid"
    );
    assert_eq!(
        np_spf_u16(after, skeleton + spf::SPF_PAGE_TITLE_ID_OFFSET),
        page.title_string_id,
        "skeleton title-id = appended page-add title string"
    );
    assert_eq!(
        np_spf_u16(after, skeleton + spf::SPF_PAGE_SEQ_OFFSET),
        188,
        "skeleton seq"
    );
    assert_eq!(
        np_spf_u16(after, skeleton + spf::SPF_PAGE_PARENT_OFFSET),
        8,
        "skeleton B = parent page slot"
    );
    assert_eq!(
        np_spf_u32(after, skeleton + spf::SPF_PAGE_IMAGE_OFFSET),
        np_spf_u32(before, bbase + parent_off + spf::SPF_PAGE_IMAGE_OFFSET),
        "u18 must be cloned from the parent page"
    );
    assert_eq!(
        np_spf_u32(after, skeleton + spf::SPF_PAGE_CNT_OFFSET),
        0,
        "skeleton cnt = 0"
    );

    for slot in 0..188 {
        assert_eq!(
            np_spf_u32(after, abase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot),
            np_spf_u32(before, bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot),
            "pre-existing slot {slot} must stay identical"
        );
    }
    for slot in 0..188 {
        let off = np_spf_u32(before, bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot) as usize;
        if off == 0 {
            continue;
        }
        let cnt = np_spf_u32(before, bbase + off + spf::SPF_PAGE_CNT_OFFSET) as usize;
        let span = spf::SPF_PAGE_HEADER_SIZE + 4 * cnt;
        assert_eq!(
            &after[abase + off..abase + off + span],
            &before[bbase + off..bbase + off + span],
            "pre-existing page body at slot {slot} must stay byte-identical"
        );
    }

    let mut len_pos = bbase + spf::SPF_HEADER_REGION_OFFSETS;
    let mut len_max = np_spf_u32(before, len_pos);
    for p in (spf::SPF_HEADER_REGION_OFFSETS..spf::SPF_PAGE_COUNT_OFFSET)
        .step_by(4)
        .skip(1)
    {
        let v = np_spf_u32(before, bbase + p);
        if v >= len_max {
            len_max = v;
            len_pos = bbase + p;
        }
    }
    assert_eq!(
        np_spf_u32(after, len_pos),
        np_spf_u32(before, len_pos) + 0x20,
        "container-length header field must grow by 0x20"
    );
    let count_field = bbase + spf::SPF_PAGE_COUNT_OFFSET..bbase + spf::SPF_PAGE_COUNT_OFFSET + 4;
    let gap_field = bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * 188
        ..bbase + spf::SPF_PAGE_TABLE_OFFSET + 4 * 188 + 4;
    let len_field = len_pos..len_pos + 4;
    for (i, (a, b)) in before.iter().zip(after.iter()).enumerate() {
        if !count_field.contains(&i) && !gap_field.contains(&i) && !len_field.contains(&i) {
            assert_eq!(
                a, b,
                "page add may only touch count, the zero gap slot and the length field at {i:#x}"
            );
        }
    }

    {
        let strings = uefi_engine::hii::strings::collect_strings(
            &parse_image(&asm.built, ImageMode::Read, "re", "np2").unwrap(),
        );
        assert!(
            strings
                .iter()
                .any(|s| s.string_id == u32::from(page.title_string_id) && s.text == NP_PAGE_TITLE),
            "the page title string must resolve to '{NP_PAGE_TITLE}'"
        );
    }

    np_assert_ref_stage(&asm, 189);

    eprintln!(
        "np2 page add: slot={} skeleton=container+{:#x} title-id={} parent=slot8@{:#x}",
        page.slot, page.page_offset, page.title_string_id, parent_off
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_ops_insert_serial_np3() {
    use uefi_engine::builder::build_image;
    use uefi_engine::hii::add_page;
    use uefi_engine::hii::add_question;
    use uefi_engine::hii::add_ref;
    use uefi_engine::hii::form_add::add_form;
    use uefi_engine::hii::schema::{parse_question_add_schema, parse_schema};
    use uefi_engine::hii::spf;
    use uefi_engine::ops::{InsertMode, insert};
    use uefi_engine::types::{ImageMode, Target};

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "i1", "s").unwrap();
    let vol_idx = img
        .root
        .children
        .iter()
        .position(|c| c.offset == NP_MAIN_FV_OFF as u32)
        .unwrap();
    let files_before = img.root.children[vol_idx].children.len();
    let mut anchor = Target::Path(vec![vol_idx, files_before - 1]);
    for name in NP_SERIAL_FILES {
        let path = std::path::Path::new(NP_SERIAL_DIR).join(name);
        let ffs = std::fs::read(&path).unwrap();
        insert(&mut img.root, &anchor, &ffs, InsertMode::After).unwrap();
        let last = img.root.children[vol_idx].children.len() - 1;
        anchor = Target::Path(vec![vol_idx, last]);
    }
    let triple = build_image(&img).unwrap();

    let mut img2 = parse_image(&triple, ImageMode::Write, "i2", "e39").unwrap();
    let s3_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("s3_questions.json"))
            .unwrap();
    let s3_list = parse_question_add_schema(&s3_json).unwrap();
    let item_10019 = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_PARENT_FORM_ID}");
    for q in &s3_list.questions {
        add_question(&mut img2, &item_10019, q).unwrap();
    }
    let s3 = build_image(&img2).unwrap();

    let mut img3 = parse_image(&s3, ImageMode::Write, "i3", "e39").unwrap();
    let form_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_form_e16p.json"))
            .unwrap();
    let form_schema = parse_schema(&form_json).unwrap();
    let setup_item = format!("{NP_SETUP_MODULE_GUID}:0x10:0");
    add_form(&mut img3, &setup_item, &form_schema).expect("form add 10101 (e16p, varstore-free)");
    add_page(
        &mut img3,
        &item_10019,
        &uefi_engine::hii::schema::PageAddSchema {
            form_id: NP_NEW_FORM_ID,
            title: NP_PAGE_TITLE.into(),
        },
    )
    .expect("page add 10101");

    let q_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_questions_v1.json"))
            .unwrap();
    let q_list = parse_question_add_schema(&q_json).unwrap();
    let item_10101 = format!("{NP_SETUP_MODULE_GUID}:0x10:0#{NP_NEW_FORM_ID}");
    for q in &q_list.questions {
        add_question(&mut img3, &item_10101, q)
            .unwrap_or_else(|e| panic!("add_question q{}: {e:?}", q.question_id));
    }

    let ref_json =
        std::fs::read_to_string(std::path::Path::new(NP_SERIAL_DIR).join("np_ref.json")).unwrap();
    let ref_list = parse_question_add_schema(&ref_json).unwrap();
    add_ref(&mut img3, &item_10019, &ref_list.refs[0]).expect("ref add q0x210");

    let built = build_image(&img3).expect("build E39");
    assert_eq!(built.len(), data.len());
    let re = parse_image(&built, ImageMode::Read, "re", "e39").unwrap();
    np_assert_fv1_layout(&re, &data);

    let final_pkg = module_form_package(&re, &module_pe32_node_path(&re, NP_SETUP_MODULE_GUID));
    let r = np_find_ref_op(&final_pkg, NP_PARENT_FORM_ID, 528).expect("REF in 10019");
    assert_eq!(final_pkg[r + 1] & 0x7F, 15);
    assert_eq!(np_pkg_u16(&final_pkg, r + 10), 0xFFFF, "voff sentinel");
    assert_eq!(
        final_pkg[r + 15],
        r_efi::hii::IFR_END_OP,
        "stock form END after REF"
    );

    let stock_img = parse_image(&data, ImageMode::Read, "st", "e39").unwrap();
    let stock_pkg = module_form_package(
        &stock_img,
        &module_pe32_node_path(&stock_img, NP_SETUP_MODULE_GUID),
    );
    let stock_spf = find_spf_leaf_body(&stock_img, NP_SETUPDATA_GUID);
    let final_spf = find_spf_leaf_body(&re, NP_SETUPDATA_GUID);
    let stock_recs = spf::scan_question_records(&stock_spf);
    let final_recs = spf::scan_question_records(&final_spf);
    assert_eq!(final_recs.len(), 393, "389 stock + q512/q513 + q600/q601");

    let mut checked = 0;
    for sr in &stock_recs {
        if !uefi_engine::hii::spf_record_resolves(&stock_pkg, sr.question_id, sr.ifr_offset) {
            continue;
        }
        let fr = final_recs
            .iter()
            .find(|f| f.question_id == sr.question_id)
            .expect("stock live record must survive");
        assert!(
            uefi_engine::hii::spf_record_resolves(&final_pkg, fr.question_id, fr.ifr_offset),
            "record q{}: ifr {:#x} must resolve to its own question op in the final package (round-11 invariant)",
            fr.question_id,
            fr.ifr_offset
        );
        checked += 1;
    }
    println!("round-11 invariant: {checked} live records stay IFR-consistent");
    assert!(
        checked > 25,
        "expected the LIVE subset of stock records, got {checked}"
    );

    for qid in [600u16, 601] {
        assert!(
            final_recs.iter().any(|x| x.question_id == qid),
            "q{qid} must carry a $SPF record"
        );
        let qi = uefi_engine::hii::question_info(&re, &format!("{item_10101}:{qid}")).unwrap();
        assert_eq!(
            qi.var_offset,
            u32::from(qid) - 505,
            "q600@95/q601@96 on stock varstore 1"
        );
    }
}
