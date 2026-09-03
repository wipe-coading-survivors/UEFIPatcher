use r_efi::hii::{IFR_EQ_ID_VAL_OP, IFR_EQUAL_OP, IFR_TRUE_OP, IFR_UINT64_OP};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    Suppress,
    Grayout,
}

impl GateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            GateKind::Suppress => "suppress",
            GateKind::Grayout => "grayout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wraps {
    Form { form_id: u16 },
    Ref { form_id: u16, host_form_id: u16 },
    Question { form_id: u16, question_id: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateExpr {
    EqConst { a: u64, b: u64 },
    EqIdVal { question_id: u16, value: u16 },
    True,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gate {
    pub kind: GateKind,
    pub wraps: Wraps,
    pub scope_offset: usize,
    pub expr_offset: usize,
    pub expr_end: usize,
    pub expr: GateExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateTarget {
    pub form_id: u16,
    pub question_id: Option<u16>,
}

pub fn decode_expr(region: &[u8]) -> GateExpr {
    let mut ops: Vec<(u8, &[u8])> = Vec::new();
    let mut i = 0usize;
    while i + 2 <= region.len() {
        let op = region[i];
        let len = (region[i + 1] & 0x7F) as usize;
        if len < 2 || i + len > region.len() {
            return GateExpr::Other;
        }
        ops.push((op, &region[i + 2..i + len]));
        i += len;
    }
    if i != region.len() {
        return GateExpr::Other;
    }
    match ops.as_slice() {
        [(IFR_UINT64_OP, a), (IFR_UINT64_OP, b), (IFR_EQUAL_OP, _)]
            if a.len() == 8 && b.len() == 8 =>
        {
            GateExpr::EqConst {
                a: u64::from_le_bytes((*a).try_into().unwrap()),
                b: u64::from_le_bytes((*b).try_into().unwrap()),
            }
        }
        [(IFR_EQ_ID_VAL_OP, p)] if p.len() == 4 => GateExpr::EqIdVal {
            question_id: u16::from_le_bytes([p[0], p[1]]),
            value: u16::from_le_bytes([p[2], p[3]]),
        },
        [(IFR_TRUE_OP, _)] => GateExpr::True,
        _ => GateExpr::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uint64(v: u64) -> Vec<u8> {
        let mut b = vec![IFR_UINT64_OP, 0x0A];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    fn uint64_quirk(v: u64) -> Vec<u8> {
        let mut b = uint64(v);
        b[1] = 0x8A;
        b
    }

    fn equal() -> Vec<u8> {
        vec![IFR_EQUAL_OP, 0x02]
    }

    fn eq_id_val(question_id: u16, value: u16) -> Vec<u8> {
        let mut b = vec![IFR_EQ_ID_VAL_OP, 0x06];
        b.extend_from_slice(&question_id.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        b
    }

    fn true_op() -> Vec<u8> {
        vec![IFR_TRUE_OP, 0x02]
    }

    fn concat(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn decode_eq_const_pair_with_equal() {
        let region = concat(&[uint64(1), uint64(1), equal()]);
        assert_eq!(decode_expr(&region), GateExpr::EqConst { a: 1, b: 1 });
    }

    #[test]
    fn decode_eq_const_ignores_scope_bit_quirk() {
        let region = concat(&[uint64_quirk(1), uint64(1), equal()]);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqConst { a: 1, b: 1 },
            "vendor firmware sets the scope bit on expression operands ([45 8a])"
        );
    }

    #[test]
    fn decode_eq_id_val() {
        let region = eq_id_val(0x009A, 1);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqIdVal {
                question_id: 0x009A,
                value: 1
            }
        );
    }

    #[test]
    fn decode_true() {
        assert_eq!(decode_expr(&true_op()), GateExpr::True);
    }

    #[test]
    fn decode_equal_consts_are_not_a_gate_class_when_distinct() {
        let region = concat(&[uint64(1), uint64(2), equal()]);
        assert_eq!(decode_expr(&region), GateExpr::EqConst { a: 1, b: 2 });
    }

    #[test]
    fn decode_unknown_opcode_is_other() {
        let region = concat(&[vec![0x42u8, 0x03, 0x07], equal()]);
        assert_eq!(decode_expr(&region), GateExpr::Other);
    }

    #[test]
    fn decode_wrong_shape_is_other() {
        assert_eq!(decode_expr(&uint64(1)), GateExpr::Other);
        assert_eq!(decode_expr(&concat(&[uint64(1), equal()])), GateExpr::Other);
        assert_eq!(
            decode_expr(&concat(&[eq_id_val(1, 1), true_op()])),
            GateExpr::Other
        );
        assert_eq!(decode_expr(&[]), GateExpr::Other);
    }

    #[test]
    fn decode_truncated_region_is_other() {
        let mut region = concat(&[uint64(1), uint64(1), equal()]);
        region.truncate(region.len() - 3);
        assert_eq!(decode_expr(&region), GateExpr::Other);
    }
}
