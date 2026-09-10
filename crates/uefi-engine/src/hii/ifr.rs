use r_efi::hii::{
    FormId, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS, StringId,
};

use super::HiiError;
use crate::types::Guid;

#[derive(Debug, Clone)]
pub struct SuppressScope {
    pub start: usize,
    pub end: usize,
}

pub(crate) fn package_bounds(body: &[u8]) -> (usize, usize) {
    if is_form_package(body) {
        let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
        (4, plen.min(body.len()))
    } else {
        (0, body.len())
    }
}

/// Обход: opcode-aligned (`i += length`), границы — `package_bounds`
/// (u24-длина пакета); глубина +1 на любой scoped-оп (бит 0x80), −1 на END —
/// AMI-quirk операнды с END-терминаторами и вложенные FORM/GRAYOUT не ломают
/// баланс. Возвращает контент всех SUPPRESS_IF пакета: [start, end) — между
/// заголовком SUPPRESS_IF и его END. Спека hii-walker-consistency §3.1.
pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope> {
    let (start, end) = package_bounds(body);
    let mut scopes = vec![];
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length = (body[i + 1] & 0x7F) as usize;
        if length < 2 || i + length > end {
            break;
        }
        if op == IFR_SUPPRESS_IF_OP && body[i + 1] & 0x80 != 0 {
            let scope_start = i + 2;
            let mut depth = 1usize;
            let mut j = scope_start;
            while j + 2 <= end {
                let inner_len = (body[j + 1] & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return scopes;
                }
                if body[j] == IFR_END_OP {
                    depth -= 1;
                    if depth == 0 {
                        scopes.push(SuppressScope {
                            start: scope_start,
                            end: j,
                        });
                        break;
                    }
                } else if body[j + 1] & 0x80 != 0 {
                    depth += 1;
                }
                j += inner_len;
            }
            i = j + 2;
        } else {
            i += length;
        }
    }
    scopes
}

pub fn unsuppress(ifr: &mut [u8], scope: &SuppressScope) {
    if scope.end + 2 > ifr.len() || ifr[scope.end] != 0x29 || ifr[scope.end + 1] != 0x02 {
        return;
    }
    for i in (scope.start..scope.end).rev() {
        ifr[i + 2] = ifr[i];
    }
    ifr[scope.start] = 0x29;
    ifr[scope.start + 1] = 0x02;
}

pub fn find_form_suppress_scope(body: &[u8], form_id: u16) -> Option<SuppressScope> {
    let (start, end) = package_bounds(body);
    let mut open: Vec<(u8, usize)> = Vec::new();
    let mut i = start;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if op_code == IFR_FORM_OP
            && length >= 6
            && u16::from_le_bytes([body[i + 2], body[i + 3]]) == form_id
            && let Some(k) = open.iter().position(|(op, _)| *op == IFR_SUPPRESS_IF_OP)
        {
            let mut depth = open.len();
            let mut j = i + length;
            while j + 2 <= end {
                let inner_op = body[j];
                let inner_ls = body[j + 1];
                let inner_len = (inner_ls & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return None;
                }
                if inner_op == IFR_END_OP {
                    depth -= 1;
                    if depth == k {
                        return Some(SuppressScope {
                            start: open[k].1,
                            end: j,
                        });
                    }
                } else if inner_ls & 0x80 != 0 {
                    depth += 1;
                }
                j += inner_len;
            }
            return None;
        }
        if op_code == IFR_END_OP {
            open.pop();
        } else if length_and_scope & 0x80 != 0 {
            open.push((op_code, i + 2));
        }
        i += length;
    }
    None
}

pub fn collect_form_string_ids(body: &[u8], form_id: u16) -> Vec<u16> {
    use r_efi::hii::{
        IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DATE_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP,
        IFR_ONE_OF_OPTION_OP, IFR_ORDERED_LIST_OP, IFR_PASSWORD_OP, IFR_REF_OP, IFR_STRING_OP,
        IFR_SUBTITLE_OP, IFR_TEXT_OP, IFR_TIME_OP,
    };
    let (start, end) = package_bounds(body);
    let mut out: Vec<u16> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push = |out: &mut Vec<u16>, seen: &mut std::collections::HashSet<u16>, id: u16| {
        if id != 0 && seen.insert(id) {
            out.push(id);
        }
    };
    let mut i = start;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return out;
        }
        if op_code == IFR_FORM_OP
            && length >= 6
            && u16::from_le_bytes([body[i + 2], body[i + 3]]) == form_id
        {
            let read_id =
                |off: usize| -> u16 { u16::from_le_bytes([body[i + off], body[i + 1 + off]]) };
            push(&mut out, &mut seen, read_id(4));
            let mut depth = 0usize;
            let mut j = i + length;
            while j + 2 <= end {
                let inner_op = body[j];
                let inner_ls = body[j + 1];
                let inner_len = (inner_ls & 0x7F) as usize;
                if inner_len < 2 || j + inner_len > end {
                    return out;
                }
                if inner_op == IFR_END_OP {
                    if depth == 0 {
                        return out;
                    }
                    depth -= 1;
                } else {
                    match inner_op {
                        IFR_SUBTITLE_OP | IFR_ONE_OF_OP | IFR_CHECKBOX_OP | IFR_NUMERIC_OP
                        | IFR_PASSWORD_OP | IFR_ACTION_OP | IFR_REF_OP | IFR_DATE_OP
                        | IFR_TIME_OP | IFR_STRING_OP | IFR_ORDERED_LIST_OP => {
                            if inner_len >= 6 {
                                let p = u16::from_le_bytes([body[j + 2], body[j + 3]]);
                                let h = u16::from_le_bytes([body[j + 4], body[j + 5]]);
                                push(&mut out, &mut seen, p);
                                push(&mut out, &mut seen, h);
                            }
                        }
                        IFR_TEXT_OP => {
                            if inner_len >= 8 {
                                push(
                                    &mut out,
                                    &mut seen,
                                    u16::from_le_bytes([body[j + 2], body[j + 3]]),
                                );
                                push(
                                    &mut out,
                                    &mut seen,
                                    u16::from_le_bytes([body[j + 4], body[j + 5]]),
                                );
                                push(
                                    &mut out,
                                    &mut seen,
                                    u16::from_le_bytes([body[j + 6], body[j + 7]]),
                                );
                            }
                        }
                        IFR_ONE_OF_OPTION_OP if inner_len >= 4 => {
                            push(
                                &mut out,
                                &mut seen,
                                u16::from_le_bytes([body[j + 2], body[j + 3]]),
                            );
                        }
                        _ => {}
                    }
                    if inner_ls & 0x80 != 0 {
                        depth += 1;
                    }
                }
                j += inner_len;
            }
            return out;
        }
        i += length;
    }
    out
}

pub fn insert_form_into_package(
    pkg: &mut Vec<u8>,
    formset_idx: usize,
    form_ifr: &[u8],
    varstores: &[u8],
) -> bool {
    let Some((after_header, before_end)) = locate_formset_insert_points(pkg, formset_idx) else {
        return false;
    };
    pkg.splice(before_end..before_end, form_ifr.iter().copied());
    pkg.splice(after_header..after_header, varstores.iter().copied());
    let grow = varstores.len() + form_ifr.len();
    let plen = (pkg[0] as usize | (pkg[1] as usize) << 8 | (pkg[2] as usize) << 16) + grow;
    pkg[0] = (plen & 0xFF) as u8;
    pkg[1] = ((plen >> 8) & 0xFF) as u8;
    pkg[2] = ((plen >> 16) & 0xFF) as u8;
    true
}

pub(crate) fn formset_at(pkg: &[u8], formset_idx: usize) -> bool {
    locate_formset_insert_points(pkg, formset_idx).is_some()
}

pub(crate) fn formset_insert_points(pkg: &[u8], formset_idx: usize) -> Option<(usize, usize)> {
    locate_formset_insert_points(pkg, formset_idx)
}

fn locate_formset_insert_points(body: &[u8], formset_idx: usize) -> Option<(usize, usize)> {
    if !is_form_package(body) {
        return None;
    }
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    let end = plen.min(body.len());
    let mut seen = 0usize;
    let mut i = 4;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if op_code == IFR_FORM_SET_OP {
            if seen == formset_idx {
                let mut depth = 1usize;
                let mut j = i + length;
                while j + 2 <= end {
                    let inner_op = body[j];
                    let inner_ls = body[j + 1];
                    let inner_len = (inner_ls & 0x7F) as usize;
                    if inner_len < 2 || j + inner_len > end {
                        return None;
                    }
                    if inner_op == IFR_END_OP {
                        depth -= 1;
                        if depth == 0 {
                            return Some((i + length, j));
                        }
                    } else if inner_ls & 0x80 != 0 {
                        depth += 1;
                    }
                    j += inner_len;
                }
                return None;
            }
            seen += 1;
        }
        i += length;
    }
    None
}

pub fn splice_question_ops(
    package: &mut Vec<u8>,
    formset_idx: usize,
    form_id: u16,
    ops: &[u8],
) -> Result<(usize, usize), HiiError> {
    if ops.is_empty() {
        return Err(HiiError::InvalidSchema("empty ops".into()));
    }
    let Some(insert_at) = locate_form_end(package, formset_idx, form_id) else {
        return Err(HiiError::NotFound);
    };
    package.splice(insert_at..insert_at, ops.iter().copied());
    let plen = (package[0] as usize | (package[1] as usize) << 8 | (package[2] as usize) << 16)
        + ops.len();
    package[0] = (plen & 0xFF) as u8;
    package[1] = ((plen >> 8) & 0xFF) as u8;
    package[2] = ((plen >> 16) & 0xFF) as u8;
    Ok((insert_at, ops.len()))
}

pub(crate) fn locate_form_end(body: &[u8], formset_idx: usize, form_id: u16) -> Option<usize> {
    if !is_form_package(body) {
        return None;
    }
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    let end = plen.min(body.len());
    let mut seen = 0usize;
    let mut i = 4;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if op_code == IFR_FORM_SET_OP {
            if seen == formset_idx {
                let mut depth = 1usize;
                let mut j = i + length;
                while j + 2 <= end {
                    let inner_op = body[j];
                    let inner_ls = body[j + 1];
                    let inner_len = (inner_ls & 0x7F) as usize;
                    if inner_len < 2 || j + inner_len > end {
                        return None;
                    }
                    if inner_op == IFR_FORM_OP
                        && inner_len >= 6
                        && u16::from_le_bytes([body[j + 2], body[j + 3]]) == form_id
                    {
                        let mut fdepth = 1usize;
                        let mut k = j + inner_len;
                        while k + 2 <= end {
                            let f_op = body[k];
                            let f_ls = body[k + 1];
                            let f_len = (f_ls & 0x7F) as usize;
                            if f_len < 2 || k + f_len > end {
                                return None;
                            }
                            if f_op == IFR_END_OP {
                                fdepth -= 1;
                                if fdepth == 0 {
                                    return Some(k);
                                }
                            } else if f_ls & 0x80 != 0 {
                                fdepth += 1;
                            }
                            k += f_len;
                        }
                        return None;
                    }
                    if inner_op == IFR_END_OP {
                        depth -= 1;
                        if depth == 0 {
                            return None;
                        }
                    } else if inner_ls & 0x80 != 0 {
                        depth += 1;
                    }
                    j += inner_len;
                }
                return None;
            }
            seen += 1;
        }
        i += length;
    }
    None
}

#[derive(Debug, Clone)]
pub struct FormSetInfo {
    pub guid: Guid,
    pub title: StringId,
    pub forms: Vec<RawForm>,
}

#[derive(Debug, Clone)]
pub struct RawForm {
    pub form_id: FormId,
    pub title: StringId,
    pub suppressed: bool,
}

pub fn is_form_package(body: &[u8]) -> bool {
    body.len() >= 5 && body[3] == PACKAGE_FORMS && body[4] == IFR_FORM_SET_OP
}

pub fn parse_form_package(body: &[u8]) -> Option<FormSetInfo> {
    if !is_form_package(body) {
        return None;
    }
    let (start, end) = package_bounds(body);
    let mut guid = None;
    let mut title: StringId = 0;
    let mut forms = Vec::new();
    let mut scope_stack: Vec<u8> = Vec::new();
    let mut i = start;
    while i + 2 <= end {
        let op_code = body[i];
        let length_and_scope = body[i + 1];
        let length = (length_and_scope & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        match op_code {
            IFR_FORM_SET_OP => {
                if length < 23 {
                    return None;
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&body[i + 2..i + 18]);
                guid = Some(Guid::from_bytes(arr));
                title = u16::from_le_bytes([body[i + 18], body[i + 19]]);
            }
            IFR_FORM_OP => {
                if length < 6 {
                    return None;
                }
                forms.push(RawForm {
                    form_id: u16::from_le_bytes([body[i + 2], body[i + 3]]),
                    title: u16::from_le_bytes([body[i + 4], body[i + 5]]),
                    suppressed: scope_stack.contains(&IFR_SUPPRESS_IF_OP),
                });
            }
            IFR_END_OP => {
                scope_stack.pop();
            }
            _ => {
                tracing::trace!(op_code, offset = i, "unknown ifr opcode skipped");
            }
        }
        if length_and_scope & 0x80 != 0 {
            scope_stack.push(op_code);
        }
        i += length;
    }
    guid.map(|g| FormSetInfo {
        guid: g,
        title,
        forms,
    })
}

/// Баланс скоупов IFR form-пакета: +1 на scoped-опкод (бит 0x80 в байте
/// length/scope), −1 на END. 0 на корректном пакете. Инвариант
/// железо-доказан раундом 8 дуги setup-new-page; спека
/// hii-walker-consistency §3.5. `pkg` — form-пакет с 4-байтовым заголовком.
pub fn scope_balance(pkg: &[u8]) -> i32 {
    let mut bal = 0i32;
    let mut i = 4;
    while i + 2 <= pkg.len() {
        let len = (pkg[i + 1] & 0x7F) as usize;
        if len < 2 {
            break;
        }
        if pkg[i] == IFR_END_OP {
            bal -= 1;
        } else if pkg[i + 1] & 0x80 != 0 {
            bal += 1;
        }
        i += len;
    }
    bal
}

#[cfg(test)]
mod tests {
    use super::*;
    use r_efi::hii::{
        IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, IFR_SUBTITLE_OP, IFR_TEXT_OP, IFR_VARSTORE_EFI_OP,
        PACKAGE_STRINGS,
    };
    use std::str::FromStr;

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

    #[test]
    fn is_form_package_detects_form_packages() {
        let pkg = package(&form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 1));
        assert!(is_form_package(&pkg));
        assert!(!is_form_package(&[0x00, 0x00, 0x00, PACKAGE_FORMS]));
        assert!(!is_form_package(&[
            0x17,
            0x00,
            0x00,
            PACKAGE_STRINGS,
            IFR_FORM_SET_OP
        ]));
    }

    #[test]
    fn parse_extracts_formset_and_forms() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let fs = parse_form_package(&package(&ifr)).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(fs.title, 7);
        assert_eq!(fs.forms.len(), 1);
        assert_eq!(fs.forms[0].form_id, 1);
        assert_eq!(fs.forms[0].title, 10);
        assert!(!fs.forms[0].suppressed);
    }

    #[test]
    fn parse_marks_suppressed_form() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(2, 20));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let fs = parse_form_package(&package(&ifr)).unwrap();
        assert_eq!(fs.forms.len(), 2);
        assert!(!fs.forms[0].suppressed);
        assert!(fs.forms[1].suppressed);
    }

    #[test]
    fn parse_returns_none_on_truncation() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        pkg.truncate(pkg.len() - 7);
        assert!(parse_form_package(&pkg).is_none());
    }

    #[test]
    fn parse_returns_none_for_non_form_package() {
        assert!(
            parse_form_package(&[0x00, 0x00, 0x00, PACKAGE_STRINGS, IFR_FORM_SET_OP]).is_none()
        );
    }

    #[test]
    fn parse_ignores_tail_beyond_declared_length() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        let declared = pkg.len();
        pkg.extend(vec![0xAA; 16]);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(fs.forms.len(), 1);
        assert_eq!(declared, pkg.len() - 16);
    }

    #[test]
    fn parse_returns_none_when_declared_length_cuts_mid_opcode() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        let shrinked = (pkg.len() - 5) as u32;
        pkg[0] = (shrinked & 0xFF) as u8;
        pkg[1] = ((shrinked >> 8) & 0xFF) as u8;
        pkg[2] = ((shrinked >> 16) & 0xFF) as u8;
        assert!(parse_form_package(&pkg).is_none());
    }

    #[test]
    fn unsuppress_rewrites_slice_in_place_preserving_length() {
        let mut buf = vec![0xEE; 4];
        buf.extend_from_slice(&[0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02]);
        buf.extend_from_slice(&[0xDD; 4]);
        let scopes = find_suppress_if_scopes(&buf[4..11]);
        assert_eq!(scopes.len(), 1);
        unsuppress(&mut buf[4..11], &scopes[0]);
        assert_eq!(buf.len(), 15);
        assert_eq!(&buf[..4], &[0xEE; 4]);
        assert_eq!(&buf[4..8], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(&buf[8..11], &[0x12, 0x03, 0x40]);
        assert_eq!(&buf[11..], &[0xDD; 4]);
    }

    #[test]
    fn unsuppress_is_noop_when_scope_not_closed_by_end() {
        let mut buf = vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x11, 0x22];
        let scope = SuppressScope { start: 2, end: 5 };
        unsuppress(&mut buf, &scope);
        assert_eq!(buf, vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x11, 0x22]);
    }

    #[test]
    fn find_form_suppress_scope_returns_outermost_wrapper() {
        let mut ifr = Vec::new();
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(2, 20));
        ifr.extend(end());
        ifr.extend(end());
        let scope1 = find_form_suppress_scope(&ifr, 1).unwrap();
        let scope2 = find_form_suppress_scope(&ifr, 2).unwrap();
        assert!(scope2.start > scope1.end);
        assert!(find_form_suppress_scope(&ifr, 3).is_none());
        unsuppress(&mut ifr, &scope2);
        assert_eq!(&ifr[scope2.start..scope2.start + 2], &[0x29, 0x02]);
        let after1 = find_form_suppress_scope(&ifr, 1).unwrap();
        assert_eq!(after1.start, scope1.start);
        assert!(find_form_suppress_scope(&ifr, 2).is_none());
    }

    #[test]
    fn find_form_suppress_scope_walks_package_body() {
        let mut ifr = form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 7);
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(form(5, 50));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let scope = find_form_suppress_scope(&pkg, 5).unwrap();
        assert_eq!(scope.start, find_suppress_if_scopes(&pkg)[0].start);
        assert!(find_form_suppress_scope(&pkg, 6).is_none());
    }

    fn suppress_if() -> Vec<u8> {
        opcode(IFR_SUPPRESS_IF_OP, true, &[])
    }

    #[test]
    fn find_suppress_if_scopes_closes_on_own_end_with_nested_form() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(suppress_if());
        ifr.extend(form(9, 19));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let scopes = find_suppress_if_scopes(&pkg);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].start, 29);
        assert_eq!(scopes[0].end, 37);
    }

    #[test]
    fn find_suppress_if_scopes_survives_scoped_operand_end_terminators() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(suppress_if());
        let mut operand = vec![0x45u8, 0x8A];
        operand.extend_from_slice(&1u64.to_le_bytes());
        ifr.extend(operand);
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let scopes = find_suppress_if_scopes(&pkg);
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].start, 29);
        assert_eq!(scopes[0].end, 41);
    }

    #[test]
    fn find_suppress_if_scopes_ignores_tail_beyond_declared_length() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(suppress_if());
        ifr.extend(end());
        ifr.extend(end());
        let mut pkg = package(&ifr);
        pkg.extend(vec![0x0A, 0x82, 0x29, 0x02]);
        let scopes = find_suppress_if_scopes(&pkg);
        assert_eq!(scopes.len(), 1, "хвост за пределами plen не читается");
    }

    fn varstore_bytes() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&[0x11; 16]);
        p.extend_from_slice(&1u16.to_le_bytes());
        p.extend_from_slice(&256u16.to_le_bytes());
        p.extend_from_slice(&2u32.to_le_bytes());
        opcode(IFR_VARSTORE_EFI_OP, false, &p)
    }

    fn new_form(id: u16, title: u16) -> Vec<u8> {
        let mut f = form(id, title);
        f.extend(end());
        f
    }

    fn single_formset_package() -> (Guid, Vec<u8>) {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        (g, package(&ifr))
    }

    #[test]
    fn insert_form_into_package_splices_varstores_and_form() {
        let (g, mut pkg) = single_formset_package();
        let vs = varstore_bytes();
        let nf = new_form(99, 90);
        let orig_len = pkg.len();
        assert!(insert_form_into_package(&mut pkg, 0, &nf, &vs));
        assert_eq!(pkg.len(), orig_len + vs.len() + nf.len());
        assert_eq!(pkg[0], (pkg.len() & 0xFF) as u8);
        assert_eq!(pkg[1], ((pkg.len() >> 8) & 0xFF) as u8);
        assert_eq!(pkg[2], ((pkg.len() >> 16) & 0xFF) as u8);
        let mut expected = form_set(&g, 7);
        expected.extend(&vs);
        expected.extend(form(1, 10));
        expected.extend(end());
        expected.extend(&nf);
        expected.extend(end());
        assert_eq!(&pkg[4..], &expected[..]);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 99]
        );
        assert_eq!(fs.forms[1].title, 90);
        assert!(!fs.forms[1].suppressed);
    }

    #[test]
    fn insert_form_into_package_targets_selected_formset() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut fs1 = form_set(&g, 7);
        fs1.extend(form(1, 10));
        fs1.extend(end());
        fs1.extend(end());
        let mut fs2 = form_set(&g, 8);
        fs2.extend(form(2, 20));
        fs2.extend(end());
        fs2.extend(end());
        let mut ifr = fs1.clone();
        ifr.extend(&fs2);
        let mut pkg = package(&ifr);
        let vs = varstore_bytes();
        let nf = new_form(99, 90);
        let orig_len = pkg.len();
        assert!(insert_form_into_package(&mut pkg, 1, &nf, &vs));
        assert_eq!(pkg.len(), orig_len + vs.len() + nf.len());
        assert_eq!(&pkg[4..4 + fs1.len()], &fs1[..]);
        let mut expected2 = form_set(&g, 8);
        expected2.extend(&vs);
        expected2.extend(form(2, 20));
        expected2.extend(end());
        expected2.extend(&nf);
        expected2.extend(end());
        assert_eq!(&pkg[4 + fs1.len()..], &expected2[..]);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 2, 99]
        );
    }

    #[test]
    fn formset_at_reports_formset_presence() {
        let (_, single) = single_formset_package();
        assert!(formset_at(&single, 0));
        assert!(!formset_at(&single, 1));

        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut fs1 = form_set(&g, 7);
        fs1.extend(form(1, 10));
        fs1.extend(end());
        fs1.extend(end());
        let mut fs2 = form_set(&g, 8);
        fs2.extend(form(2, 20));
        fs2.extend(end());
        fs2.extend(end());
        let mut ifr = fs1.clone();
        ifr.extend(&fs2);
        let two = package(&ifr);
        assert!(formset_at(&two, 0));
        assert!(formset_at(&two, 1));
        assert!(!formset_at(&two, 2));

        let (_, mut truncated) = single_formset_package();
        truncated.truncate(truncated.len() - 7);
        assert!(!formset_at(&truncated, 0));
        assert!(!formset_at(&[0x00, 0x00, 0x00, PACKAGE_FORMS], 0));
    }

    #[test]
    fn insert_form_into_package_rejects_out_of_range_formset_idx() {
        let (_, mut pkg) = single_formset_package();
        let before = pkg.clone();
        let vs = varstore_bytes();
        let nf = new_form(99, 90);
        assert!(!insert_form_into_package(&mut pkg, 1, &nf, &vs));
        assert!(!insert_form_into_package(&mut pkg, 5, &[], &[]));
        assert_eq!(pkg, before);
    }

    #[test]
    fn insert_form_into_package_rejects_invalid_package_without_mutation() {
        let (_, mut truncated) = single_formset_package();
        truncated.truncate(truncated.len() - 7);
        let before = truncated.clone();
        let nf = new_form(99, 90);
        assert!(!insert_form_into_package(
            &mut truncated,
            0,
            &nf,
            &varstore_bytes()
        ));
        assert_eq!(truncated, before);

        let (_, mut wrong_kind) = single_formset_package();
        wrong_kind[3] = PACKAGE_STRINGS;
        let before_kind = wrong_kind.clone();
        assert!(!insert_form_into_package(
            &mut wrong_kind,
            0,
            &nf,
            &varstore_bytes()
        ));
        assert_eq!(wrong_kind, before_kind);
    }

    #[test]
    fn insert_form_into_package_allows_empty_varstores() {
        let (g, mut pkg) = single_formset_package();
        let nf = new_form(99, 90);
        let orig_len = pkg.len();
        assert!(insert_form_into_package(&mut pkg, 0, &nf, &[]));
        assert_eq!(pkg.len(), orig_len + nf.len());
        assert_eq!(pkg[0], (pkg.len() & 0xFF) as u8);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(fs.guid, g);
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 99]
        );
    }

    fn ifr_op(opc: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(2 + payload.len());
        v.push(opc);
        v.push((2 + payload.len()) as u8 | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn u16p(x: u16) -> Vec<u8> {
        x.to_le_bytes().to_vec()
    }

    fn two_form_raw_package() -> Vec<u8> {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(
            0x0E,
            true,
            &[u16p(0x0100).as_slice(), u16p(0x0101).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(0x0A, true, &[]));
        ifr.extend(ifr_op(0x12, false, &[0x40]));
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[u16p(901).as_slice(), u16p(0x1325).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_SUBTITLE_OP,
            false,
            &[u16p(4894).as_slice(), u16p(4895).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OP,
            true,
            &[
                u16p(4965).as_slice(),
                u16p(4966).as_slice(),
                u16p(0x0D5F).as_slice(),
                u16p(1).as_slice(),
            ]
            .concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[u16p(4969).as_slice(), &[0x00, 0x05], u16p(1).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[u16p(2638).as_slice(), &[0x00, 0x05], u16p(0).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[u16p(902).as_slice(), u16p(0x1400).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_TEXT_OP,
            false,
            &[
                u16p(0x1401).as_slice(),
                u16p(0x1402).as_slice(),
                u16p(0x1403).as_slice(),
            ]
            .concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        let mut pkg = vec![(4 + ifr.len()) as u8, 0, 0, r_efi::hii::PACKAGE_FORMS];
        let len = 4 + ifr.len();
        pkg[0] = (len & 0xFF) as u8;
        pkg[1] = ((len >> 8) & 0xFF) as u8;
        pkg[2] = ((len >> 16) & 0xFF) as u8;
        pkg.extend(ifr);
        pkg
    }

    #[test]
    fn collect_form_string_ids_walks_target_form_only() {
        let pkg = two_form_raw_package();
        assert_eq!(
            collect_form_string_ids(&pkg, 901),
            vec![0x1325, 4894, 4895, 4965, 4966, 4969, 2638]
        );
        assert_eq!(
            collect_form_string_ids(&pkg, 902),
            vec![0x1400, 0x1401, 0x1402, 0x1403]
        );
    }

    #[test]
    fn collect_form_string_ids_unknown_form_is_empty() {
        let pkg = two_form_raw_package();
        assert!(collect_form_string_ids(&pkg, 9999).is_empty());
    }

    #[test]
    fn collect_form_string_ids_filters_zero_and_dedups() {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(
            IFR_FORM_OP,
            true,
            &[u16p(7).as_slice(), u16p(0x22).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            IFR_SUBTITLE_OP,
            false,
            &[u16p(0).as_slice(), u16p(0x22).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        ifr.extend(ifr_op(IFR_END_OP, false, &[]));
        assert_eq!(collect_form_string_ids(&ifr, 7), vec![0x22]);
    }

    fn one_of_question() -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x21u16.to_le_bytes());
        p.extend_from_slice(&0x22u16.to_le_bytes());
        p.extend_from_slice(&0x31u16.to_le_bytes());
        p.extend_from_slice(&1u16.to_le_bytes());
        p.extend_from_slice(&0x40u16.to_le_bytes());
        p.push(0u8);
        let mut q = opcode(IFR_ONE_OF_OP, true, &p);
        q.extend(opcode(
            IFR_ONE_OF_OPTION_OP,
            false,
            &[0x23, 0x00, 0x00, 0x05, 0x01, 0x00],
        ));
        q.extend(end());
        q
    }

    fn two_form_package() -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(100, 10));
        ifr.extend(one_of_question());
        ifr.extend(end());
        ifr.extend(form(200, 20));
        ifr.extend(end());
        ifr.extend(end());
        package(&ifr)
    }

    #[test]
    fn splice_question_ops_inserts_before_end_form() {
        let mut pkg = two_form_package(); // формы 100 и 200, в 100 — один one_of
        let before_len = pkg.len();
        let ops = opcode(
            0x05,
            true,
            &[
                0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10,
            ],
        );
        let (at, delta) = splice_question_ops(&mut pkg, 0, 100, &ops).unwrap();
        assert_eq!(delta, ops.len());
        assert_eq!(pkg.len(), before_len + delta);
        // ops лежат после существующего вопроса формы 100 и перед её END (0x29)
        assert_eq!(&pkg[at..at + ops.len()], &ops[..]);
        let window = &pkg[at + ops.len()..at + ops.len() + 2];
        assert_eq!(window, &[0x29, 0x02]); // END_FORM сразу после вставки
    }

    #[test]
    fn splice_question_ops_targets_selected_formset() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut fs1 = form_set(&g, 7);
        fs1.extend(form(100, 10));
        fs1.extend(one_of_question());
        fs1.extend(end());
        fs1.extend(end());
        let mut fs2 = form_set(&g, 8);
        fs2.extend(form(200, 20));
        fs2.extend(end());
        fs2.extend(end());
        let mut ifr = fs1.clone();
        ifr.extend(&fs2);
        let mut pkg = package(&ifr);
        let orig_len = pkg.len();
        let ops = opcode(
            0x05,
            true,
            &[
                0x21, 0x00, 0x22, 0x00, 0xD2, 0x01, 0x01, 0x00, 0x5E, 0x00, 0x10,
            ],
        );
        assert!(matches!(
            splice_question_ops(&mut pkg, 0, 200, &ops),
            Err(HiiError::NotFound)
        ));
        let (at, delta) = splice_question_ops(&mut pkg, 1, 200, &ops).unwrap();
        assert_eq!(delta, ops.len());
        assert_eq!(pkg.len(), orig_len + delta);
        assert_eq!(
            at,
            4 + fs1.len() + form_set(&g, 8).len() + form(200, 20).len()
        );
        assert_eq!(&pkg[4..4 + fs1.len()], &fs1[..]);
        let mut expected = fs1.clone();
        let mut e2 = form_set(&g, 8);
        e2.extend(form(200, 20));
        e2.extend(&ops);
        e2.extend(end());
        e2.extend(end());
        expected.extend(&e2);
        assert_eq!(&pkg[4..], &expected[..]);
        assert_eq!(&pkg[at + ops.len()..at + ops.len() + 2], &[0x29, 0x02]);
        assert_eq!(pkg[0], (pkg.len() & 0xFF) as u8);
        assert_eq!(pkg[1], ((pkg.len() >> 8) & 0xFF) as u8);
        assert_eq!(pkg[2], ((pkg.len() >> 16) & 0xFF) as u8);
        let fs = parse_form_package(&pkg).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![100, 200]
        );
    }

    #[test]
    fn splice_question_ops_returns_not_found_for_unknown_form_or_formset() {
        let mut pkg = two_form_package();
        let before = pkg.clone();
        let ops = opcode(
            0x05,
            true,
            &[
                0x11, 0x00, 0x12, 0x00, 0xD1, 0x01, 0x01, 0x00, 0x5D, 0x00, 0x10,
            ],
        );
        assert!(matches!(
            splice_question_ops(&mut pkg, 0, 999, &ops),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            splice_question_ops(&mut pkg, 1, 100, &ops),
            Err(HiiError::NotFound)
        ));
        assert_eq!(pkg, before);
    }

    #[test]
    fn splice_question_ops_rejects_empty_ops() {
        let mut pkg = two_form_package();
        let before = pkg.clone();
        assert!(matches!(
            splice_question_ops(&mut pkg, 0, 100, &[]),
            Err(HiiError::InvalidSchema(_))
        ));
        assert_eq!(pkg, before);
    }

    #[test]
    fn scope_balance_is_zero_for_balanced_package() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        ifr.extend(end());
        assert_eq!(scope_balance(&package(&ifr)), 0);
    }

    #[test]
    fn scope_balance_counts_unclosed_scope() {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut ifr = form_set(&g, 7);
        ifr.extend(form(1, 10));
        ifr.extend(end());
        assert_eq!(scope_balance(&package(&ifr)), 1);
    }
}
