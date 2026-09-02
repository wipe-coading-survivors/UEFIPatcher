pub mod ami_patcher;
pub mod ffs_assembler;
pub mod form_add;
pub mod forms;
pub mod formset_add;
pub mod ifr;
pub mod ifr_builder;
pub mod package_list;
pub mod pe_resource;
pub mod schema;
pub mod string_pack;
pub mod strings;

use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::ops;
use crate::types::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HiiError {
    #[error("not found")]
    NotFound,
    #[error("not a setup item")]
    NotASetupItem,
    #[error("invalid IFR")]
    InvalidIfr,
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    #[error("string package not found")]
    StringPackageNotFound,
    #[error("string id {0} is already occupied")]
    IdOccupied(u16),
    #[error("AMI files not found (setupdataBin/amitseSct)")]
    AmiFilesNotFound,
    #[error("IFR build error: {0}")]
    IfrBuildError(String),
    #[error("FFS assembly error: {0}")]
    FfsAssemblyError(String),
    #[error("image is not writable (open in Write mode first)")]
    NotWritable,
    #[error(
        "target is behind a compressed/guided section that cannot be recompressed (Tiano, LZMAF86, standard compression, unknown GUID)"
    )]
    MutationBehindCompression,
    #[error("cannot grow PE resource section")]
    PeGrowthUnsupported,
    #[error("PRC token patch unsupported for this target")]
    PrcPatchUnsupported,
    #[error("unsupported SIBT block 0x{0:02x} with pending inserts")]
    SibtBlockUnsupported(u8),
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, visible), err)]
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target_str, form_id) = match item_id.rsplit_once('#') {
        Some((t, f)) => match f.parse::<u16>() {
            Ok(id) => (t, Some(id)),
            Err(_) => return Err(HiiError::NotFound),
        },
        None => (item_id, None),
    };
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == EFI_SECTION_COMPRESSION
                || ancestor.subtype == EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &ancestor.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            )
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }
    let mut changed = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if node.subtype == EFI_SECTION_RAW || ifr::is_form_package(&node.body) {
            if visible {
                let scope = match form_id {
                    Some(fid) => ifr::find_form_suppress_scope(&node.body, fid),
                    None => ifr::find_suppress_if_scopes(&node.body).into_iter().next(),
                };
                if let Some(scope) = scope {
                    ifr::unsuppress(&mut node.body, &scope);
                    changed = true;
                }
            }
        } else if node.subtype == EFI_SECTION_PE32 {
            let Some(packages) = pe_resource_form_packages(&node.body) else {
                return Err(HiiError::NotASetupItem);
            };
            let prc_entries = if visible {
                match form_id {
                    Some(fid) => plan_prc_entries(&node.body, fid)?,
                    None => None,
                }
            } else {
                None
            };
            let prc_entry_refs: Option<Vec<(u16, &str)>> = prc_entries
                .as_ref()
                .map(|es| es.iter().map(|(i, t)| (*i, t.as_str())).collect());
            if let Some(entries) = &prc_entry_refs
                && !entries.is_empty()
            {
                let mut probe = node.body.clone();
                string_pack::insert_strings_at_ids_in_resource(
                    &mut probe,
                    PRC_TOKEN_LANGUAGE,
                    entries,
                )
                .map_err(|e| match e {
                    HiiError::IdOccupied(id) => HiiError::IdOccupied(id),
                    _ => HiiError::PrcPatchUnsupported,
                })?;
            }
            if visible {
                for (start, len) in packages {
                    let scope = {
                        let seg = &node.body[start..start + len];
                        match form_id {
                            Some(fid) => ifr::find_form_suppress_scope(seg, fid),
                            None => ifr::find_suppress_if_scopes(seg).into_iter().next(),
                        }
                    };
                    if let Some(scope) = scope {
                        ifr::unsuppress(&mut node.body[start..start + len], &scope);
                        changed = true;
                        break;
                    }
                }
            }
            if let Some(entries) = &prc_entry_refs
                && !entries.is_empty()
            {
                string_pack::insert_strings_at_ids_in_resource(
                    &mut node.body,
                    PRC_TOKEN_LANGUAGE,
                    entries,
                )
                .map_err(|_| HiiError::PrcPatchUnsupported)?;
                changed = true;
            }
        } else {
            return Err(HiiError::NotASetupItem);
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(changed, "set_item_visibility done");
    Ok(())
}

fn pe_resource_form_packages(body: &[u8]) -> Option<Vec<(usize, usize)>> {
    let mut out = Vec::new();
    for (off, len) in crate::hii::pe_resource::hii_resource_ranges(body) {
        let Some(blob) = body.get(off..off + len) else {
            continue;
        };
        let Some(list) = crate::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS {
                let start = off + (pkg.bytes.as_ptr() as usize - blob.as_ptr() as usize);
                out.push((start, pkg.bytes.len()));
            }
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

const PRC_TOKEN_LANGUAGE: &str = "x-UEFI-AMI";
const PRC_NAME_PREFIX: &str = "UPG";

fn plan_prc_entries(pe: &[u8], form_id: u16) -> Result<Option<Vec<(u16, String)>>, HiiError> {
    for (off, len) in crate::hii::pe_resource::hii_resource_ranges(pe) {
        let Some(blob) = pe.get(off..off + len) else {
            continue;
        };
        let Some(list) = crate::hii::package_list::parse_package_list(blob) else {
            continue;
        };
        let mut form_ids: Option<Vec<u16>> = None;
        let mut display: Option<crate::hii::strings::ParsedStringPackage> = None;
        let mut token: Option<crate::hii::strings::ParsedStringPackage> = None;
        for pkg in &list.packages {
            if pkg.kind == r_efi::hii::PACKAGE_FORMS && form_ids.is_none() {
                let ids = ifr::collect_form_string_ids(pkg.bytes, form_id);
                if !ids.is_empty() {
                    form_ids = Some(ids);
                }
            }
            if pkg.kind == r_efi::hii::PACKAGE_STRINGS
                && let Some(sp) = crate::hii::strings::parse_string_package(pkg.bytes)
            {
                if sp.language == PRC_TOKEN_LANGUAGE {
                    token = Some(sp);
                } else if display.is_none() {
                    display = Some(sp);
                }
            }
        }
        let (Some(ids), Some(display), Some(token)) = (form_ids, display, token) else {
            continue;
        };
        let display_by_id: std::collections::HashMap<u16, &String> =
            display.strings.iter().map(|(i, t)| (*i, t)).collect();
        let token_ids: std::collections::HashSet<u16> =
            token.strings.iter().map(|(i, _)| *i).collect();
        let token_texts: std::collections::HashSet<&str> =
            token.strings.iter().map(|(_, t)| t.as_str()).collect();
        let mut counter = 1u32;
        let mut entries = Vec::new();
        for id in ids {
            if token_ids.contains(&id) {
                continue;
            }
            let Some(text) = display_by_id.get(&id) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let mut name = format!("{PRC_NAME_PREFIX}{counter:04X}");
            while token_texts.contains(name.as_str()) {
                counter += 1;
                name = format!("{PRC_NAME_PREFIX}{counter:04X}");
            }
            counter += 1;
            entries.push((id, name));
        }
        return Ok(Some(entries));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode, ParsingData};
    use std::str::FromStr;

    #[test]
    fn pe_growth_unsupported_display() {
        assert_eq!(
            HiiError::PeGrowthUnsupported.to_string(),
            "cannot grow PE resource section"
        );
    }

    #[test]
    fn unsuppress_makes_block_empty() {
        let mut ifr = vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02];
        let scopes = ifr::find_suppress_if_scopes(&ifr);
        assert_eq!(scopes.len(), 1);
        ifr::unsuppress(&mut ifr, &scopes[0]);
        assert_eq!(&ifr[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(ifr.len(), 7);
    }

    #[test]
    fn set_item_visibility_true_unsuppresses_and_cascades() {
        let mut image = sample_image_with_ifr();
        set_item_visibility(&mut image, "0/0/0", true).unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(section.action, Action::Rebuild);
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

    fn sample_image_with_ifr() -> Image {
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    const FILE_GUID_STR: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn sample_image_with_ifr_guid() -> Image {
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn set_item_visibility_guid_section_target_unsuppresses() {
        let mut image = sample_image_with_ifr_guid();
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(image.root.action, Action::Rebuild);
    }

    #[test]
    fn set_item_visibility_guid_target_rejects_non_section() {
        let mut image = sample_image_with_ifr_guid();
        let err = set_item_visibility(&mut image, FILE_GUID_STR, true).unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn set_item_visibility_unknown_guid_is_not_found() {
        let mut image = sample_image_with_ifr_guid();
        let err = set_item_visibility(
            &mut image,
            "00000000-0000-0000-0000-000000000001:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn set_item_visibility_refuses_read_only_mode() {
        let mut image = sample_image_with_ifr_guid();
        image.mode = ImageMode::Read;
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotWritable));
    }

    #[test]
    fn set_item_visibility_refuses_mutation_behind_compression() {
        let mut inner = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::MutationBehindCompression));
    }

    #[test]
    fn set_item_visibility_allows_lzma_backed_wrapper() {
        let mut inner = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        wrapper.parsing_data =
            crate::types::ParsingData::GuidedSection(crate::types::GuidedSectionParsingData {
                guid: crate::ffs::lzma_guid(),
                dictionary_size: 0x0080_0000,
            });
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let body_before = image.root.children[0].children[0].children[0].children[0]
            .body
            .clone();
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .expect("lzma-backed wrapper is recompressable; mutation must be allowed");
        let body_after = image.root.children[0].children[0].children[0].children[0]
            .body
            .clone();
        assert_ne!(
            body_before, body_after,
            "unsuppress must patch the form body"
        );
    }

    #[test]
    fn set_item_visibility_refuses_uncompressed_pe32_target() {
        let mut section = mk_node(FfsType::Section, vec![b'M', b'Z', 0, 0], vec![]);
        section.subtype = 0x10;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn set_item_visibility_targets_form_inside_nested_fv() {
        let inner_file_guid = "899407D7-92A6-4174-968F-6F0B47F86A99";
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
        let mut inner_file = mk_node(FfsType::File, vec![], vec![section.clone()]);
        inner_file.guid = Some(Guid::from_str(inner_file_guid).unwrap());
        let inner_vol = mk_node(FfsType::Volume, vec![], vec![inner_file]);
        let mut fv_sec = mk_node(FfsType::Section, vec![], vec![inner_vol]);
        fv_sec.subtype = 0x17;
        let mut outer_file = mk_node(FfsType::File, vec![], vec![fv_sec.clone()]);
        outer_file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![outer_file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        set_item_visibility(
            &mut image,
            "899407D7-92A6-4174-968F-6F0B47F86A99:0x19:0",
            true,
        )
        .unwrap();
        let fv_sec_after = &image.root.children[0].children[0].children[0];
        assert_eq!(fv_sec_after.subtype, 0x17);
        assert_eq!(fv_sec_after.action, Action::Rebuild);
        let section_after = &fv_sec_after.children[0].children[0].children[0];
        assert_eq!(&section_after.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(section_after.action, Action::Rebuild);
    }

    #[test]
    fn set_item_visibility_patches_form_inside_pe_resource() {
        let list_guid = Guid::from_str(FILE_GUID_STR).unwrap();
        let mut form_pkg = vec![11u8, 0, 0, r_efi::hii::PACKAGE_FORMS];
        form_pkg.extend_from_slice(&[0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02]);
        let mut list = list_guid.to_bytes().to_vec();
        let total = 20 + form_pkg.len() + 4;
        list.extend_from_slice(&(total as u32).to_le_bytes());
        list.extend_from_slice(&form_pkg);
        list.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let mut section = mk_node(FfsType::Section, pe.clone(), vec![]);
        section.subtype = 0x10;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            true,
        )
        .unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(section.subtype, 0x10);
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(section.body.len(), pe.len());
        assert_eq!(
            section
                .body
                .iter()
                .zip(pe.iter())
                .filter(|(a, b)| a != b)
                .count(),
            5,
            "exactly the suppress-scope rewrite may change bytes"
        );
        let ranges = crate::hii::pe_resource::hii_resource_ranges(&section.body);
        assert_eq!(ranges.len(), 1);
        let (off, len) = ranges[0];
        let blob = &section.body[off..off + len];
        assert_eq!(&blob[24..28], &[0x0A, 0x82, 0x29, 0x02]);
    }

    #[test]
    fn set_item_visibility_form_discriminator_unsuppresses_specific_form() {
        let mut body: Vec<u8> = vec![0x0A, 0x82, 0x12, 0x03, 0x40];
        body.extend_from_slice(&[0x01, 0x86, 0x01, 0x00, 0x01, 0x00]);
        body.extend_from_slice(&[0x29, 0x02]);
        body.extend_from_slice(&[0x29, 0x02]);
        body.extend_from_slice(&[0x0A, 0x82, 0x12, 0x03, 0x40]);
        body.extend_from_slice(&[0x01, 0x86, 0x02, 0x00, 0x02, 0x00]);
        body.extend_from_slice(&[0x29, 0x02]);
        body.extend_from_slice(&[0x29, 0x02]);
        let mut section = mk_node(FfsType::Section, body, vec![]);
        section.subtype = 0x19;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#2",
            true,
        )
        .unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x12, 0x03]);
        assert_eq!(&section.body[15..19], &[0x0A, 0x82, 0x29, 0x02]);
        assert!(ifr::find_form_suppress_scope(&section.body, 1).is_some());
        assert!(ifr::find_form_suppress_scope(&section.body, 2).is_none());
    }

    #[test]
    fn set_item_visibility_rejects_malformed_discriminator() {
        let mut image = sample_image_with_ifr();
        image.mode = ImageMode::Write;
        let err = set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#notanumber",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    fn ifr_op(opc: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(2 + payload.len());
        v.push(opc);
        v.push((2 + payload.len()) as u8 | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn u16p(x: u16) -> Vec<u8> {
        x.to_le_bytes().to_vec()
    }

    fn suppressed_form_901_pkg() -> Vec<u8> {
        let mut ifr = Vec::new();
        ifr.extend(ifr_op(
            0x0E,
            true,
            &[u16p(1).as_slice(), u16p(2).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(0x0A, true, &[]));
        ifr.extend(ifr_op(0x12, false, &[0x40]));
        ifr.extend(ifr_op(
            0x01,
            true,
            &[u16p(901).as_slice(), u16p(0x1325).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            0x02,
            false,
            &[u16p(4894).as_slice(), u16p(4895).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            0x05,
            true,
            &[
                u16p(4965).as_slice(),
                u16p(4966).as_slice(),
                u16p(0x0D5F).as_slice(),
                u16p(1).as_slice(),
            ]
            .concat(),
        ));
        ifr.extend(ifr_op(
            0x09,
            false,
            &[u16p(4969).as_slice(), &[0x00, 0x05], u16p(1).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(
            0x09,
            false,
            &[u16p(2638).as_slice(), &[0x00, 0x05], u16p(0).as_slice()].concat(),
        ));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        ifr.extend(ifr_op(0x29, false, &[]));
        let len = 4 + ifr.len();
        let mut pkg = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend(ifr);
        pkg
    }

    fn sppkg(language: &str, sibt: &[u8]) -> Vec<u8> {
        let hdr_size: u32 = (46 + language.len() + 1) as u32;
        let info_off = hdr_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 3]);
        buf.push(0x04);
        buf.extend_from_slice(&hdr_size.to_le_bytes());
        buf.extend_from_slice(&info_off.to_le_bytes());
        while buf.len() < 44 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(language.as_bytes());
        buf.push(0);
        while buf.len() < info_off as usize {
            buf.push(0);
        }
        buf.extend_from_slice(sibt);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn display_sibt() -> Vec<u8> {
        vec![
            0x10, b'b', b'o', b'o', b't', 0, 0x21, 0x4C, 0x0A, 0x10, b'A', b'u', b't', b'o', 0,
            0x21, 0xCF, 0x08, 0x10, b'H', b'W', b'P', b'M', 0, 0x21, 0x46, 0x00, 0x10, b'W', b'S',
            b'u', b'p', 0, 0x21, 0x03, 0x00, 0x10, b'E', b'n', b'a', 0, 0x21, 0x65, 0x00, 0x10,
            0x00, 0x00,
        ]
    }

    fn token_sibt() -> Vec<u8> {
        vec![
            0x10, b'P', b'R', b'C', b'0', b'1', 0, 0x21, 0x4C, 0x0A, 0x10, b'P', b'R', b'C', b'0',
            b'A', b'E', 0, 0x00,
        ]
    }

    fn prc_blob(forms: Vec<u8>, display: Vec<u8>, token: Vec<u8>) -> Vec<u8> {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + forms.len() + display.len() + token.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&forms);
        b.extend_from_slice(&display);
        b.extend_from_slice(&token);
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    fn pe32_image_with(blob: &[u8]) -> Image {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", blob);
        let mut section = mk_node(FfsType::Section, pe, vec![]);
        section.subtype = EFI_SECTION_PE32;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::try_parse("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    fn resource_string_pkgs(image: &Image) -> Vec<crate::hii::strings::ParsedStringPackage> {
        let node = &image.root.children[0].children[0].children[0];
        crate::hii::strings::resource_string_packages(&node.body)
    }

    fn image_snapshot(image: &Image) -> Vec<u8> {
        image.root.children[0].children[0].children[0].body.clone()
    }

    #[test]
    fn set_item_visibility_patches_prc_tokens_for_unhidden_form() {
        let blob = prc_blob(
            suppressed_form_901_pkg(),
            sppkg("en-US", &display_sibt()),
            sppkg("x-UEFI-AMI", &token_sibt()),
        );
        let mut image = pe32_image_with(&blob);
        set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap();
        let pkgs = resource_string_pkgs(&image);
        assert_eq!(pkgs.len(), 2);
        let display = pkgs.iter().find(|p| p.language == "en-US").unwrap();
        let token = pkgs.iter().find(|p| p.language == "x-UEFI-AMI").unwrap();
        let tok_by_id: std::collections::HashMap<u16, &String> =
            token.strings.iter().map(|(i, t)| (*i, t)).collect();
        assert_eq!(tok_by_id.get(&4894).map(|s| s.as_str()), Some("UPG0001"));
        assert_eq!(tok_by_id.get(&4965).map(|s| s.as_str()), Some("UPG0002"));
        assert_eq!(tok_by_id.get(&4969).map(|s| s.as_str()), Some("UPG0003"));
        assert!(!tok_by_id.contains_key(&5071));
        assert_eq!(tok_by_id.get(&2638).map(|s| s.as_str()), Some("PRC0AE"));
        let disp_by_id: std::collections::HashMap<u16, &String> =
            display.strings.iter().map(|(i, t)| (*i, t)).collect();
        assert_eq!(disp_by_id.get(&4894).map(|s| s.as_str()), Some("HWPM"));
        assert_eq!(disp_by_id.get(&5071).map(|s| s.as_str()), Some(""));
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::Rebuild
        );
    }

    #[test]
    fn set_item_visibility_no_token_package_is_noop_unhide() {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let forms = suppressed_form_901_pkg();
        let display = sppkg("en-US", &display_sibt());
        let mut blob = g.to_bytes().to_vec();
        let total = 20 + forms.len() + display.len() + 4;
        blob.extend_from_slice(&(total as u32).to_le_bytes());
        blob.extend_from_slice(&forms);
        blob.extend_from_slice(&display);
        blob.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        let mut image = pe32_image_with(&blob);
        set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap();
        let pkgs = resource_string_pkgs(&image);
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].language, "en-US");
        assert_eq!(pkgs[0].strings.len(), 6);
    }

    #[test]
    fn set_item_visibility_prc_growth_failure_leaves_image_untouched() {
        let blob = prc_blob(
            suppressed_form_901_pkg(),
            sppkg("en-US", &display_sibt()),
            sppkg("x-UEFI-AMI", &token_sibt()),
        );
        let mut image = pe32_image_with(&blob);
        {
            let node = &mut image.root.children[0].children[0].children[0];
            node.body[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        }
        let snapshot = image_snapshot(&image);
        let err = set_item_visibility(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#901",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::PrcPatchUnsupported));
        assert_eq!(image_snapshot(&image), snapshot);
    }
}
