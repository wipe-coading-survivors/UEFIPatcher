use std::collections::HashMap;

use r_efi::hii::IFR_FORM_OP;

use super::HiiError;
use super::ami_patcher;
use super::form_add;
use super::gates;
use super::ifr;
use super::ops;
use super::schema;
use super::spf;
use super::string_pack;
use super::values;
use crate::types::*;

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

pub fn rewrite_question_strings(pkg: &mut [u8], q_off: usize, prompt_id: u16, help_id: u16) {
    pkg[q_off + 2..q_off + 4].copy_from_slice(&prompt_id.to_le_bytes());
    pkg[q_off + 4..q_off + 6].copy_from_slice(&help_id.to_le_bytes());
}

pub struct HelpControlEdit {
    pub question_id: u16,
    pub offset: usize,
    pub old_string_id: u16,
    pub new_string_id: u16,
}

pub struct HelpRecordEdit {
    pub question_id: u16,
    pub record_offset: usize,
    pub old_string_id: u16,
    pub new_string_id: u16,
}

pub struct HijackResult {
    pub string_ids: HashMap<String, u16>,
    pub unlock_flips: Vec<String>,
    pub help_controls: Vec<HelpControlEdit>,
    pub help_records: Vec<HelpRecordEdit>,
    pub form_ifr_start: u32,
    pub form_ifr_end: u32,
}

#[tracing::instrument(level = "debug", skip(image, hijack), fields(item_id = %item_id), err)]
pub fn hijack_form(
    image: &mut Image,
    item_id: &str,
    hijack: &schema::HijackSchema,
    setupdata_guid: Option<&Guid>,
) -> Result<HijackResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target, form_id, question_id) = crate::hii::parse_item_id(item_id)?;
    if question_id.is_some() {
        return Err(HiiError::NotFound);
    }
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == crate::ffs::EFI_SECTION_COMPRESSION
                || ancestor.subtype == crate::ffs::EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &ancestor.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            )
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }

    let bare_channel = {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if node.subtype == crate::ffs::EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
            true
        } else if node.subtype == crate::ffs::EFI_SECTION_PE32
            && form_add::resource_forms_package(&node.body).is_some()
        {
            false
        } else {
            return Err(HiiError::NotASetupItem);
        }
    };

    let span = {
        let pkg = package_of(&image.root, &target)?;
        locate_form(pkg, form_id).ok_or(HiiError::NotFound)?
    };
    let form_ifr_start = span.form_op as u32;
    let form_ifr_end = span.next_form_op as u32;
    {
        let pkg = package_of(&image.root, &target)?;
        let qs = locate_questions(pkg, form_id);
        for q in &hijack.questions {
            if !qs.iter().any(|&(_, qid)| qid == q.question_id) {
                return Err(HiiError::NotFound);
            }
        }
    }

    let unlock_flips: Vec<gates::PlannedFlip> = {
        let pkg = package_of(&image.root, &target)?;
        let mut targets: Vec<gates::GateTarget> = vec![gates::GateTarget {
            form_id,
            question_id: None,
        }];
        targets.extend(hijack.questions.iter().map(|q| gates::GateTarget {
            form_id,
            question_id: Some(q.question_id),
        }));
        let mut merged: Vec<gates::PlannedFlip> = Vec::new();
        for gt in &targets {
            let found = gates::find_gates(pkg, gt);
            if found.is_empty() {
                continue;
            }
            let flips = gates::plan_gates_skip_unlocked(pkg, &found)
                .map_err(HiiError::GateExpressionUnsupported)?;
            for f in flips {
                if !merged.iter().any(|m| m.offset == f.offset) {
                    merged.push(f);
                }
            }
        }
        merged
    };

    let sd_path = ami_patcher::pfs_payload_path(image, setupdata_guid)?;
    let spf_records = {
        let node = node_at(&image.root, &sd_path);
        node.body.clone()
    };
    let record_edits: Vec<(u16, usize, u16)> = {
        let records = spf::scan_question_records(&spf_records);
        let mut out = Vec::new();
        for q in &hijack.questions {
            let matched: Vec<_> = records
                .iter()
                .filter(|r| {
                    r.question_id == q.question_id
                        && r.ifr_offset >= form_ifr_start
                        && r.ifr_offset < form_ifr_end
                })
                .collect();
            if matched.len() != 1 {
                return Err(HiiError::NotFound);
            }
            let off = matched[0].offset;
            out.push((
                q.question_id,
                off,
                u16::from_le_bytes([
                    spf_records[off + spf::SPF_RECORD_HELP_ID],
                    spf_records[off + spf::SPF_RECORD_HELP_ID + 1],
                ]),
            ));
        }
        out
    };

    let help_ids: Vec<(u16, u16)> = {
        let pkg = package_of(&image.root, &target)?;
        let qs = locate_questions(pkg, form_id);
        let mut out = Vec::new();
        for q in &hijack.questions {
            let off = qs
                .iter()
                .find(|&&(_, qid)| qid == q.question_id)
                .map(|&(o, _)| o)
                .ok_or(HiiError::NotFound)?;
            out.push((
                q.question_id,
                u16::from_le_bytes([pkg[off + 4], pkg[off + 5]]),
            ));
        }
        out
    };
    let control_edits: Vec<(u16, usize, u16)> = {
        let controls = spf::scan_string_controls(&spf_records);
        let mut out = Vec::new();
        for &(qid, sid) in &help_ids {
            let matched: Vec<_> = controls.iter().filter(|c| c.string_id == sid).collect();
            if matched.len() != 1 {
                return Err(HiiError::NotFound);
            }
            out.push((qid, matched[0].offset, sid));
        }
        out
    };

    let mut strings: Vec<String> = Vec::with_capacity(hijack.questions.len() * 2);
    for q in &hijack.questions {
        strings.push(q.prompt.clone());
        strings.push(q.help.clone());
    }
    strings.dedup();
    let string_ids = if bare_channel {
        let owner = form_add::owner_guid_by_path(&image.root, &path);
        string_pack::add_strings(image, owner.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        match string_pack::add_strings_to_resource(&mut node.body, &strings) {
            Ok(ids) => ids,
            Err(string_pack::AddStringsToResourceError::NotFound) => {
                return Err(HiiError::StringPackageNotFound);
            }
            Err(string_pack::AddStringsToResourceError::GrowthUnsupported) => {
                return Err(HiiError::PeGrowthUnsupported);
            }
        }
    };

    {
        let root = &mut image.root;
        let pkg = package_of_mut(root, &target)?;
        apply_hijack_ifr(pkg, form_id, hijack, &string_ids)?;
        gates::apply_flips(pkg, &unlock_flips).map_err(HiiError::GateExpressionUnsupported)?;
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);

    let (help_controls, help_records): (Vec<HelpControlEdit>, Vec<HelpRecordEdit>) = {
        let node = node_at_mut(&mut image.root, &sd_path);
        let mut ctrls = Vec::new();
        let mut recs = Vec::new();
        for (&(qid, off, old), &(_, rec_off, rec_old)) in
            control_edits.iter().zip(record_edits.iter())
        {
            let help = &hijack
                .questions
                .iter()
                .find(|q| q.question_id == qid)
                .unwrap()
                .help;
            let new_id = *string_ids.get(help).ok_or(HiiError::InvalidIfr)?;
            spf::write_record_help_id(&mut node.body, rec_off, new_id);
            spf::write_string_control(&mut node.body, off, new_id);
            recs.push(HelpRecordEdit {
                question_id: qid,
                record_offset: rec_off,
                old_string_id: rec_old,
                new_string_id: new_id,
            });
            ctrls.push(HelpControlEdit {
                question_id: qid,
                offset: off,
                old_string_id: old,
                new_string_id: new_id,
            });
        }
        (ctrls, recs)
    };
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);

    tracing::warn!(flips = unlock_flips.len(), "hijack auto-unlock applied");
    tracing::debug!(questions = hijack.questions.len(), "hijack_form done");
    Ok(HijackResult {
        string_ids,
        unlock_flips: unlock_flips
            .iter()
            .map(|f| crate::hii::flip_text(0, f))
            .collect(),
        help_controls,
        help_records,
        form_ifr_start,
        form_ifr_end,
    })
}

fn package_of<'a>(root: &'a FfsNode, target: &crate::types::Target) -> Result<&'a [u8], HiiError> {
    let node = crate::parser::target::find_item(root, target).map_err(|_| HiiError::NotFound)?;
    if node.subtype == crate::ffs::EFI_SECTION_RAW {
        Ok(&node.body)
    } else {
        let (off, len) =
            form_add::resource_forms_package(&node.body).ok_or(HiiError::NotASetupItem)?;
        node.body.get(off..off + len).ok_or(HiiError::InvalidIfr)
    }
}

fn package_of_mut<'a>(
    root: &'a mut FfsNode,
    target: &crate::types::Target,
) -> Result<&'a mut [u8], HiiError> {
    let node =
        crate::parser::target::find_item_mut(root, target).map_err(|_| HiiError::NotFound)?;
    if node.subtype == crate::ffs::EFI_SECTION_RAW {
        Ok(&mut node.body)
    } else {
        let (off, len) =
            form_add::resource_forms_package(&node.body).ok_or(HiiError::NotASetupItem)?;
        let end = off.checked_add(len).ok_or(HiiError::InvalidIfr)?;
        node.body.get_mut(off..end).ok_or(HiiError::InvalidIfr)
    }
}

fn apply_hijack_ifr(
    pkg: &mut [u8],
    form_id: u16,
    hijack: &schema::HijackSchema,
    string_ids: &HashMap<String, u16>,
) -> Result<(), HiiError> {
    let qs = locate_questions(pkg, form_id);
    for q in &hijack.questions {
        let prompt_id = *string_ids.get(&q.prompt).ok_or(HiiError::InvalidIfr)?;
        let help_id = *string_ids.get(&q.help).ok_or(HiiError::InvalidIfr)?;
        let q_off = qs
            .iter()
            .find(|&&(_, qid)| qid == q.question_id)
            .map(|&(off, _)| off)
            .ok_or(HiiError::NotFound)?;
        rewrite_question_strings(pkg, q_off, prompt_id, help_id);
    }
    Ok(())
}

fn node_at<'a>(root: &'a FfsNode, path: &[usize]) -> &'a FfsNode {
    let mut node = root;
    for &i in path {
        node = &node.children[i];
    }
    node
}

fn node_at_mut<'a>(root: &'a mut FfsNode, path: &[usize]) -> &'a mut FfsNode {
    let mut node = root;
    for &i in path {
        node = &mut node.children[i];
    }
    node
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use super::locate_questions;
    use crate::ffs::{
        EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE, EFI_SECTION_FREEFORM_SUBTYPE_GUID,
        EFI_SECTION_RAW, EFI_SECTION_UI, size_to_uint24,
    };
    use crate::hii::ifr_builder::IfrBuilder;
    use crate::hii::spf;
    use crate::types::Guid;
    use std::str::FromStr;

    pub(crate) const FORMSET_GUID: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";
    pub(crate) const FILE_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    pub(crate) const SETUPDATA_GUID_STR: &str = "12345678-90AB-CDEF-1234-567890ABCDEF";

    pub(crate) fn package_with_form() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        b.emit_form_set(&Guid::from_str(FORMSET_GUID).unwrap(), 1, 1, &[]);
        b.emit_form(7, 1);
        b.emit_one_of(2, 0, 0x11, 1, 0, 0, 1);
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

    pub(crate) fn section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    pub(crate) fn ffs_file_bytes(guid: &Guid, content: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 24];
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = crate::ffs::EFI_FV_FILETYPE_RAW;
        buf[20..23].copy_from_slice(&size_to_uint24((24 + content.len()) as u32));
        buf.extend_from_slice(content);
        buf
    }

    pub(crate) fn flash_with_files(files: Vec<Vec<u8>>) -> Vec<u8> {
        let mut body = Vec::new();
        for f in files {
            let aligned = (body.len() + 7) & !7;
            body.resize(aligned, 0xFF);
            body.extend_from_slice(&f);
        }
        body.extend_from_slice(&[0xFF; 2048]);
        let total = 56 + body.len();
        let mut buf = vec![0xFFu8; 32 + total];
        let fv = &mut buf[32..];
        fv[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        fv[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        fv[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        fv[48..50].copy_from_slice(&56u16.to_le_bytes());
        fv[55] = 2;
        fv[56..].copy_from_slice(&body);
        buf
    }

    pub(crate) fn string_package_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, r_efi::hii::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x10);
        buf.extend_from_slice(b"first");
        buf.push(0x00);
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    pub(crate) fn spf_container(records: &[spf::SpfQuestionRecord], body_len: usize) -> Vec<u8> {
        let mut body = vec![0u8; body_len];
        body[16..20].copy_from_slice(b"$SPF");
        for r in records {
            let mut rec = vec![0u8; spf::SPF_RECORD_SIZE];
            rec[0..4].copy_from_slice(&(r.question_id as u32).to_le_bytes());
            rec[8..10].copy_from_slice(&6u16.to_le_bytes());
            rec[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
            rec[16] = 0x09;
            rec[28..32].copy_from_slice(&r.ifr_offset.to_le_bytes());
            rec[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
            rec[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
            rec[52] = r.failsafe;
            rec[53] = r.optimal;
            body[r.offset..r.offset + spf::SPF_RECORD_SIZE].copy_from_slice(&rec);
        }
        let ctrl = 0x100 + records.len() * spf::SPF_RECORD_SIZE;
        body[ctrl..ctrl + 2].copy_from_slice(&5u16.to_le_bytes());
        body[ctrl + 4..ctrl + 6].copy_from_slice(&0u16.to_le_bytes());
        body[ctrl + 10..ctrl + 12].copy_from_slice(&78u16.to_le_bytes());
        body[ctrl + 0x14..ctrl + 0x18].copy_from_slice(&57u32.to_le_bytes());
        body
    }

    pub(crate) fn hijack_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = package_with_form();
        let qs = locate_questions(&pkg, 7);
        let records: Vec<spf::SpfQuestionRecord> = qs
            .iter()
            .enumerate()
            .map(|(i, &(off, qid))| spf::SpfQuestionRecord {
                offset: 0x100 + i * spf::SPF_RECORD_SIZE,
                question_id: qid,
                ifr_offset: off as u32,
                failsafe: 0,
                optimal: 0,
            })
            .collect();
        let spf_body = spf_container(&records, 0x400);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_UI, &ui_name("AMITSESetupData")),
                section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
            ]),
        );
        let flash = flash_with_files(vec![setup, sd]);
        (flash, pkg, spf_body)
    }

    pub(crate) fn file_sections(sections: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        for s in sections {
            out.extend_from_slice(s);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        }
        out
    }

    pub(crate) fn ui_name(name: &str) -> Vec<u8> {
        name.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .chain(0u16.to_le_bytes())
            .collect()
    }

    pub(crate) fn hijack_test_flash() -> Vec<u8> {
        hijack_flash_image().0
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::*;
    use super::*;
    use crate::builder::build_image;
    use crate::ffs::{EFI_SECTION_FREEFORM_SUBTYPE_GUID, EFI_SECTION_RAW, EFI_SECTION_UI};
    use crate::hii::spf;
    use crate::parser::image::parse_image;
    use crate::types::Guid;
    use r_efi::hii::IFR_ONE_OF_OP;
    use std::str::FromStr;

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
    fn rewrite_question_strings_same_length() {
        let mut pkg = package_with_form();
        let q = locate_questions(&pkg, 7)[0].0;
        let before = pkg.clone();
        rewrite_question_strings(&mut pkg, q, 0x1111, 0x2222);
        assert_eq!(pkg.len(), before.len());
        assert_eq!(&pkg[q + 2..q + 4], &[0x11, 0x11]);
        assert_eq!(&pkg[q + 4..q + 6], &[0x22, 0x22]);
        for i in 0..pkg.len() {
            let touched = i >= q + 2 && i < q + 6;
            if !touched {
                assert_eq!(pkg[i], before[i]);
            }
        }
    }

    fn hijack_schema() -> crate::hii::schema::HijackSchema {
        crate::hii::schema::parse_hijack_schema(
            r#"{"questions": [{"question_id": 17, "prompt": "PQ", "help": "PH"}]}"#,
        )
        .unwrap()
    }

    const ITEM: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7";

    #[test]
    fn hijack_form_rewrites_ifr_and_leaves_spf_defaults_untouched() {
        let (flash, pkg_before, spf_before) = hijack_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let res = hijack_form(&mut img, ITEM, &hijack_schema(), None).unwrap();
        assert_eq!(res.string_ids.len(), 2);
        assert!(res.string_ids.contains_key("PH"));
        assert!(res.unlock_flips.is_empty());
        let built = build_image(&img).unwrap();

        let reparsed = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = reparsed
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = sd_file
            .children
            .iter()
            .find(|c| c.body.windows(4).any(|w| w == b"$SPF"))
            .unwrap();
        assert_eq!(
            pfs_leaf.body.len(),
            spf_before.len(),
            "$SPF length invariant"
        );
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].question_id, 17);
        assert_eq!(recs[0].failsafe, 0, "fs must not be written");
        assert_eq!(recs[0].optimal, 0, "opt must not be written");

        let pkg_after = form_pkg_bytes(&reparsed);
        let span_before = locate_form(&pkg_before, 7).unwrap();
        let span_after = locate_form(&pkg_after, 7).unwrap();
        assert_eq!(
            pkg_after[span_after.form_op + 4..span_after.form_op + 6],
            pkg_before[span_before.form_op + 4..span_before.form_op + 6],
            "IFR form title bytes must stay untouched"
        );
        assert_eq!(
            pkg_before.len(),
            pkg_after.len(),
            "form package length invariant"
        );
    }

    fn form_pkg_bytes(img: &Image) -> Vec<u8> {
        let node = crate::parser::target::find_item(
            &img.root,
            &crate::parser::target::parse_target(&format!("{FILE_GUID}:0x19:0")).unwrap(),
        )
        .unwrap();
        node.body.clone()
    }

    fn hii_list_blob(guid: &Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    #[test]
    fn hijack_form_pe32_resource_channel_appends_strings_to_resource() {
        let pkg = package_with_form();
        let qs = locate_questions(&pkg, 7);
        let records: Vec<spf::SpfQuestionRecord> = qs
            .iter()
            .enumerate()
            .map(|(i, &(off, qid))| spf::SpfQuestionRecord {
                offset: 0x100 + i * spf::SPF_RECORD_SIZE,
                question_id: qid,
                ifr_offset: off as u32,
                failsafe: 0,
                optimal: 0,
            })
            .collect();
        let spf_body = spf_container(&records, 0x400);
        let blob = hii_list_blob(
            &Guid::from_str(FILE_GUID).unwrap(),
            &[&pkg, &string_package_bytes()],
        );
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(crate::ffs::EFI_SECTION_PE32, &pe),
        );
        let sd = ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &file_sections(&[
                section_bytes(crate::ffs::EFI_SECTION_UI, &ui_name("AMITSESetupData")),
                section_bytes(crate::ffs::EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
            ]),
        );
        let data = flash_with_files(vec![setup, sd]);

        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let item = format!("{FILE_GUID}:0x10:0#7");
        let res =
            hijack_form(&mut img, &item, &hijack_schema(), None).expect("PE32 resource hijack");
        assert_eq!(res.string_ids.len(), 2);
        assert!(res.string_ids.contains_key("PH"));

        let built = build_image(&img).unwrap();
        assert_eq!(built.len(), data.len());

        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let pe_target = crate::parser::target::parse_target(&format!("{FILE_GUID}:0x10:0"))
            .expect("pe32 target");
        let pe_node = crate::parser::target::find_item(&re.root, &pe_target).unwrap();
        assert!(
            pe_node.body.windows(2).any(|w| w == b"PH"),
            "appended strings must land inside the PE resource"
        );
        assert!(
            pe_node.body.windows(2).any(|w| w == b"PQ"),
            "appended prompt must land inside the PE resource"
        );

        let sd_file = re
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = find_spf_leaf(sd_file).unwrap();
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].question_id, 17);
        assert_eq!(recs[0].failsafe, 0, "fs must not be written");
        assert_eq!(recs[0].optimal, 0, "opt must not be written");

        let pkg_after = package_of(&re.root, &pe_target).expect("forms package in PE resource");
        let span_before = locate_form(&pkg, 7).unwrap();
        let span_after = locate_form(pkg_after, 7).unwrap();
        assert_eq!(
            pkg_after[span_after.form_op + 4..span_after.form_op + 6],
            pkg[span_before.form_op + 4..span_before.form_op + 6],
            "IFR form title bytes must stay untouched"
        );
    }

    fn lzma_guided_file_bytes(guid: &Guid, children: &[u8]) -> (Vec<u8>, usize) {
        let mut stream = crate::compress::compress_lzma(children).unwrap();
        stream.resize(stream.len().max(64) + 32, 0x00);
        let mut body = crate::ffs::lzma_guid().to_bytes().to_vec();
        body.extend_from_slice(&0x18u16.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&stream);
        let sec = section_bytes(crate::ffs::EFI_SECTION_GUID_DEFINED, &body);
        let slot_len = stream.len();
        (ffs_file_bytes(guid, &sec), slot_len)
    }

    #[test]
    fn hijack_form_survives_lzma_slot_fit() {
        let (_, pkg, spf_body) = hijack_flash_image();
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = lzma_guided_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
        );
        let data = flash_with_files(vec![sd.0.clone(), setup]);
        let sd_start = data
            .windows(16)
            .position(|w| w == sd_guid_bytes())
            .expect("sd file header at slot start");
        let sd_slot = (sd_start, sd_start + sd.0.len());
        let setup_start = (sd_slot.1 + 7) & !7;

        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        hijack_form(
            &mut img,
            ITEM,
            &hijack_schema(),
            Some(&Guid::from_str(SETUPDATA_GUID_STR).unwrap()),
        )
        .unwrap();
        let built = build_image(&img).unwrap();
        assert_eq!(
            built.len(),
            data.len(),
            "slot-fit must preserve total length"
        );
        assert_eq!(
            &built[..sd_start],
            &data[..sd_start],
            "bytes before the $SPF LZMA slot must not change"
        );

        for (i, (a, b)) in data.iter().zip(built.iter()).enumerate() {
            if a != b {
                assert!(
                    (i >= sd_slot.0 && i < sd_slot.1) || i >= setup_start,
                    "byte {i:#x} changed outside the $SPF LZMA slot and the setup module slot"
                );
            }
        }

        let reparsed = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = reparsed
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = find_spf_leaf(sd_file).unwrap();
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs[0].failsafe, 0, "fs must stay untouched");
        assert_eq!(recs[0].optimal, 0);
    }

    fn sd_guid_bytes() -> [u8; 16] {
        Guid::from_str(SETUPDATA_GUID_STR).unwrap().to_bytes()
    }

    fn find_spf_leaf<'a>(node: &'a FfsNode) -> Option<&'a FfsNode> {
        if node.children.is_empty() && node.body.windows(4).any(|w| w == b"$SPF") {
            return Some(node);
        }
        node.children.iter().find_map(find_spf_leaf)
    }

    #[test]
    fn hijack_form_atomic_precheck_failures() {
        for (item, qid) in [
            ("5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#99", 17u16),
            (ITEM, 99),
        ] {
            let (flash, _, _) = hijack_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = build_image(&img).unwrap();
            let mut sc = hijack_schema();
            sc.questions[0].question_id = qid;
            assert!(
                hijack_form(&mut img, item, &sc, None).is_err(),
                "must reject {item}"
            );
            assert_eq!(
                build_image(&img).unwrap(),
                before,
                "no mutation on precheck failure"
            );
        }

        let (flash, _, _) = hijack_flash_image();
        let mut img2 = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let sd_guid = Guid::from_str(SETUPDATA_GUID_STR).unwrap();
        let vol = img2
            .root
            .children
            .iter_mut()
            .find(|v| v.children.iter().any(|f| f.guid == Some(sd_guid)))
            .unwrap();
        vol.children.retain(|f| f.guid != Some(sd_guid));
        assert!(
            hijack_form(
                &mut img2,
                ITEM,
                &hijack_schema(),
                Some(&Guid::from_str("00000000-0000-0000-0000-00000000DEAD").unwrap())
            )
            .is_err(),
            "missing $SPF file"
        );
    }

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn gated_pkg() -> Vec<u8> {
        use r_efi::hii::{
            IFR_EQ_ID_VAL_OP, IFR_EQUAL_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_GRAY_OUT_IF_OP,
            IFR_ONE_OF_OP, IFR_SUPPRESS_IF_OP, IFR_UINT64_OP, PACKAGE_FORMS,
        };
        let form_set = {
            let mut p = Vec::new();
            p.extend_from_slice(&Guid::from_str(FORMSET_GUID).unwrap().to_bytes());
            p.extend_from_slice(&7u16.to_le_bytes());
            p.extend_from_slice(&0u16.to_le_bytes());
            p.push(0u8);
            opcode(IFR_FORM_SET_OP, true, &p)
        };
        let uint64 = |v: u64| {
            let mut b = vec![IFR_UINT64_OP, 0x0A];
            b.extend_from_slice(&v.to_le_bytes());
            b
        };
        let equal = || vec![IFR_EQUAL_OP, 0x02];
        let eq_id_val = |qid: u16, val: u16| {
            let mut b = vec![IFR_EQ_ID_VAL_OP, 0x06];
            b.extend_from_slice(&qid.to_le_bytes());
            b.extend_from_slice(&val.to_le_bytes());
            b
        };
        let ref_op = |form_id: u16| {
            let mut v = vec![r_efi::hii::IFR_REF_OP, 0x0F];
            v.extend_from_slice(&[0u8; 11]);
            v.extend_from_slice(&form_id.to_le_bytes());
            v
        };
        let mut ifr = form_set;
        ifr.extend(opcode(
            IFR_FORM_OP,
            true,
            &[6u16.to_le_bytes(), 1u16.to_le_bytes()].concat(),
        ));
        ifr.extend(opcode(IFR_SUPPRESS_IF_OP, true, &[]));
        ifr.extend(uint64(1));
        ifr.extend(uint64(1));
        ifr.extend(equal());
        ifr.extend(ref_op(7));
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend(opcode(
            IFR_FORM_OP,
            true,
            &[7u16.to_le_bytes(), 1u16.to_le_bytes()].concat(),
        ));
        ifr.extend(opcode(IFR_GRAY_OUT_IF_OP, true, &[]));
        ifr.extend(eq_id_val(0x9A, 1));
        ifr.extend(opcode(IFR_ONE_OF_OP, true, &{
            let mut p = vec![0u8; 11];
            p[4..6].copy_from_slice(&0x11u16.to_le_bytes());
            p
        }));
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        ifr.extend(vec![r_efi::hii::IFR_END_OP, 0x02]);
        let total = 4 + ifr.len();
        let mut pkg = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            PACKAGE_FORMS,
        ];
        pkg.extend_from_slice(&ifr);
        pkg
    }

    fn gated_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = gated_pkg();
        let qs = locate_questions(&pkg, 7);
        let records: Vec<spf::SpfQuestionRecord> = qs
            .iter()
            .enumerate()
            .map(|(i, &(off, qid))| spf::SpfQuestionRecord {
                offset: 0x100 + i * spf::SPF_RECORD_SIZE,
                question_id: qid,
                ifr_offset: off as u32,
                failsafe: 0,
                optimal: 0,
            })
            .collect();
        let spf_body = spf_container(&records, 0x400);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_UI, &ui_name("AMITSESetupData")),
                section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, &spf_body),
            ]),
        );
        let flash = flash_with_files(vec![setup, sd]);
        (flash, pkg, spf_body)
    }

    const GATED_ITEM: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#7";

    #[test]
    fn hijack_auto_unlocks_victim_gates() {
        let (flash, pkg_before, _) = gated_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let res = hijack_form(&mut img, GATED_ITEM, &hijack_schema(), None).unwrap();
        assert_eq!(res.unlock_flips.len(), 2, "REF suppress + question grayout");

        let built = build_image(&img).unwrap();
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let node = crate::parser::target::find_item(
            &re.root,
            &crate::parser::target::parse_target(&format!("{FILE_GUID}:0x19:0")).unwrap(),
        )
        .unwrap();
        assert_eq!(node.body.len(), pkg_before.len());
        let after = &node.body;
        let mut diffs: Vec<usize> = Vec::new();
        for i in 0..pkg_before.len() {
            if pkg_before[i] != after[i] {
                diffs.push(i);
            }
        }
        let suppressed_const = {
            let needle = {
                let mut n = vec![r_efi::hii::IFR_UINT64_OP, 0x0A];
                n.extend_from_slice(&1u64.to_le_bytes());
                n
            };
            let mut hits = pkg_before
                .windows(needle.len())
                .enumerate()
                .filter(|(_, w)| *w == &needle[..]);
            let first = hits.next().unwrap().0;
            let second = hits.next().unwrap().0;
            assert_eq!(second, first + 10, "two adjacent uint64(1) operands");
            second + 2
        };
        assert_eq!(pkg_before[suppressed_const], 1);
        assert_eq!(after[suppressed_const], 2);
        let grayout_const = after
            .windows(6)
            .position(|w| {
                w[0] == 0x12 && w[1] == 0x06 && w[2..4] == [0x9A, 0] && w[4..6] == [0xFF, 0xFF]
            })
            .unwrap();
        assert_eq!(pkg_before[grayout_const + 4], 1);
    }

    #[test]
    fn hijack_keeps_atomicity_on_schema_refusal() {
        let (flash, _, _) = gated_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let before = build_image(&img).unwrap();
        assert!(
            hijack_form(
                &mut img,
                GATED_ITEM,
                &{
                    let mut sc = hijack_schema();
                    sc.questions[0].question_id = 99;
                    sc
                },
                None
            )
            .is_err()
        );
        assert_eq!(
            build_image(&img).unwrap(),
            before,
            "no mutation on precheck failure"
        );
    }

    #[test]
    fn hijack_rewrites_help_control() {
        let (flash, _, spf_before) = hijack_flash_image();
        let rec_before = spf::scan_question_records(&spf_before)[0];
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let res = hijack_form(&mut img, ITEM, &hijack_schema(), None).unwrap();
        assert_eq!(res.help_controls.len(), 1);
        assert_eq!(res.help_controls[0].question_id, 17);
        assert_eq!(res.help_controls[0].old_string_id, 0);
        let new_help = *res.string_ids.get("PH").unwrap();
        assert_eq!(res.help_controls[0].new_string_id, new_help);
        assert_eq!(res.help_records.len(), 1);
        assert_eq!(res.help_records[0].question_id, 17);
        assert_eq!(res.help_records[0].record_offset, rec_before.offset);
        assert_eq!(res.help_records[0].old_string_id, 0);
        assert_eq!(res.help_records[0].new_string_id, new_help);

        let built = build_image(&img).unwrap();
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let sd_file = re
            .root
            .children
            .iter()
            .find_map(|v| {
                v.children
                    .iter()
                    .find(|f| f.guid == Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap()))
            })
            .unwrap();
        let pfs_leaf = sd_file
            .children
            .iter()
            .find(|c| c.body.windows(4).any(|w| w == b"$SPF"))
            .unwrap();
        assert_eq!(
            pfs_leaf.body.len(),
            spf_before.len(),
            "$SPF length invariant"
        );
        let ctrls = spf::scan_string_controls(&pfs_leaf.body);
        assert_eq!(ctrls.len(), 1);
        assert_eq!(ctrls[0].string_id, new_help);
        assert_eq!(ctrls[0].offset, res.help_controls[0].offset);
        let recs = spf::scan_question_records(&pfs_leaf.body);
        assert_eq!(recs.len(), 1);
        let s14 = u16::from_le_bytes([
            pfs_leaf.body[recs[0].offset + spf::SPF_RECORD_HELP_ID],
            pfs_leaf.body[recs[0].offset + spf::SPF_RECORD_HELP_ID + 1],
        ]);
        assert_eq!(s14, new_help, "record s14 must carry the new help id");
        assert_eq!(recs[0].offset, res.help_records[0].record_offset);
    }

    #[test]
    fn hijack_fails_when_help_control_missing_or_ambiguous() {
        for dup in [false, true] {
            let (flash, _, _) = hijack_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let sd_guid = Guid::from_str(SETUPDATA_GUID_STR).unwrap();
            let vol = img
                .root
                .children
                .iter_mut()
                .find(|v| v.children.iter().any(|f| f.guid == Some(sd_guid)))
                .unwrap();
            let file = vol
                .children
                .iter_mut()
                .find(|f| f.guid == Some(sd_guid))
                .unwrap();
            let leaf = file
                .children
                .iter_mut()
                .find(|c| c.body.windows(4).any(|w| w == b"$SPF"))
                .unwrap();
            let first = spf::scan_string_controls(&leaf.body)[0].offset;
            if dup {
                let second = 0x300;
                leaf.body[second..second + 2].copy_from_slice(&5u16.to_le_bytes());
                leaf.body[second + 4..second + 6].copy_from_slice(&0u16.to_le_bytes());
                leaf.body[second + 10..second + 12].copy_from_slice(&78u16.to_le_bytes());
            } else {
                leaf.body[first - 2..first].copy_from_slice(&7u16.to_le_bytes());
            }
            let before = build_image(&img).unwrap();
            assert!(
                hijack_form(&mut img, ITEM, &hijack_schema(), None).is_err(),
                "dup={dup}"
            );
            assert_eq!(
                build_image(&img).unwrap(),
                before,
                "atomic: no mutation beyond fixture corruption"
            );
        }
    }

    #[test]
    fn hijack_idempotent_rerun() {
        let (flash, _, _) = hijack_flash_image();
        let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
        let res1 = hijack_form(&mut img, ITEM, &hijack_schema(), None).unwrap();
        let res2 = hijack_form(&mut img, ITEM, &hijack_schema(), None).unwrap();
        assert!(res2.unlock_flips.is_empty(), "gates already unlocked");
        assert_eq!(res2.help_controls.len(), 1);
        assert_eq!(
            res2.help_controls[0].old_string_id,
            res1.help_controls[0].new_string_id
        );
        assert_eq!(res2.help_records.len(), 1);
        assert_eq!(
            res2.help_records[0].old_string_id,
            res1.help_records[0].new_string_id
        );
    }
}
