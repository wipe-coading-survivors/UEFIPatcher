use crate::ops;
use crate::types::*;

use super::HiiError;

pub const AMI_RECORD_SIZE: usize = 108;
pub const AMI_DEFAULT_ACCESS_LEVEL: u8 = 0x05;
const AMI_PAGE_ID_OFFSET: usize = 24;
const AMI_ACCESS_LEVEL_OFFSET: usize = 32;
const AMI_FAILSAFE_OFFSET: usize = 104;
const AMI_OPTIMAL_OFFSET: usize = 106;

pub struct QuestionAmiRecord {
    pub question_id: u16,
    pub page_id: Option<u16>,
    pub access_level: u8,
    pub failsafe: u8,
    pub optimal: u8,
}

pub fn make_ami_record(rec: &QuestionAmiRecord) -> Vec<u8> {
    let mut buf = vec![0u8; AMI_RECORD_SIZE];
    buf[0..2].copy_from_slice(&rec.question_id.to_le_bytes());
    if let Some(pid) = rec.page_id {
        buf[AMI_PAGE_ID_OFFSET..AMI_PAGE_ID_OFFSET + 2].copy_from_slice(&pid.to_le_bytes());
    }
    buf[AMI_ACCESS_LEVEL_OFFSET] = rec.access_level;
    buf[AMI_FAILSAFE_OFFSET] = rec.failsafe;
    buf[AMI_OPTIMAL_OFFSET] = rec.optimal;
    buf
}

pub fn patch_ami(
    image: &mut Image,
    formset_guid: &Guid,
    form_ids: &[u16],
    questions: &[QuestionAmiRecord],
    setupdata_guid: Option<&Guid>,
    amitse_guid: Option<&Guid>,
) -> Result<(), HiiError> {
    let (sd_vi, sd_fi) = find_ami_module(image, setupdata_guid, "setupdata")?;
    let (am_vi, am_fi) = find_ami_module(image, amitse_guid, "AMITSE")?;
    {
        let setupdata = &mut image.root.children[sd_vi].children[sd_fi];
        for q in questions {
            let record = make_ami_record(q);
            setupdata.body.extend_from_slice(&record);
        }
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &[sd_vi, sd_fi]);
    {
        let amitse = &mut image.root.children[am_vi].children[am_fi];
        let formset_marker = &formset_guid.to_bytes()[12..14];
        let insert_pos =
            find_formset_marker_position(&amitse.body, formset_marker).unwrap_or(amitse.body.len());
        let mut entries = Vec::with_capacity(form_ids.len() * 2);
        for &fid in form_ids {
            entries.extend_from_slice(&fid.to_le_bytes());
        }
        amitse
            .body
            .splice(insert_pos..insert_pos, entries.iter().copied());
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &[am_vi, am_fi]);
    Ok(())
}

pub(crate) fn precheck_ami_modules(
    image: &Image,
    setupdata_guid: Option<&Guid>,
    amitse_guid: Option<&Guid>,
) -> Result<(), HiiError> {
    find_ami_module(image, setupdata_guid, "setupdata")?;
    find_ami_module(image, amitse_guid, "AMITSE")?;
    Ok(())
}

fn find_ami_module(
    image: &Image,
    guid: Option<&Guid>,
    name_hint: &str,
) -> Result<(usize, usize), HiiError> {
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if let Some(g) = guid
                && file.guid == Some(*g)
            {
                return Ok((vi, fi));
            }
        }
    }
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if has_name_section(&file.children, name_hint) {
                return Ok((vi, fi));
            }
        }
    }
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if has_question_id_markers(&file.body) {
                return Ok((vi, fi));
            }
        }
    }
    Err(HiiError::AmiFilesNotFound)
}

fn has_name_section(children: &[FfsNode], name: &str) -> bool {
    children
        .iter()
        .any(|c| c.subtype == crate::ffs::EFI_SECTION_UI && ucs2_body_to_string(&c.body) == name)
}

fn ucs2_body_to_string(body: &[u8]) -> String {
    let text: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&text)
}

fn has_question_id_markers(body: &[u8]) -> bool {
    body.len() >= AMI_RECORD_SIZE && body.len().is_multiple_of(AMI_RECORD_SIZE)
}

fn find_formset_marker_position(body: &[u8], marker: &[u8]) -> Option<usize> {
    body.windows(2).position(|w| w == marker)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ami_record_offsets() {
        let rec = QuestionAmiRecord {
            question_id: 0x100,
            page_id: Some(5),
            access_level: 0x05,
            failsafe: 1,
            optimal: 0,
        };
        let buf = make_ami_record(&rec);
        assert_eq!(buf.len(), AMI_RECORD_SIZE);
        assert_eq!(u16::from_le_bytes([buf[0], buf[1]]), 0x100);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 5);
        assert_eq!(buf[32], 0x05);
        assert_eq!(buf[104], 1);
        assert_eq!(buf[106], 0);
    }

    #[test]
    fn ami_record_no_page_id() {
        let rec = QuestionAmiRecord {
            question_id: 0x200,
            page_id: None,
            access_level: 0x05,
            failsafe: 0,
            optimal: 1,
        };
        let buf = make_ami_record(&rec);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 0);
    }

    #[test]
    fn ami_record_all_zero_except_qid() {
        let rec = QuestionAmiRecord {
            question_id: 1,
            page_id: None,
            access_level: 0,
            failsafe: 0,
            optimal: 0,
        };
        let buf = make_ami_record(&rec);
        assert_eq!(buf[32], 0);
        assert_eq!(buf[50], 0);
        assert_eq!(buf[107], 0);
    }

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn ui_section(name: &str) -> FfsNode {
        let mut ucs2: Vec<u8> = Vec::new();
        for u in name.encode_utf16() {
            ucs2.extend_from_slice(&u.to_le_bytes());
        }
        ucs2.extend_from_slice(&[0, 0]);
        FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: crate::ffs::EFI_SECTION_UI,
            offset: 0,
            header: vec![],
            body: ucs2,
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn make_image_named(setupdata_body: Vec<u8>, amitse_body: Vec<u8>) -> Image {
        let setupdata = mk_node(FfsType::File, setupdata_body, vec![ui_section("setupdata")]);
        let amitse = mk_node(FfsType::File, amitse_body, vec![ui_section("AMITSE")]);
        let volume = mk_node(FfsType::Volume, vec![], vec![setupdata, amitse]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn patch_ami_appends_records_and_marks_rebuild() {
        let mut image = make_image_named(vec![0u8; AMI_RECORD_SIZE], vec![]);
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        let q = QuestionAmiRecord {
            question_id: 0x10,
            page_id: None,
            access_level: 0x05,
            failsafe: 1,
            optimal: 0,
        };
        patch_ami(&mut image, &formset_guid, &[], &[q], None, None).unwrap();
        let setupdata = &image.root.children[0].children[0];
        assert_eq!(setupdata.body.len(), 2 * AMI_RECORD_SIZE);
        assert_eq!(
            u16::from_le_bytes([
                setupdata.body[AMI_RECORD_SIZE],
                setupdata.body[AMI_RECORD_SIZE + 1]
            ]),
            0x10
        );
        assert_eq!(setupdata.action, Action::Rebuild);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
    }

    #[test]
    fn patch_ami_inserts_form_ids_in_order() {
        let mut image = make_image_named(vec![0u8; AMI_RECORD_SIZE], vec![0xA1, 0xB2, 0xC3, 0xD4]);
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        patch_ami(
            &mut image,
            &formset_guid,
            &[0x0001, 0x0002],
            &[],
            None,
            None,
        )
        .unwrap();
        let amitse = &image.root.children[0].children[1];
        assert_eq!(u16::from_le_bytes([amitse.body[4], amitse.body[5]]), 0x0001);
        assert_eq!(u16::from_le_bytes([amitse.body[6], amitse.body[7]]), 0x0002);
    }

    #[test]
    fn patch_ami_returns_error_when_files_not_found() {
        let other = mk_node(FfsType::File, vec![0xAB], vec![ui_section("Other")]);
        let volume = mk_node(FfsType::Volume, vec![], vec![other]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        assert!(matches!(
            patch_ami(&mut image, &formset_guid, &[], &[], None, None),
            Err(HiiError::AmiFilesNotFound)
        ));
    }
}
