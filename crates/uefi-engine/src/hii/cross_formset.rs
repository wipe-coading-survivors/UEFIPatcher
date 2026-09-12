use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::hii::form_package_ranges;
use crate::hii::gates::{self, GateTarget};
use crate::types::{FfsNode, FfsType, Guid, Image, guid_to_upper_string};

pub(crate) struct CrossGateSite {
    pub path: Vec<usize>,
    pub source_ffs: String,
    pub pkg_start: usize,
    pub pkg_len: usize,
    pub gates: Vec<gates::Gate>,
}

/// Кросс-формсетные гейты по всему образу (спека formset-unlock §3 U2a):
/// form-пакеты всех секций, кроме собственной (skip_path). Фильтр цели —
/// в GateTarget.formset_guid. Порядок — порядок обхода дерева. Только
/// поиск, без мутации.
pub fn find_cross_gates(image: &Image, skip_path: &[usize], gt: &GateTarget) -> Vec<CrossGateSite> {
    let mut out = Vec::new();
    let mut path: Vec<usize> = Vec::new();
    walk_files(&image.root, &mut path, skip_path, gt, &mut out);
    out
}

fn walk_files(
    node: &FfsNode,
    path: &mut Vec<usize>,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if child.node_type == FfsType::File
            && let Some(file_guid) = child.guid.as_ref()
        {
            walk_sections(child, path, file_guid, skip_path, gt, out);
        }
        walk_files(child, path, skip_path, gt, out);
        path.pop();
    }
}

fn walk_sections(
    file: &FfsNode,
    path: &mut Vec<usize>,
    file_guid: &Guid,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    for (i, sec) in file.children.iter().enumerate() {
        if sec.node_type != FfsType::Section {
            continue;
        }
        path.push(i);
        match sec.subtype {
            EFI_SECTION_RAW | EFI_SECTION_PE32 => {
                scan_section(sec, path, file_guid, skip_path, gt, out);
            }
            EFI_SECTION_COMPRESSION | EFI_SECTION_GUID_DEFINED => {
                walk_sections(sec, path, file_guid, skip_path, gt, out);
            }
            _ => {}
        }
        path.pop();
    }
}

fn scan_section(
    sec: &FfsNode,
    path: &[usize],
    file_guid: &Guid,
    skip_path: &[usize],
    gt: &GateTarget,
    out: &mut Vec<CrossGateSite>,
) {
    if path == skip_path {
        return;
    }
    for (start, len) in form_package_ranges(sec) {
        let pkg = &sec.body[start..start + len];
        let found = gates::find_gates(pkg, gt);
        if found.is_empty() {
            continue;
        }
        out.push(CrossGateSite {
            path: path.to_vec(),
            source_ffs: guid_to_upper_string(file_guid),
            pkg_start: start,
            pkg_len: len,
            gates: found,
        });
    }
}

#[cfg(test)]
pub(crate) mod cross_fixtures {
    use crate::types::{Action, FfsNode, FfsType, Guid, Image, ImageMode, ParsingData};
    use r_efi::hii::{
        IFR_END_OP, IFR_EQUAL_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, IFR_UINT64_OP,
        PACKAGE_FORMS,
    };
    use std::str::FromStr;

    pub(crate) const SETUP_FILE: &str = "899407d7-99fe-43d8-9a21-79ec328cac21";
    pub(crate) const TARGET_FILE: &str = "abbce13d-e25a-4d9f-a1f9-2f7710786892";
    pub(crate) const RC_SET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    pub(crate) fn opcode(op: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    pub(crate) fn package(ifr: &[u8]) -> Vec<u8> {
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

    pub(crate) fn uint64(v: u64) -> Vec<u8> {
        vec![IFR_UINT64_OP, 0x0A]
            .into_iter()
            .chain(v.to_le_bytes())
            .collect()
    }

    /// Пакет корневого Setup: форма 10001 c suppressed REF3 → RC_SET#1.
    pub(crate) fn donor_pkg() -> Vec<u8> {
        let g = Guid::from_str(RC_SET).unwrap();
        let ref3 = [
            vec![0u8; 11],
            1u16.to_le_bytes().to_vec(),
            0xFFFFu16.to_le_bytes().to_vec(),
            g.to_bytes().to_vec(),
        ]
        .concat();
        let mut ifr = opcode(IFR_FORM_SET_OP, true, &[0u8; 21]);
        ifr.extend(opcode(
            IFR_FORM_OP,
            true,
            &[10001u16.to_le_bytes(), 20u16.to_le_bytes()].concat(),
        ));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(vec![IFR_EQUAL_OP, 0x02]);
        ifr.extend(opcode(r_efi::hii::IFR_REF_OP, false, &ref3));
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        package(&ifr)
    }

    /// Пакет IntelRCSetup: formset GUID RC_SET, форма 1 без гейтов.
    pub(crate) fn target_pkg() -> Vec<u8> {
        let g = Guid::from_str(RC_SET).unwrap();
        let mut p = g.to_bytes().to_vec();
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = opcode(IFR_FORM_SET_OP, true, &p);
        ifr.extend(opcode(
            IFR_FORM_OP,
            true,
            &[1u16.to_le_bytes(), 21u16.to_le_bytes()].concat(),
        ));
        ifr.extend(vec![IFR_END_OP, 0x02]);
        ifr.extend(vec![IFR_END_OP, 0x02]);
        package(&ifr)
    }

    pub(crate) fn mk_node(
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

    pub(crate) fn two_file_image() -> Image {
        let donor_sec = mk_node(None, FfsType::Section, 0x19, donor_pkg(), vec![]);
        let donor_file = mk_node(
            Some(Guid::from_str(SETUP_FILE).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![donor_sec],
        );
        let target_sec = mk_node(None, FfsType::Section, 0x19, target_pkg(), vec![]);
        let target_file = mk_node(
            Some(Guid::from_str(TARGET_FILE).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![target_sec],
        );
        let volume = mk_node(
            None,
            FfsType::Volume,
            0,
            vec![],
            vec![donor_file, target_file],
        );
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    pub(crate) fn target_section_target() -> crate::types::Target {
        crate::types::Target::GuidSection {
            guid: Guid::from_str(TARGET_FILE).unwrap(),
            section_type: 0x19,
            section_index: Some(0),
        }
    }

    pub(crate) fn skip_path_of_target(image: &Image) -> Vec<usize> {
        crate::parser::target::find_item_path(&image.root, &target_section_target()).unwrap()
    }

    /// Тело донорской секции (volume0 → donor file → section0).
    pub(crate) fn donor_section_body(image: &Image) -> &[u8] {
        &image.root.children[0].children[0].children[0].body
    }
}

#[cfg(test)]
mod tests {
    use super::cross_fixtures::*;
    use super::*;
    use std::str::FromStr;

    fn node_by_path<'a>(root: &'a FfsNode, path: &[usize]) -> &'a FfsNode {
        let mut n = root;
        for &i in path {
            n = &n.children[i];
        }
        n
    }

    #[test]
    fn find_cross_gates_locates_donor_site_and_skips_own_section() {
        let image = two_file_image();
        let gt = GateTarget {
            form_id: 1,
            question_id: None,
            formset_guid: Some(Guid::from_str(RC_SET).unwrap()),
        };
        let sites = find_cross_gates(&image, &skip_path_of_target(&image), &gt);
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].source_ffs, "899407D7-99FE-43D8-9A21-79EC328CAC21");
        assert_eq!(sites[0].pkg_start, 0);
        assert_eq!(sites[0].gates.len(), 1);
        let node = node_by_path(&image.root, &sites[0].path);
        assert_eq!(node.subtype, 0x19);
        assert!(
            node.body
                .windows(2)
                .any(|w| w[0] == r_efi::hii::IFR_REF_OP && w[1] == 33)
        );
    }

    #[test]
    fn find_cross_gates_empty_when_no_donor() {
        let image = two_file_image();
        let gt = GateTarget {
            form_id: 42,
            question_id: None,
            formset_guid: Some(Guid::from_str(RC_SET).unwrap()),
        };
        assert!(find_cross_gates(&image, &skip_path_of_target(&image), &gt).is_empty());
    }
}
