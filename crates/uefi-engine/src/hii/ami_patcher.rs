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
    let (sd_path, am_path) = resolve_ami_payloads(image, setupdata_guid, amitse_guid)?;
    {
        let node = node_at_mut(&mut image.root, &sd_path);
        for q in questions {
            let record = make_ami_record(q);
            node.body.extend_from_slice(&record);
        }
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    {
        let node = node_at_mut(&mut image.root, &am_path);
        let formset_marker = &formset_guid.to_bytes()[12..14];
        let insert_pos =
            find_formset_marker_position(&node.body, formset_marker).unwrap_or(node.body.len());
        let mut entries = Vec::with_capacity(form_ids.len() * 2);
        for &fid in form_ids {
            entries.extend_from_slice(&fid.to_le_bytes());
        }
        node.body
            .splice(insert_pos..insert_pos, entries.iter().copied());
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &am_path);
    Ok(())
}

fn node_at_mut<'a>(root: &'a mut FfsNode, path: &[usize]) -> &'a mut FfsNode {
    let mut node = root;
    for &i in path {
        node = &mut node.children[i];
    }
    node
}

pub(crate) fn precheck_ami_modules(
    image: &Image,
    setupdata_guid: Option<&Guid>,
    amitse_guid: Option<&Guid>,
) -> Result<(), HiiError> {
    resolve_ami_payloads(image, setupdata_guid, amitse_guid)?;
    Ok(())
}

type PayloadPred = fn(&FfsNode) -> bool;

fn is_pfs_payload(node: &FfsNode) -> bool {
    node.node_type == FfsType::Section
        && node.children.is_empty()
        && node.body.len() >= 20
        && node.body[..node.body.len().min(0x40)]
            .windows(4)
            .any(|w| w == b"$SPF")
}

fn is_pe32_payload(node: &FfsNode) -> bool {
    node.node_type == FfsType::Section
        && node.children.is_empty()
        && node.subtype == crate::ffs::EFI_SECTION_PE32
}

fn resolve_ami_payloads(
    image: &Image,
    setupdata_guid: Option<&Guid>,
    amitse_guid: Option<&Guid>,
) -> Result<(Vec<usize>, Vec<usize>), HiiError> {
    let (sd_vi, sd_fi) = find_ami_module(image, setupdata_guid, "setupdata")?;
    let (am_vi, am_fi) = find_ami_module(image, amitse_guid, "AMITSE")?;
    let sd_rel = find_payload_path(&image.root.children[sd_vi].children[sd_fi], is_pfs_payload)?;
    let am_rel = find_payload_path(&image.root.children[am_vi].children[am_fi], is_pe32_payload)?;
    let sd_path = [vec![sd_vi, sd_fi], sd_rel].concat();
    let am_path = [vec![am_vi, am_fi], am_rel].concat();
    Ok((sd_path, am_path))
}

fn find_payload_path(file: &FfsNode, pred: PayloadPred) -> Result<Vec<usize>, HiiError> {
    let mut path = Vec::new();
    let mut blocked = false;
    let found = walk_for_payload(file, pred, false, &mut path, &mut blocked);
    match found {
        Some(p) => Ok(p),
        None if blocked => Err(HiiError::MutationBehindCompression),
        None => Err(HiiError::AmiFilesNotFound),
    }
}

fn walk_for_payload(
    node: &FfsNode,
    pred: PayloadPred,
    behind_bad_wrapper: bool,
    path: &mut Vec<usize>,
    skipped_behind_bad_wrapper: &mut bool,
) -> Option<Vec<usize>> {
    for (i, child) in node.children.iter().enumerate() {
        if pred(child) {
            if behind_bad_wrapper {
                *skipped_behind_bad_wrapper = true;
                continue;
            }
            path.push(i);
            return Some(path.clone());
        }
        path.push(i);
        if let Some(found) = walk_for_payload(
            child,
            pred,
            behind_bad_wrapper || super::formset_add::is_non_recompressable_wrapper(child),
            path,
            skipped_behind_bad_wrapper,
        ) {
            return Some(found);
        }
        path.pop();
    }
    None
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
    let matches = |n: &str| {
        n.eq_ignore_ascii_case(name)
            || (name == "setupdata" && n.eq_ignore_ascii_case("AMITSESetupData"))
    };
    children
        .iter()
        .any(|c| c.subtype == crate::ffs::EFI_SECTION_UI && matches(&ucs2_body_to_string(&c.body)))
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

    fn section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&crate::ffs::size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    fn ffs_file_bytes(guid: &Guid, content: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 24];
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = crate::ffs::EFI_FV_FILETYPE_RAW;
        buf[20..23].copy_from_slice(&crate::ffs::size_to_uint24((24 + content.len()) as u32));
        buf.extend_from_slice(content);
        buf
    }

    fn flash_with_files(files: Vec<Vec<u8>>) -> Vec<u8> {
        let mut body = Vec::new();
        for f in files {
            let aligned = (body.len() + 7) & !7;
            body.resize(aligned, 0xFF);
            body.extend_from_slice(&f);
        }
        body.extend_from_slice(&[0xFF; 1024]);
        let total = 56 + body.len();
        let mut buf = vec![0xFFu8; 32 + total];
        let fv = &mut buf[32..];
        fv[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        fv[40..44].copy_from_slice(&crate::ffs::EFI_FVH_SIGNATURE.to_le_bytes());
        fv[44..48].copy_from_slice(&crate::ffs::EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        fv[48..50].copy_from_slice(&56u16.to_le_bytes());
        fv[55] = 2;
        fv[56..].copy_from_slice(&body);
        buf
    }

    fn ami_flash_image() -> Vec<u8> {
        let mut pfs = vec![0u8; 16];
        pfs.extend_from_slice(b"$SPF");
        pfs.extend_from_slice(&[0u8; 8]);
        let sd = ffs_file_bytes(
            &Guid::try_parse(SETUPDATA_GUID_STR).unwrap(),
            &section_bytes(crate::ffs::EFI_SECTION_FREEFORM_SUBTYPE_GUID, &pfs),
        );
        let am = ffs_file_bytes(
            &Guid::try_parse(AMITSE_GUID_STR).unwrap(),
            &section_bytes(crate::ffs::EFI_SECTION_PE32, &[0x4Du8, 0x5A, 0x00, 0x00]),
        );
        flash_with_files(vec![sd, am])
    }

    fn find_leaf(node: &FfsNode, pred: impl Fn(&FfsNode) -> bool + Copy) -> Option<&FfsNode> {
        for c in &node.children {
            if pred(c) {
                return Some(c);
            }
            if let Some(found) = find_leaf(c, pred) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn patch_ami_records_survive_build_image() {
        let data = ami_flash_image();
        let mut img = crate::parser::image::parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        let q = QuestionAmiRecord {
            question_id: 0x123,
            page_id: Some(5),
            access_level: 0x05,
            failsafe: 1,
            optimal: 0,
        };
        let record = make_ami_record(&q);
        patch_ami(
            &mut img,
            &formset_guid,
            &[0x0001, 0x0002],
            &[q],
            Some(&sd),
            Some(&am),
        )
        .unwrap();
        let built = crate::builder::build_image(&img).unwrap();

        assert!(
            built.windows(record.len()).any(|w| w == record),
            "SDP record must be present in assembled bytes"
        );
        assert!(
            built.windows(4).any(|w| w == [0x01, 0x00, 0x02, 0x00]),
            "AMITSE form-id entries must be present in assembled bytes"
        );

        let re = crate::parser::image::parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = re
            .root
            .children
            .iter()
            .find_map(|vol| vol.children.iter().find(|f| f.guid == Some(sd)))
            .unwrap();
        let pfs = find_leaf(sd_file, |n| {
            n.children.is_empty() && n.body.windows(4).any(|w| w == b"$SPF")
        })
        .unwrap();
        assert_eq!(pfs.body.len(), 28 + AMI_RECORD_SIZE);
        assert_eq!(
            &pfs.body[pfs.body.len() - AMI_RECORD_SIZE..],
            record.as_slice()
        );
        let am_file = re
            .root
            .children
            .iter()
            .find_map(|vol| vol.children.iter().find(|f| f.guid == Some(am)))
            .unwrap();
        let pe32 = find_leaf(am_file, |n| {
            n.children.is_empty() && n.subtype == crate::ffs::EFI_SECTION_PE32
        })
        .unwrap();
        assert_eq!(
            pe32.body,
            vec![0x4Du8, 0x5A, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00]
        );
    }

    #[test]
    fn patch_ami_appends_records_into_pfs_section_and_marks_rebuild() {
        let mut image = ami_image(
            vec![pfs_section(), ui_section("AMITSESetupData")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        let q = QuestionAmiRecord {
            question_id: 0x10,
            page_id: None,
            access_level: 0x05,
            failsafe: 1,
            optimal: 0,
        };
        let record = make_ami_record(&q);
        let pfs_before = image.root.children[0].children[0].children[0].body.clone();
        let file_body_before = image.root.children[0].children[0].body.clone();
        patch_ami(&mut image, &formset_guid, &[], &[q], None, None).unwrap();
        let pfs = &image.root.children[0].children[0].children[0];
        assert_eq!(pfs.body.len(), pfs_before.len() + AMI_RECORD_SIZE);
        let tail = &pfs.body[pfs.body.len() - AMI_RECORD_SIZE..];
        assert_eq!(u16::from_le_bytes([tail[0], tail[1]]), 0x10);
        assert_eq!(tail, record.as_slice());
        assert_eq!(pfs.action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].body, file_body_before);
    }

    #[test]
    fn patch_ami_appends_form_ids_into_pe32_section() {
        let mut image = ami_image(
            vec![pfs_section(), ui_section("AMITSESetupData")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
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
        let pe32 = &image.root.children[0].children[1].children[0];
        assert_eq!(
            pe32.body,
            vec![0x4Du8, 0x5A, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00]
        );
        assert_eq!(pe32.action, Action::Rebuild);
    }

    #[test]
    fn patch_ami_splices_form_ids_at_formset_marker() {
        let mut image = ami_image(
            vec![pfs_section(), ui_section("AMITSESetupData")],
            vec![pe32_section(), ui_section("AMITSE")],
        );
        image.root.children[0].children[1].children[0].body = vec![0x4Du8, 0x5A, 0x34, 0x56, 0x00];
        let formset_guid: Guid = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".parse().unwrap();
        patch_ami(&mut image, &formset_guid, &[0x0003], &[], None, None).unwrap();
        let pe32 = &image.root.children[0].children[1].children[0];
        assert_eq!(pe32.body, vec![0x4Du8, 0x5A, 0x03, 0x00, 0x34, 0x56, 0x00]);
    }

    const SETUPDATA_GUID_STR: &str = "12345678-90AB-CDEF-1234-567890ABCDEF";
    const AMITSE_GUID_STR: &str = "87654321-FEDC-BA09-8765-432109FEDCBA";

    fn pfs_section() -> FfsNode {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(b"$SPF");
        body.extend_from_slice(&[0u8; 8]);
        let mut n = mk_node(FfsType::Section, body, vec![]);
        n.subtype = crate::ffs::EFI_SECTION_FREEFORM_SUBTYPE_GUID;
        n
    }

    fn pe32_section() -> FfsNode {
        let mut n = mk_node(FfsType::Section, vec![0x4Du8, 0x5A, 0, 0], vec![]);
        n.subtype = crate::ffs::EFI_SECTION_PE32;
        n
    }

    fn guided_wrapper(guid: Guid, children: Vec<FfsNode>) -> FfsNode {
        let mut n = mk_node(FfsType::Section, vec![], children);
        n.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
        n.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
            guid,
            dictionary_size: 0x0080_0000,
        });
        n
    }

    fn ami_image(setupdata_children: Vec<FfsNode>, amitse_children: Vec<FfsNode>) -> Image {
        let mut setupdata = mk_node(FfsType::File, vec![], setupdata_children);
        setupdata.guid = Some(Guid::try_parse(SETUPDATA_GUID_STR).unwrap());
        let mut amitse = mk_node(FfsType::File, vec![], amitse_children);
        amitse.guid = Some(Guid::try_parse(AMITSE_GUID_STR).unwrap());
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
    fn precheck_rejects_setupdata_without_pfs_payload() {
        let img = ami_image(vec![ui_section("AMITSESetupData")], vec![pe32_section()]);
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        assert!(matches!(
            precheck_ami_modules(&img, Some(&sd), Some(&am)),
            Err(HiiError::AmiFilesNotFound)
        ));
    }

    #[test]
    fn precheck_rejects_amitse_without_pe32_payload() {
        let img = ami_image(vec![pfs_section()], vec![ui_section("AMITSE")]);
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        assert!(matches!(
            precheck_ami_modules(&img, Some(&sd), Some(&am)),
            Err(HiiError::AmiFilesNotFound)
        ));
    }

    #[test]
    fn precheck_rejects_pfs_behind_tiano_wrapper() {
        let img = ami_image(
            vec![guided_wrapper(
                crate::ffs::tiano_guid(),
                vec![pfs_section()],
            )],
            vec![pe32_section()],
        );
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        assert!(matches!(
            precheck_ami_modules(&img, Some(&sd), Some(&am)),
            Err(HiiError::MutationBehindCompression)
        ));
    }

    #[test]
    fn precheck_rejects_pe32_behind_tiano_wrapper() {
        let img = ami_image(
            vec![pfs_section()],
            vec![guided_wrapper(
                crate::ffs::tiano_guid(),
                vec![pe32_section()],
            )],
        );
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        assert!(matches!(
            precheck_ami_modules(&img, Some(&sd), Some(&am)),
            Err(HiiError::MutationBehindCompression)
        ));
    }

    #[test]
    fn precheck_accepts_payloads_behind_lzma_wrapper() {
        let img = ami_image(
            vec![guided_wrapper(crate::ffs::lzma_guid(), vec![pfs_section()])],
            vec![guided_wrapper(
                crate::ffs::lzma_guid(),
                vec![pe32_section()],
            )],
        );
        let sd = Guid::try_parse(SETUPDATA_GUID_STR).unwrap();
        let am = Guid::try_parse(AMITSE_GUID_STR).unwrap();
        assert!(precheck_ami_modules(&img, Some(&sd), Some(&am)).is_ok());
    }

    #[test]
    fn find_ami_module_matches_hnx_ui_names() {
        let setupdata = mk_node(FfsType::File, vec![], vec![ui_section("AMITSESetupData")]);
        let amitse = mk_node(FfsType::File, vec![], vec![ui_section("AMITSE")]);
        let volume = mk_node(FfsType::Volume, vec![], vec![setupdata, amitse]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let img = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let (vi, fi) = find_ami_module(&img, None, "setupdata").unwrap();
        assert_eq!((vi, fi), (0, 0));
        let (vi, fi) = find_ami_module(&img, None, "AMITSE").unwrap();
        assert_eq!((vi, fi), (0, 1));
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
