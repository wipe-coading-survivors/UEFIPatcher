use r_efi::hii::{
    FormId, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_SUPPRESS_IF_OP, PACKAGE_FORMS, StringId,
};

use crate::types::Guid;

#[derive(Debug, Clone)]
pub struct SuppressScope {
    pub start: usize,
    pub end: usize,
}

pub fn find_suppress_if_scopes(body: &[u8]) -> Vec<SuppressScope> {
    let mut scopes = vec![];
    let mut i = 0;
    while i + 2 <= body.len() {
        if body[i] == 0x0A && body[i + 1] & 0x80 != 0 {
            let scope_start = i + 2;
            let mut depth = 1;
            let mut j = scope_start;
            while j + 2 <= body.len() {
                if body[j] == 0x0A && body[j + 1] & 0x80 != 0 {
                    depth += 1;
                } else if body[j] == 0x29 && body[j + 1] == 0x02 {
                    depth -= 1;
                    if depth == 0 {
                        scopes.push(SuppressScope {
                            start: scope_start,
                            end: j,
                        });
                        break;
                    }
                }
                let len = if body.len() > j + 1 {
                    body[j + 1] as usize & 0x7F
                } else {
                    2
                };
                j += len.max(2);
            }
            i = j + 2;
        } else {
            i += 1;
        }
    }
    scopes
}

pub fn unsuppress(ifr: &mut Vec<u8>, scope: &SuppressScope) {
    if scope.end + 2 <= ifr.len() && ifr[scope.end] == 0x29 && ifr[scope.end + 1] == 0x02 {
        ifr.drain(scope.end..scope.end + 2);
    }
    ifr.splice(scope.start..scope.start, [0x29, 0x02]);
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
    let plen = body[0] as usize | (body[1] as usize) << 8 | (body[2] as usize) << 16;
    let end = plen.min(body.len());
    let mut guid = None;
    let mut title: StringId = 0;
    let mut forms = Vec::new();
    let mut scope_stack: Vec<u8> = Vec::new();
    let mut i = 4;
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

#[cfg(test)]
mod tests {
    use super::*;
    use r_efi::hii::PACKAGE_STRINGS;
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
}
