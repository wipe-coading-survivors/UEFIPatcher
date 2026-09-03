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
