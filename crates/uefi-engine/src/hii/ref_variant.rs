use crate::types::Guid;

/// REF-цель по UEFI 2.10 §33.3.8.3.59: один опкод 0x0F, варианты
/// различаются length. Спека formset-unlock §2. REF5 (len 13) —
/// Dynamic: цель приходит из runtime-value вопроса, статически
/// неразрешима — гейты/рёбра по нему не строятся.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RefTarget {
    Form {
        form_id: u16,
    },
    FormQuestion {
        form_id: u16,
        question_id: u16,
    },
    Formset {
        formset_guid: Guid,
        form_id: u16,
        question_id: u16,
    },
    Dynamic,
}

/// Парсит REF-стейтмент (opcode + length + тело). Принимает только
/// канонические длины {13, 15, 17, 33, 35}; прочие — None.
#[allow(dead_code)]
pub(crate) fn parse_ref(op: u8, stmt: &[u8]) -> Option<RefTarget> {
    if op != r_efi::hii::IFR_REF_OP || stmt.len() < 2 {
        return None;
    }
    let len = (stmt[1] & 0x7F) as usize;
    if len > stmt.len() {
        return None;
    }
    let u16_at = |at: usize| -> Option<u16> {
        stmt.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    match len {
        13 => Some(RefTarget::Dynamic),
        15 => u16_at(13).map(|form_id| RefTarget::Form { form_id }),
        17 => {
            let form_id = u16_at(13)?;
            let question_id = u16_at(15)?;
            Some(RefTarget::FormQuestion {
                form_id,
                question_id,
            })
        }
        33 | 35 => {
            let form_id = u16_at(13)?;
            let question_id = u16_at(15)?;
            let guid: [u8; 16] = stmt.get(17..33)?.try_into().ok()?;
            Some(RefTarget::Formset {
                formset_guid: Guid::from_bytes(guid),
                form_id,
                question_id,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    const CROSS_FORMSET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    /// question-header 11 байт (prompt/help/qid/vsid/voff/flags — EDK2
    /// EFI_IFR_QUESTION_HEADER, спека §2) + хвост варианта.
    fn ref_stmt(total_len: usize, tail: &[u8]) -> Vec<u8> {
        assert_eq!(total_len, 2 + 11 + tail.len());
        let mut v = vec![r_efi::hii::IFR_REF_OP, total_len as u8];
        v.extend_from_slice(&[0u8; 11]);
        v.extend_from_slice(tail);
        v
    }

    #[test]
    fn parse_ref_plain_and_ref2() {
        let plain = ref_stmt(15, &1u16.to_le_bytes());
        assert_eq!(
            parse_ref(plain[0], &plain),
            Some(RefTarget::Form { form_id: 1 })
        );
        let ref2 = ref_stmt(17, &[1u16.to_le_bytes(), 0x24u16.to_le_bytes()].concat());
        assert_eq!(
            parse_ref(ref2[0], &ref2),
            Some(RefTarget::FormQuestion {
                form_id: 1,
                question_id: 0x24
            })
        );
    }

    #[test]
    fn parse_ref3_reads_formset_guid_at_17() {
        let g = Guid::from_str(CROSS_FORMSET).unwrap();
        let tail = [
            1u16.to_le_bytes().as_slice(),
            0xFFFFu16.to_le_bytes().as_slice(),
            g.to_bytes().as_slice(),
        ]
        .concat();
        let ref3 = ref_stmt(33, &tail);
        assert_eq!(
            parse_ref(ref3[0], &ref3),
            Some(RefTarget::Formset {
                formset_guid: g,
                form_id: 1,
                question_id: 0xFFFF,
            })
        );
    }

    #[test]
    fn parse_ref4_treated_as_formset_device_path_ignored() {
        let g = Guid::from_str(CROSS_FORMSET).unwrap();
        let tail = [
            2u16.to_le_bytes().as_slice(),
            0xFFFFu16.to_le_bytes().as_slice(),
            g.to_bytes().as_slice(),
            0x00AAu16.to_le_bytes().as_slice(),
        ]
        .concat();
        let ref4 = ref_stmt(35, &tail);
        assert_eq!(
            parse_ref(ref4[0], &ref4),
            Some(RefTarget::Formset {
                formset_guid: g,
                form_id: 2,
                question_id: 0xFFFF,
            })
        );
    }

    #[test]
    fn parse_ref5_is_dynamic() {
        let ref5 = ref_stmt(13, &[]);
        assert_eq!(parse_ref(ref5[0], &ref5), Some(RefTarget::Dynamic));
    }

    #[test]
    fn parse_ref_rejects_other_opcodes_and_lengths() {
        let mut weird = ref_stmt(15, &1u16.to_le_bytes());
        weird[0] = r_efi::hii::IFR_ONE_OF_OP;
        assert_eq!(parse_ref(weird[0], &weird), None);
        for len in [12u8, 14, 16, 18, 32, 34, 36] {
            let mut v = vec![r_efi::hii::IFR_REF_OP, len];
            v.resize(v.len() + len as usize, 0);
            assert_eq!(parse_ref(v[0], &v), None, "len={len}");
        }
    }
}
