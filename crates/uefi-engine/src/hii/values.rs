use super::ifr::is_form_package;
use crate::types::Guid;
use r_efi::hii::{
    IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DATE_OP, IFR_DEFAULT_OP, IFR_END_OP, IFR_FORM_OP,
    IFR_GRAY_OUT_IF_OP, IFR_NUMERIC_OP, IFR_NUMERIC_SIZE, IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP,
    IFR_ORDERED_LIST_OP, IFR_PASSWORD_OP, IFR_REF_OP, IFR_STRING_OP, IFR_SUBTITLE_OP,
    IFR_SUPPRESS_IF_OP, IFR_TEXT_OP, IFR_TIME_OP, IFR_VARSTORE_EFI_OP, IFR_VARSTORE_OP,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarStoreMap {
    pub id: u16,
    pub guid: Option<Guid>,
    pub size: u16,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionKind {
    OneOf,
    CheckBox,
    Numeric,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionEntry {
    pub string_id: u16,
    pub flags: u8,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultEntry {
    pub default_id: u16,
    pub type_: u8,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionMap {
    pub question_id: u16,
    pub kind: QuestionKind,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub width: u8,
    pub varstore: Option<VarStoreMap>,
    pub options: Vec<OptionEntry>,
    pub defaults: Vec<DefaultEntry>,
    pub min: u64,
    pub max: u64,
    pub step: u64,
}

fn package_bounds(body: &[u8]) -> (usize, usize) {
    if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    }
}

fn is_gate_op(op: u8) -> bool {
    op == IFR_SUPPRESS_IF_OP || op == IFR_GRAY_OUT_IF_OP
}

fn is_statement_op(op: u8) -> bool {
    matches!(
        op,
        IFR_FORM_OP
            | IFR_SUBTITLE_OP
            | IFR_TEXT_OP
            | IFR_REF_OP
            | IFR_ONE_OF_OP
            | IFR_CHECKBOX_OP
            | IFR_NUMERIC_OP
            | IFR_PASSWORD_OP
            | IFR_ORDERED_LIST_OP
            | IFR_STRING_OP
            | IFR_DATE_OP
            | IFR_TIME_OP
            | IFR_ACTION_OP
            | IFR_ONE_OF_OPTION_OP
            | IFR_DEFAULT_OP
            | IFR_VARSTORE_OP
            | IFR_VARSTORE_EFI_OP
    )
}

fn is_question_op(op: u8) -> bool {
    matches!(
        op,
        IFR_ONE_OF_OP
            | IFR_CHECKBOX_OP
            | IFR_NUMERIC_OP
            | IFR_PASSWORD_OP
            | IFR_ORDERED_LIST_OP
            | IFR_STRING_OP
            | IFR_DATE_OP
            | IFR_TIME_OP
            | IFR_ACTION_OP
    )
}

struct Frame {
    op: u8,
    expr_end: Option<usize>,
    form_id: Option<u16>,
}

fn walk_statements(body: &[u8], mut visit: impl FnMut(u8, usize, usize, Option<u16>)) {
    let (start, end) = package_bounds(body);
    let mut stack: Vec<Frame> = Vec::new();
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            break;
        }
        if op == IFR_END_OP {
            stack.pop();
            i += length;
            continue;
        }
        let in_gate_expr =
            matches!(stack.last(), Some(f) if is_gate_op(f.op) && f.expr_end.is_none());
        if in_gate_expr && !is_statement_op(op) && !is_gate_op(op) && length_and_scope & 0x80 == 0 {
            i += length;
            continue;
        }
        let mut form_id = None;
        if is_statement_op(op) {
            for f in stack.iter_mut() {
                if f.expr_end.is_none() {
                    f.expr_end = Some(i);
                }
            }
            let current_form = stack.iter().rev().find_map(|f| f.form_id);
            visit(op, i, length, current_form);
            if op == IFR_FORM_OP && length >= 6 {
                form_id = Some(u16::from_le_bytes([body[i + 2], body[i + 3]]));
            }
        }
        if length_and_scope & 0x80 != 0 {
            stack.push(Frame {
                op,
                expr_end: None,
                form_id,
            });
        }
        i += length;
    }
}

fn read_le_u64(pkg: &[u8], off: usize, count: usize) -> u64 {
    let mut buf = [0u8; 8];
    let n = count.min(8).min(pkg.len().saturating_sub(off));
    buf[..n].copy_from_slice(&pkg[off..off + n]);
    u64::from_le_bytes(buf)
}

fn ascii_strz(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn ucs2_strz(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn varstore_map(pkg: &[u8]) -> Vec<VarStoreMap> {
    let mut out = Vec::new();
    walk_statements(pkg, |op, off, len, _| match op {
        IFR_VARSTORE_OP if len >= 22 => {
            let mut guid_bytes = [0u8; 16];
            guid_bytes.copy_from_slice(&pkg[off + 2..off + 18]);
            out.push(VarStoreMap {
                id: u16::from_le_bytes([pkg[off + 18], pkg[off + 19]]),
                guid: Some(Guid::from_bytes(guid_bytes)),
                size: u16::from_le_bytes([pkg[off + 20], pkg[off + 21]]),
                name: ascii_strz(&pkg[off + 22..off + len]),
            });
        }
        IFR_VARSTORE_EFI_OP if len >= 26 => {
            let mut guid_bytes = [0u8; 16];
            guid_bytes.copy_from_slice(&pkg[off + 4..off + 20]);
            out.push(VarStoreMap {
                id: u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]),
                guid: Some(Guid::from_bytes(guid_bytes)),
                size: u16::from_le_bytes([pkg[off + 24], pkg[off + 25]]),
                name: ucs2_strz(&pkg[off + 26..off + len]),
            });
        }
        _ => {}
    });
    out
}

fn scan_options(
    pkg: &[u8],
    mut i: usize,
    options: &mut Vec<OptionEntry>,
    defaults: &mut Vec<DefaultEntry>,
) {
    let (_, end) = package_bounds(pkg);
    let end = end.min(pkg.len());
    let mut depth = 0usize;
    while i + 2 <= end {
        let op = pkg[i];
        let length_and_scope = pkg[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            break;
        }
        if op == IFR_END_OP {
            if depth == 0 {
                break;
            }
            depth -= 1;
        } else {
            if depth == 0 {
                match op {
                    IFR_ONE_OF_OPTION_OP if length >= 7 => {
                        options.push(OptionEntry {
                            string_id: u16::from_le_bytes([pkg[i + 2], pkg[i + 3]]),
                            flags: pkg[i + 4],
                            value: read_le_u64(pkg, i + 6, length - 6),
                        });
                    }
                    IFR_DEFAULT_OP if length >= 6 => {
                        defaults.push(DefaultEntry {
                            default_id: u16::from_le_bytes([pkg[i + 2], pkg[i + 3]]),
                            type_: pkg[i + 4],
                            value: read_le_u64(pkg, i + 5, length - 5),
                        });
                    }
                    _ => {}
                }
            }
            if length_and_scope & 0x80 != 0 {
                depth += 1;
            }
        }
        i += length;
    }
}

fn value_bytes_needed(v: u64) -> u8 {
    if v == 0 {
        0
    } else {
        (64 - v.leading_zeros() as u8).div_ceil(8)
    }
}

fn one_of_width(options: &[OptionEntry], defaults: &[DefaultEntry]) -> u8 {
    let mut width = 1u8;
    for o in options {
        width = width.max(value_bytes_needed(o.value));
    }
    for d in defaults {
        if (1..=4).contains(&d.type_) {
            width = width.max(1u8 << (d.type_ - 1));
        }
    }
    width
}

pub fn find_question(pkg: &[u8], form_id: u16, question_id: u16) -> Option<QuestionMap> {
    let mut found: Option<(usize, usize)> = None;
    walk_statements(pkg, |op, off, len, current_form| {
        if found.is_none()
            && is_question_op(op)
            && len >= 13
            && u16::from_le_bytes([pkg[off + 6], pkg[off + 7]]) == question_id
            && current_form == Some(form_id)
        {
            found = Some((off, len));
        }
    });
    let (q_off, q_len) = found?;
    let op = pkg[q_off];
    let kind = match op {
        IFR_ONE_OF_OP => QuestionKind::OneOf,
        IFR_CHECKBOX_OP => QuestionKind::CheckBox,
        IFR_NUMERIC_OP => QuestionKind::Numeric,
        _ => QuestionKind::Other,
    };
    let var_store_id = u16::from_le_bytes([pkg[q_off + 8], pkg[q_off + 9]]);
    let var_offset = u16::from_le_bytes([pkg[q_off + 10], pkg[q_off + 11]]);
    let mut options = Vec::new();
    let mut defaults = Vec::new();
    let mut width = 0u8;
    let mut min = 0u64;
    let mut max = 0u64;
    let mut step = 0u64;
    match kind {
        QuestionKind::CheckBox => width = 1,
        QuestionKind::Numeric if q_len >= 14 => {
            width = 1u8 << (pkg[q_off + 13] & IFR_NUMERIC_SIZE);
            let w = width as usize;
            let read = |pos: usize| read_le_u64(pkg, q_off + 14 + pos * w, w);
            min = read(0);
            max = read(1);
            step = read(2);
        }
        QuestionKind::OneOf => {
            scan_options(pkg, q_off + q_len, &mut options, &mut defaults);
            width = one_of_width(&options, &defaults);
        }
        QuestionKind::Numeric | QuestionKind::Other => {}
    }
    let varstore = if var_store_id != 0 {
        varstore_map(pkg).into_iter().find(|v| v.id == var_store_id)
    } else {
        None
    };
    Some(QuestionMap {
        question_id,
        kind,
        var_store_id,
        var_offset,
        width,
        varstore,
        options,
        defaults,
        min,
        max,
        step,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use r_efi::hii::{IFR_FORM_SET_OP, PACKAGE_FORMS};
    use std::str::FromStr;

    const FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";
    const VARSTORE_GUID_STR: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn form_set(title: u16) -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        opcode(
            IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), title.to_le_bytes()].concat(),
        )
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

    fn varstore_buffer(id: u16, size: u16, name: &str) -> Vec<u8> {
        let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&size.to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        p.push(0);
        opcode(IFR_VARSTORE_OP, false, &p)
    }

    fn varstore_efi(id: u16, size: u16, name_ucs2: &str) -> Vec<u8> {
        let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(&size.to_le_bytes());
        for u in name_ucs2.encode_utf16().chain(std::iter::once(0)) {
            p.extend_from_slice(&u.to_le_bytes());
        }
        opcode(IFR_VARSTORE_EFI_OP, false, &p)
    }

    fn question_header(qid: u16, var_store_id: u16, var_offset: u16, qflags: u8) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x01A3u16.to_le_bytes());
        p.extend_from_slice(&0x01A4u16.to_le_bytes());
        p.extend_from_slice(&qid.to_le_bytes());
        p.extend_from_slice(&var_store_id.to_le_bytes());
        p.extend_from_slice(&var_offset.to_le_bytes());
        p.push(qflags);
        p
    }

    fn one_of_4g() -> Vec<u8> {
        let mut p = question_header(0x003B, 1, 0x003A, 0x10);
        p.push(0x10);
        p.extend_from_slice(&[0x00, 0x01, 0x00]);
        opcode(IFR_ONE_OF_OP, true, &p)
    }

    fn option(string_id: u16, flags: u8, value_bytes: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&string_id.to_le_bytes());
        p.push(flags);
        p.push(0x00);
        p.extend_from_slice(value_bytes);
        opcode(IFR_ONE_OF_OPTION_OP, false, &p)
    }

    fn default_op(default_id: u16, type_: u8, value_bytes: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&default_id.to_le_bytes());
        p.push(type_);
        p.extend_from_slice(value_bytes);
        opcode(IFR_DEFAULT_OP, false, &p)
    }

    fn numeric_op(qid: u16, size_flags: u8, min: u8, max: u8, step: u8) -> Vec<u8> {
        let mut p = question_header(qid, 1, 0x0010, 0x00);
        p.push(size_flags);
        p.extend_from_slice(&[min, max, step]);
        opcode(IFR_NUMERIC_OP, true, &p)
    }

    fn checkbox_op(qid: u16) -> Vec<u8> {
        opcode(
            IFR_CHECKBOX_OP,
            true,
            &question_header(qid, 1, 0x0020, 0x00),
        )
    }

    #[test]
    fn varstore_map_reads_buffer_and_efi_declarations() {
        let mut ifr = form_set(7);
        ifr.extend(varstore_buffer(1, 0x72, "Setup"));
        ifr.extend(varstore_efi(2, 4, "EfVar"));
        ifr.extend(end());
        let pkg = package(&ifr);
        let map = varstore_map(&pkg);
        assert_eq!(map.len(), 2);
        assert_eq!(map[0].id, 1);
        assert_eq!(map[0].size, 0x72);
        assert_eq!(map[0].name, "Setup");
        assert_eq!(
            map[0].guid,
            Some(Guid::from_str(VARSTORE_GUID_STR).unwrap())
        );
        assert_eq!(map[1].id, 2);
        assert_eq!(map[1].size, 4);
        assert_eq!(map[1].name, "EfVar");
        assert_eq!(
            map[1].guid,
            Some(Guid::from_str(VARSTORE_GUID_STR).unwrap())
        );
    }

    #[test]
    fn varstore_map_empty_package_is_empty() {
        let pkg = package(&[form_set(7), end()].concat());
        assert!(varstore_map(&pkg).is_empty());
    }

    fn four_g_ifr() -> Vec<u8> {
        [
            form_set(7),
            varstore_buffer(1, 0x72, "Setup"),
            form(10029, 21),
            one_of_4g(),
            option(4, 0x30, &[0]),
            option(3, 0x00, &[1]),
            end(),
            end(),
            end(),
        ]
        .concat()
    }

    #[test]
    fn find_question_4g_like_fixture() {
        let pkg = package(&four_g_ifr());
        let q = find_question(&pkg, 10029, 0x003B).expect("question found");
        assert_eq!(q.question_id, 0x003B);
        assert_eq!(q.kind, QuestionKind::OneOf);
        assert_eq!(q.var_store_id, 1);
        assert_eq!(q.var_offset, 0x003A);
        assert_eq!(q.width, 1);
        let vs = q.varstore.expect("varstore resolved");
        assert_eq!(vs.name, "Setup");
        assert_eq!(vs.size, 0x72);
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.options[0].string_id, 4);
        assert_eq!(q.options[0].flags, 0x30);
        assert_eq!(q.options[0].value, 0);
        assert_eq!(q.options[1].string_id, 3);
        assert_eq!(q.options[1].flags, 0x00);
        assert_eq!(q.options[1].value, 1);
        assert!(q.defaults.is_empty());
    }

    #[test]
    fn find_question_numeric_min_max_step() {
        let ifr = [
            form_set(7),
            varstore_buffer(1, 0x72, "Setup"),
            form(10029, 21),
            numeric_op(0x55, r_efi::hii::IFR_NUMERIC_SIZE_1, 5, 9, 2),
            end(),
            end(),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let q = find_question(&pkg, 10029, 0x55).expect("numeric found");
        assert_eq!(q.kind, QuestionKind::Numeric);
        assert_eq!(q.width, 1);
        assert_eq!(q.min, 5);
        assert_eq!(q.max, 9);
        assert_eq!(q.step, 2);
    }

    #[test]
    fn find_question_checkbox_width_one() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            checkbox_op(0x66),
            end(),
            end(),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let q = find_question(&pkg, 10029, 0x66).expect("checkbox found");
        assert_eq!(q.kind, QuestionKind::CheckBox);
        assert_eq!(q.width, 1);
    }

    #[test]
    fn find_question_ignores_other_form_and_unknown_qid() {
        let pkg = package(&four_g_ifr());
        assert!(find_question(&pkg, 10028, 0x003B).is_none());
        assert!(find_question(&pkg, 10029, 0x009A).is_none());
    }

    #[test]
    fn find_question_reads_default_opcodes_in_scope() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            one_of_4g(),
            default_op(0, 1, &[0]),
            option(4, 0x30, &[0]),
            option(3, 0x00, &[1]),
            end(),
            end(),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let q = find_question(&pkg, 10029, 0x003B).expect("question found");
        assert_eq!(q.defaults.len(), 1);
        assert_eq!(q.defaults[0].default_id, 0);
        assert_eq!(q.defaults[0].type_, 1);
        assert_eq!(q.defaults[0].value, 0);
    }

    #[test]
    fn find_question_two_byte_option_values() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            one_of_4g(),
            option(4, 0x00, &[0x02, 0x01]),
            option(3, 0x00, &[0x01, 0x00]),
            end(),
            end(),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let q = find_question(&pkg, 10029, 0x003B).expect("question found");
        assert_eq!(q.width, 2);
        assert_eq!(q.options[0].value, 0x0102);
    }

    #[test]
    fn find_question_varstore_unknown_is_none_field() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            one_of_4g(),
            option(4, 0x30, &[0]),
            end(),
            end(),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let q = find_question(&pkg, 10029, 0x003B).expect("question found");
        assert!(q.varstore.is_none());
    }

    #[test]
    fn varstore_map_stops_on_truncated_package() {
        let mut pkg = package(&four_g_ifr());
        pkg.truncate(pkg.len() - 5);
        let map = varstore_map(&pkg);
        assert!(map.len() <= 1);
    }
}
