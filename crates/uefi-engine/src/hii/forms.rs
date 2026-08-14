use std::collections::HashMap;

use uefi_proto::FormInfo;

use crate::hii::ifr::parse_form_package;
use crate::hii::strings::collect_strings;
use crate::types::{FfsNode, FfsType, Guid, Image, guid_to_upper_string};

pub fn collect_forms(image: &Image) -> Vec<FormInfo> {
    let mut titles: HashMap<u16, String> = HashMap::new();
    for s in collect_strings(image) {
        titles.entry(s.string_id as u16).or_insert(s.text);
    }
    let mut out = Vec::new();
    walk(&image.root, None, &titles, &mut out);
    out
}

fn walk(
    node: &FfsNode,
    file_guid: Option<Guid>,
    titles: &HashMap<u16, String>,
    out: &mut Vec<FormInfo>,
) {
    let file_guid = if node.node_type == FfsType::File {
        node.guid.or(file_guid)
    } else {
        file_guid
    };
    let mut counters: HashMap<u8, usize> = HashMap::new();
    for child in &node.children {
        let idx = *counters.entry(child.subtype).or_insert(0);
        if child.node_type == FfsType::Section
            && let Some(fs) = parse_form_package(&child.body)
            && let Some(fg) = file_guid
        {
            let target = format!("{}:{:#04x}:{}", fg, child.subtype, idx);
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
        counters.insert(child.subtype, idx + 1);
        walk(child, file_guid, titles, out);
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
    const LANGUAGE_OFFSET: usize = 12;
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
        let info_off = LANGUAGE_OFFSET + lang.len() + 1;
        let mut b = vec![0u8; 3];
        b.push(0x04);
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&(info_off as u32).to_le_bytes());
        while b.len() < LANGUAGE_OFFSET {
            b.push(0);
        }
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
}
