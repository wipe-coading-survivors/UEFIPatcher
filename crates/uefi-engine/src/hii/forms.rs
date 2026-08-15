use std::collections::HashMap;

use r_efi::hii::{PACKAGE_FORMS, PACKAGE_STRINGS};
use uefi_proto::FormInfo;

use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::hii::ifr::{FormSetInfo, parse_form_package};
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::{bare_form_packages, hii_resource_blobs, hii_resource_ranges};
use crate::hii::strings::{declared_len_sane, parse_string_package};
use crate::types::{FfsNode, FfsType, Guid, Image, guid_to_upper_string};

pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    let mut out = Vec::new();
    walk_files(&image.root, &mut out);
    out
}

fn walk_files(node: &FfsNode, out: &mut Vec<FormInfo>) {
    for child in &node.children {
        if child.node_type == FfsType::File {
            collect_file_forms(child, out);
        }
        walk_files(child, out);
    }
}

fn collect_file_forms(file: &FfsNode, out: &mut Vec<FormInfo>) {
    let Some(fg) = file.guid else {
        return;
    };
    let mut titles: HashMap<u16, String> = HashMap::new();
    let mut found: Vec<(String, FormSetInfo)> = Vec::new();
    let mut counters: HashMap<u8, usize> = HashMap::new();
    walk_sections(file, fg, &mut titles, &mut found, &mut counters);
    for (target, fs) in found {
        for raw in &fs.forms {
            out.push(FormInfo {
                form_id: target.clone(),
                formset_guid: guid_to_upper_string(&fs.guid),
                form_id_ifr: raw.form_id as u32,
                title: titles.get(&raw.title).cloned().unwrap_or_default(),
                visible: !raw.suppressed,
            });
        }
    }
}

fn walk_sections(
    node: &FfsNode,
    fg: Guid,
    titles: &mut HashMap<u16, String>,
    found: &mut Vec<(String, FormSetInfo)>,
    counters: &mut HashMap<u8, usize>,
) {
    for child in &node.children {
        if child.node_type != FfsType::Section {
            continue;
        }
        let idx = *counters.entry(child.subtype).or_insert(0);
        counters.insert(child.subtype, idx + 1);
        let target = format!("{}:{:#04x}:{}", fg, child.subtype, idx);
        if child.subtype == EFI_SECTION_RAW {
            if declared_len_sane(&child.body)
                && let Some(pkg) = parse_string_package(&child.body)
            {
                for (sid, text) in pkg.strings {
                    titles.entry(sid).or_insert(text);
                }
            } else if let Some(fs) = parse_form_package(&child.body) {
                found.push((target, fs));
            }
        } else if child.subtype == EFI_SECTION_PE32 {
            let ranges = hii_resource_ranges(&child.body);
            for blob in hii_resource_blobs(&child.body) {
                let Some(list) = parse_package_list(blob) else {
                    continue;
                };
                for pkg in &list.packages {
                    match pkg.kind {
                        PACKAGE_FORMS => {
                            if let Some(fs) = parse_form_package(pkg.bytes) {
                                found.push((target.clone(), fs));
                            }
                        }
                        PACKAGE_STRINGS => {
                            if let Some(sp) = parse_string_package(pkg.bytes) {
                                for (sid, text) in sp.strings {
                                    titles.entry(sid).or_insert(text);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            for pkg in bare_form_packages(&child.body, &ranges) {
                if let Some(fs) = parse_form_package(pkg) {
                    found.push((target.clone(), fs));
                }
            }
        }
        if child.subtype == EFI_SECTION_COMPRESSION || child.subtype == EFI_SECTION_GUID_DEFINED {
            walk_sections(child, fg, titles, found, counters);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsType, ImageMode, ParsingData, Target};
    use r_efi::hii::{IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS};
    use std::str::FromStr;

    const SIBT_STRING_SCSU: u8 = 0x10;
    const SIBT_END: u8 = 0x00;
    const FILE_GUID: &str = "899407d7-92a6-4174-968f-6f0b47f86a23";
    const FORMSET_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set(guid: &Guid, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&guid.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        opcode(IFR_FORM_OP, true, &p)
    }

    fn end() -> Vec<u8> {
        vec![IFR_END_OP, 0x02]
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

    fn string_pkg() -> Vec<u8> {
        let lang = "eng";
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 44 {
            b.push(0);
        }
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        b.extend_from_slice(&[
            SIBT_STRING_SCSU,
            b'M',
            b'a',
            b'i',
            b'n',
            0,
            SIBT_STRING_SCSU,
            b'H',
            b'i',
            b'd',
            b'd',
            b'e',
            b'n',
            0,
            SIBT_END,
        ]);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    fn form_pkg(form_title_id: u16) -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 1);
        ifr.extend(form(1, form_title_id));
        ifr.extend(end());
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(2, 2));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        package(&ifr)
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
        let str_sec = mk_node(None, FfsType::Section, 0x19, string_pkg(), vec![]);
        let form_sec = mk_node(None, FfsType::Section, 0x19, form_body, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![str_sec, form_sec],
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
    fn collect_forms_resolves_titles_and_visibility() {
        let image = mk_image(form_pkg(1));
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x19:1"));
        assert_eq!(forms[0].formset_guid, FORMSET_GUID);
        assert_eq!(forms[0].form_id_ifr, 1);
        assert_eq!(forms[0].title, "Main");
        assert!(forms[0].visible);
        assert_eq!(forms[1].form_id, format!("{FILE_GUID}:0x19:1"));
        assert_eq!(forms[1].form_id_ifr, 2);
        assert_eq!(forms[1].title, "Hidden");
        assert!(!forms[1].visible);
    }

    #[test]
    fn collect_forms_target_round_trips() {
        let image = mk_image(form_pkg(1));
        let forms = collect_forms(&image);
        let t = crate::parser::target::parse_target(&forms[0].form_id).unwrap();
        match t {
            Target::GuidSection {
                section_type,
                section_index,
                ..
            } => {
                assert_eq!(section_type, 0x19);
                assert_eq!(section_index, Some(1));
            }
            other => panic!("expected GuidSection, got {other:?}"),
        }
    }

    #[test]
    fn collect_forms_title_fallback_empty() {
        let image = mk_image(form_pkg(99));
        let forms = collect_forms(&image);
        assert_eq!(forms[0].title, "");
    }

    #[test]
    fn collect_forms_empty_image() {
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        assert!(collect_forms(&image).is_empty());
    }

    fn hii_list(form: &[u8], string: &[u8]) -> Vec<u8> {
        let g = Guid::from_str(FILE_GUID).unwrap();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + form.len() + string.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(form);
        b.extend_from_slice(string);
        b.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        b
    }

    fn string_pkg_with(text: &[u8]) -> Vec<u8> {
        let lang = "eng";
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 44 {
            b.push(0);
        }
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        b.push(SIBT_STRING_SCSU);
        b.extend_from_slice(text);
        b.push(0);
        b.push(SIBT_END);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    #[test]
    fn collect_forms_titles_are_scoped_to_file() {
        let mk_file = |guid: &str, text: &[u8]| {
            let str_sec = mk_node(None, FfsType::Section, 0x19, string_pkg_with(text), vec![]);
            let form_sec = mk_node(None, FfsType::Section, 0x19, form_pkg(1), vec![]);
            mk_node(
                Some(Guid::from_str(guid).unwrap()),
                FfsType::File,
                0x07,
                vec![],
                vec![str_sec, form_sec],
            )
        };
        let f1 = mk_file(FILE_GUID, b"MainA");
        let f2 = mk_file("899407d7-92a6-4174-968f-6f0b47f86a99", b"MainB");
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![f1, f2]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 4);
        let main_a = forms.iter().find(|f| f.title == "MainA").unwrap();
        let main_b = forms.iter().find(|f| f.title == "MainB").unwrap();
        assert!(main_a.form_id.starts_with(FILE_GUID));
        assert!(
            main_b
                .form_id
                .starts_with("899407d7-92a6-4174-968f-6f0b47f86a99")
        );
    }

    #[test]
    fn collect_forms_sees_forms_inside_pe_resources() {
        let list = hii_list(&form_pkg(1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let pe_sec = mk_node(None, FfsType::Section, 0x10, pe, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let forms = collect_forms(&image);
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
        assert_eq!(forms[0].title, "Main");
        assert_eq!(forms[0].formset_guid, FORMSET_GUID);
    }

    #[test]
    fn collect_forms_sees_bare_form_package_in_pe_body() {
        let mut body = vec![0x44u8; 16];
        body.extend_from_slice(include_bytes!(
            "../../tests/fixtures/hii_rk3588_bare_form.bin"
        ));
        let pe_sec = mk_node(None, FfsType::Section, 0x10, body, vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![pe_sec],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let forms = collect_forms(&image);
        assert!(!forms.is_empty());
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
        assert_eq!(
            forms[0].formset_guid,
            "642237C7-35D4-472D-8365-12E0CCF27A22"
        );
    }

    #[test]
    fn collect_forms_numbers_sections_through_wrappers() {
        let list = hii_list(&form_pkg(1), &string_pkg());
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let inner = mk_node(None, FfsType::Section, 0x10, pe, vec![]);
        let wrapper = mk_node(None, FfsType::Section, 0x02, vec![], vec![inner]);
        let direct = mk_node(None, FfsType::Section, 0x10, vec![0x55; 8], vec![]);
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            vec![wrapper, direct],
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let forms = collect_forms(&image);
        assert!(!forms.is_empty());
        assert_eq!(forms[0].form_id, format!("{FILE_GUID}:0x10:0"));
    }
}
