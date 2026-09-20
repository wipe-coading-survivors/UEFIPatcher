use std::path::PathBuf;

use uefi_engine::ffs::EFI_SECTION_COMPRESSION;
use uefi_engine::parser::image::parse_image;
use uefi_engine::types::{FfsNode, FfsType, ImageMode, ParsingData};

fn asrock_path(name: &str) -> PathBuf {
    if let Ok(dir) = std::env::var("UEFIPATCHER_TEST_ASROCK_DIR") {
        return PathBuf::from(dir).join(name);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../refs/amibcp")
        .join(name)
}

fn count_algo1(node: &FfsNode, total: &mut usize, expanded: &mut usize) {
    if node.node_type == FfsType::Section
        && node.subtype == EFI_SECTION_COMPRESSION
        && let ParsingData::CompressedSection(cd) = &node.parsing_data
        && cd.compression_type == 1
        && node.body.len() > 5
    {
        *total += 1;
        if !node.children.is_empty() {
            *expanded += 1;
        }
    }
    for child in &node.children {
        count_algo1(child, total, expanded);
    }
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_tiano_expanded_hii_alive_round_trip() {
    for name in ["C275D4I3.20", "226D2IL3.30", "226D2IL3.50"] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        let image = parse_image(&data, ImageMode::Write, "t", "s").unwrap();

        let mut total = 0;
        let mut expanded = 0;
        count_algo1(&image.root, &mut total, &mut expanded);
        assert!(
            total > 0,
            "{name}: ожидали хотя бы одну algo-1 секцию, получено {total}"
        );
        if name.starts_with("C275") {
            assert!(
                total > 100,
                "{name}: ожидали сотни algo-1 секций, получено {total}"
            );
        }
        assert_eq!(
            total, expanded,
            "{name}: все algo-1 секции должны развернуться"
        );

        let forms = uefi_engine::hii::forms::collect_forms(&image);
        assert!(
            !forms.is_empty(),
            "{name}: HII-канал должен ожить после декомпрессии"
        );
        if name == "C275D4I3.20" {
            let guids: std::collections::HashSet<&str> =
                forms.iter().map(|f| f.formset_guid.as_str()).collect();
            assert!(
                guids.contains("7B59104A-C00D-4158-87FF-F04D6396A915"),
                "{name}: Setup FormSet-GUID не найден, фактические: {guids:?}"
            );
        }

        let built = uefi_engine::builder::build_image(&image).unwrap();
        assert_eq!(
            built, data,
            "{name}: round-trip без правок должен быть байт-точным"
        );

        eprintln!("{name}: algo1={total} forms={}", forms.len());
    }
}

fn count_depth1(stores: &[uefi_engine::nvar::NvarStoreListing]) -> usize {
    stores
        .iter()
        .flat_map(|s| s.rows.iter())
        .filter(|r| r.depth == 1)
        .count()
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_226d2il_nvar_listing_geometry() {
    for name in ["226D2IL3.30", "226D2IL3.50"] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        let image = parse_image(&data, ImageMode::Read, "t", "s").unwrap();
        let stores = uefi_engine::nvar::nvar_list(&image, None, false).unwrap();
        assert_eq!(stores.len(), 1, "{name}: один NVRAM-стор");
        let s = &stores[0];
        assert_eq!(count_depth1(&stores), 14, "{name}: 14 переменных");
        let main = s
            .rows
            .iter()
            .find(|r| r.name == "Setup" && r.size == 1217)
            .unwrap_or_else(|| panic!("{name}: main Setup"));
        assert_eq!(main.offset, 0x500088, "{name}");
        assert_eq!(
            main.guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .as_deref(),
            Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            "{name}"
        );
        let second_size = if name == "226D2IL3.30" { 204 } else { 206 };
        let second = s
            .rows
            .iter()
            .find(|r| r.name == "Setup" && r.size == second_size)
            .unwrap_or_else(|| panic!("{name}: second Setup"));
        assert_eq!(
            second
                .guid
                .map(|g| g.to_string().to_ascii_uppercase())
                .as_deref(),
            Some("01239999-FC0E-4B6E-9E79-D54D5DB6CD20"),
            "{name}"
        );
        let items = uefi_engine::parser::image::list_items(&image.root, None);
        let nvar_nodes: Vec<_> = items.iter().filter(|n| n.is_nvar).collect();
        assert_eq!(
            nvar_nodes.len(),
            2,
            "{name}: живой стор + запечённая копия за Tiano"
        );
        assert!(nvar_nodes.iter().all(|n| n.name == "NVRAM store"), "{name}");
    }
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_226d2il_nvar_bake_sol_4g() {
    for name in ["226D2IL3.30", "226D2IL3.50"] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        assert_eq!(data[0x500089], 0, "{name}: SOL-байт до правки");
        assert_eq!(data[0x5004FD], 0, "{name}: 4G-байт до правки");
        let mut img = parse_image(&data, ImageMode::Write, "t", "s").unwrap();
        let out = uefi_engine::nvar::nvar_set(
            &mut img,
            "Setup",
            Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            1,
            1,
            1,
        )
        .unwrap();
        assert_eq!(out.applied.len(), 1, "{name}: один raw-стор");
        let out = uefi_engine::nvar::nvar_set(
            &mut img,
            "Setup",
            Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            1141,
            1,
            1,
        )
        .unwrap();
        assert_eq!(out.applied.len(), 1, "{name}");
        let built = uefi_engine::builder::build_image(&img).unwrap();
        assert_eq!(built.len(), data.len(), "{name}: длина образа сохранена");
        let diff: Vec<usize> = (0..data.len()).filter(|&i| data[i] != built[i]).collect();
        assert_eq!(
            diff,
            vec![0x500089, 0x5004FD],
            "{name}: дифф ровно два байта"
        );
        let re = parse_image(&built, ImageMode::Read, "t2", "s2").unwrap();
        let built2 = uefi_engine::builder::build_image(&re).unwrap();
        assert_eq!(built, built2, "{name}: round-trip байт-точен (FFS валиден)");
        let stores = uefi_engine::nvar::nvar_list(&re, None, true).unwrap();
        let main = stores[0]
            .rows
            .iter()
            .find(|r| r.name == "Setup" && r.size == 1217)
            .unwrap();
        assert_eq!(main.data[1], 1, "{name}: SOL=1 в повторном листинге");
        assert_eq!(main.data[1141], 1, "{name}: 4G=1");
    }
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_c275_nvar_listing() {
    let data = std::fs::read(asrock_path("C275D4I3.20")).unwrap();
    let image = parse_image(&data, ImageMode::Read, "t", "s").unwrap();
    let stores = uefi_engine::nvar::nvar_list(&image, None, false).unwrap();
    assert!(!stores.is_empty());
    for (name, size) in [("Setup", 226), ("IntelSetup", 608), ("ServerSetup", 456)] {
        assert!(
            stores
                .iter()
                .flat_map(|s| s.rows.iter())
                .any(|r| r.name == name && r.size == size),
            "C275: {name} {size}b отсутствует"
        );
    }
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored); bakes deliverable artifacts"]
fn real_asrock_bake_sol4g_artifacts() {
    for (name, out_name) in [
        ("226D2IL3.30", "asr1-330-sol4g-on.bin"),
        ("226D2IL3.50", "asr1-350-sol4g-on.bin"),
    ] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        let mut img = parse_image(&data, ImageMode::Write, "t", "s").unwrap();
        uefi_engine::nvar::nvar_set(
            &mut img,
            "Setup",
            Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            1,
            1,
            1,
        )
        .unwrap();
        uefi_engine::nvar::nvar_set(
            &mut img,
            "Setup",
            Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            1141,
            1,
            1,
        )
        .unwrap();
        let built = uefi_engine::builder::build_image(&img).unwrap();
        let diff: Vec<usize> = (0..data.len()).filter(|&i| data[i] != built[i]).collect();
        assert_eq!(diff, vec![0x500089, 0x5004FD], "{name}");
        let out_path = asrock_path(out_name);
        std::fs::write(&out_path, &built).unwrap();
        println!("{out_name}: {}", out_path.display());
    }
}

const ASR1_SETUP_FILE: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
const ASR1_4G_ITEM: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21:0x18:0#1158:0x010D";

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_226d2il_forms_in_freeform_subtype_guid() {
    for name in ["226D2IL3.30", "226D2IL3.50"] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        let image = parse_image(&data, ImageMode::Read, "t", "s").unwrap();
        let forms = uefi_engine::hii::forms::collect_forms(&image);
        let target_prefix = format!("{ASR1_SETUP_FILE}:0x18:0");
        let from_0x18: Vec<_> = forms
            .iter()
            .filter(|f| f.form_id.starts_with(&target_prefix))
            .collect();
        assert_eq!(
            from_0x18.len(),
            85,
            "{name}: 85 форм из 0x18-канала (7 форм-пакетов probe 2026-09-21)"
        );
        let titled = from_0x18.iter().filter(|f| !f.title.is_empty()).count();
        assert!(
            titled >= 60,
            "{name}: титулы из строкового пакета, titled={titled}"
        );
        assert_eq!(
            from_0x18[0].formset_guid, "985EEE91-BCAC-4238-8778-57EFDC93F24E",
            "{name}"
        );
        let sol =
            uefi_engine::hii::list_questions(&image, &format!("{ASR1_SETUP_FILE}:0x18:0"), 1025)
                .unwrap();
        assert!(
            sol.iter().any(|q| q.question_id == 5 && q.width == 1),
            "{name}: SOL-вопрос (form 0x0401 qid 0x0005) виден"
        );
    }
    let data = std::fs::read(asrock_path("C275D4I3.20")).unwrap();
    let image = parse_image(&data, ImageMode::Read, "t", "s").unwrap();
    let forms = uefi_engine::hii::forms::collect_forms(&image);
    assert!(!forms.is_empty(), "C275: формы из .rsrc как раньше");
    assert!(
        forms.iter().all(|f| !f.form_id.contains(":0x18:")),
        "C275: канал 0x18 не добавляет задвоений"
    );
}

#[test]
#[ignore = "requires external ASRock images under refs/amibcp/ (gitignored)"]
fn real_asrock_226d2il_set_value_bakes_4g_default() {
    for name in ["226D2IL3.30", "226D2IL3.50"] {
        let data = std::fs::read(asrock_path(name)).unwrap();
        assert_eq!(data[0x5004FD], 0, "{name}: 4G-байт до правки");
        let mut img = parse_image(&data, ImageMode::Write, "t", "s").unwrap();
        let out = uefi_engine::hii::set_value(&mut img, ASR1_4G_ITEM, 1).unwrap();
        assert_eq!(out.applied.len(), 1, "{name}: только живая raw-копия");
        let built = uefi_engine::builder::build_image(&img).unwrap();
        let diff: Vec<usize> = (0..data.len()).filter(|&i| data[i] != built[i]).collect();
        assert_eq!(
            diff,
            vec![0x5004FD],
            "{name}: дифф ровно один байт — совпадает с nvar-путём"
        );
        assert_eq!(built[0x5004FD], 1, "{name}");
        let re = parse_image(&built, ImageMode::Read, "t2", "s2").unwrap();
        let forms = uefi_engine::hii::forms::collect_forms(&re);
        assert_eq!(
            forms
                .iter()
                .filter(|f| f.form_id.starts_with(&format!("{ASR1_SETUP_FILE}:0x18:0")))
                .count(),
            85,
            "{name}: формы живы после round-trip"
        );
    }
}
