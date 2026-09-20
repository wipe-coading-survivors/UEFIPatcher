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
    if node.node_type == FfsType::Section && node.subtype == EFI_SECTION_COMPRESSION {
        if let ParsingData::CompressedSection(cd) = &node.parsing_data {
            if cd.compression_type == 1 && node.body.len() > 5 {
                *total += 1;
                if !node.children.is_empty() {
                    *expanded += 1;
                }
            }
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

        let built = uefi_engine::builder::build_image(&image).unwrap();
        assert_eq!(
            built, data,
            "{name}: round-trip без правок должен быть байт-точным"
        );

        eprintln!("{name}: algo1={total} forms={}", forms.len());
    }
}
