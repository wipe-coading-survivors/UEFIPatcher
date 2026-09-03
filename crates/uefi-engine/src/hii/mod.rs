pub mod ami_patcher;
pub mod ffs_assembler;
pub mod form_add;
pub mod forms;
pub mod formset_add;
pub mod gates;
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
    #[error("gating expression not reducible to a hardware-validated flip: {0}")]
    GateExpressionUnsupported(String),
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, visible), err)]
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), HiiError> {
    let (target, form_id) = match item_id.rsplit_once('#') {
        Some(_) => {
            let (target, form_id, question_id) = parse_item_id(item_id)?;
            if question_id.is_some() {
                return Err(HiiError::NotFound);
            }
            (target, Some(form_id))
        }
        None => (
            crate::parser::target::parse_target(item_id).map_err(|_| HiiError::NotFound)?,
            None,
        ),
    };
    let path = resolve_writable_path(image, &target)?;
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

fn parse_u16_loose(s: &str) -> Option<u16> {
    if let Some(hex) = s.strip_prefix("0x") {
        u16::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u16>().ok()
    }
}

fn parse_item_id(item_id: &str) -> Result<(crate::types::Target, u16, Option<u16>), HiiError> {
    let (target_str, disc) = item_id.rsplit_once('#').ok_or(HiiError::NotFound)?;
    let (form_str, qid_str) = match disc.split_once(':') {
        Some((f, q)) => (f, Some(q)),
        None => (disc, None),
    };
    let form_id = form_str.parse::<u16>().map_err(|_| HiiError::NotFound)?;
    let question_id = match qid_str {
        Some(q) => Some(parse_u16_loose(q).ok_or(HiiError::NotFound)?),
        None => None,
    };
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    Ok((target, form_id, question_id))
}

fn resolve_writable_path(
    image: &Image,
    target: &crate::types::Target,
) -> Result<Vec<usize>, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let path =
        crate::parser::target::find_item_path(&image.root, target).ok_or(HiiError::NotFound)?;
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
    Ok(path)
}

fn form_package_ranges(node: &FfsNode) -> Vec<(usize, usize)> {
    if node.subtype == EFI_SECTION_RAW || ifr::is_form_package(&node.body) {
        if ifr::is_form_package(&node.body) {
            vec![(0, node.body.len())]
        } else {
            vec![]
        }
    } else if node.subtype == EFI_SECTION_PE32 {
        pe_resource_form_packages(&node.body).unwrap_or_default()
    } else {
        vec![]
    }
}

fn expr_text(expr: &gates::GateExpr, region: &[u8]) -> String {
    match expr {
        gates::GateExpr::EqConst { a, b } => format!("{a} == {b}"),
        gates::GateExpr::EqIdVal { question_id, value } => {
            format!("{question_id:#06X} == {value:#06X}")
        }
        gates::GateExpr::True => "true".to_string(),
        gates::GateExpr::Other => format!(
            "[{}]",
            region
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

fn flip_text(base: usize, flip: &gates::PlannedFlip) -> String {
    let hex = |bs: &[u8]| {
        bs.iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        "pkg+{:#x}: {} -> {}",
        base + flip.offset,
        hex(&flip.from),
        hex(&flip.to)
    )
}

fn gate_info(pkg: &[u8], gate: &gates::Gate, base: usize) -> uefi_proto::GateInfo {
    let region = &pkg[gate.expr_offset..gate.expr_end.min(pkg.len())];
    let flip = gates::plan_flip(pkg, gate);
    let (wraps, form_id, host_form_id, question_id) = match gate.wraps {
        gates::Wraps::Form { form_id } => ("form", form_id as u32, form_id as u32, 0),
        gates::Wraps::Ref {
            form_id,
            host_form_id,
        } => ("ref", form_id as u32, host_form_id as u32, 0),
        gates::Wraps::Question {
            form_id,
            question_id,
        } => (
            "question",
            form_id as u32,
            form_id as u32,
            question_id as u32,
        ),
    };
    uefi_proto::GateInfo {
        gate_kind: gate.kind.as_str().to_string(),
        wraps: wraps.to_string(),
        form_id,
        host_form_id,
        question_id,
        expression: expr_text(&gate.expr, region),
        flippable: flip.is_some(),
        flip: flip
            .as_ref()
            .map(|f| flip_text(base, f))
            .unwrap_or_default(),
        scope_offset: (base + gate.scope_offset) as u32,
    }
}

pub fn gates_list(image: &Image, item_id: &str) -> Result<Vec<uefi_proto::GateInfo>, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let node =
        crate::parser::target::find_item(&image.root, &target).map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let gt = gates::GateTarget {
        form_id,
        question_id,
    };
    let mut out = Vec::new();
    for (start, len) in form_package_ranges(node) {
        let pkg = &node.body[start..start + len];
        for gate in gates::find_gates(pkg, &gt) {
            out.push(gate_info(pkg, &gate, start));
        }
    }
    Ok(out)
}

#[derive(Debug)]
pub struct UnlockOutcome {
    pub gates: Vec<uefi_proto::GateInfo>,
    pub applied: Vec<String>,
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id), err)]
pub fn unlock(image: &mut Image, item_id: &str) -> Result<UnlockOutcome, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let path = resolve_writable_path(image, &target)?;
    let gt = gates::GateTarget {
        form_id,
        question_id,
    };
    let mut infos = Vec::new();
    let mut applied = Vec::new();
    let mutated = {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        let ranges = form_package_ranges(node);
        let mut body = std::mem::take(&mut node.body);
        let mut absolute_flips: Vec<gates::PlannedFlip> = Vec::new();
        let mut plan_err: Option<HiiError> = None;
        for (start, len) in ranges {
            let pkg = &body[start..start + len];
            let found = gates::find_gates(pkg, &gt);
            if found.is_empty() {
                continue;
            }
            match gates::plan_gates(pkg, &found) {
                Ok(flips) => {
                    for gate in &found {
                        infos.push(gate_info(pkg, gate, start));
                    }
                    absolute_flips.extend(flips.into_iter().map(|f| gates::PlannedFlip {
                        offset: start + f.offset,
                        from: f.from,
                        to: f.to,
                    }));
                }
                Err(e) => {
                    plan_err = Some(HiiError::GateExpressionUnsupported(e));
                    break;
                }
            }
        }
        if let Some(err) = plan_err {
            node.body = body;
            return Err(err);
        }
        if absolute_flips.is_empty() {
            node.body = body;
            return Ok(UnlockOutcome {
                gates: infos,
                applied,
            });
        }
        let body_len = body.len();
        if let Err(e) = gates::apply_flips(&mut body, &absolute_flips) {
            node.body = body;
            return Err(HiiError::GateExpressionUnsupported(e));
        }
        assert_eq!(body.len(), body_len, "unlock is length-preserving");
        applied = absolute_flips.iter().map(|f| flip_text(0, f)).collect();
        node.body = body;
        true
    };
    if mutated {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(flips = applied.len(), "unlock done");
    Ok(UnlockOutcome {
        gates: infos,
        applied,
    })
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

    use crate::hii::gates::{GateExpr, GateTarget};

    const VENDOR_FORMSET_GUID_STR: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn g_opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn g_uint64(v: u64) -> Vec<u8> {
        let mut b = vec![r_efi::hii::IFR_UINT64_OP, 0x0A];
        b.extend_from_slice(&v.to_le_bytes());
        b
    }

    fn g_equal() -> Vec<u8> {
        vec![r_efi::hii::IFR_EQUAL_OP, 0x02]
    }

    fn g_eq_id_val(question_id: u16, value: u16) -> Vec<u8> {
        let mut b = vec![r_efi::hii::IFR_EQ_ID_VAL_OP, 0x06];
        b.extend_from_slice(&question_id.to_le_bytes());
        b.extend_from_slice(&value.to_le_bytes());
        b
    }

    fn g_form(id: u16) -> Vec<u8> {
        g_opcode(
            r_efi::hii::IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), 7u16.to_le_bytes()].concat(),
        )
    }

    fn g_ref(form_id: u16) -> Vec<u8> {
        let p = vec![0u8; 11];
        let mut v = vec![r_efi::hii::IFR_REF_OP, 0x0F];
        v.extend_from_slice(&p);
        v.extend_from_slice(&form_id.to_le_bytes());
        v
    }

    fn g_one_of(question_id: u16) -> Vec<u8> {
        let mut p = vec![0u8; 10];
        p[4..6].copy_from_slice(&question_id.to_le_bytes());
        g_opcode(r_efi::hii::IFR_ONE_OF_OP, true, &p)
    }

    fn g_end() -> Vec<u8> {
        vec![r_efi::hii::IFR_END_OP, 0x02]
    }

    fn forms_pkg(extra_ifr: Vec<u8>) -> Vec<u8> {
        let g = Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = g_opcode(r_efi::hii::IFR_FORM_SET_OP, true, &p);
        ifr.extend(extra_ifr);
        let total = 4 + ifr.len();
        let mut pkg = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend(ifr);
        pkg
    }

    fn vendor_forms_pkg() -> Vec<u8> {
        forms_pkg(
            vec![
                g_form(10002),
                g_opcode(r_efi::hii::IFR_SUPPRESS_IF_OP, true, &[]),
                g_uint64(1),
                g_uint64(1),
                g_equal(),
                g_ref(10029),
                g_end(),
                g_end(),
                g_form(10029),
                g_opcode(r_efi::hii::IFR_GRAY_OUT_IF_OP, true, &[]),
                g_eq_id_val(0x009A, 1),
                g_one_of(0x003B),
                g_end(),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        )
    }

    fn vendor_image_with(section_subtype: u8, body: Vec<u8>) -> Image {
        let mut section = mk_node(FfsType::Section, body, vec![]);
        section.subtype = section_subtype;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    fn vendor_hii_blob() -> Vec<u8> {
        let g = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let forms = vendor_forms_pkg();
        let mut b = g.to_bytes().to_vec();
        let total = 20 + forms.len() + 4;
        b.extend_from_slice(&(total as u32).to_le_bytes());
        b.extend_from_slice(&forms);
        b.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        b
    }

    const VENDOR_FORM_ITEM: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029";
    const VENDOR_QUESTION_ITEM: &str = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:0x3B";

    #[test]
    fn parse_item_id_accepts_form_and_question_forms() {
        let (_, form_id, qid) = parse_item_id(VENDOR_FORM_ITEM).unwrap();
        assert_eq!(form_id, 10029);
        assert_eq!(qid, None);
        let (_, form_id, qid) = parse_item_id(VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(form_id, 10029);
        assert_eq!(qid, Some(0x3B));
        let (_, _, qid) =
            parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:59").unwrap();
        assert_eq!(qid, Some(59));
    }

    #[test]
    fn parse_item_id_rejects_garbage() {
        assert!(parse_item_id("no-discriminator").is_err());
        assert!(parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#nope").is_err());
        assert!(parse_item_id("5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:zz").is_err());
    }

    #[test]
    fn set_item_visibility_rejects_question_suffix() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let err = set_item_visibility(&mut image, VENDOR_QUESTION_ITEM, true).unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn gates_list_bare_channel_reports_form_and_question_gates() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].gate_kind, "suppress");
        assert_eq!(gates[0].wraps, "ref");
        assert_eq!(gates[0].host_form_id, 10002);
        assert_eq!(gates[0].expression, "1 == 1");
        assert!(gates[0].flippable);
        assert!(gates[0].flip.contains("-> 02"));
        let qgates = gates_list(&image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(qgates.len(), 1);
        assert_eq!(qgates[0].gate_kind, "grayout");
        assert_eq!(qgates[0].expression, "0x009A == 0x0001");
        assert_eq!(
            qgates[0].flip,
            format!("pkg+{:#x}: 01 00 -> ff ff", qgates[0].scope_offset + 6)
        );
    }

    #[test]
    fn gates_list_works_behind_non_recompressable_wrapper() {
        let mut inner_section = mk_node(FfsType::Section, vendor_forms_pkg(), vec![]);
        inner_section.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner_section]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        };
        let gates = gates_list(&image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(gates.len(), 1);
    }

    #[test]
    fn gates_list_resource_channel() {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &vendor_hii_blob());
        let mut image = vendor_image_with(0x10, pe);
        image.mode = ImageMode::Read;
        let gates =
            gates_list(&image, "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029").unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].wraps, "ref");
        assert!(gates[0].flip.contains("-> 02"));
    }

    #[test]
    fn gates_list_unknown_target_is_not_found() {
        let image = vendor_image_with(0x19, vendor_forms_pkg());
        assert!(matches!(
            gates_list(&image, "00000000-0000-0000-0000-000000000001:0x19:0#10029"),
            Err(HiiError::NotFound)
        ));
    }

    #[test]
    fn unlock_flips_gates_in_place_and_cascades_rebuild() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let before = image.root.children[0].children[0].children[0].body.clone();
        let outcome = unlock(&mut image, VENDOR_FORM_ITEM).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        {
            let section = &image.root.children[0].children[0].children[0];
            let gates = crate::hii::gates::find_gates(
                &section.body,
                &GateTarget {
                    form_id: 10029,
                    question_id: None,
                },
            );
            assert_eq!(gates[0].expr, GateExpr::EqConst { a: 1, b: 2 });
            assert_eq!(section.action, Action::Rebuild);
            assert_eq!(image.root.action, Action::Rebuild);
            assert_eq!(section.body.len(), before.len());
        }
        let outcome = unlock(&mut image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        let section = &image.root.children[0].children[0].children[0];
        let diff = before
            .iter()
            .zip(section.body.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff, 3,
            "form + question unlock together must flip exactly 3 bytes"
        );
    }

    #[test]
    fn unlock_resource_channel_changes_exactly_flip_bytes() {
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &vendor_hii_blob());
        let mut image = vendor_image_with(0x10, pe.clone());
        let outcome = unlock(
            &mut image,
            "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029:0x3B",
        )
        .unwrap();
        assert_eq!(outcome.applied.len(), 1);
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(section.body.len(), pe.len());
        let diff = section
            .body
            .iter()
            .zip(pe.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff, 2,
            "eq_id_val flip touches exactly the two value bytes"
        );
    }

    #[test]
    fn unlock_refuses_read_only_mode() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        image.mode = ImageMode::Read;
        assert!(matches!(
            unlock(&mut image, VENDOR_FORM_ITEM),
            Err(HiiError::NotWritable)
        ));
    }

    #[test]
    fn unlock_refuses_mutation_behind_compression() {
        let mut inner = mk_node(FfsType::Section, vendor_forms_pkg(), vec![]);
        inner.subtype = 0x19;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = 0x02;
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        assert!(matches!(
            unlock(&mut image, VENDOR_FORM_ITEM),
            Err(HiiError::MutationBehindCompression)
        ));
    }

    #[test]
    fn unlock_no_gates_is_ok_empty() {
        let pkg = forms_pkg([g_form(10029), g_end(), g_end()].concat());
        let mut image = vendor_image_with(0x19, pkg);
        let outcome = unlock(&mut image, VENDOR_FORM_ITEM).unwrap();
        assert!(outcome.gates.is_empty());
        assert!(outcome.applied.is_empty());
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::NoAction
        );
    }

    #[test]
    fn unlock_unflippable_gate_refuses_without_mutation() {
        let pkg = forms_pkg(
            [
                g_form(10002),
                g_opcode(r_efi::hii::IFR_SUPPRESS_IF_OP, true, &[]),
                vec![r_efi::hii::IFR_TRUE_OP, 0x02],
                g_ref(10029),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        );
        let mut image = vendor_image_with(0x19, pkg.clone());
        let err = unlock(&mut image, VENDOR_FORM_ITEM).unwrap_err();
        assert!(matches!(err, HiiError::GateExpressionUnsupported(_)));
        assert_eq!(image.root.children[0].children[0].children[0].body, pkg);
    }
}
