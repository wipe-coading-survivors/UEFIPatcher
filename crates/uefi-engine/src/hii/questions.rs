use std::collections::HashMap;

use r_efi::hii::{
    IFR_CHECKBOX_OP, IFR_NUMERIC_OP, IFR_NUMERIC_SIZE, IFR_ONE_OF_OP, PACKAGE_STRINGS,
};

use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::hii::package_list::parse_package_list;
use crate::hii::pe_resource::hii_resource_blobs;
use crate::hii::strings::{declared_len_sane, parse_string_package};
use crate::hii::values::{
    QuestionKind, is_question_op, one_of_width, scan_options, walk_statements,
};
use crate::types::{FfsNode, FfsType};

pub struct RawQuestion {
    pub question_id: u16,
    pub prompt_sid: u16,
    pub kind: QuestionKind,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub width: u8,
}

/// Вопросы формы `form_id` одного form-пакета. Строится на выверенном
/// `walk_statements` (форм-скоуп, quirk-маски, bounds); НЕ парсит
/// options/defaults — только summary-поля. Спека tui-forms-view §3.4.
pub fn questions(pkg: &[u8], form_id: u16) -> Vec<RawQuestion> {
    let mut out = Vec::new();
    walk_statements(pkg, |op, off, len, current| {
        if current != Some(form_id) || !is_question_op(op) || len < 13 {
            return;
        }
        let kind = match op {
            IFR_ONE_OF_OP => QuestionKind::OneOf,
            IFR_CHECKBOX_OP => QuestionKind::CheckBox,
            IFR_NUMERIC_OP => QuestionKind::Numeric,
            _ => QuestionKind::Other,
        };
        let width = match kind {
            QuestionKind::CheckBox => 1,
            QuestionKind::Numeric if len >= 14 => 1u8 << (pkg[off + 13] & IFR_NUMERIC_SIZE),
            QuestionKind::OneOf => {
                let mut options = Vec::new();
                let mut defaults = Vec::new();
                scan_options(pkg, off + len, &mut options, &mut defaults);
                one_of_width(&options, &defaults)
            }
            QuestionKind::Numeric | QuestionKind::Other => 0,
        };
        out.push(RawQuestion {
            prompt_sid: u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]),
            question_id: u16::from_le_bytes([pkg[off + 6], pkg[off + 7]]),
            kind,
            var_store_id: u16::from_le_bytes([pkg[off + 8], pkg[off + 9]]),
            var_offset: u16::from_le_bytes([pkg[off + 10], pkg[off + 11]]),
            width,
        });
    });
    out
}

/// string_id → text для файла formset'а: string-пакеты обоих каналов
/// (RAW-секция + PE-resource), рекурсивно через compression/GUIDED.
/// Та же схема, что у титулов форм (forms.rs). Спека tui-forms-view §3.4.
pub fn prompt_texts(file: &FfsNode) -> HashMap<u16, String> {
    let mut titles = HashMap::new();
    collect_string_sections(file, &mut titles);
    titles
}

fn collect_string_sections(node: &FfsNode, titles: &mut HashMap<u16, String>) {
    for child in &node.children {
        if child.node_type != FfsType::Section {
            continue;
        }
        match child.subtype {
            EFI_SECTION_RAW => {
                if declared_len_sane(&child.body)
                    && let Some(pkg) = parse_string_package(&child.body)
                {
                    for (sid, text) in pkg.strings {
                        titles.entry(sid).or_insert(text);
                    }
                }
            }
            EFI_SECTION_PE32 => {
                for blob in hii_resource_blobs(&child.body) {
                    let Some(list) = parse_package_list(blob) else {
                        continue;
                    };
                    for pkg in &list.packages {
                        if pkg.kind == PACKAGE_STRINGS
                            && let Some(sp) = parse_string_package(pkg.bytes)
                        {
                            for (sid, text) in sp.strings {
                                titles.entry(sid).or_insert(text);
                            }
                        }
                    }
                }
            }
            EFI_SECTION_COMPRESSION | EFI_SECTION_GUID_DEFINED => {
                collect_string_sections(child, titles);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use r_efi::hii::{
        IFR_CHECKBOX_OP, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP,
        IFR_ONE_OF_OPTION_OP, PACKAGE_FORMS,
    };

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
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

    fn form_set() -> Vec<u8> {
        let mut p = vec![0u8; 23];
        p[0] = 1;
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16) -> Vec<u8> {
        opcode(
            IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), 0u16.to_le_bytes()].concat(),
        )
    }

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

    fn one_of(prompt: u16, qid: u16) -> Vec<u8> {
        let mut p = question_header(prompt, qid, 1, 0x003A);
        p.push(0x10);
        p.extend_from_slice(&[0x00, 0x01, 0x00]);
        opcode(IFR_ONE_OF_OP, true, &p)
    }

    fn option(string_id: u16, value: u8) -> Vec<u8> {
        opcode(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[string_id.to_le_bytes().as_slice(), &[0x30, 0x00, value]].concat(),
        )
    }

    fn numeric(prompt: u16, qid: u16) -> Vec<u8> {
        let mut p = question_header(prompt, qid, 2, 0x0010);
        p.push(0x00);
        p.extend_from_slice(&[0, 255, 1]);
        opcode(IFR_NUMERIC_OP, true, &p)
    }

    fn checkbox(prompt: u16, qid: u16) -> Vec<u8> {
        opcode(
            IFR_CHECKBOX_OP,
            true,
            &question_header(prompt, qid, 1, 0x0020),
        )
    }

    fn two_forms_pkg() -> Vec<u8> {
        package(
            &[
                form_set(),
                form(10029),
                one_of(0x01A3, 0x003B),
                option(4, 0),
                option(3, 1),
                end(),
                end(),
                form(10030),
                numeric(0x0201, 0x0055),
                end(),
                end(),
                end(),
            ]
            .concat(),
        )
    }

    #[test]
    fn questions_of_form_10029() {
        let pkg = two_forms_pkg();
        let qs = questions(&pkg, 10029);
        assert_eq!(qs.len(), 1);
        let q = &qs[0];
        assert_eq!(q.question_id, 0x003B);
        assert_eq!(q.prompt_sid, 0x01A3);
        assert_eq!(q.kind, QuestionKind::OneOf);
        assert_eq!(q.var_store_id, 1);
        assert_eq!(q.var_offset, 0x003A);
        assert_eq!(q.width, 1);
    }

    #[test]
    fn questions_of_form_10030() {
        let pkg = two_forms_pkg();
        let qs = questions(&pkg, 10030);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].question_id, 0x0055);
        assert_eq!(qs[0].kind, QuestionKind::Numeric);
        assert_eq!(qs[0].width, 1);
    }

    #[test]
    fn questions_unknown_form_empty_and_checkbox_width() {
        let pkg = package(
            &[
                form_set(),
                form(10031),
                checkbox(0x0300, 0x0060),
                end(),
                end(),
                end(),
            ]
            .concat(),
        );
        assert!(questions(&pkg, 65535).is_empty());
        let qs = questions(&pkg, 10031);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].kind, QuestionKind::CheckBox);
        assert_eq!(qs[0].width, 1);
        assert_eq!(qs[0].prompt_sid, 0x0300);
    }
}
