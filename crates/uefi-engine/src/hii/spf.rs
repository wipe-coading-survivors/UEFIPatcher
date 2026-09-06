pub const SPF_RECORD_SIZE: usize = 72;
pub const SPF_RECORD_IFR_OFFSET: usize = 28;
pub const SPF_RECORD_FAILSAFE: usize = 52;
pub const SPF_RECORD_OPTIMAL: usize = 53;
pub const SPF_RECORD_HELP_ID: usize = 0x14;
pub const SPF_RECORD_COUNTER_OFFSET: usize = 0x18;
pub const SPF_RECORD_PROMPT_ID_OFFSET: usize = 0x30;

pub const SPF_CONTAINER_BASE: usize = 0x10;
pub const SPF_HEADER_REGION_OFFSETS: usize = 0x30;
pub const SPF_PAGE_COUNT_OFFSET: usize = 0x60;
pub const SPF_PAGE_TABLE_OFFSET: usize = 0x64;
pub const SPF_PAGE_HEADER_SIZE: usize = 0x20;
pub const SPF_PAGE_CNT_OFFSET: usize = 0x1C;
pub const SPF_PAGE_LIST_OFFSET: usize = 0x20;
pub const SPF_STRING_CONTROL_SIZE: usize = 0x0C;

const SPF_SIGNATURE: [u8; 8] = [0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
const SPF_TAIL: [u8; 4] = [0x01, 0x00, 0x01, 0x00];
const SPF_CONTAINER_SIG: [u8; 4] = *b"$SPF";

pub fn container_start(body: &[u8]) -> Option<usize> {
    if body.len() < SPF_CONTAINER_SIG.len() {
        return None;
    }
    (0..=body.len() - SPF_CONTAINER_SIG.len()).find(|&p| body[p..p + 4] == SPF_CONTAINER_SIG)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpfQuestionRecord {
    pub offset: usize,
    pub question_id: u16,
    pub ifr_offset: u32,
    pub failsafe: u8,
    pub optimal: u8,
}

pub fn scan_question_records(body: &[u8]) -> Vec<SpfQuestionRecord> {
    let mut out = Vec::new();
    if body.len() < SPF_RECORD_SIZE {
        return out;
    }
    for p in 0..=body.len() - SPF_RECORD_SIZE {
        if body[p + 36..p + 44] != SPF_SIGNATURE || body[p + 44..p + 48] != SPF_TAIL {
            continue;
        }
        let qid = u32::from_le_bytes(body[p..p + 4].try_into().unwrap());
        if qid == 0 || qid > u16::MAX as u32 {
            continue;
        }
        out.push(SpfQuestionRecord {
            offset: p,
            question_id: qid as u16,
            ifr_offset: u32::from_le_bytes(
                body[p + SPF_RECORD_IFR_OFFSET..p + SPF_RECORD_IFR_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            ),
            failsafe: body[p + SPF_RECORD_FAILSAFE],
            optimal: body[p + SPF_RECORD_OPTIMAL],
        });
    }
    out
}

pub fn write_record_defaults(body: &mut [u8], offset: usize, failsafe: u8, optimal: u8) {
    body[offset + SPF_RECORD_FAILSAFE] = failsafe;
    body[offset + SPF_RECORD_OPTIMAL] = optimal;
}

pub fn write_record_help_id(body: &mut [u8], offset: usize, help_id: u16) {
    body[offset + SPF_RECORD_HELP_ID..offset + SPF_RECORD_HELP_ID + 2]
        .copy_from_slice(&help_id.to_le_bytes());
}

pub const SPF_STRING_CONTROL_STR_ID: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpfStringControl {
    pub offset: usize,
    pub string_id: u16,
}

pub fn scan_string_controls(body: &[u8]) -> Vec<SpfStringControl> {
    let mut out = Vec::new();
    if body.len() < 0xC {
        return out;
    }
    for p in 0..=body.len() - 0xC {
        if u16::from_le_bytes([body[p], body[p + 1]]) == 5
            && u16::from_le_bytes([body[p + 4], body[p + 5]]) == 0
            && u16::from_le_bytes([body[p + 0xA], body[p + 0xB]]) == 78
        {
            out.push(SpfStringControl {
                offset: p + SPF_STRING_CONTROL_STR_ID,
                string_id: u16::from_le_bytes([body[p + 2], body[p + 3]]),
            });
        }
    }
    out
}

pub fn write_string_control(body: &mut [u8], offset: usize, string_id: u16) {
    body[offset..offset + 2].copy_from_slice(&string_id.to_le_bytes());
}

#[allow(clippy::too_many_arguments)]
pub fn append_question_record(
    body: &mut Vec<u8>,
    template: usize,
    qid: u16,
    help_id: u16,
    prompt_id: u16,
    ifr_offset: u32,
    counter: u32,
    failsafe: u8,
    optimal: u8,
) -> usize {
    let base = container_start(body).expect("$SPF signature not found");
    let src = base + template;
    let mut rec = body[src..src + SPF_RECORD_SIZE].to_vec();
    rec[0..4].copy_from_slice(&(qid as u32).to_le_bytes());
    rec[SPF_RECORD_HELP_ID..SPF_RECORD_HELP_ID + 2].copy_from_slice(&help_id.to_le_bytes());
    rec[SPF_RECORD_PROMPT_ID_OFFSET..SPF_RECORD_PROMPT_ID_OFFSET + 2]
        .copy_from_slice(&prompt_id.to_le_bytes());
    rec[SPF_RECORD_IFR_OFFSET..SPF_RECORD_IFR_OFFSET + 4]
        .copy_from_slice(&ifr_offset.to_le_bytes());
    rec[SPF_RECORD_COUNTER_OFFSET..SPF_RECORD_COUNTER_OFFSET + 4]
        .copy_from_slice(&counter.to_le_bytes());
    rec[SPF_RECORD_FAILSAFE] = failsafe;
    rec[SPF_RECORD_OPTIMAL] = optimal;
    let offset = body.len() - base;
    body.extend_from_slice(&rec);
    offset
}

pub fn append_string_control(body: &mut Vec<u8>, template: usize, string_id: u16) -> usize {
    let base = container_start(body).expect("$SPF signature not found");
    let src = base + template - SPF_STRING_CONTROL_STR_ID;
    let mut ctrl = body[src..src + SPF_STRING_CONTROL_SIZE].to_vec();
    ctrl[SPF_STRING_CONTROL_STR_ID..SPF_STRING_CONTROL_STR_ID + 2]
        .copy_from_slice(&string_id.to_le_bytes());
    let offset = body.len() - base + SPF_STRING_CONTROL_STR_ID;
    body.extend_from_slice(&ctrl);
    offset
}

pub fn clone_page_with_controls(
    body: &mut Vec<u8>,
    page_offset: usize,
    extra_ctrls: &[u32],
) -> usize {
    let base = container_start(body).expect("$SPF signature not found");
    let page = base + page_offset;
    let cnt = u32::from_le_bytes(
        body[page + SPF_PAGE_CNT_OFFSET..page + SPF_PAGE_CNT_OFFSET + 4]
            .try_into()
            .unwrap(),
    );
    let list = page + SPF_PAGE_LIST_OFFSET;
    let tail = list + 4 * cnt as usize;
    let trailing = if tail + 4 <= body.len() {
        let t = u32::from_le_bytes(body[tail..tail + 4].try_into().unwrap());
        (t != 0).then_some(t)
    } else {
        None
    };
    let mut clone = body[page..page + SPF_PAGE_HEADER_SIZE].to_vec();
    clone.extend_from_slice(&body[list..tail]);
    for &ctrl in extra_ctrls {
        clone.extend_from_slice(&ctrl.to_le_bytes());
    }
    if let Some(t) = trailing {
        clone.extend_from_slice(&t.to_le_bytes());
    }
    let new_cnt = cnt + extra_ctrls.len() as u32;
    clone[SPF_PAGE_CNT_OFFSET..SPF_PAGE_CNT_OFFSET + 4].copy_from_slice(&new_cnt.to_le_bytes());
    let offset = body.len() - base;
    body.extend_from_slice(&clone);
    offset
}

pub fn repoint_page_slot(body: &mut [u8], slot: usize, new_offset: u32) {
    let base = container_start(body).expect("$SPF signature not found");
    let at = base + SPF_PAGE_TABLE_OFFSET + 4 * slot;
    body[at..at + 4].copy_from_slice(&new_offset.to_le_bytes());
}

pub fn bump_container_length(body: &mut [u8], new_len: usize) {
    let base = container_start(body).expect("$SPF signature not found");
    let mut at = base + SPF_HEADER_REGION_OFFSETS;
    let mut max = u32::from_le_bytes(body[at..at + 4].try_into().unwrap());
    for p in (SPF_HEADER_REGION_OFFSETS..SPF_PAGE_COUNT_OFFSET)
        .step_by(4)
        .skip(1)
    {
        let field = base + p;
        let v = u32::from_le_bytes(body[field..field + 4].try_into().unwrap());
        if v >= max {
            max = v;
            at = field;
        }
    }
    body[at..at + 4].copy_from_slice(&(new_len as u32).to_le_bytes());
}

pub fn fixup_record_ifr_offsets(body: &mut [u8], threshold: u32, delta: u32) -> usize {
    container_start(body).expect("$SPF signature not found");
    let mut patched = 0;
    for rec in scan_question_records(body) {
        if rec.ifr_offset >= threshold {
            let at = rec.offset + SPF_RECORD_IFR_OFFSET;
            body[at..at + 4].copy_from_slice(&(rec.ifr_offset + delta).to_le_bytes());
            patched += 1;
        }
    }
    patched
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question_record(qid: u16, ifr: u32, fs: u8, opt: u8) -> Vec<u8> {
        let mut r = vec![0u8; SPF_RECORD_SIZE];
        r[0..4].copy_from_slice(&(qid as u32).to_le_bytes());
        r[8..10].copy_from_slice(&6u16.to_le_bytes());
        r[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        r[16] = 0x09;
        r[20..24].copy_from_slice(&0x1A4u32.to_le_bytes());
        r[24..28].copy_from_slice(&0x10066u32.to_le_bytes());
        r[28..32].copy_from_slice(&ifr.to_le_bytes());
        r[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        r[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
        r[48..52].copy_from_slice(&0x1A3u32.to_le_bytes());
        r[52] = fs;
        r[53] = opt;
        r
    }

    #[test]
    fn scan_finds_question_records_and_fields() {
        let mut body = vec![0xABu8; 64];
        body.extend_from_slice(&question_record(0x3B, 0x0DD3, 0, 0));
        body.extend_from_slice(&question_record(0x36, 0x0CA8, 2, 3));
        body.extend_from_slice(&question_record(0x3C, 0x0DFE, 255, 255));
        body.extend_from_slice(&[0xFFu8; 100]);
        let recs = scan_question_records(&body);
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[0].offset, 64);
        assert_eq!(recs[0].question_id, 0x3B);
        assert_eq!(recs[0].ifr_offset, 0x0DD3);
        assert_eq!(recs[0].failsafe, 0);
        assert_eq!(recs[0].optimal, 0);
        assert_eq!(recs[1].question_id, 0x36);
        assert_eq!(recs[2].failsafe, 255);
    }

    #[test]
    fn scan_rejects_qid_above_u16() {
        let mut r = question_record(0x3B, 0x0DD3, 0, 0);
        r[2] = 0x01;
        let recs = scan_question_records(&r);
        assert!(recs.is_empty());
    }

    #[test]
    fn scan_skips_bare_signature_in_noise() {
        let mut body = vec![0x00u8; 128];
        body[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        body[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
        assert!(scan_question_records(&body).is_empty());
    }

    #[test]
    fn write_record_defaults_touches_exactly_two_bytes() {
        let mut body = question_record(0x3B, 0x0DD3, 0, 0);
        let before = body.clone();
        write_record_defaults(&mut body, 0, 1, 2);
        assert_eq!(body[52], 1);
        assert_eq!(body[53], 2);
        for i in 0..body.len() {
            if i != 52 && i != 53 {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    #[test]
    fn write_record_help_id_touches_exactly_two_bytes() {
        let mut body = vec![0u8; 0x200];
        let rec = question_record(0x3B, 0x0DD3, 0, 0);
        body[0x80..0x80 + SPF_RECORD_SIZE].copy_from_slice(&rec);
        let before = body.clone();
        write_record_help_id(&mut body, 0x80, 0xBEEF);
        assert_eq!(
            &body[0x80 + SPF_RECORD_HELP_ID..0x80 + SPF_RECORD_HELP_ID + 2],
            &[0xEF, 0xBE]
        );
        for i in 0..body.len() {
            if i != 0x80 + SPF_RECORD_HELP_ID && i != 0x80 + SPF_RECORD_HELP_ID + 1 {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    fn string_control(str_id: u16, idx_a: u16, idx_b: u16) -> Vec<u8> {
        let mut c = vec![0u8; 0x108];
        c[0..2].copy_from_slice(&5u16.to_le_bytes());
        c[2..4].copy_from_slice(&str_id.to_le_bytes());
        c[6..8].copy_from_slice(&idx_a.to_le_bytes());
        c[8..10].copy_from_slice(&idx_b.to_le_bytes());
        c[10..12].copy_from_slice(&78u16.to_le_bytes());
        c[0x14..0x18].copy_from_slice(&57u32.to_le_bytes());
        c
    }

    #[test]
    fn scan_string_controls_finds_ladder() {
        let mut body = vec![0u8; 0x500];
        let rec = question_record(9, 0x10, 0, 0);
        body[0x40..0x40 + SPF_RECORD_SIZE].copy_from_slice(&rec);
        let ctrl_a = string_control(10, 1199, 120);
        let ctrl_b = string_control(11, 1200, 121);
        let ctrl_c = string_control(12, 1201, 122);
        body[0x100..0x100 + 0x108].copy_from_slice(&ctrl_a);
        body[0x208..0x208 + 0x108].copy_from_slice(&ctrl_b);
        body[0x310..0x310 + 0x108].copy_from_slice(&ctrl_c);

        let found = scan_string_controls(&body);
        assert_eq!(
            found.len(),
            3,
            "question record must not match control signature"
        );
        assert_eq!(found[0].string_id, 10);
        assert_eq!(found[0].offset, 0x100 + SPF_STRING_CONTROL_STR_ID);
        assert_eq!(found[2].string_id, 12);

        assert!(scan_string_controls(&question_record(9, 0x10, 0, 0)).is_empty());
    }

    #[test]
    fn write_string_control_same_length() {
        let mut body = vec![0u8; 0x200];
        let ctrl = string_control(420, 1208, 129);
        body[0x80..0x80 + 0x108].copy_from_slice(&ctrl);
        let before = body.clone();
        let off = 0x80 + SPF_STRING_CONTROL_STR_ID;
        write_string_control(&mut body, off, 751);
        assert_eq!(&body[off..off + 2], &[0xEF, 0x02]);
        for i in 0..body.len() {
            if i != off && i != off + 1 {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    const SYNTH_CONTAINER_LEN: usize = 0x170;
    const TEMPLATE_Q35: usize = 0x0CC;
    const SYNTH_REC_B: usize = 0x114;
    const SYNTH_PAGE_WITH_TAIL: usize = 0x070;
    const SYNTH_PAGE_NO_TAIL: usize = 0x09C;
    const SYNTH_STRING_CTRL_FIELD: usize = 0x15E;

    fn synth_container() -> Vec<u8> {
        let mut c = vec![0u8; SYNTH_CONTAINER_LEN];
        c[0..4].copy_from_slice(b"$SPF");
        c[4..8].copy_from_slice(&0x200u32.to_le_bytes());
        c[8..12].copy_from_slice(&0x210u32.to_le_bytes());
        c[0x0C..0x1C]
            .copy_from_slice(&[0x43, 0xD6, 0x87, 0xEC, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        c[0x1C..0x20].copy_from_slice(&0x48u32.to_le_bytes());
        c[0x30..0x34].copy_from_slice(&(TEMPLATE_Q35 as u32).to_le_bytes());
        c[0x50..0x54].copy_from_slice(&(TEMPLATE_Q35 as u32).to_le_bytes());
        c[0x54..0x58].copy_from_slice(&(SYNTH_REC_B as u32).to_le_bytes());
        c[0x58..0x5C].copy_from_slice(&0x168u32.to_le_bytes());
        c[0x5C..0x60].copy_from_slice(&(SYNTH_CONTAINER_LEN as u32).to_le_bytes());
        c[0x60..0x64].copy_from_slice(&2u32.to_le_bytes());
        c[0x64..0x68].copy_from_slice(&(SYNTH_PAGE_WITH_TAIL as u32).to_le_bytes());
        c[0x68..0x6C].copy_from_slice(&(SYNTH_PAGE_NO_TAIL as u32).to_le_bytes());
        c[0x78] = 2;
        c[0x7A..0x7C].copy_from_slice(&10019u16.to_le_bytes());
        c[0x7E..0x80].copy_from_slice(&0x100u16.to_le_bytes());
        c[0x80..0x82].copy_from_slice(&7u16.to_le_bytes());
        c[0x82] = 1;
        c[0x88..0x8C].copy_from_slice(&0x1234u32.to_le_bytes());
        c[0x8C..0x90].copy_from_slice(&2u32.to_le_bytes());
        c[0x90..0x94].copy_from_slice(&(TEMPLATE_Q35 as u32).to_le_bytes());
        c[0x94..0x98].copy_from_slice(&(SYNTH_REC_B as u32).to_le_bytes());
        c[0x98..0x9C].copy_from_slice(&0x55u32.to_le_bytes());
        c[0xA4] = 3;
        c[0xA6..0xA8].copy_from_slice(&10020u16.to_le_bytes());
        c[0xAA..0xAC].copy_from_slice(&0x101u16.to_le_bytes());
        c[0xAC..0xAE].copy_from_slice(&8u16.to_le_bytes());
        c[0xB4..0xB8].copy_from_slice(&0xBEEFu32.to_le_bytes());
        c[0xB8..0xBC].copy_from_slice(&2u32.to_le_bytes());
        c[0xBC..0xC0].copy_from_slice(&(TEMPLATE_Q35 as u32).to_le_bytes());
        c[0xC0..0xC4].copy_from_slice(&(SYNTH_REC_B as u32).to_le_bytes());
        let qa = question_record(0x35, 0x0DD3, 0, 0);
        c[TEMPLATE_Q35..TEMPLATE_Q35 + SPF_RECORD_SIZE].copy_from_slice(&qa);
        let qb = question_record(0x36, 0x0500, 2, 3);
        c[SYNTH_REC_B..SYNTH_REC_B + SPF_RECORD_SIZE].copy_from_slice(&qb);
        c[0x15C..0x15E].copy_from_slice(&5u16.to_le_bytes());
        c[0x15E..0x160].copy_from_slice(&748u16.to_le_bytes());
        c[0x160..0x162].copy_from_slice(&0u16.to_le_bytes());
        c[0x162..0x164].copy_from_slice(&0x77u16.to_le_bytes());
        c[0x164..0x166].copy_from_slice(&0x88u16.to_le_bytes());
        c[0x166..0x168].copy_from_slice(&78u16.to_le_bytes());
        c[0x168..0x170].fill(0xEE);
        c
    }

    #[test]
    fn container_start_finds_spf_signature() {
        let c = synth_container();
        assert_eq!(container_start(&c), Some(0));
        let mut prefixed = vec![0x11u8; SPF_CONTAINER_BASE];
        prefixed.extend_from_slice(&c);
        assert_eq!(container_start(&prefixed), Some(SPF_CONTAINER_BASE));
        assert_eq!(container_start(&[0u8; 8]), None);
    }

    #[test]
    fn append_question_record_clones_template_and_patches_fields() {
        let mut body = synth_container();
        let base_len = body.len();
        let off = append_question_record(
            &mut body,
            TEMPLATE_Q35,
            0x200,
            900,
            901,
            0x993,
            0x1_0095,
            0,
            0,
        );
        assert_eq!(body.len(), base_len + SPF_RECORD_SIZE);
        assert_eq!(&body[..base_len], &synth_container()[..]);
        assert_eq!(
            u32::from_le_bytes(body[off..off + 4].try_into().unwrap()),
            0x200
        );
        assert_eq!(
            u16::from_le_bytes(
                body[off + SPF_RECORD_HELP_ID..off + SPF_RECORD_HELP_ID + 2]
                    .try_into()
                    .unwrap()
            ),
            900
        );
        assert_eq!(
            u32::from_le_bytes(
                body[off + SPF_RECORD_IFR_OFFSET..off + SPF_RECORD_IFR_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x993
        );
    }

    #[test]
    fn append_question_record_patches_all_fields_and_uses_container_offsets() {
        let mut pristine = vec![0x11u8; SPF_CONTAINER_BASE];
        pristine.extend_from_slice(&synth_container());
        let mut body = pristine.clone();
        let base_len = body.len();
        let off = append_question_record(
            &mut body,
            TEMPLATE_Q35,
            0x201,
            902,
            903,
            0x994,
            0x1_0096,
            1,
            2,
        );
        assert_eq!(off, base_len - SPF_CONTAINER_BASE);
        assert_eq!(body.len(), base_len + SPF_RECORD_SIZE);
        assert_eq!(&body[..base_len], &pristine[..]);
        let rec = SPF_CONTAINER_BASE + off;
        assert_eq!(
            u32::from_le_bytes(body[rec..rec + 4].try_into().unwrap()),
            0x201
        );
        assert_eq!(
            u16::from_le_bytes(
                body[rec + SPF_RECORD_HELP_ID..rec + SPF_RECORD_HELP_ID + 2]
                    .try_into()
                    .unwrap()
            ),
            902
        );
        assert_eq!(
            u16::from_le_bytes(
                body[rec + SPF_RECORD_PROMPT_ID_OFFSET..rec + SPF_RECORD_PROMPT_ID_OFFSET + 2]
                    .try_into()
                    .unwrap()
            ),
            903
        );
        assert_eq!(
            u32::from_le_bytes(
                body[rec + SPF_RECORD_IFR_OFFSET..rec + SPF_RECORD_IFR_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x994
        );
        assert_eq!(
            u32::from_le_bytes(
                body[rec + SPF_RECORD_COUNTER_OFFSET..rec + SPF_RECORD_COUNTER_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            0x1_0096
        );
        assert_eq!(body[rec + SPF_RECORD_FAILSAFE], 1);
        assert_eq!(body[rec + SPF_RECORD_OPTIMAL], 2);
        assert_eq!(body[rec + 36..rec + 44], SPF_SIGNATURE);
    }

    #[test]
    fn append_string_control_clones_template_and_patches_str_id() {
        let mut body = synth_container();
        let base_len = body.len();
        let off = append_string_control(&mut body, SYNTH_STRING_CTRL_FIELD, 999);
        assert_eq!(body.len(), base_len + 0x0C);
        assert_eq!(&body[..base_len], &synth_container()[..]);
        assert_eq!(
            u16::from_le_bytes(body[off..off + 2].try_into().unwrap()),
            999
        );
        let block = off - SPF_STRING_CONTROL_STR_ID;
        let tpl_block = SYNTH_STRING_CTRL_FIELD - SPF_STRING_CONTROL_STR_ID;
        assert_eq!(
            u16::from_le_bytes(body[block..block + 2].try_into().unwrap()),
            5
        );
        assert_eq!(
            &body[block + 4..block + 0x0C],
            &synth_container()[tpl_block + 4..tpl_block + 0x0C]
        );
        let found = scan_string_controls(&body);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].offset, off);
        assert_eq!(found[1].string_id, 999);
    }

    #[test]
    fn clone_page_with_controls_keeps_trailing_slot_terminal() {
        let mut body = synth_container();
        let base_len = body.len();
        let clone = clone_page_with_controls(&mut body, SYNTH_PAGE_WITH_TAIL, &[0x200, 0x204]);
        assert_eq!(clone, base_len);
        assert_eq!(body.len(), base_len + SPF_PAGE_HEADER_SIZE + 4 * 4 + 4);
        assert_eq!(&body[..base_len], &synth_container()[..]);
        assert_eq!(
            &body[clone..clone + 0x1C],
            &synth_container()[SYNTH_PAGE_WITH_TAIL..SYNTH_PAGE_WITH_TAIL + 0x1C]
        );
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_CNT_OFFSET..clone + SPF_PAGE_CNT_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            4
        );
        let src_list = SYNTH_PAGE_WITH_TAIL + SPF_PAGE_LIST_OFFSET;
        for i in 0..2 {
            assert_eq!(
                body[clone + SPF_PAGE_LIST_OFFSET + 4 * i
                    ..clone + SPF_PAGE_LIST_OFFSET + 4 * i + 4],
                synth_container()[src_list + 4 * i..src_list + 4 * i + 4]
            );
        }
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_LIST_OFFSET + 8..clone + SPF_PAGE_LIST_OFFSET + 12]
                    .try_into()
                    .unwrap()
            ),
            0x200
        );
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_LIST_OFFSET + 12..clone + SPF_PAGE_LIST_OFFSET + 16]
                    .try_into()
                    .unwrap()
            ),
            0x204
        );
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_LIST_OFFSET + 16..clone + SPF_PAGE_LIST_OFFSET + 20]
                    .try_into()
                    .unwrap()
            ),
            0x55
        );
    }

    #[test]
    fn clone_page_without_trailing_slot_appends_controls() {
        let mut body = synth_container();
        let base_len = body.len();
        let clone = clone_page_with_controls(&mut body, SYNTH_PAGE_NO_TAIL, &[0x208]);
        assert_eq!(clone, base_len);
        assert_eq!(body.len(), base_len + SPF_PAGE_HEADER_SIZE + 4 * 3);
        assert_eq!(&body[..base_len], &synth_container()[..]);
        assert_eq!(
            &body[clone..clone + 0x1C],
            &synth_container()[SYNTH_PAGE_NO_TAIL..SYNTH_PAGE_NO_TAIL + 0x1C]
        );
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_CNT_OFFSET..clone + SPF_PAGE_CNT_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            3
        );
        let src_list = SYNTH_PAGE_NO_TAIL + SPF_PAGE_LIST_OFFSET;
        for i in 0..2 {
            assert_eq!(
                body[clone + SPF_PAGE_LIST_OFFSET + 4 * i
                    ..clone + SPF_PAGE_LIST_OFFSET + 4 * i + 4],
                synth_container()[src_list + 4 * i..src_list + 4 * i + 4]
            );
        }
        assert_eq!(
            u32::from_le_bytes(
                body[clone + SPF_PAGE_LIST_OFFSET + 8..clone + SPF_PAGE_LIST_OFFSET + 12]
                    .try_into()
                    .unwrap()
            ),
            0x208
        );
    }

    #[test]
    fn repoint_page_slot_writes_table_entry() {
        let mut body = synth_container();
        let before = body.clone();
        repoint_page_slot(&mut body, 2, 0x1A4);
        let at = SPF_PAGE_TABLE_OFFSET + 4 * 2;
        assert_eq!(
            u32::from_le_bytes(body[at..at + 4].try_into().unwrap()),
            0x1A4
        );
        for i in 0..body.len() {
            if !(at..at + 4).contains(&i) {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    #[test]
    fn fixup_record_ifr_offsets_patches_only_above_threshold() {
        let mut body = synth_container();
        let boundary_off = body.len();
        body.extend_from_slice(&question_record(0x37, 0x0D00, 0, 0));
        let before = body.clone();
        let n = fixup_record_ifr_offsets(&mut body, 0x0D00, 4);
        assert_eq!(n, 2);
        let patched_a = TEMPLATE_Q35 + SPF_RECORD_IFR_OFFSET;
        assert_eq!(
            u32::from_le_bytes(body[patched_a..patched_a + 4].try_into().unwrap()),
            0x0DD7
        );
        let patched_boundary = boundary_off + SPF_RECORD_IFR_OFFSET;
        assert_eq!(
            u32::from_le_bytes(
                body[patched_boundary..patched_boundary + 4]
                    .try_into()
                    .unwrap()
            ),
            0x0D04
        );
        let kept = SYNTH_REC_B + SPF_RECORD_IFR_OFFSET;
        assert_eq!(
            u32::from_le_bytes(body[kept..kept + 4].try_into().unwrap()),
            0x0500
        );
        for i in 0..body.len() {
            if !(patched_a..patched_a + 4).contains(&i)
                && !(patched_boundary..patched_boundary + 4).contains(&i)
            {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    #[test]
    fn bump_container_length_patches_last_header_offset() {
        let mut body = synth_container();
        let before = body.clone();
        let at = 0x5C;
        assert_eq!(
            u32::from_le_bytes(body[at..at + 4].try_into().unwrap()),
            SYNTH_CONTAINER_LEN as u32
        );
        bump_container_length(&mut body, 0x374);
        assert_eq!(
            u32::from_le_bytes(body[at..at + 4].try_into().unwrap()),
            0x374
        );
        for i in 0..body.len() {
            if !(at..at + 4).contains(&i) {
                assert_eq!(body[i], before[i]);
            }
        }
    }

    #[test]
    fn bump_container_length_finds_stale_length_after_appends() {
        let mut body = synth_container();
        let base_len = body.len();
        append_question_record(
            &mut body,
            TEMPLATE_Q35,
            0x202,
            904,
            905,
            0x995,
            0x1_0097,
            0,
            0,
        );
        let grown = body.len();
        assert_eq!(grown, base_len + SPF_RECORD_SIZE);
        let at = 0x5C;
        assert_eq!(
            u32::from_le_bytes(body[at..at + 4].try_into().unwrap()),
            SYNTH_CONTAINER_LEN as u32
        );
        let after_append = body.clone();
        bump_container_length(&mut body, grown);
        assert_eq!(
            u32::from_le_bytes(body[at..at + 4].try_into().unwrap()),
            grown as u32
        );
        for i in 0..body.len() {
            if !(at..at + 4).contains(&i) {
                assert_eq!(body[i], after_append[i]);
            }
        }
        let after_bump = body.clone();
        bump_container_length(&mut body, grown);
        assert_eq!(body, after_bump);
    }

    #[test]
    fn append_string_control_template_is_container_relative_from_scan() {
        let mut body = vec![0x11u8; SPF_CONTAINER_BASE];
        body.extend_from_slice(&synth_container());
        let pristine = body.clone();
        let scanned = scan_string_controls(&body);
        assert_eq!(scanned.len(), 1);
        assert_eq!(
            scanned[0].offset,
            SPF_CONTAINER_BASE + SYNTH_STRING_CTRL_FIELD
        );
        let base = container_start(&body).expect("synth container");
        let off = append_string_control(&mut body, scanned[0].offset - base, 0x300);
        assert_eq!(&body[..pristine.len()], &pristine[..]);
        assert_eq!(
            u16::from_le_bytes(body[base + off..base + off + 2].try_into().unwrap()),
            0x300
        );
        let block = base + off - SPF_STRING_CONTROL_STR_ID;
        let tpl_block = SPF_CONTAINER_BASE + SYNTH_STRING_CTRL_FIELD - SPF_STRING_CONTROL_STR_ID;
        assert_eq!(
            u16::from_le_bytes(body[block..block + 2].try_into().unwrap()),
            5
        );
        assert_eq!(
            &body[block + 4..block + 0x0C],
            &pristine[tpl_block + 4..tpl_block + 0x0C]
        );
        assert_eq!(
            u16::from_le_bytes(body[block + 0xA..block + 0xC].try_into().unwrap()),
            78
        );
    }

    #[test]
    #[should_panic(expected = "$SPF")]
    fn append_question_record_panics_without_spf_container() {
        let mut body = vec![0u8; 0x200];
        append_question_record(&mut body, 0, 1, 2, 3, 4, 5, 0, 0);
    }

    #[test]
    #[should_panic(expected = "$SPF")]
    fn repoint_page_slot_panics_without_spf_container() {
        let mut body = vec![0u8; 0x200];
        repoint_page_slot(&mut body, 0, 1);
    }

    #[test]
    #[should_panic(expected = "$SPF")]
    fn bump_container_length_panics_without_spf_container() {
        let mut body = vec![0u8; 0x200];
        bump_container_length(&mut body, 0x200);
    }

    #[test]
    #[should_panic(expected = "$SPF")]
    fn fixup_record_ifr_offsets_panics_without_spf_container() {
        let mut body = vec![0u8; 0x200];
        fixup_record_ifr_offsets(&mut body, 0, 0);
    }
}
