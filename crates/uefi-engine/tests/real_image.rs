use std::path::PathBuf;

use uefi_engine::ffs::{EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, is_lzma_guid};
use uefi_engine::parser::file::parse_file;
use uefi_engine::parser::image::{list_items, parse_image, search};
use uefi_engine::parser::section::parse_sections;
use uefi_engine::parser::target::{find_item, parse_target};
use uefi_engine::parser::volume::parse_volume;
use uefi_engine::types::{Action, FfsNode, FfsType, ParsingData};

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
    assert!(
        strings
            .iter()
            .all(|s| s.language.to_lowercase().starts_with("en")),
        "expected primary language ~English (en/en-US/eng), got {:?}",
        strings.first().map(|s| s.language.as_str())
    );

    eprintln!(
        "real_image hii: {} forms across {} formsets {:?}; {} strings (language {:?})",
        forms.len(),
        formsets.len(),
        formsets,
        strings.len(),
        strings.first().map(|s| &s.language)
    );
}

#[test]
#[ignore = "requires external real BIOS image under refs/fw/ (gitignored)"]
fn real_image_hii_form_visibility_round_trip() {
    use uefi_engine::hii::forms::collect_forms;
    use uefi_engine::hii::set_item_visibility;
    use uefi_engine::types::ImageMode;

    let data = load_fw();
    let mut img = parse_image(&data, ImageMode::Write, "img1", "s1").expect("parse_image");

    let forms = collect_forms(&img);
    assert!(!forms.is_empty());
    let form_id = forms[0].form_id.clone();

    let t = parse_target(&form_id).expect("form_id parses as target");
    let node = find_item(&img.root, &t).expect("form_id resolves in tree");
    assert_eq!(
        node.node_type,
        FfsType::Section,
        "form target must resolve to a Section"
    );

    let err = set_item_visibility(&mut img, &form_id, true)
        .expect_err("HNX99TF HII lives inside LZMA-guided wrappers; mutation must be refused until the recompression phase (spec §6)");
    assert!(matches!(
        err,
        uefi_engine::hii::HiiError::MutationBehindCompression
    ));
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
