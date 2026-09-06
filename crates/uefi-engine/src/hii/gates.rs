use super::ifr::is_form_package;
use r_efi::hii::{
    IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DATE_OP, IFR_DEFAULT_OP, IFR_END_OP, IFR_EQ_ID_VAL_OP,
    IFR_EQUAL_OP, IFR_FORM_OP, IFR_GRAY_OUT_IF_OP, IFR_NUMERIC_OP, IFR_NUMERIC_SIZE, IFR_ONE_OF_OP,
    IFR_ONE_OF_OPTION_OP, IFR_ORDERED_LIST_OP, IFR_PASSWORD_OP, IFR_REF_OP, IFR_STRING_OP,
    IFR_SUBTITLE_OP, IFR_SUPPRESS_IF_OP, IFR_TEXT_OP, IFR_TIME_OP, IFR_TRUE_OP, IFR_UINT64_OP,
};

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
        if op == IFR_END_OP {
            i += len;
            continue;
        }
        ops.push((op, &region[i + 2..i + len]));
        i += len;
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
    offset: usize,
    expr_end: Option<usize>,
    form_id: Option<u16>,
}

pub fn find_gates(body: &[u8], target: &GateTarget) -> Vec<Gate> {
    let (start, end) = package_bounds(body);
    let mut gates = Vec::new();
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
            emit_gates(body, &stack, i, op, length, target, &mut gates);
            if op == IFR_FORM_OP && length >= 6 {
                form_id = Some(u16::from_le_bytes([body[i + 2], body[i + 3]]));
            }
        }
        if length_and_scope & 0x80 != 0 {
            stack.push(Frame {
                op,
                offset: i,
                expr_end: None,
                form_id,
            });
        }
        i += length;
    }
    gates
}

fn emit_gates(
    body: &[u8],
    stack: &[Frame],
    stmt_offset: usize,
    op: u8,
    length: usize,
    target: &GateTarget,
    gates: &mut Vec<Gate>,
) {
    let current_form = stack.iter().rev().find_map(|f| f.form_id);
    let wraps = if op == IFR_FORM_OP && length >= 6 {
        let fid = u16::from_le_bytes([body[stmt_offset + 2], body[stmt_offset + 3]]);
        if fid == target.form_id && target.question_id.is_none() {
            Some(Wraps::Form { form_id: fid })
        } else {
            None
        }
    } else if op == IFR_REF_OP && length >= 15 && target.question_id.is_none() {
        let fid = u16::from_le_bytes([body[stmt_offset + 13], body[stmt_offset + 14]]);
        if fid == target.form_id {
            Some(Wraps::Ref {
                form_id: fid,
                host_form_id: current_form.unwrap_or(0),
            })
        } else {
            None
        }
    } else if is_question_op(op) && length >= 8 {
        let qid = u16::from_le_bytes([body[stmt_offset + 6], body[stmt_offset + 7]]);
        if target.question_id == Some(qid) && current_form == Some(target.form_id) {
            Some(Wraps::Question {
                form_id: target.form_id,
                question_id: qid,
            })
        } else {
            None
        }
    } else {
        None
    };
    let Some(wraps) = wraps else { return };
    for f in stack.iter().rev() {
        if !is_gate_op(f.op) {
            continue;
        }
        let expr_end = f.expr_end.unwrap_or(stmt_offset);
        gates.push(Gate {
            kind: if f.op == IFR_SUPPRESS_IF_OP {
                GateKind::Suppress
            } else {
                GateKind::Grayout
            },
            wraps,
            scope_offset: f.offset,
            expr_offset: f.offset + 2,
            expr_end,
            expr: decode_expr(&body[f.offset + 2..expr_end.min(body.len())]),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFlip {
    pub offset: usize,
    pub from: Vec<u8>,
    pub to: Vec<u8>,
}

pub(crate) fn question_storage_width(body: &[u8], question_id: u16) -> Option<u8> {
    let (start, end) = package_bounds(body);
    let mut i = start;
    while i + 2 <= end {
        let op = body[i];
        let length = (body[i + 1] & 0x7F) as usize;
        if length < 2 || i + length > end {
            return None;
        }
        if is_question_op(op)
            && length >= 8
            && u16::from_le_bytes([body[i + 6], body[i + 7]]) == question_id
        {
            return match op {
                IFR_CHECKBOX_OP => Some(1),
                IFR_NUMERIC_OP if length >= 13 => Some(1u8 << (body[i + 12] & IFR_NUMERIC_SIZE)),
                _ => None,
            };
        }
        i += length;
    }
    None
}

pub(crate) fn plan_flip(body: &[u8], gate: &Gate) -> Option<PlannedFlip> {
    match gate.expr {
        GateExpr::EqConst { a, b } if a == b => {
            let first_len = (body[gate.expr_offset + 1] & 0x7F) as usize;
            let offset = gate.expr_offset + first_len + 2;
            let from = body[offset];
            Some(PlannedFlip {
                offset,
                from: vec![from],
                to: vec![from.wrapping_add(1)],
            })
        }
        GateExpr::EqIdVal { question_id, value } if value != 0xFFFF => {
            if question_storage_width(body, question_id).is_some_and(|w| w > 1) {
                return None;
            }
            Some(PlannedFlip {
                offset: gate.expr_offset + 4,
                from: value.to_le_bytes().to_vec(),
                to: 0xFFFFu16.to_le_bytes().to_vec(),
            })
        }
        _ => None,
    }
}

pub fn plan_gates(body: &[u8], gates: &[Gate]) -> Result<Vec<PlannedFlip>, String> {
    let mut flips = Vec::new();
    for gate in gates {
        match plan_flip(body, gate) {
            Some(flip) => flips.push(flip),
            None => {
                let region = &body[gate.expr_offset..gate.expr_end.min(body.len())];
                return Err(format!(
                    "{} gate at pkg+{:#x} wrapping {:?}: expression [{}] is not a hardware-validated flip class",
                    gate.kind.as_str(),
                    gate.scope_offset,
                    gate.wraps,
                    region
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }
    Ok(flips)
}

fn is_unlocked_expr(expr: &GateExpr) -> bool {
    match expr {
        GateExpr::EqConst { a, b } => a != b,
        GateExpr::EqIdVal { value, .. } => *value == 0xFFFF,
        _ => false,
    }
}

pub fn plan_gates_skip_unlocked(body: &[u8], gates: &[Gate]) -> Result<Vec<PlannedFlip>, String> {
    let mut flips = Vec::new();
    for gate in gates {
        if is_unlocked_expr(&gate.expr) {
            continue;
        }
        match plan_flip(body, gate) {
            Some(flip) => flips.push(flip),
            None => {
                let region = &body[gate.expr_offset..gate.expr_end.min(body.len())];
                return Err(format!(
                    "{} gate at pkg+{:#x} wrapping {:?}: expression [{}] is not a hardware-validated flip class",
                    gate.kind.as_str(),
                    gate.scope_offset,
                    gate.wraps,
                    region
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
    }
    Ok(flips)
}

pub fn apply_flips(body: &mut [u8], flips: &[PlannedFlip]) -> Result<(), String> {
    for flip in flips {
        if flip.offset + flip.from.len() > body.len()
            || flip.offset + flip.to.len() > body.len()
            || body[flip.offset..flip.offset + flip.from.len()] != flip.from[..]
        {
            return Err(format!(
                "flip at pkg+{:#x} precondition failed",
                flip.offset
            ));
        }
    }
    for flip in flips {
        body[flip.offset..flip.offset + flip.to.len()].copy_from_slice(&flip.to);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Guid;
    use r_efi::hii::{
        IFR_FORM_OP, IFR_FORM_SET_OP, IFR_GRAY_OUT_IF_OP, IFR_ONE_OF_OP, IFR_SUPPRESS_IF_OP,
        PACKAGE_FORMS,
    };
    use std::str::FromStr;

    const FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";

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
        vec![r_efi::hii::IFR_END_OP, 0x02]
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

    fn ref_op(form_id: u16, question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 11];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        let mut v = vec![r_efi::hii::IFR_REF_OP, 0x0F];
        v.extend_from_slice(&p);
        v.extend_from_slice(&form_id.to_le_bytes());
        v
    }

    fn one_of_op(question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 10];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        opcode(IFR_ONE_OF_OP, true, &p)
    }

    fn vendor_ifr() -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10002, 20));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(ref_op(10029, 0x003A));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    fn numeric_op(question_id: u16, flags: u8) -> Vec<u8> {
        let mut p = vec![0u8; 11];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        p[10] = flags;
        opcode(IFR_NUMERIC_OP, true, &p)
    }

    fn master_switch_ifr(width_flags: u8) -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(numeric_op(0x009A, width_flags));
        ifr.extend(end());
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    const FORM_GATE_TARGET: GateTarget = GateTarget {
        form_id: 10029,
        question_id: None,
    };
    const QUESTION_GATE_TARGET: GateTarget = GateTarget {
        form_id: 10029,
        question_id: Some(0x003B),
    };

    #[test]
    fn find_gates_reports_ref_suppress_for_target_form() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].kind, GateKind::Suppress);
        assert_eq!(
            gates[0].wraps,
            Wraps::Ref {
                form_id: 10029,
                host_form_id: 10002
            }
        );
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
        assert_eq!(pkg[gates[0].scope_offset], IFR_SUPPRESS_IF_OP);
        assert_eq!(pkg[gates[0].expr_offset], IFR_UINT64_OP);
        assert!(gates[0].expr_offset < gates[0].expr_end);
        assert_eq!(
            &pkg[gates[0].expr_end..gates[0].expr_end + 2],
            &ref_op(10029, 0x003A)[..2]
        );
    }

    #[test]
    fn find_gates_reports_question_grayout() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].kind, GateKind::Grayout);
        assert_eq!(
            gates[0].wraps,
            Wraps::Question {
                form_id: 10029,
                question_id: 0x003B
            }
        );
        assert_eq!(
            gates[0].expr,
            GateExpr::EqIdVal {
                question_id: 0x009A,
                value: 1
            }
        );
        assert_eq!(pkg[gates[0].scope_offset], IFR_GRAY_OUT_IF_OP);
    }

    #[test]
    fn find_gates_walks_despite_scope_bit_quirk_in_expr() {
        let mut ifr = vendor_ifr();
        let quirk_pos = ifr
            .windows(2)
            .position(|w| w == [IFR_SUPPRESS_IF_OP, 0x82])
            .unwrap()
            + 2;
        assert_eq!(ifr[quirk_pos + 1], 0x0A);
        ifr[quirk_pos + 1] = 0x8A;
        let pkg = package(&ifr);
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(
            gates.len(),
            1,
            "scope bit on expression operand must not derail the walk"
        );
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
    }

    fn real_quirk_ifr() -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10002, 20));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64_quirk(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(end());
        ifr.extend(ref_op(10029, 0x003A));
        ifr.extend(end());
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    #[test]
    fn find_gates_scoped_expr_operands_with_end_terminators_keep_gate_frames() {
        let pkg = package(&real_quirk_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert_eq!(
            gates.len(),
            1,
            "END terminator of a scoped operand must pop the operand frame, not the gate frame"
        );
        assert_eq!(gates[0].kind, GateKind::Suppress);
        assert_eq!(
            gates[0].wraps,
            Wraps::Ref {
                form_id: 10029,
                host_form_id: 10002
            }
        );
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 1 });
        assert!(gates[0].expr_offset < gates[0].expr_end);

        let qgates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert_eq!(
            qgates.len(),
            1,
            "stack must stay balanced past the quirk for later forms"
        );
        assert_eq!(qgates[0].kind, GateKind::Grayout);
        assert_eq!(
            qgates[0].expr,
            GateExpr::EqIdVal {
                question_id: 0x009A,
                value: 1
            }
        );
    }

    #[test]
    fn find_gates_reports_direct_form_suppress() {
        let mut ifr = form_set(7);
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(form(901, 30));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let gates = find_gates(
            &package(&ifr),
            &GateTarget {
                form_id: 901,
                question_id: None,
            },
        );
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].wraps, Wraps::Form { form_id: 901 });
    }

    #[test]
    fn find_gates_reports_every_enclosing_gate_innermost_first() {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let gates = find_gates(&package(&ifr), &QUESTION_GATE_TARGET);
        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0].kind, GateKind::Grayout);
        assert_eq!(
            gates[0].expr,
            GateExpr::EqIdVal {
                question_id: 0x009A,
                value: 1
            }
        );
        assert_eq!(gates[1].kind, GateKind::Suppress);
        assert_eq!(
            gates[1].expr,
            GateExpr::Other,
            "outer suppress region contains the nested grayout opcode"
        );
    }

    #[test]
    fn find_gates_ignores_question_in_other_form() {
        let ifr = vendor_ifr();
        let pkg = package(&ifr);
        let wrong_form = find_gates(
            &pkg,
            &GateTarget {
                form_id: 10028,
                question_id: Some(0x003B),
            },
        );
        assert!(wrong_form.is_empty());
    }

    #[test]
    fn find_gates_unknown_target_is_empty() {
        let pkg = package(&vendor_ifr());
        assert!(
            find_gates(
                &pkg,
                &GateTarget {
                    form_id: 4242,
                    question_id: None,
                }
            )
            .is_empty()
        );
        assert!(
            find_gates(
                &pkg,
                &GateTarget {
                    form_id: 4242,
                    question_id: Some(9),
                }
            )
            .is_empty()
        );
    }

    #[test]
    fn find_gates_stops_gracefully_on_truncated_package() {
        let mut pkg = package(&vendor_ifr());
        pkg.truncate(pkg.len() - 3);
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert!(gates.len() <= 1);
    }

    #[test]
    fn plan_eq_const_flip_targets_second_operand_lsb() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let flips = plan_gates(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].offset, gates[0].expr_offset + 12);
        assert_eq!(flips[0].from, vec![1]);
        assert_eq!(flips[0].to, vec![2]);
    }

    #[test]
    fn plan_eq_id_val_flip_rewrites_value_to_unreachable() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        let flips = plan_gates(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].offset, gates[0].expr_offset + 4);
        assert_eq!(flips[0].from, vec![1, 0]);
        assert_eq!(flips[0].to, vec![0xFF, 0xFF]);
    }

    #[test]
    fn plan_eq_const_distinct_operands_have_no_flip() {
        let mut ifr = form_set(7);
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(2));
        ifr.extend(equal());
        ifr.extend(form(901, 30));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let gates = find_gates(
            &pkg,
            &GateTarget {
                form_id: 901,
                question_id: None,
            },
        );
        assert!(plan_gates(&pkg, &gates).is_err());
    }

    #[test]
    fn plan_refuses_eq_id_val_when_master_storage_is_two_bytes() {
        let pkg = package(&master_switch_ifr(r_efi::hii::IFR_NUMERIC_SIZE_2));
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        let err = plan_gates(&pkg, &gates).unwrap_err();
        assert!(
            err.contains("pkg+"),
            "diagnostics must carry the gate offset: {err}"
        );
    }

    #[test]
    fn plan_allows_eq_id_val_when_master_is_one_byte_numeric() {
        let pkg = package(&master_switch_ifr(r_efi::hii::IFR_NUMERIC_SIZE_1));
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(plan_gates(&pkg, &gates).is_ok());
    }

    #[test]
    fn plan_allows_eq_id_val_when_master_absent_from_package() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(
            plan_gates(&pkg, &gates).is_ok(),
            "vendor fixture has no 0x009A statement at all — width unknown, flip allowed"
        );
    }

    #[test]
    fn apply_flips_write_in_place_preserving_length() {
        let pkg = package(&vendor_ifr());
        let before = pkg.clone();
        let mut body = pkg.clone();
        for target in [FORM_GATE_TARGET, QUESTION_GATE_TARGET] {
            let flips = plan_gates(&body, &find_gates(&body, &target)).unwrap();
            apply_flips(&mut body, &flips).unwrap();
        }
        assert_eq!(body.len(), before.len());
        let diff: Vec<(usize, u8, u8)> = before
            .iter()
            .zip(body.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        assert_eq!(diff.len(), 3);
        assert!(diff.iter().any(|(_, a, b)| *a == 1 && *b == 2));
        assert!(diff.iter().filter(|(_, _, b)| *b == 0xFF).count() == 2);
    }

    #[test]
    fn apply_flips_rejects_stale_from_bytes() {
        let pkg = package(&vendor_ifr());
        let gates = find_gates(&pkg, &FORM_GATE_TARGET);
        let mut flips = plan_gates(&pkg, &gates).unwrap();
        let mut body = pkg.clone();
        let stale = flips[0].offset;
        body[stale] = 0x77;
        assert!(apply_flips(&mut body, &flips).is_err());
        assert_eq!(body[stale], 0x77);
        flips[0].from = vec![0x77];
        flips[0].to = vec![0x78];
        apply_flips(&mut body, &flips).unwrap();
        assert_eq!(body[stale], 0x78);
    }

    #[test]
    fn plan_after_flip_refuses_second_unlock() {
        let pkg = package(&vendor_ifr());
        let mut body = pkg.clone();
        let flips = plan_gates(&body, &find_gates(&body, &FORM_GATE_TARGET)).unwrap();
        apply_flips(&mut body, &flips).unwrap();
        let gates = find_gates(&body, &FORM_GATE_TARGET);
        assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 2 });
        assert!(
            plan_gates(&body, &gates).is_err(),
            "already-false expression must not flip again"
        );
    }

    fn grayout_ffff_ifr() -> Vec<u8> {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 0xFFFF));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr
    }

    #[test]
    fn plan_eq_id_val_with_ffff_value_is_not_a_flip() {
        let pkg = package(&grayout_ffff_ifr());
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert_eq!(
            gates[0].expr,
            GateExpr::EqIdVal {
                question_id: 0x009A,
                value: 0xFFFF
            }
        );
        assert!(plan_flip(&pkg, &gates[0]).is_none());
        assert!(plan_gates(&pkg, &gates).is_err());
    }

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
    fn decode_stops_at_end_terminators_of_scoped_operands() {
        let region = concat(&[uint64_quirk(1), end(), uint64_quirk(1), end(), equal()]);
        assert_eq!(
            decode_expr(&region),
            GateExpr::EqConst { a: 1, b: 1 },
            "scoped operands ([45 8a]) carry their END terminators inside the expression region"
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

    #[test]
    fn plan_gates_skip_unlocked_passes_already_unlocked() {
        let mut ifr = form_set(7);
        ifr.extend(form(10002, 20));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(2));
        ifr.extend(equal());
        ifr.extend(ref_op(10029, 0x003A));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 0xFFFF));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);

        let form_gates = find_gates(&pkg, &FORM_GATE_TARGET);
        assert!(!form_gates.is_empty());
        assert!(
            plan_gates_skip_unlocked(&pkg, &form_gates)
                .unwrap()
                .is_empty()
        );

        let q_gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(!q_gates.is_empty());
        assert!(plan_gates_skip_unlocked(&pkg, &q_gates).unwrap().is_empty());
    }

    #[test]
    fn plan_gates_skip_unlocked_plans_only_locked() {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009B, 0xFFFF));
        ifr.extend(one_of_op(0x003C));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let gates = find_gates(
            &pkg,
            &GateTarget {
                form_id: 10029,
                question_id: Some(0x003B),
            },
        );
        let flips = plan_gates_skip_unlocked(&pkg, &gates).unwrap();
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].to, 0xFFFFu16.to_le_bytes().to_vec());
    }

    #[test]
    fn plan_gates_skip_unlocked_errors_on_wide_storage() {
        let mut ifr = form_set(7);
        ifr.extend(form(10029, 21));
        ifr.extend(numeric_op(0x009A, 0x01));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x009A, 1));
        ifr.extend(one_of_op(0x003B));
        ifr.extend(end());
        ifr.extend(end());
        ifr.extend(end());
        let pkg = package(&ifr);
        let gates = find_gates(&pkg, &QUESTION_GATE_TARGET);
        assert!(plan_gates_skip_unlocked(&pkg, &gates).is_err());
    }
}
