pub const SPF_RECORD_SIZE: usize = 72;
pub const SPF_RECORD_IFR_OFFSET: usize = 28;
pub const SPF_RECORD_FAILSAFE: usize = 52;
pub const SPF_RECORD_OPTIMAL: usize = 53;

const SPF_SIGNATURE: [u8; 8] = [0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
const SPF_TAIL: [u8; 4] = [0x01, 0x00, 0x01, 0x00];

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
}
