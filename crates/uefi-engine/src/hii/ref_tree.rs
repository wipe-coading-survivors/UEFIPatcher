use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::hii::ifr::parse_form_package;
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::{bare_form_packages, hii_resource_blobs, hii_resource_ranges};
use crate::hii::values::walk_statements;
use crate::types::{FfsNode, FfsType, Image, guid_to_upper_string};
use r_efi::hii::{IFR_REF_OP, PACKAGE_FORMS};

/// REF-рёбра одного form-пакета: в форме X каждый IFR_REF_OP с
/// length >= 15 даёт ребро X → FormId@+13 (паттерн чтения — gates.rs,
/// ветка Wraps::Ref). REF вне формы и короткие REF пропускаются;
/// дубликаты схлопываются. Спека tui-forms-view §3.5.
pub(crate) fn package_edges(pkg: &[u8]) -> Vec<(u16, u16)> {
    let mut out: Vec<(u16, u16)> = Vec::new();
    walk_statements(pkg, |op, off, len, current| {
        if op != IFR_REF_OP || len < 15 {
            return;
        }
        let Some(parent) = current else { return };
        let child = u16::from_le_bytes([pkg[off + 13], pkg[off + 14]]);
        if !out.contains(&(parent, child)) {
            out.push((parent, child));
        }
    });
    out
}

/// REF-рёбра всех form-пакетов образа (обход как у collect_forms:
/// RAW / PE-resource / bare, рекурсивно через compression). NOT
/// read-only-ограничен — работает в любом ImageMode; висячие цели
/// отдаются как есть. Спека tui-forms-view §3.5.
pub fn collect_edges(image: &Image) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    walk_files(&image.root, &mut out);
    out
}

fn walk_files(node: &FfsNode, out: &mut Vec<(String, u32, u32)>) {
    for child in &node.children {
        if child.node_type == FfsType::File {
            collect_file_edges(child, out);
        }
        walk_files(child, out);
    }
}

fn collect_file_edges(file: &FfsNode, out: &mut Vec<(String, u32, u32)>) {
    if file.guid.is_none() {
        return;
    }
    walk_sections(file, out);
}

fn walk_sections(node: &FfsNode, out: &mut Vec<(String, u32, u32)>) {
    for child in &node.children {
        if child.node_type != FfsType::Section {
            continue;
        }
        match child.subtype {
            EFI_SECTION_RAW => {
                if let Some(fs) = parse_form_package(&child.body) {
                    push_edges(&child.body, &fs.guid, out);
                }
            }
            EFI_SECTION_PE32 => {
                let ranges = hii_resource_ranges(&child.body);
                for blob in hii_resource_blobs(&child.body) {
                    let Some(list) = parse_package_list(blob) else {
                        continue;
                    };
                    for pkg in &list.packages {
                        if pkg.kind != PACKAGE_FORMS {
                            continue;
                        }
                        if let Some(fs) = parse_form_package(pkg.bytes) {
                            push_edges(pkg.bytes, &fs.guid, out);
                        }
                    }
                }
                for pkg in bare_form_packages(&child.body, &ranges) {
                    if let Some(fs) = parse_form_package(pkg) {
                        push_edges(pkg, &fs.guid, out);
                    }
                }
            }
            EFI_SECTION_COMPRESSION | EFI_SECTION_GUID_DEFINED => walk_sections(child, out),
            _ => {}
        }
    }
}

fn push_edges(pkg: &[u8], formset: &crate::types::Guid, out: &mut Vec<(String, u32, u32)>) {
    let guid = guid_to_upper_string(formset);
    for (parent, child) in package_edges(pkg) {
        let e = (guid.clone(), u32::from(parent), u32::from(child));
        if !out.contains(&e) {
            out.push(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use crate::types::{Action, Guid, ImageMode, ParsingData};
    use r_efi::hii::{IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_REF_OP, PACKAGE_FORMS};

    const FILE_GUID: &str = "899407d7-92a6-4174-968f-6f0b47f86a23";
    const FORMSET_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set() -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&1u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16) -> Vec<u8> {
        opcode(
            IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), 0u16.to_le_bytes()].concat(),
        )
    }

    fn end() -> Vec<u8> {
        vec![IFR_END_OP, 0x02]
    }

    /// 11 байт question-header: prompt@0 help@2 qid@4 store@6 offset@8 flags@10
    /// (абсолютно: +2..+13; REF FormId читается на +13 — gates.rs:201).
    fn question_header(prompt: u16, qid: u16, store: u16, off: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&prompt.to_le_bytes());
        p.extend_from_slice(&0x01A4u16.to_le_bytes());
        p.extend_from_slice(&qid.to_le_bytes());
        p.extend_from_slice(&store.to_le_bytes());
        p.extend_from_slice(&off.to_le_bytes());
        p.push(0x00);
        p
    }

    /// IFR_REF: header(11) + FormId u16 → length = 15 (минимум для чтения +13).
    fn ref_op(qid: u16, target_form: u16) -> Vec<u8> {
        let mut p = question_header(0x10, qid, 0xFFFF, 0);
        p.extend_from_slice(&target_form.to_le_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn package(ifr: &[u8]) -> Vec<u8> {
        let total = 4 + ifr.len();
        let mut b = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            PACKAGE_FORMS,
        ];
        b.extend_from_slice(ifr);
        b
    }

    fn tree_pkg() -> Vec<u8> {
        package(
            &[
                form_set(),
                form(10001),
                ref_op(0x30, 10019),
                ref_op(0x31, 10019),
                end(),
                form(10019),
                ref_op(0x32, 10030),
                end(),
                end(),
                end(),
            ]
            .concat(),
        )
    }

    fn mk_node(
        guid: Option<Guid>,
        node_type: FfsType,
        subtype: u8,
        body: Vec<u8>,
        children: Vec<FfsNode>,
    ) -> FfsNode {
        FfsNode {
            guid,
            node_type,
            subtype,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn mk_image(form_body: Vec<u8>) -> Image {
        let form_sec = mk_node(None, FfsType::Section, 0x19, form_body, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![form_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        }
    }

    #[test]
    fn package_edges_dedup_and_order() {
        let pkg = tree_pkg();
        assert_eq!(
            package_edges(&pkg),
            vec![(10001, 10019), (10019, 10030)],
            "дубль (10001,10019) схлопнулся, порядок первого появления"
        );
    }

    #[test]
    fn package_edges_skips_short_and_orphan_refs() {
        let short = opcode(
            IFR_REF_OP,
            false,
            &question_header(0x10, 0x30, 0xFFFF, 0)[..10],
        );
        let outside = ref_op(0x33, 10040);
        let pkg = package(
            &[
                form_set(),
                outside,
                short,
                form(10001),
                ref_op(0x30, 10019),
                end(),
                end(),
                end(),
            ]
            .concat(),
        );
        assert_eq!(
            package_edges(&pkg),
            vec![(10001, 10019)],
            "REF вне формы и REF с len<15 не дают рёбер"
        );
    }

    #[test]
    fn collect_edges_walks_raw_section_and_keeps_dangling() {
        let edges = collect_edges(&mk_image(tree_pkg()));
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0], (FORMSET_GUID.to_string(), 10001, 10019));
        assert_eq!(edges[1], (FORMSET_GUID.to_string(), 10019, 10030));
    }

    #[test]
    fn collect_edges_dangling_target_not_dropped() {
        let pkg = package(
            &[
                form_set(),
                form(10001),
                ref_op(0x30, 65535),
                end(),
                end(),
                end(),
            ]
            .concat(),
        );
        let edges = collect_edges(&mk_image(pkg));
        assert_eq!(edges, vec![(FORMSET_GUID.to_string(), 10001, 65535)]);
    }

    #[test]
    fn collect_edges_empty_image() {
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![]);
        let img = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        assert!(collect_edges(&img).is_empty());
    }
}
