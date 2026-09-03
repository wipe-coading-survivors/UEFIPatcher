use r_efi::hii::IFR_FORM_OP;

use super::values;

pub struct HijackFormSpan {
    pub form_op: usize,
    pub next_form_op: usize,
}

pub fn locate_form(pkg: &[u8], form_id: u16) -> Option<HijackFormSpan> {
    let mut forms: Vec<(usize, u16)> = Vec::new();
    values::walk_statements(pkg, |op, off, len, _| {
        if op == IFR_FORM_OP && len >= 6 {
            forms.push((off, u16::from_le_bytes([pkg[off + 2], pkg[off + 3]])));
        }
    });
    let idx = forms.iter().position(|&(_, id)| id == form_id)?;
    let next_form_op = forms.get(idx + 1).map(|&(off, _)| off).unwrap_or(pkg.len());
    Some(HijackFormSpan {
        form_op: forms[idx].0,
        next_form_op,
    })
}

pub fn locate_questions(pkg: &[u8], form_id: u16) -> Vec<(usize, u16)> {
    let mut qs = Vec::new();
    values::walk_statements(pkg, |op, off, len, current_form| {
        if values::is_question_op(op) && len >= 13 && current_form == Some(form_id) {
            qs.push((off, u16::from_le_bytes([pkg[off + 6], pkg[off + 7]])));
        }
    });
    qs
}

pub fn rewrite_form_title(pkg: &mut [u8], form_op: usize, title_id: u16) {
    pkg[form_op + 4..form_op + 6].copy_from_slice(&title_id.to_le_bytes());
}

pub fn rewrite_question_strings(pkg: &mut [u8], q_off: usize, prompt_id: u16, help_id: u16) {
    pkg[q_off + 2..q_off + 4].copy_from_slice(&prompt_id.to_le_bytes());
    pkg[q_off + 4..q_off + 6].copy_from_slice(&help_id.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hii::ifr_builder::IfrBuilder;
    use crate::types::Guid;
    use r_efi::hii::IFR_ONE_OF_OP;
    use std::str::FromStr;

    const FORMSET_GUID: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";

    fn package_with_form() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        b.emit_form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 1, 1, &[]);
        b.emit_form(7, 1);
        b.emit_one_of(2, 3, 0x11, 1, 0, 0, 1);
        b.emit_end();
        b.emit_end();
        let ifr = b.build();
        let mut pkg = vec![0u8; 4];
        let len = 4 + ifr.len() as u32;
        pkg[0] = (len & 0xFF) as u8;
        pkg[1] = ((len >> 8) & 0xFF) as u8;
        pkg[2] = ((len >> 16) & 0xFF) as u8;
        pkg[3] = r_efi::hii::PACKAGE_FORMS;
        pkg.extend_from_slice(&ifr);
        pkg
    }

    #[test]
    fn locate_form_span_and_questions() {
        let pkg = package_with_form();
        let span = locate_form(&pkg, 7).expect("form 7");
        assert!(span.form_op >= 4 && span.form_op < pkg.len());
        assert_eq!(span.next_form_op, pkg.len());
        let qs = locate_questions(&pkg, 7);
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].1, 0x11);
        assert!(qs[0].0 > span.form_op);
        assert_eq!(pkg[qs[0].0], IFR_ONE_OF_OP);
        assert!(locate_form(&pkg, 8).is_none());
        assert!(locate_questions(&pkg, 8).is_empty());
    }

    #[test]
    fn rewrite_title_and_strings_same_length() {
        let mut pkg = package_with_form();
        let span = locate_form(&pkg, 7).unwrap();
        let q = locate_questions(&pkg, 7)[0].0;
        let before = pkg.clone();
        rewrite_form_title(&mut pkg, span.form_op, 0xBEEF);
        rewrite_question_strings(&mut pkg, q, 0x1111, 0x2222);
        assert_eq!(pkg.len(), before.len());
        assert_eq!(&pkg[span.form_op + 4..span.form_op + 6], &[0xEF, 0xBE]);
        assert_eq!(&pkg[q + 2..q + 4], &[0x11, 0x11]);
        assert_eq!(&pkg[q + 4..q + 6], &[0x22, 0x22]);
        for i in 0..pkg.len() {
            let touched =
                (i >= span.form_op + 4 && i < span.form_op + 6) || (i >= q + 2 && i < q + 6);
            if !touched {
                assert_eq!(pkg[i], before[i]);
            }
        }
    }
}
