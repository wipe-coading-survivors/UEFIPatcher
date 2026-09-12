pub mod ami_patcher;
mod cross_formset;
pub mod ffs_assembler;
pub mod form_add;
pub mod form_hijack;
pub mod forms;
pub mod formset_add;
pub mod gates;
pub mod ifr;
pub mod ifr_builder;
pub mod nvar;
pub mod package_list;
pub mod pe_resource;
pub mod questions;
pub mod ref_tree;
mod ref_variant;
pub mod schema;
pub mod spf;
pub mod string_pack;
pub mod strings;
pub mod values;

use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::ops;
use crate::types::*;
use std::collections::HashMap;
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
    #[error("value operation not supported: {0}")]
    ValueOpUnsupported(String),
    #[error(
        "form has no suppress-if scope of its own; REF-parent gates are the unlock op's domain"
    )]
    NoSuppressScope,
    #[error("hiding (visible=false) is not implemented: only unsuppress exists")]
    HidingUnsupported,
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
        if !visible {
            return Err(HiiError::HidingUnsupported);
        }
        if node.subtype == EFI_SECTION_RAW || ifr::is_form_package(&node.body) {
            let scope = match form_id {
                Some(fid) => ifr::find_form_suppress_scope(&node.body, fid),
                None => ifr::find_suppress_if_scopes(&node.body).into_iter().next(),
            };
            if let Some(scope) = scope {
                ifr::unsuppress(&mut node.body, &scope);
                changed = true;
            }
        } else if node.subtype == EFI_SECTION_PE32 {
            let Some(packages) = pe_resource_form_packages(&node.body) else {
                return Err(HiiError::NotASetupItem);
            };
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
        } else {
            return Err(HiiError::NotASetupItem);
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    } else {
        return Err(HiiError::NoSuppressScope);
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

pub(crate) fn parse_item_id(
    item_id: &str,
) -> Result<(crate::types::Target, u16, Option<u16>), HiiError> {
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

pub(crate) fn form_package_ranges(node: &FfsNode) -> Vec<(usize, usize)> {
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

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Все печатаемые смещения — pkg+: от начала form-пакета, включая его
/// 4-байтовый заголовок (u24 length + kind). Единая точка истины для
/// GateInfo.flip/scope_offset, UnlockOutcome.applied и текстов ошибок
/// планировщика. Спека hii-walker-consistency §3.3.
pub(crate) fn pkg_off(off: usize) -> String {
    format!("pkg+{off:#x}")
}

pub(crate) fn flip_text(flip: &gates::PlannedFlip) -> String {
    format!(
        "{}: {} -> {}",
        pkg_off(flip.offset),
        hex(&flip.from),
        hex(&flip.to)
    )
}

fn gate_info(pkg: &[u8], gate: &gates::Gate) -> uefi_proto::GateInfo {
    let region = &pkg[gate.expr_offset..gate.expr_end.min(pkg.len())];
    let flip = gates::plan_flip(pkg, gate);
    let (wraps, form_id, host_form_id, question_id) = match gate.wraps {
        gates::Wraps::Form { form_id } => ("form", form_id as u32, form_id as u32, 0),
        gates::Wraps::Ref {
            form_id,
            host_form_id,
        } => ("ref", form_id as u32, host_form_id as u32, 0),
        gates::Wraps::CrossFormsetRef {
            form_id,
            host_form_id,
            ..
        } => ("cross_ref", form_id as u32, host_form_id as u32, 0),
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
        flip: flip.as_ref().map(flip_text).unwrap_or_default(),
        scope_offset: gate.scope_offset as u32,
        source_target: String::new(),
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
        formset_guid: None,
    };
    let mut out = Vec::new();
    for (start, len) in form_package_ranges(node) {
        let pkg = &node.body[start..start + len];
        for gate in gates::find_gates(pkg, &gt) {
            out.push(gate_info(pkg, &gate));
        }
    }
    if gt.question_id.is_none()
        && let Some(own_formset) = own_formset_guid(node)
        && let Some(skip_path) = crate::parser::target::find_item_path(&image.root, &target)
    {
        let gt_cross = gates::GateTarget {
            formset_guid: Some(own_formset),
            ..gt
        };
        for site in cross_formset::find_cross_gates(image, &skip_path, &gt_cross) {
            let pkg = &node_at(&image.root, &site.path).body
                [site.pkg_start..site.pkg_start + site.pkg_len];
            for gate in &site.gates {
                let mut gi = gate_info(pkg, gate);
                gi.source_target = site.source_ffs.clone();
                out.push(gi);
            }
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
        formset_guid: None,
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
            match gates::plan_gates_skip_unlocked(pkg, &found) {
                Ok(flips) => {
                    for gate in &found {
                        infos.push(gate_info(pkg, gate));
                    }
                    applied.extend(flips.iter().map(flip_text));
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
            false
        } else {
            let body_len = body.len();
            if let Err(e) = gates::apply_flips(&mut body, &absolute_flips) {
                node.body = body;
                return Err(HiiError::GateExpressionUnsupported(e));
            }
            assert_eq!(body.len(), body_len, "unlock is length-preserving");
            node.body = body;
            true
        }
    };
    let cross_applied = apply_cross_formset_gates(image, &target, gt, &mut infos, &mut applied)?;
    let mutated = mutated || cross_applied;
    if mutated {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(flips = applied.len(), "unlock done");
    Ok(UnlockOutcome {
        gates: infos,
        applied,
    })
}

fn own_formset_guid(node: &FfsNode) -> Option<Guid> {
    for (start, len) in form_package_ranges(node) {
        if let Some(fs) = ifr::parse_form_package(&node.body[start..start + len]) {
            return Some(fs.guid);
        }
    }
    None
}

/// Кросс-формсетная фаза unlock (спека §3 U2a): гейты в чужих секциях,
/// мутация донора по path. Возвращает true, если хоть один флип применён.
/// Только для form-таргетов (question_id == None): Question-рука find_gates
/// матчится по голой (form_id, question_id) паре без привязки к формсету —
/// question-таргетный кросс флипал бы чужих доноров по совпадающей паре.
fn apply_cross_formset_gates(
    image: &mut Image,
    target: &crate::types::Target,
    gt: gates::GateTarget,
    infos: &mut Vec<uefi_proto::GateInfo>,
    applied: &mut Vec<String>,
) -> Result<bool, HiiError> {
    if gt.question_id.is_some() {
        return Ok(false);
    }
    let (skip_path, own_formset) = {
        let node = crate::parser::target::find_item(&image.root, target)
            .map_err(|_| HiiError::NotFound)?;
        let Some(own_formset) = own_formset_guid(node) else {
            return Ok(false);
        };
        let skip_path =
            crate::parser::target::find_item_path(&image.root, target).ok_or(HiiError::NotFound)?;
        (skip_path, own_formset)
    };
    let gt_cross = gates::GateTarget {
        formset_guid: Some(own_formset),
        ..gt
    };
    let sites = cross_formset::find_cross_gates(image, &skip_path, &gt_cross);
    let mut any = false;
    for site in sites {
        resolve_writable_path(image, &crate::types::Target::Path(site.path.clone()))?;
        let pkg = {
            let node = node_at(&image.root, &site.path);
            node.body[site.pkg_start..site.pkg_start + site.pkg_len].to_vec()
        };
        let flips = gates::plan_gates_skip_unlocked(&pkg, &site.gates)
            .map_err(HiiError::GateExpressionUnsupported)?;
        for gate in &site.gates {
            let mut gi = gate_info(&pkg, gate);
            gi.source_target = site.source_ffs.clone();
            infos.push(gi);
        }
        if flips.is_empty() {
            continue;
        }
        applied.extend(flips.iter().map(flip_text));
        let absolute: Vec<gates::PlannedFlip> = flips
            .into_iter()
            .map(|f| gates::PlannedFlip {
                offset: site.pkg_start + f.offset,
                from: f.from,
                to: f.to,
            })
            .collect();
        let node = node_at_mut(&mut image.root, &site.path);
        gates::apply_flips(&mut node.body, &absolute)
            .map_err(HiiError::GateExpressionUnsupported)?;
        ops::mark_rebuild_to_root_by_path(&mut image.root, &site.path);
        any = true;
    }
    Ok(any)
}

fn question_kind_str(kind: values::QuestionKind) -> &'static str {
    match kind {
        values::QuestionKind::OneOf => "one_of",
        values::QuestionKind::CheckBox => "checkbox",
        values::QuestionKind::Numeric => "numeric",
        values::QuestionKind::Other => "other",
    }
}

fn question_info_proto(
    form_id: u16,
    map: &values::QuestionMap,
    texts: &HashMap<u16, String>,
) -> uefi_proto::QuestionInfo {
    uefi_proto::QuestionInfo {
        form_id: form_id as u32,
        question_id: map.question_id as u32,
        kind: question_kind_str(map.kind).to_string(),
        var_store_id: map.var_store_id as u32,
        varstore: map.varstore.as_ref().map(|v| uefi_proto::VarStoreInfo {
            id: v.id as u32,
            guid: v
                .guid
                .as_ref()
                .map(crate::guid_to_upper_string)
                .unwrap_or_default(),
            size: v.size as u32,
            name: v.name.clone(),
        }),
        var_offset: map.var_offset as u32,
        width: map.width as u32,
        min: map.min,
        max: map.max,
        step: map.step,
        options: map
            .options
            .iter()
            .map(|o| uefi_proto::OptionEntry {
                string_id: o.string_id as u32,
                value: o.value,
                flags: o.flags as u32,
                text: texts.get(&o.string_id).cloned().unwrap_or_default(),
            })
            .collect(),
        defaults: map
            .defaults
            .iter()
            .map(|d| uefi_proto::DefaultEntry {
                default_id: d.default_id as u32,
                r#type: d.type_ as u32,
                value: d.value,
            })
            .collect(),
    }
}

fn find_question_map(
    image: &Image,
    target: &crate::types::Target,
    form_id: u16,
    question_id: u16,
) -> Result<(values::QuestionMap, HashMap<u16, String>), HiiError> {
    let path =
        crate::parser::target::find_item_path(&image.root, target).ok_or(HiiError::NotFound)?;
    let mut node = &image.root;
    for &i in &path {
        node = &node.children[i];
    }
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let mut file = &image.root;
    for &i in &path[..path.len() - 1] {
        file = &file.children[i];
    }
    let texts = questions::prompt_texts(file);
    for (start, len) in form_package_ranges(node) {
        if let Some(map) =
            values::find_question(&node.body[start..start + len], form_id, question_id)
        {
            return Ok((map, texts));
        }
    }
    Err(HiiError::NotFound)
}

pub fn question_info(image: &Image, item_id: &str) -> Result<uefi_proto::QuestionInfo, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let Some(question_id) = question_id else {
        return Err(HiiError::NotFound);
    };
    let (map, texts) = find_question_map(image, &target, form_id, question_id)?;
    Ok(question_info_proto(form_id, &map, &texts))
}

/// Вопросы формы по target-строке (например "GUID:0x19:0") + числовой
/// form_id. Read-only: работает в любом ImageMode. НЕ проверяет
/// writability-барьеры — это просмотр (мутации — set_value/unlock).
/// Спека tui-forms-view §3.4.
pub fn list_questions(
    image: &Image,
    target_str: &str,
    form_id: u16,
) -> Result<Vec<uefi_proto::QuestionSummary>, HiiError> {
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut node = &image.root;
    for &i in &path {
        node = &node.children[i];
    }
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let mut file = &image.root;
    for &i in &path[..path.len() - 1] {
        file = &file.children[i];
    }
    let titles = questions::prompt_texts(file);
    let mut out = Vec::new();
    for (start, len) in form_package_ranges(node) {
        for q in questions::questions(&node.body[start..start + len], form_id) {
            out.push(uefi_proto::QuestionSummary {
                question_id: q.question_id as u32,
                kind: question_kind_str(q.kind).to_string(),
                prompt: titles.get(&q.prompt_sid).cloned().unwrap_or_default(),
                var_store_id: q.var_store_id as u32,
                var_offset: q.var_offset as u32,
                width: q.width as u32,
            });
        }
    }
    Ok(out)
}

fn validate_set_value(map: &values::QuestionMap, value: u64) -> Result<u8, HiiError> {
    if matches!(map.kind, values::QuestionKind::Other) {
        return Err(HiiError::ValueOpUnsupported(
            "question kind is not value-settable".into(),
        ));
    }
    if map.width == 0 || map.width > 8 {
        return Err(HiiError::ValueOpUnsupported(format!(
            "question width {} is not settable",
            map.width
        )));
    }
    let bits = 8 * map.width as u32;
    if bits < 64 && value >= 1u64 << bits {
        return Err(HiiError::ValueOpUnsupported(format!(
            "value {value} does not fit in {bits} bits"
        )));
    }
    match map.kind {
        values::QuestionKind::CheckBox if value > 1 => Err(HiiError::ValueOpUnsupported(
            "checkbox accepts only 0 or 1".into(),
        )),
        values::QuestionKind::OneOf if !map.options.iter().any(|o| o.value == value) => {
            let allowed = map
                .options
                .iter()
                .map(|o| o.value.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(HiiError::ValueOpUnsupported(format!(
                "value {value} is not among one_of options [{allowed}]"
            )))
        }
        values::QuestionKind::Numeric if !(map.min..=map.max).contains(&value) => {
            Err(HiiError::ValueOpUnsupported(format!(
                "value {value} is outside numeric range {}..={}",
                map.min, map.max
            )))
        }
        _ => Ok(map.width),
    }
}

#[derive(Debug)]
pub struct ValueOutcome {
    pub question: uefi_proto::QuestionInfo,
    pub applied: Vec<String>,
    pub stores: Vec<String>,
}

struct StoreHit {
    path: Vec<usize>,
    desc: String,
    body_offset: usize,
}

fn store_desc(node: &FfsNode, file_guid: Option<&Guid>) -> String {
    let guid_text = file_guid
        .map(crate::guid_to_upper_string)
        .unwrap_or_else(|| "unknown".into());
    if node.node_type == FfsType::File {
        format!("file {guid_text} (raw body)")
    } else {
        format!("file {guid_text} section {:#04x} raw body", node.subtype)
    }
}

fn collect_std_defaults_hits(
    node: &FfsNode,
    path: &mut Vec<usize>,
    barrier: bool,
    file_guid: Option<&Guid>,
    name: &str,
    data_len: usize,
    out: &mut Vec<StoreHit>,
) -> Result<(), HiiError> {
    let own_file_guid = if node.node_type == FfsType::File {
        node.guid.as_ref()
    } else {
        file_guid
    };
    let is_store_body = |n: &FfsNode| {
        matches!(n.node_type, FfsType::File | FfsType::Section)
            && (n.node_type != FfsType::File || n.children.is_empty())
            && nvar::is_std_defaults(&n.body)
    };
    if is_store_body(node) {
        if barrier {
            return Err(HiiError::MutationBehindCompression);
        }
        if let Some((off, _)) = nvar::find_varstore_record(&node.body, name, data_len) {
            out.push(StoreHit {
                path: path.clone(),
                desc: store_desc(node, own_file_guid),
                body_offset: off,
            });
        } else {
            return Err(HiiError::ValueOpUnsupported(format!(
                "StdDefaults store {} has no record {:?} of {} bytes",
                store_desc(node, own_file_guid),
                name,
                data_len
            )));
        }
        return Ok(());
    }
    let child_barrier = barrier
        || (node.node_type == FfsType::Section
            && (node.subtype == EFI_SECTION_COMPRESSION
                || node.subtype == EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &node.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            ));
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_std_defaults_hits(
            child,
            path,
            child_barrier,
            own_file_guid,
            name,
            data_len,
            out,
        )?;
        path.pop();
    }
    Ok(())
}

pub(crate) fn node_at<'a>(root: &'a FfsNode, path: &[usize]) -> &'a FfsNode {
    let mut node = root;
    for &i in path {
        node = &node.children[i];
    }
    node
}

pub(crate) fn node_at_mut<'a>(root: &'a mut FfsNode, path: &[usize]) -> &'a mut FfsNode {
    let mut node = root;
    for &i in path {
        node = &mut node.children[i];
    }
    node
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, value), err)]
pub fn set_value(image: &mut Image, item_id: &str, value: u64) -> Result<ValueOutcome, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    let Some(question_id) = question_id else {
        return Err(HiiError::NotFound);
    };
    let (map, texts) = find_question_map(image, &target, form_id, question_id)?;
    let info = question_info_proto(form_id, &map, &texts);
    let width = validate_set_value(&map, value)?;
    let varstore = map.varstore.as_ref().ok_or_else(|| {
        HiiError::ValueOpUnsupported("question varstore is not declared in the form package".into())
    })?;
    if varstore.name.is_empty() {
        return Err(HiiError::ValueOpUnsupported(
            "varstore has no name; cannot match a StdDefaults record".into(),
        ));
    }
    if map.var_offset as usize + width as usize > varstore.size as usize {
        return Err(HiiError::ValueOpUnsupported(format!(
            "var_offset {:#x} + width {} exceeds varstore size {:#x}",
            map.var_offset, width, varstore.size
        )));
    }
    let mut hits = Vec::new();
    let mut path = Vec::new();
    collect_std_defaults_hits(
        &image.root,
        &mut path,
        false,
        None,
        &varstore.name,
        varstore.size as usize,
        &mut hits,
    )?;
    if hits.is_empty() {
        return Err(HiiError::ValueOpUnsupported(
            "no NVAR StdDefaults stores found in the image".into(),
        ));
    }
    struct Plan {
        hit_index: usize,
        from: Vec<u8>,
        to: Vec<u8>,
    }
    let mut plans = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        let node = node_at(&image.root, &hit.path);
        let off = hit.body_offset + map.var_offset as usize;
        let from = node
            .body
            .get(off..off + width as usize)
            .ok_or_else(|| HiiError::ValueOpUnsupported("record data out of bounds".into()))?
            .to_vec();
        let mut to = value.to_le_bytes().to_vec();
        to.truncate(width as usize);
        plans.push(Plan {
            hit_index: i,
            from,
            to,
        });
    }
    let mut applied = Vec::new();
    for plan in &plans {
        let hit = &hits[plan.hit_index];
        if plan.from == plan.to {
            continue;
        }
        let off = hit.body_offset + map.var_offset as usize;
        {
            let node = node_at_mut(&mut image.root, &hit.path);
            node.body[off..off + width as usize].copy_from_slice(&plan.to);
        }
        ops::mark_rebuild_to_root_by_path(&mut image.root, &hit.path);
        applied.push(format!(
            "{} store+{:#x}: {} -> {}",
            hit.desc,
            off,
            hex(&plan.from),
            hex(&plan.to)
        ));
    }
    tracing::debug!(stores = hits.len(), "set_value done");
    Ok(ValueOutcome {
        question: info,
        applied,
        stores: hits.iter().map(|h| h.desc.clone()).collect(),
    })
}

#[derive(Debug)]
pub struct AddQuestionResult {
    pub question_id: u16,
    pub string_ids: HashMap<String, u16>,
    pub spf_record_offset: usize,
}

fn validate_question_add(
    schema: &schema::QuestionAddSchema,
) -> Result<(Option<u64>, Option<u64>), HiiError> {
    if schema.size != 1 {
        return Err(HiiError::InvalidSchema(format!(
            "size {} violates one_of u8 semantics (must be 1)",
            schema.size
        )));
    }
    if schema.options.is_empty() {
        return Err(HiiError::InvalidSchema("options must not be empty".into()));
    }
    for o in &schema.options {
        if o.value > u8::MAX as u64 {
            return Err(HiiError::InvalidSchema(format!(
                "option value {:#x} does not fit one_of u8 semantics",
                o.value
            )));
        }
    }
    let marked = |class: schema::DefaultClass| {
        schema
            .options
            .iter()
            .filter(|o| o.default == Some(class))
            .count()
    };
    if marked(schema::DefaultClass::Optimized) > 1 {
        return Err(HiiError::InvalidSchema(
            "only one option may carry default \"optimized\"".into(),
        ));
    }
    if marked(schema::DefaultClass::Failsafe) > 1 {
        return Err(HiiError::InvalidSchema(
            "only one option may carry default \"failsafe\"".into(),
        ));
    }
    let optimized_opt = schema
        .options
        .iter()
        .find(|o| o.default == Some(schema::DefaultClass::Optimized));
    let optimized = match (optimized_opt, schema.defaults) {
        (Some(o), Some(d)) if o.value != d.optimized => {
            return Err(HiiError::InvalidSchema(format!(
                "defaults.optimized {} disagrees with the option marked optimized ({})",
                d.optimized, o.value
            )));
        }
        (Some(o), _) => Some(o.value),
        (None, Some(d)) => Some(d.optimized),
        (None, None) => None,
    };
    if let Some(v) = optimized
        && !schema.options.iter().any(|o| o.value == v)
    {
        return Err(HiiError::InvalidSchema(format!(
            "optimized default {v} is not among option values"
        )));
    }
    let failsafe = schema
        .options
        .iter()
        .find(|o| o.default == Some(schema::DefaultClass::Failsafe))
        .map(|o| o.value);
    Ok((optimized, failsafe))
}

fn question_forms_package<'a>(
    root: &'a FfsNode,
    target: &crate::types::Target,
    bare_channel: bool,
) -> Result<&'a [u8], HiiError> {
    let node = crate::parser::target::find_item(root, target).map_err(|_| HiiError::NotFound)?;
    if bare_channel {
        Ok(&node.body)
    } else {
        let (off, len) =
            form_add::resource_forms_package(&node.body).ok_or(HiiError::NotASetupItem)?;
        node.body.get(off..off + len).ok_or(HiiError::InvalidIfr)
    }
}

struct RsrcSpliceCheck {
    pkg_off: usize,
    old_len: usize,
    blob_end: usize,
    new_blob_len: u32,
    new_total: u32,
}

fn check_rsrc_question_splice(
    pe: &[u8],
    form_id: u16,
    ops_len: usize,
) -> Result<RsrcSpliceCheck, HiiError> {
    let (pkg_off, old_len) = form_add::resource_forms_package(pe).ok_or(HiiError::NotASetupItem)?;
    let pkg = pe
        .get(pkg_off..pkg_off + old_len)
        .ok_or(HiiError::InvalidIfr)?;
    ifr::locate_form_end(pkg, 0, form_id).ok_or(HiiError::NotFound)?;
    let (_, blob_off, blob_len) = pe_resource::hii_entry_locations(pe)
        .first()
        .copied()
        .ok_or(HiiError::InvalidIfr)?;
    let blob_end = blob_off.checked_add(blob_len).ok_or(HiiError::InvalidIfr)?;
    let blob = pe.get(blob_off..blob_end).ok_or(HiiError::InvalidIfr)?;
    let list = package_list::parse_package_list(blob).ok_or(HiiError::InvalidIfr)?;
    let sum: usize = list.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len
        .checked_add(ops_len)
        .ok_or(HiiError::PeGrowthUnsupported)?;
    let new_total = 20u64 + sum as u64 + ops_len as u64 + 4;
    if new_total > u32::MAX as u64 || new_blob_len > u32::MAX as usize {
        return Err(HiiError::PeGrowthUnsupported);
    }
    let plan =
        pe_resource::plan_rsrc_blob_growth(pe, ops_len).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !pe_resource::can_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    Ok(RsrcSpliceCheck {
        pkg_off,
        old_len,
        blob_end,
        new_blob_len: new_blob_len as u32,
        new_total: new_total as u32,
    })
}

fn splice_question_ops_into_resource(
    pe: &mut Vec<u8>,
    form_id: u16,
    ops: &[u8],
) -> Result<(usize, usize), HiiError> {
    let chk = check_rsrc_question_splice(pe, form_id, ops.len())?;
    let mut pkg = pe[chk.pkg_off..chk.pkg_off + chk.old_len].to_vec();
    let res = ifr::splice_question_ops(&mut pkg, 0, form_id, ops)?;
    let delta = pkg.len() - chk.old_len;
    let plan =
        pe_resource::plan_rsrc_blob_growth(pe, delta).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !pe_resource::try_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    pe.copy_within(
        chk.pkg_off + chk.old_len..chk.blob_end,
        chk.pkg_off + pkg.len(),
    );
    pe[chk.pkg_off..chk.pkg_off + pkg.len()].copy_from_slice(&pkg);
    pe_resource::write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        chk.new_blob_len,
        chk.new_total,
    );
    Ok(res)
}

struct SpfAppendPlan {
    page_slot: usize,
    page_offset: usize,
    record_template: usize,
    ctrl_template: usize,
    counter: u32,
    selected_records: Vec<usize>,
}

pub fn spf_record_resolves(pkg: &[u8], question_id: u16, ifr_offset: u32) -> bool {
    let ifr = ifr_offset as usize;
    ifr + 8 <= pkg.len()
        && values::is_question_op(pkg[ifr])
        && u16::from_le_bytes([pkg[ifr + 6], pkg[ifr + 7]]) == question_id
}

fn record_counter(body: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        body[offset + spf::SPF_RECORD_COUNTER_OFFSET..offset + spf::SPF_RECORD_COUNTER_OFFSET + 4]
            .try_into()
            .unwrap(),
    )
}

fn select_resolving_records(records: &[spf::SpfQuestionRecord], pkg: &[u8]) -> Vec<usize> {
    records
        .iter()
        .filter(|r| spf_record_resolves(pkg, r.question_id, r.ifr_offset))
        .map(|r| r.offset)
        .collect()
}

fn plan_spf_append(
    body: &[u8],
    pkg: &[u8],
    form_start: u32,
    form_end: u32,
    form_id: u16,
) -> Result<SpfAppendPlan, HiiError> {
    let base = spf::container_start(body).ok_or(HiiError::NotFound)?;
    let records = spf::scan_question_records(body);
    let first = records.first().ok_or(HiiError::NotFound)?;
    let template = records
        .iter()
        .find(|r| r.ifr_offset >= form_start && r.ifr_offset < form_end)
        .unwrap_or(first);
    let max_low = records
        .iter()
        .map(|r| record_counter(body, r.offset) & 0xFFFF)
        .max()
        .unwrap_or(0);
    let counter = (0x0001u32 << 16) | (1 + max_low);
    let selected_records = select_resolving_records(&records, pkg);
    let count_at = base + spf::SPF_PAGE_COUNT_OFFSET;
    let count_bytes = body.get(count_at..count_at + 4).ok_or(HiiError::NotFound)?;
    let page_count = u32::from_le_bytes(count_bytes.try_into().unwrap()) as usize;
    let mut page_slot = None;
    let mut page_offset = 0usize;
    for slot in 0..page_count {
        let at = base + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot;
        let Some(slot_bytes) = body.get(at..at + 4) else {
            break;
        };
        let off = u32::from_le_bytes(slot_bytes.try_into().unwrap()) as usize;
        if off == 0 {
            continue;
        }
        let fid_at = base + off + 0xA;
        if let Some(fb) = body.get(fid_at..fid_at + 2)
            && u16::from_le_bytes(fb.try_into().unwrap()) == form_id
        {
            page_slot = Some(slot);
            page_offset = off;
            break;
        }
    }
    let page_slot = page_slot.ok_or(HiiError::NotFound)?;
    let controls = spf::scan_string_controls(body);
    let ctrl_template = controls
        .iter()
        .find(|c| c.offset >= base + page_offset)
        .unwrap_or(controls.first().ok_or(HiiError::NotFound)?)
        .offset
        - base;
    Ok(SpfAppendPlan {
        page_slot,
        page_offset,
        record_template: template.offset - base,
        ctrl_template,
        counter,
        selected_records,
    })
}

fn apply_spf_ifr_fixup(body: &mut [u8], plan: &SpfAppendPlan, insert_at: u32, delta: u32) {
    spf::fixup_selected_record_ifr_offsets(body, &plan.selected_records, insert_at, delta);
}

#[allow(clippy::too_many_arguments)]
fn apply_spf_question(
    node: &mut FfsNode,
    plan: &SpfAppendPlan,
    schema: &schema::QuestionAddSchema,
    prompt_id: u16,
    help_id: u16,
    insert_at: u32,
    delta: u32,
    optimized: Option<u64>,
    failsafe: Option<u64>,
) -> Result<usize, HiiError> {
    apply_spf_ifr_fixup(&mut node.body, plan, insert_at, delta);
    let body = &mut node.body;
    let optimal = optimized.map_or(0, |v| v as u8);
    let failsafe_v = failsafe.map_or(0, |v| v as u8);
    let rec_off = spf::append_question_record(
        body,
        plan.record_template,
        schema.question_id,
        help_id,
        prompt_id,
        insert_at,
        plan.counter,
        failsafe_v,
        optimal,
    );
    spf::append_string_control(body, plan.ctrl_template, help_id);
    let clone_off = spf::clone_page_with_controls(body, plan.page_offset, &[rec_off as u32]);
    spf::repoint_page_slot(body, plan.page_slot, clone_off as u32);
    let base = spf::container_start(body).expect("$SPF survives appends");
    let new_len = body.len() - base;
    spf::bump_container_length(body, new_len);
    Ok(rec_off)
}

fn build_question_ops(
    schema: &schema::QuestionAddSchema,
    prompt_id: u16,
    help_id: u16,
    string_id_of: impl Fn(&str) -> Option<u16>,
    optimized: Option<u64>,
) -> Result<Vec<u8>, HiiError> {
    let mut b = ifr_builder::IfrBuilder::new();
    b.emit_one_of(
        prompt_id,
        help_id,
        schema.question_id,
        schema.var_store_id,
        schema.var_offset,
        0,
        1,
    );
    for o in &schema.options {
        let flags = match o.default {
            Some(schema::DefaultClass::Optimized) => ifr_builder::IFR_OPTION_DEFAULT,
            Some(schema::DefaultClass::Failsafe) => ifr_builder::IFR_OPTION_DEFAULT_MFG,
            None => 0,
        };
        let text_id = string_id_of(&o.text).ok_or(HiiError::InvalidIfr)?;
        b.emit_one_of_option(text_id, flags, ifr_builder::TYPE_NUM_SIZE_8, o.value, 1);
    }
    if let Some(v) = optimized {
        b.emit_default(
            ifr_builder::DEFAULT_ID_STANDARD,
            ifr_builder::TYPE_NUM_SIZE_8,
            v,
            1,
        );
    }
    b.emit_end();
    Ok(b.build())
}

fn preflight_question_splice(
    image: &Image,
    target: &crate::types::Target,
    bare_channel: bool,
    form_id: u16,
    ops_len: usize,
    strings: &[String],
) -> Result<(), HiiError> {
    let node =
        crate::parser::target::find_item(&image.root, target).map_err(|_| HiiError::NotFound)?;
    if bare_channel {
        return ifr::locate_form_end(&node.body, 0, form_id)
            .map(|_| ())
            .ok_or(HiiError::NotFound);
    }
    let mut post_strings = node.body.clone();
    match string_pack::add_strings_to_resource(&mut post_strings, strings) {
        Ok(_) => {}
        Err(string_pack::AddStringsToResourceError::NotFound) => {
            return Err(HiiError::StringPackageNotFound);
        }
        Err(string_pack::AddStringsToResourceError::GrowthUnsupported) => {
            return Err(HiiError::PeGrowthUnsupported);
        }
    }
    check_rsrc_question_splice(&post_strings, form_id, ops_len)?;
    Ok(())
}

struct QuestionTarget {
    target: crate::types::Target,
    form_id: u16,
    bare_channel: bool,
    pkg: Vec<u8>,
    span: form_hijack::HijackFormSpan,
}

fn resolve_question_target(image: &Image, item_id: &str) -> Result<QuestionTarget, HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    if question_id.is_some() {
        return Err(HiiError::NotFound);
    }
    let bare_channel = {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if node.subtype == EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
            true
        } else if node.subtype == EFI_SECTION_PE32
            && form_add::resource_forms_package(&node.body).is_some()
        {
            false
        } else {
            return Err(HiiError::NotASetupItem);
        }
    };
    let pkg = question_forms_package(&image.root, &target, bare_channel)?.to_vec();
    let span = form_hijack::locate_form(&pkg, form_id).ok_or(HiiError::NotFound)?;
    Ok(QuestionTarget {
        target,
        form_id,
        bare_channel,
        pkg,
        span,
    })
}

fn check_question_slots(
    schema: &schema::QuestionAddSchema,
    pkg: &[u8],
    pending: &[(u16, u16, u32, u32)],
) -> Result<(), HiiError> {
    let slots = values::scan_question_slots(pkg);
    if slots.iter().any(|s| s.question_id == schema.question_id) {
        return Err(HiiError::InvalidSchema(format!(
            "question id {:#x} already exists in the formset",
            schema.question_id
        )));
    }
    let n_start = u32::from(schema.var_offset);
    let n_end = n_start + u32::from(schema.size);
    for s in slots
        .iter()
        .filter(|s| s.var_store_id == schema.var_store_id)
    {
        let width = if s.width == 0 { 1 } else { u16::from(s.width) };
        let s_start = u32::from(s.var_offset);
        let s_end = s_start + u32::from(width);
        if n_start < s_end && s_start < n_end {
            return Err(HiiError::InvalidSchema(format!(
                "var_offset {:#x} overlaps question {:#x} at var_offset {:#x} in var store {}",
                schema.var_offset, s.question_id, s.var_offset, schema.var_store_id
            )));
        }
    }
    for &(qid, vsid, s_start, s_end) in pending {
        if qid == schema.question_id {
            return Err(HiiError::InvalidSchema(format!(
                "question id {:#x} duplicates an earlier question in the same request",
                schema.question_id
            )));
        }
        if vsid == schema.var_store_id && n_start < s_end && s_start < n_end {
            return Err(HiiError::InvalidSchema(format!(
                "var_offset {:#x} overlaps question {:#x} in the same request",
                schema.var_offset, qid
            )));
        }
    }
    let varstores = values::varstore_map(pkg);
    let vs = varstores
        .iter()
        .find(|v| v.id == schema.var_store_id)
        .ok_or_else(|| {
            HiiError::InvalidSchema(format!(
                "var store {} is not declared in the formset",
                schema.var_store_id
            ))
        })?;
    if n_start + u32::from(schema.size) > u32::from(vs.size) {
        return Err(HiiError::InvalidSchema(format!(
            "var_offset {:#x} + size {} exceeds var store size {:#x}",
            schema.var_offset, schema.size, vs.size
        )));
    }
    Ok(())
}

#[tracing::instrument(level = "debug", skip(image, schema), fields(item_id = %item_id), err)]
pub fn add_question(
    image: &mut Image,
    item_id: &str,
    schema: &schema::QuestionAddSchema,
) -> Result<AddQuestionResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let qt = resolve_question_target(image, item_id)?;
    if schema.form_id != qt.form_id {
        return Err(HiiError::InvalidSchema(format!(
            "schema form_id {} does not match target form {}",
            schema.form_id, qt.form_id
        )));
    }
    let (optimized, failsafe) = validate_question_add(schema)?;
    let path = resolve_writable_path(image, &qt.target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    let spf_plan = {
        let body = node_at(&image.root, &sd_path).body.clone();
        plan_spf_append(
            &body,
            &qt.pkg,
            qt.span.form_op as u32,
            qt.span.next_form_op as u32,
            qt.form_id,
        )?
    };
    check_question_slots(schema, &qt.pkg, &[])?;

    let mut strings: Vec<String> = vec![schema.prompt.clone(), schema.help.clone()];
    strings.extend(schema.options.iter().map(|o| o.text.clone()));
    strings.dedup();
    preflight_question_splice(
        image,
        &qt.target,
        qt.bare_channel,
        qt.form_id,
        build_question_ops(schema, 0, 0, |_| Some(0), optimized)?.len(),
        &strings,
    )?;
    let string_ids = if qt.bare_channel {
        let owner = form_add::owner_guid_by_path(&image.root, &path);
        string_pack::add_strings(image, owner.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &qt.target)
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

    let prompt_id = *string_ids.get(&schema.prompt).ok_or(HiiError::InvalidIfr)?;
    let help_id = *string_ids.get(&schema.help).ok_or(HiiError::InvalidIfr)?;
    let ops = build_question_ops(
        schema,
        prompt_id,
        help_id,
        |s| string_ids.get(s).copied(),
        optimized,
    )?;
    let (insert_at, delta) = {
        let node = crate::parser::target::find_item_mut(&mut image.root, &qt.target)
            .map_err(|_| HiiError::NotFound)?;
        if qt.bare_channel {
            ifr::splice_question_ops(&mut node.body, 0, qt.form_id, &ops)?
        } else {
            splice_question_ops_into_resource(&mut node.body, qt.form_id, &ops)?
        }
    };
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);

    let spf_record_offset = {
        let node = node_at_mut(&mut image.root, &sd_path);
        apply_spf_question(
            node,
            &spf_plan,
            schema,
            prompt_id,
            help_id,
            insert_at as u32,
            delta as u32,
            optimized,
            failsafe,
        )?
    };
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    tracing::debug!(
        question_id = schema.question_id,
        insert_at,
        "add_question done"
    );
    Ok(AddQuestionResult {
        question_id: schema.question_id,
        string_ids,
        spf_record_offset,
    })
}

pub fn check_question_add(
    image: &Image,
    item_id: &str,
    schemas: &[schema::QuestionAddSchema],
) -> Result<(), HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let qt = resolve_question_target(image, item_id)?;
    for schema in schemas {
        if schema.form_id != qt.form_id {
            return Err(HiiError::InvalidSchema(format!(
                "schema form_id {} does not match target form {}",
                schema.form_id, qt.form_id
            )));
        }
    }
    resolve_writable_path(image, &qt.target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    {
        let body = node_at(&image.root, &sd_path).body.clone();
        plan_spf_append(
            &body,
            &qt.pkg,
            qt.span.form_op as u32,
            qt.span.next_form_op as u32,
            qt.form_id,
        )?;
    }
    let mut pending: Vec<(u16, u16, u32, u32)> = Vec::new();
    for schema in schemas {
        let (optimized, _) = validate_question_add(schema)?;
        check_question_slots(schema, &qt.pkg, &pending)?;
        let mut strings: Vec<String> = vec![schema.prompt.clone(), schema.help.clone()];
        strings.extend(schema.options.iter().map(|o| o.text.clone()));
        strings.dedup();
        preflight_question_splice(
            image,
            &qt.target,
            qt.bare_channel,
            qt.form_id,
            build_question_ops(schema, 0, 0, |_| Some(0), optimized)?.len(),
            &strings,
        )?;
        pending.push((
            schema.question_id,
            schema.var_store_id,
            u32::from(schema.var_offset),
            u32::from(schema.var_offset) + u32::from(schema.size),
        ));
    }
    Ok(())
}

fn build_ref_ops(
    schema: &schema::QuestionAddRefSchema,
    prompt_id: u16,
    help_id: u16,
) -> Result<Vec<u8>, HiiError> {
    let mut b = ifr_builder::IfrBuilder::new();
    match &schema.formset_guid {
        None => b.emit_ref(
            prompt_id,
            help_id,
            schema.question_id,
            0,
            0xFFFF,
            schema.form_id,
        ),
        Some(gs) => {
            let g = crate::types::Guid::try_parse(gs).map_err(|_| {
                HiiError::InvalidSchema(format!("formset_guid '{gs}' is not a GUID"))
            })?;
            b.emit_ref3(prompt_id, help_id, schema.question_id, schema.form_id, &g);
        }
    }
    Ok(b.build())
}

fn check_ref_slots(
    schema: &schema::QuestionAddRefSchema,
    pkg: &[u8],
    pending_qids: &[u16],
) -> Result<(), HiiError> {
    let slots = values::scan_question_slots(pkg);
    if slots.iter().any(|s| s.question_id == schema.question_id) {
        return Err(HiiError::InvalidSchema(format!(
            "question id {:#x} already exists in the formset",
            schema.question_id
        )));
    }
    if pending_qids.contains(&schema.question_id) {
        return Err(HiiError::InvalidSchema(format!(
            "question id {:#x} duplicates an earlier question in the same request",
            schema.question_id
        )));
    }
    Ok(())
}

#[derive(Debug)]
pub struct AddRefResult {
    pub question_id: u16,
    pub string_ids: HashMap<String, u16>,
}

#[tracing::instrument(level = "debug", skip(image, schema), fields(item_id = %item_id), err)]
pub fn add_ref(
    image: &mut Image,
    item_id: &str,
    schema: &schema::QuestionAddRefSchema,
) -> Result<AddRefResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let qt = resolve_question_target(image, item_id)?;
    let path = resolve_writable_path(image, &qt.target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    let spf_plan = {
        let body = node_at(&image.root, &sd_path).body.clone();
        plan_spf_append(
            &body,
            &qt.pkg,
            qt.span.form_op as u32,
            qt.span.next_form_op as u32,
            qt.form_id,
        )?
    };
    check_ref_slots(schema, &qt.pkg, &[])?;

    let mut strings: Vec<String> = vec![schema.prompt.clone(), schema.help.clone()];
    strings.dedup();
    preflight_question_splice(
        image,
        &qt.target,
        qt.bare_channel,
        qt.form_id,
        build_ref_ops(schema, 0, 0)?.len(),
        &strings,
    )?;
    let string_ids = if qt.bare_channel {
        let owner = form_add::owner_guid_by_path(&image.root, &path);
        string_pack::add_strings(image, owner.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &qt.target)
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

    let prompt_id = *string_ids.get(&schema.prompt).ok_or(HiiError::InvalidIfr)?;
    let help_id = *string_ids.get(&schema.help).ok_or(HiiError::InvalidIfr)?;
    let ops = build_ref_ops(schema, prompt_id, help_id)?;
    let (insert_at, delta) = {
        let node = crate::parser::target::find_item_mut(&mut image.root, &qt.target)
            .map_err(|_| HiiError::NotFound)?;
        if qt.bare_channel {
            ifr::splice_question_ops(&mut node.body, 0, qt.form_id, &ops)?
        } else {
            splice_question_ops_into_resource(&mut node.body, qt.form_id, &ops)?
        }
    };
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);

    {
        let node = node_at_mut(&mut image.root, &sd_path);
        apply_spf_ifr_fixup(&mut node.body, &spf_plan, insert_at as u32, delta as u32);
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    tracing::debug!(question_id = schema.question_id, insert_at, "add_ref done");
    Ok(AddRefResult {
        question_id: schema.question_id,
        string_ids,
    })
}

pub fn check_ref_add(
    image: &Image,
    item_id: &str,
    refs: &[schema::QuestionAddRefSchema],
    question_qids: &[u16],
) -> Result<(), HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let qt = resolve_question_target(image, item_id)?;
    resolve_writable_path(image, &qt.target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    {
        let body = node_at(&image.root, &sd_path).body.clone();
        plan_spf_append(
            &body,
            &qt.pkg,
            qt.span.form_op as u32,
            qt.span.next_form_op as u32,
            qt.form_id,
        )?;
    }
    let mut pending: Vec<u16> = question_qids.to_vec();
    for schema in refs {
        check_ref_slots(schema, &qt.pkg, &pending)?;
        let mut strings: Vec<String> = vec![schema.prompt.clone(), schema.help.clone()];
        strings.dedup();
        preflight_question_splice(
            image,
            &qt.target,
            qt.bare_channel,
            qt.form_id,
            build_ref_ops(schema, 0, 0)?.len(),
            &strings,
        )?;
        pending.push(schema.question_id);
    }
    Ok(())
}

fn spf_page_registration_slot(body: &[u8], form_id: u16) -> Result<u16, HiiError> {
    let base = spf::container_start(body).ok_or(HiiError::NotFound)?;
    let count_at = base + spf::SPF_PAGE_COUNT_OFFSET;
    let count_bytes = body.get(count_at..count_at + 4).ok_or(HiiError::NotFound)?;
    let count = u32::from_le_bytes(count_bytes.try_into().unwrap());
    for slot in 0..count as usize {
        let at = base + spf::SPF_PAGE_TABLE_OFFSET + 4 * slot;
        let Some(slot_bytes) = body.get(at..at + 4) else {
            break;
        };
        let off = u32::from_le_bytes(slot_bytes.try_into().unwrap()) as usize;
        if off == 0 {
            continue;
        }
        let fid_at = base + off + spf::SPF_PAGE_FORM_ID_OFFSET;
        if let Some(fb) = body.get(fid_at..fid_at + 2)
            && u16::from_le_bytes(fb.try_into().unwrap()) == form_id
        {
            return Err(HiiError::InvalidSchema(format!(
                "page {form_id} is already registered in the $SPF page table"
            )));
        }
    }
    let gap_at = base + spf::SPF_PAGE_TABLE_OFFSET + 4 * count as usize;
    let gap = body.get(gap_at..gap_at + 4).ok_or(HiiError::NotFound)?;
    if u32::from_le_bytes(gap.try_into().unwrap()) != 0 {
        return Err(HiiError::InvalidSchema("page table gap occupied".into()));
    }
    Ok(count as u16)
}

#[derive(Debug)]
pub struct AddPageResult {
    pub form_id: u16,
    pub slot: usize,
    pub page_offset: usize,
    pub title_string_id: u16,
}

#[tracing::instrument(level = "debug", skip(image, schema), fields(item_id = %item_id), err)]
pub fn add_page(
    image: &mut Image,
    item_id: &str,
    schema: &schema::PageAddSchema,
) -> Result<AddPageResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    if schema.title.is_empty() {
        return Err(HiiError::InvalidSchema("title must not be empty".into()));
    }
    let qt = resolve_question_target(image, item_id)?;
    let path = resolve_writable_path(image, &qt.target)?;
    let sd_path = ami_patcher::discover_pfs_payload_path(image)?;
    let spf_plan = {
        let body = node_at(&image.root, &sd_path).body.clone();
        plan_spf_append(
            &body,
            &qt.pkg,
            qt.span.form_op as u32,
            qt.span.next_form_op as u32,
            qt.form_id,
        )?
    };
    let seq = {
        let node = node_at(&image.root, &sd_path);
        spf_page_registration_slot(&node.body, schema.form_id)?
    };

    let strings = vec![schema.title.clone()];
    let string_ids = if qt.bare_channel {
        let owner = form_add::owner_guid_by_path(&image.root, &path);
        string_pack::add_strings(image, owner.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &qt.target)
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
    let title_id = *string_ids.get(&schema.title).ok_or(HiiError::InvalidIfr)?;
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);

    let (slot, page_offset) = {
        let node = node_at_mut(&mut image.root, &sd_path);
        let skeleton = spf::append_page_skeleton(
            &mut node.body,
            spf_plan.page_offset,
            schema.form_id,
            title_id,
            seq,
            spf_plan.page_slot as u16,
        );
        let slot = spf::register_page_slot(&mut node.body, skeleton)
            .ok_or_else(|| HiiError::InvalidSchema("page table gap occupied".into()))?;
        let base = spf::container_start(&node.body).expect("$SPF survives appends");
        let new_len = node.body.len() - base;
        spf::bump_container_length(&mut node.body, new_len);
        (slot, skeleton)
    };
    ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    tracing::debug!(form_id = schema.form_id, slot, "add_page done");
    Ok(AddPageResult {
        form_id: schema.form_id,
        slot,
        page_offset,
        title_string_id: title_id,
    })
}

#[cfg(test)]
pub(crate) mod question_add_fixtures {
    use crate::ffs::{
        EFI_SECTION_FREEFORM_SUBTYPE_GUID, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32,
        EFI_SECTION_RAW, EFI_SECTION_UI,
    };
    use crate::hii::form_hijack::test_fixtures::{
        FILE_GUID, FORMSET_GUID, SETUPDATA_GUID_STR, ffs_file_bytes, file_sections,
        flash_with_files, section_bytes, string_package_bytes, ui_name,
    };
    use crate::hii::ifr_builder::{IfrBuilder, TYPE_NUM_SIZE_8};
    use crate::hii::spf;
    use crate::types::Guid;
    use std::str::FromStr;

    pub(crate) fn question_add_forms_pkg() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        b.emit_form_set(&g, 1, 1, &[]);
        b.emit_var_store(1, &g, 0x100, "Setup");
        b.emit_form(10019, 1);
        b.emit_one_of(0x01A3, 0x01A4, 0x3B, 1, 0x3A, 0, 1);
        b.emit_one_of_option(4, 0x30, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_one_of_option(3, 0x00, TYPE_NUM_SIZE_8, 1, 1);
        b.emit_end();
        b.emit_end();
        b.emit_form(10020, 2);
        b.emit_one_of(0x01A5, 0x01A6, 0x55, 1, 0x40, 0, 1);
        b.emit_one_of_option(5, 0x00, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_end();
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

    pub(crate) fn spf_record(qid: u16, ifr: u32, counter: u32, help: u16, prompt: u16) -> Vec<u8> {
        let mut r = vec![0u8; spf::SPF_RECORD_SIZE];
        r[0..4].copy_from_slice(&(qid as u32).to_le_bytes());
        r[8..10].copy_from_slice(&6u16.to_le_bytes());
        r[12..14].copy_from_slice(&0xFFFFu16.to_le_bytes());
        r[16] = 0x09;
        r[20..22].copy_from_slice(&help.to_le_bytes());
        r[24..28].copy_from_slice(&counter.to_le_bytes());
        r[28..32].copy_from_slice(&ifr.to_le_bytes());
        r[36..44].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        r[44..48].copy_from_slice(&[0x01, 0x00, 0x01, 0x00]);
        r[48..50].copy_from_slice(&prompt.to_le_bytes());
        r
    }

    pub(crate) fn question_add_spf_body(rec0: &[u8], rec1: &[u8], foreign: &[u8]) -> Vec<u8> {
        let mut body = vec![0u8; 0x188];
        body[0x10..0x14].copy_from_slice(b"$SPF");
        body[0x14..0x18].copy_from_slice(&0x200u32.to_le_bytes());
        body[0x18..0x1C].copy_from_slice(&0x210u32.to_le_bytes());
        body[0x1C..0x2C]
            .copy_from_slice(&[0x43, 0xD6, 0x87, 0xEC, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        body[0x2C..0x30].copy_from_slice(&0x48u32.to_le_bytes());
        body[0x40..0x44].copy_from_slice(&0x94u32.to_le_bytes());
        body[0x6C..0x70].copy_from_slice(&0x178u32.to_le_bytes());
        body[0x70..0x74].copy_from_slice(&1u32.to_le_bytes());
        body[0x74..0x78].copy_from_slice(&0x68u32.to_le_bytes());
        body[0x78..0x98].copy_from_slice(&[0u8; 0x20]);
        body[0x82..0x84].copy_from_slice(&10019u16.to_le_bytes());
        body[0x94..0x98].copy_from_slice(&1u32.to_le_bytes());
        body[0x98..0x9C].copy_from_slice(&0x94u32.to_le_bytes());
        body[0xA4..0xEC].copy_from_slice(rec0);
        body[0xEC..0x134].copy_from_slice(rec1);
        body[0x134..0x17C].copy_from_slice(foreign);
        body[0x17C..0x17E].copy_from_slice(&5u16.to_le_bytes());
        body[0x17E..0x180].copy_from_slice(&0x01A4u16.to_le_bytes());
        body[0x180..0x182].copy_from_slice(&0u16.to_le_bytes());
        body[0x182..0x184].copy_from_slice(&0x77u16.to_le_bytes());
        body[0x184..0x186].copy_from_slice(&0x88u16.to_le_bytes());
        body[0x186..0x188].copy_from_slice(&78u16.to_le_bytes());
        body
    }

    pub(crate) fn lzma_guided_section_bytes(children: &[u8]) -> Vec<u8> {
        let mut stream = crate::compress::compress_lzma(children).unwrap();
        stream.resize(stream.len().max(64) + 32, 0x00);
        let mut body = crate::ffs::lzma_guid().to_bytes().to_vec();
        body.extend_from_slice(&0x18u16.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&stream);
        section_bytes(EFI_SECTION_GUID_DEFINED, &body)
    }

    pub(crate) fn hii_list_blob(pkgs: &[&[u8]]) -> Vec<u8> {
        let guid = Guid::from_str(FORMSET_GUID).unwrap();
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, r_efi::hii::PACKAGE_END]);
        b
    }

    pub(crate) fn question_add_spf_body_for(pkg: &[u8]) -> Vec<u8> {
        let q10019 = crate::hii::form_hijack::locate_questions(pkg, 10019)[0].0;
        let q10020 = crate::hii::form_hijack::locate_questions(pkg, 10020)[0].0;
        let rec0 = spf_record(0x3B, q10019 as u32, 0x0001_0066, 0x01A4, 0x01A3);
        let rec1 = spf_record(0x55, q10020 as u32, 0x0001_0066, 0x01A6, 0x01A5);
        let foreign = spf_record(0x66, q10020 as u32, 0x0001_0066, 0x01A8, 0x01A7);
        question_add_spf_body(&rec0, &rec1, &foreign)
    }

    pub(crate) fn sd_file_direct(spf_body: &[u8]) -> Vec<u8> {
        ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_UI, &ui_name("AMITSESetupData")),
                lzma_guided_section_bytes(&section_bytes(
                    EFI_SECTION_FREEFORM_SUBTYPE_GUID,
                    spf_body,
                )),
            ]),
        )
    }

    pub(crate) fn sd_file_nested(spf_body: &[u8]) -> Vec<u8> {
        ffs_file_bytes(
            &Guid::from_str(SETUPDATA_GUID_STR).unwrap(),
            &lzma_guided_section_bytes(&file_sections(&[
                section_bytes(EFI_SECTION_FREEFORM_SUBTYPE_GUID, spf_body),
                section_bytes(EFI_SECTION_UI, &ui_name("AMITSESetupData")),
            ])),
        )
    }

    pub(crate) fn question_add_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&pkg);
        let blob = hii_list_blob(&[&pkg, &string_package_bytes()]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        );
        let sd = sd_file_direct(&spf_body);
        (flash_with_files(vec![setup, sd]), pkg, spf_body)
    }

    pub(crate) fn question_add_bare_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&pkg);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = sd_file_direct(&spf_body);
        (flash_with_files(vec![setup, sd]), pkg, spf_body)
    }

    pub(crate) fn question_add_nested_ui_flash_image() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let pkg = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&pkg);
        let blob = hii_list_blob(&[&pkg, &string_package_bytes()]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        );
        let sd = sd_file_nested(&spf_body);
        (flash_with_files(vec![setup, sd]), pkg, spf_body)
    }

    pub(crate) fn corrupt_last_option_of_form(pkg: &mut [u8], form_id: u16) {
        let span = crate::hii::form_hijack::locate_form(pkg, form_id).unwrap();
        let one_of_op = 2 + 12 + 3;
        let option_op = 2 + 4 + 1;
        let second_option = span.form_op + 6 + one_of_op + option_op;
        pkg[second_option + 1] = 0x7F;
    }

    pub(crate) fn question_add_malformed_resource_flash() -> Vec<u8> {
        let clean = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&clean);
        let mut pkg = clean;
        corrupt_last_option_of_form(&mut pkg, 10019);
        let blob = hii_list_blob(&[&pkg, &string_package_bytes()]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        );
        let sd = sd_file_direct(&spf_body);
        flash_with_files(vec![setup, sd])
    }

    pub(crate) fn question_add_malformed_bare_flash() -> Vec<u8> {
        let clean = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&clean);
        let mut pkg = clean;
        corrupt_last_option_of_form(&mut pkg, 10019);
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &file_sections(&[
                section_bytes(EFI_SECTION_RAW, &pkg),
                section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            ]),
        );
        let sd = sd_file_direct(&spf_body);
        flash_with_files(vec![setup, sd])
    }

    pub(crate) fn question_add_growth_blocked_flash() -> Vec<u8> {
        let pkg = question_add_forms_pkg();
        let spf_body = question_add_spf_body_for(&pkg);
        let blob = hii_list_blob(&[&pkg, &string_package_bytes()]);
        let mut pe = crate::hii::pe_resource::synth_hii_pe("HII", &blob);
        let strings: Vec<String> = vec![
            "Serial Console".into(),
            "Serial console help".into(),
            "Disabled".into(),
            "Enabled".into(),
        ];
        let mut probe = pe.clone();
        crate::hii::string_pack::add_strings_to_resource(&mut probe, &strings).unwrap();
        let blob_len0 = crate::hii::pe_resource::hii_entry_locations(&pe)[0].2;
        let blob_len1 = crate::hii::pe_resource::hii_entry_locations(&probe)[0].2;
        let strings_delta = blob_len1 - blob_len0;
        let old_virt = u32::from_le_bytes(pe[0x150..0x154].try_into().unwrap());
        let old_raw = u32::from_le_bytes(pe[0x158..0x15c].try_into().unwrap());
        pe.resize(pe.len() + strings_delta, 0);
        pe[0x150..0x154].copy_from_slice(&(old_virt + strings_delta as u32).to_le_bytes());
        pe[0x158..0x15c].copy_from_slice(&(old_raw + strings_delta as u32).to_le_bytes());
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let setup = ffs_file_bytes(
            &Guid::from_str(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        );
        let sd = sd_file_direct(&spf_body);
        flash_with_files(vec![setup, sd])
    }
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
        let mut form_pkg = vec![36u8, 0, 0, r_efi::hii::PACKAGE_FORMS];
        form_pkg.extend_from_slice(&[r_efi::hii::IFR_FORM_SET_OP, 0x97]);
        form_pkg.extend_from_slice(&list_guid.to_bytes());
        form_pkg.extend_from_slice(&7u16.to_le_bytes());
        form_pkg.extend_from_slice(&0u16.to_le_bytes());
        form_pkg.push(0u8);
        form_pkg.extend_from_slice(&[0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02]);
        form_pkg.extend_from_slice(&[0x29, 0x02]);
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
        assert_eq!(&blob[47..51], &[0x0A, 0x82, 0x29, 0x02]);
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
    fn set_item_visibility_false_is_hiding_unsupported() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let err = set_item_visibility(&mut image, VENDOR_FORM_ITEM, false).unwrap_err();
        assert!(matches!(err, HiiError::HidingUnsupported));
    }

    #[test]
    fn set_item_visibility_without_own_scope_is_no_suppress_scope() {
        let mut image = vendor_image_with(0x19, vendor_forms_pkg());
        let err = set_item_visibility(&mut image, VENDOR_FORM_ITEM, true).unwrap_err();
        assert!(matches!(err, HiiError::NoSuppressScope));
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
        assert_eq!(gates[0].scope_offset, 0x21);
        assert_eq!(gates[0].flip, "pkg+0x2f: 01 -> 02");
        let qgates = gates_list(&image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(qgates.len(), 1);
        assert_eq!(qgates[0].gate_kind, "grayout");
        assert_eq!(qgates[0].expression, "0x009A == 0x0001");
        assert_eq!(qgates[0].scope_offset, 0x52);
        assert_eq!(qgates[0].flip, "pkg+0x58: 01 00 -> ff ff");
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
                    formset_guid: None,
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
    fn gates_and_unlock_print_pkg_relative_offsets_in_second_package() {
        let list_guid = Guid::try_parse("ABBCE13D-E25A-4D9F-A1F9-2F7710786892").unwrap();
        let pkg1 = forms_pkg([g_form(9), g_end(), g_end()].concat());
        let pkg2 = vendor_forms_pkg();
        let mut list = list_guid.to_bytes().to_vec();
        let total = 20 + pkg1.len() + pkg2.len() + 4;
        list.extend_from_slice(&(total as u32).to_le_bytes());
        list.extend_from_slice(&pkg1);
        list.extend_from_slice(&pkg2);
        list.extend_from_slice(&[4, 0, 0, r_efi::hii::PACKAGE_END]);
        let pe = crate::hii::pe_resource::synth_hii_pe("HII", &list);
        let mut image = vendor_image_with(0x10, pe);
        let item = "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x10:0#10029:0x3B";

        let gates = gates_list(&image, item).unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].scope_offset, 0x52);
        assert_eq!(gates[0].flip, "pkg+0x58: 01 00 -> ff ff");

        let node = &image.root.children[0].children[0].children[0];
        let ranges = form_package_ranges(node);
        assert_eq!(ranges.len(), 2);
        let (start2, _) = ranges[1];
        assert_eq!(
            &node.body[start2 + 0x58..start2 + 0x5A],
            &[0x01, 0x00],
            "байт по напечатанному смещению == from"
        );

        let outcome = unlock(&mut image, item).unwrap();
        assert_eq!(
            outcome.applied,
            vec!["pkg+0x58: 01 00 -> ff ff".to_string()]
        );
        let node = &image.root.children[0].children[0].children[0];
        assert_eq!(&node.body[start2 + 0x58..start2 + 0x5A], &[0xFF, 0xFF]);
        let ranges_after = form_package_ranges(node);
        let (start1, len1) = ranges_after[0];
        assert_eq!(
            &node.body[start1..start1 + len1],
            &pkg1[..],
            "первый пакет нетронут"
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

    #[test]
    fn unlock_skips_already_unlocked_gate() {
        let mut eq = vec![r_efi::hii::IFR_EQ_ID_VAL_OP, 0x06, 0xB4, 0x00];
        eq.extend_from_slice(&0xFFFFu16.to_le_bytes());
        let pkg = forms_pkg(
            [
                g_form(10002),
                g_opcode(r_efi::hii::IFR_SUPPRESS_IF_OP, true, &[]),
                eq,
                g_ref(10029),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        );
        let mut image = vendor_image_with(0x19, pkg.clone());
        let outcome = unlock(&mut image, VENDOR_FORM_ITEM).unwrap();
        assert!(
            outcome.applied.is_empty(),
            "гейт с константой 0xFFFF уже разблокирован — флипать нечего"
        );
        assert_eq!(outcome.gates.len(), 1, "гейт всё равно перечислен в infos");
        assert_eq!(
            image.root.children[0].children[0].children[0].body, pkg,
            "байты пакета не тронуты"
        );
    }

    #[test]
    fn unlock_flips_cross_formset_gate_in_donor_section() {
        let mut image = cross_formset::cross_fixtures::two_file_image();
        let item = "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1";
        let before = cross_formset::cross_fixtures::donor_section_body(&image).to_vec();
        let outcome = unlock(&mut image, item).unwrap();
        assert!(!outcome.applied.is_empty(), "кросс-гейт донора флипнут");
        assert!(
            outcome.gates.iter().any(|g| g.wraps == "cross_ref"
                && g.source_target == "899407D7-99FE-43D8-9A21-79EC328CAC21")
        );
        let after = cross_formset::cross_fixtures::donor_section_body(&image).to_vec();
        assert_ne!(after, before, "донорская секция мутировала in-place");
        assert_eq!(after.len(), before.len(), "unlock is length-preserving");
        let diff: Vec<(usize, u8, u8)> = before
            .iter()
            .zip(after.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .collect();
        assert_eq!(diff.len(), 1, "ровно один флип-байт в донорской секции");
        assert_eq!((diff[0].1, diff[0].2), (1, 2));
        let cross = outcome
            .gates
            .iter()
            .find(|g| g.wraps == "cross_ref")
            .unwrap();
        assert_eq!(cross.flip, format!("pkg+{:#x}: 01 -> 02", diff[0].0));
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::Rebuild,
            "донорская секция помечена на rebuild"
        );
    }

    #[test]
    fn gates_lists_cross_formset_gates_with_source_target() {
        let mut image = cross_formset::cross_fixtures::two_file_image();
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1").unwrap();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].wraps, "cross_ref");
        assert_eq!(
            gates[0].source_target,
            "899407D7-99FE-43D8-9A21-79EC328CAC21"
        );
    }

    #[test]
    fn unlock_without_cross_gates_leaves_donor_untouched() {
        let mut image = cross_formset::cross_fixtures::two_file_image();
        let before = cross_formset::cross_fixtures::donor_section_body(&image).to_vec();
        let outcome = unlock(&mut image, "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#42").unwrap();
        assert!(outcome.applied.is_empty());
        assert!(
            outcome.gates.is_empty(),
            "нет ни своих, ни кросс-гейтов на форму 42"
        );
        assert_eq!(
            cross_formset::cross_fixtures::donor_section_body(&image),
            &before[..],
            "тело донора не тронуто (P0: ноль флипов — ноль мутаций)"
        );
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::NoAction
        );
        assert_eq!(image.root.action, Action::NoAction);
    }

    #[test]
    fn unlock_question_target_skips_cross_phase_and_donor() {
        let mut image = cross_formset::cross_fixtures::two_file_image_with(
            cross_formset::cross_fixtures::donor_coincident_question_pkg(),
            cross_formset::cross_fixtures::target_pkg(),
        );
        let before = cross_formset::cross_fixtures::donor_section_body(&image).to_vec();
        let outcome = unlock(
            &mut image,
            "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1:0x55",
        )
        .unwrap();
        assert!(outcome.applied.is_empty());
        assert!(
            outcome.gates.is_empty(),
            "кросс-инфосов нет: кросс-фаза только для form-таргетов (спека §3 U2a)"
        );
        assert_eq!(
            cross_formset::cross_fixtures::donor_section_body(&image),
            &before[..],
            "донор с совпадающей (form, qid) парой не тронут"
        );
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::NoAction
        );
        assert_eq!(image.root.action, Action::NoAction);
        let listed =
            gates_list(&image, "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1:0x55").unwrap();
        assert!(
            listed.is_empty(),
            "gates_list-кросс-фаза тоже отключена для question-таргетов"
        );
    }

    #[test]
    fn gates_list_no_double_listing_when_own_section_matches_too() {
        let mut image = cross_formset::cross_fixtures::two_file_image_with(
            cross_formset::cross_fixtures::donor_pkg(),
            cross_formset::cross_fixtures::own_cross_pkg(),
        );
        image.mode = ImageMode::Read;
        let gates = gates_list(&image, "ABBCE13D-E25A-4D9F-A1F9-2F7710786892:0x19:0#1").unwrap();
        assert_eq!(
            gates.len(),
            1,
            "гейт собственной секции не дублируется через кросс-фазу"
        );
        assert_eq!(
            gates[0].source_target,
            "899407D7-99FE-43D8-9A21-79EC328CAC21"
        );
    }

    const NVAR_FV0_GUID_STR: &str = "10000000-0000-4000-8000-000000000001";
    const NVAR_FV2_GUID_STR: &str = "20000000-0000-4000-8000-000000000002";

    fn g_varstore(id: u16, size: u16, name: &str) -> Vec<u8> {
        let g = Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&size.to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        p.push(0);
        g_opcode(r_efi::hii::IFR_VARSTORE_OP, false, &p)
    }

    fn g_one_of_varstore(question_id: u16, var_store_id: u16, var_offset: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&0x01A3u16.to_le_bytes());
        p.extend_from_slice(&0x01A4u16.to_le_bytes());
        p.extend_from_slice(&question_id.to_le_bytes());
        p.extend_from_slice(&var_store_id.to_le_bytes());
        p.extend_from_slice(&var_offset.to_le_bytes());
        p.push(0x10);
        p.push(0x10);
        p.extend_from_slice(&[0x00, 0x01, 0x00]);
        g_opcode(r_efi::hii::IFR_ONE_OF_OP, true, &p)
    }

    fn g_option(string_id: u16, flags: u8, value: u8) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&string_id.to_le_bytes());
        p.push(flags);
        p.push(0x00);
        p.push(value);
        g_opcode(r_efi::hii::IFR_ONE_OF_OPTION_OP, false, &p)
    }

    fn value_forms_pkg() -> Vec<u8> {
        forms_pkg(
            [
                g_varstore(1, 0x72, "Setup"),
                g_form(10029),
                g_one_of_varstore(0x3B, 1, 0x3A),
                g_option(4, 0x30, 0),
                g_option(3, 0x00, 1),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        )
    }

    const SIBT_STRING_SCSU: u8 = 0x10;
    const SIBT_END: u8 = 0x00;

    fn test_string_pkg() -> Vec<u8> {
        let texts = ["Main", "Hidden", "Enabled"];
        let lang = "eng";
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0u8; 3];
        b.push(r_efi::hii::PACKAGE_STRINGS);
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 44 {
            b.push(0);
        }
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        for t in texts {
            b.push(SIBT_STRING_SCSU);
            b.extend_from_slice(t.as_bytes());
            b.push(0);
        }
        b.push(SIBT_END);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    fn nvar_entry(
        name: Option<&str>,
        data: &[u8],
        attributes: u8,
        guid_index: Option<u8>,
    ) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        let mut body = Vec::new();
        if let Some(gi) = guid_index {
            body.push(gi);
        }
        if let Some(n) = name {
            body.extend_from_slice(n.as_bytes());
            body.push(0);
        }
        body.extend_from_slice(data);
        let size = 10 + body.len();
        e.extend_from_slice(&(size as u16).to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(attributes);
        e.extend_from_slice(&body);
        e
    }

    const SETUP_DECOY_DATA_OFF: usize = 0x28;
    const SETUP_114_DATA_OFF: usize = 0x3F;

    fn nvar_store_body() -> Vec<u8> {
        let mut inner = nvar_entry(Some("Setup"), &[0x11u8; 6], 0x82, Some(0));
        inner.extend_from_slice(&nvar_entry(Some("Setup"), &[0u8; 114], 0x82, Some(0)));
        nvar_entry(Some("StdDefaults"), &inner, 0x82, Some(0))
    }

    fn image_with_nvar_stores() -> Image {
        let mut nvar_file = mk_node(FfsType::File, nvar_store_body(), vec![]);
        nvar_file.guid = Some(Guid::from_str(NVAR_FV0_GUID_STR).unwrap());
        let mut forms_section = mk_node(FfsType::Section, value_forms_pkg(), vec![]);
        forms_section.subtype = 0x19;
        let mut strings_section = mk_node(FfsType::Section, test_string_pkg(), vec![]);
        strings_section.subtype = 0x19;
        let mut forms_file = mk_node(FfsType::File, vec![], vec![forms_section, strings_section]);
        forms_file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());
        let fv0 = mk_node(FfsType::Volume, vec![], vec![nvar_file, forms_file]);

        let mut raw_store = mk_node(FfsType::Section, nvar_store_body(), vec![]);
        raw_store.subtype = 0x19;
        let mut guided = mk_node(FfsType::Section, vec![], vec![raw_store]);
        guided.subtype = EFI_SECTION_GUID_DEFINED;
        guided.parsing_data = ParsingData::GuidedSection(crate::types::GuidedSectionParsingData {
            guid: crate::ffs::lzma_guid(),
            dictionary_size: 0x0080_0000,
        });
        let mut guided_file = mk_node(FfsType::File, vec![], vec![guided]);
        guided_file.guid = Some(Guid::from_str(NVAR_FV2_GUID_STR).unwrap());
        let fv2 = mk_node(FfsType::Volume, vec![], vec![guided_file]);

        let root = mk_node(FfsType::Image, vec![], vec![fv0, fv2]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    fn fv0_store_body(image: &Image) -> &[u8] {
        &image.root.children[0].children[0].body
    }

    fn fv2_store_body(image: &Image) -> &[u8] {
        &image.root.children[1].children[0].children[0].children[0].body
    }

    fn marked_count(node: &FfsNode) -> usize {
        node.children.iter().map(marked_count).sum::<usize>()
            + usize::from(node.action != Action::NoAction)
    }

    #[test]
    fn question_info_reports_4g_like_question() {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        let q = question_info(&image, VENDOR_QUESTION_ITEM).unwrap();
        assert_eq!(q.form_id, 10029);
        assert_eq!(q.question_id, 0x3B);
        assert_eq!(q.kind, "one_of");
        assert_eq!(q.var_store_id, 1);
        assert_eq!(q.var_offset, 0x3A);
        assert_eq!(q.width, 1);
        let vs = q.varstore.expect("varstore resolved");
        assert_eq!(vs.id, 1);
        assert_eq!(vs.name, "Setup");
        assert_eq!(vs.size, 0x72);
        assert_eq!(vs.guid, VENDOR_FORMSET_GUID_STR);
        assert_eq!(
            q.options.iter().map(|o| o.value).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            q.options.iter().map(|o| o.flags).collect::<Vec<_>>(),
            vec![0x30, 0x00]
        );
        assert_eq!(
            q.options
                .iter()
                .map(|o| o.text.as_str())
                .collect::<Vec<_>>(),
            vec!["", "Enabled"],
            "sid 3 резолвится через string-пакет файла, sid 4 в пакете нет — пустой текст"
        );
        assert!(q.defaults.is_empty());
    }

    #[test]
    fn list_questions_reports_vendor_form() {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        let target = format!("{}:0x19:0", VENDOR_FORMSET_GUID_STR.to_lowercase());
        let qs = list_questions(&image, &target, 10029).unwrap();
        assert_eq!(qs.len(), 1);
        let q = &qs[0];
        assert_eq!(q.question_id, 0x3B);
        assert_eq!(q.kind, "one_of");
        assert_eq!(q.var_store_id, 1);
        assert_eq!(q.var_offset, 0x3A);
        assert_eq!(q.width, 1);
        assert!(q.prompt.is_empty(), "fixture has no string package");
    }

    #[test]
    fn list_questions_bad_target_and_unknown_form() {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        assert!(matches!(
            list_questions(&image, "not-a-target", 10029),
            Err(HiiError::NotFound)
        ));
        let target = format!("{}:0x19:0", VENDOR_FORMSET_GUID_STR.to_lowercase());
        assert!(list_questions(&image, &target, 65535).unwrap().is_empty());
    }

    #[test]
    fn question_info_requires_question_discriminator() {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        assert!(matches!(
            question_info(&image, VENDOR_FORM_ITEM),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            question_info(
                &image,
                "5c60f367-a505-419a-859e-2a4ff6ca6fe5:0x19:0#10029:0x77"
            ),
            Err(HiiError::NotFound)
        ));
        assert!(matches!(
            question_info(&image, "10000000-0000-4000-8000-000000000001#10029:0x3B"),
            Err(HiiError::NotASetupItem)
        ));
    }

    #[test]
    fn set_value_flips_both_stores_atomically() {
        let mut image = image_with_nvar_stores();
        let fv0_len = fv0_store_body(&image).len();
        let fv2_len = fv2_store_body(&image).len();
        let outcome = set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        assert_eq!(outcome.applied.len(), 2);
        assert_eq!(outcome.stores.len(), 2);
        assert_eq!(outcome.question.question_id, 0x3B);
        assert_ne!(outcome.stores[0], outcome.stores[1]);
        assert!(outcome.stores[0].contains(NVAR_FV0_GUID_STR));
        assert!(outcome.stores[0].starts_with("file "));
        assert!(outcome.stores[1].contains(NVAR_FV2_GUID_STR));
        assert!(outcome.stores[1].contains("section"));
        assert_eq!(
            image.root.children[0].children[0].body[SETUP_114_DATA_OFF + 0x3A],
            1
        );
        assert_eq!(fv2_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
        assert_eq!(
            &fv0_store_body(&image)[SETUP_DECOY_DATA_OFF..SETUP_DECOY_DATA_OFF + 6],
            &[0x11u8; 6],
            "decoy 6-byte Setup record must not be touched"
        );
        assert_eq!(image.root.children[0].children[0].body.len(), fv0_len);
        assert_eq!(fv2_store_body(&image).len(), fv2_len);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
        assert_eq!(
            image.root.children[1].children[0].children[0].children[0].action,
            Action::Rebuild
        );
        assert_eq!(
            image.root.children[1].children[0].children[0].action,
            Action::Rebuild
        );
        assert_eq!(image.root.children[1].children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[1].action, Action::Rebuild);
        assert_eq!(image.root.action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[1].action, Action::NoAction);
    }

    #[test]
    fn set_value_refuses_read_only() {
        let mut image = image_with_nvar_stores();
        image.mode = ImageMode::Read;
        assert!(matches!(
            set_value(&mut image, VENDOR_QUESTION_ITEM, 1),
            Err(HiiError::NotWritable)
        ));
        assert_eq!(fv0_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 0);
    }

    #[test]
    fn set_value_refuses_value_not_in_options() {
        let mut image = image_with_nvar_stores();
        let err = set_value(&mut image, VENDOR_QUESTION_ITEM, 7).unwrap_err();
        assert!(matches!(err, HiiError::ValueOpUnsupported(ref m) if m.contains("7")));
        assert_eq!(fv0_store_body(&image), nvar_store_body());
        assert_eq!(fv2_store_body(&image), nvar_store_body());
        assert_eq!(image.root.children[0].children[0].action, Action::NoAction);
    }

    #[test]
    fn set_value_refuses_unknown_varstore() {
        let pkg = forms_pkg(
            [
                g_form(10029),
                g_one_of_varstore(0x3B, 1, 0x3A),
                g_option(4, 0x30, 0),
                g_option(3, 0x00, 1),
                g_end(),
                g_end(),
                g_end(),
            ]
            .concat(),
        );
        let mut image = vendor_image_with(0x19, pkg);
        let err = set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap_err();
        assert!(
            matches!(err, HiiError::ValueOpUnsupported(ref m) if m.contains("varstore is not declared"))
        );
    }

    #[test]
    fn set_value_refuses_when_no_stores() {
        let mut image = vendor_image_with(0x19, value_forms_pkg());
        let err = set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap_err();
        assert!(
            matches!(err, HiiError::ValueOpUnsupported(ref m) if m.contains("no NVAR StdDefaults stores"))
        );
        assert_eq!(
            image.root.children[0].children[0].children[0].action,
            Action::NoAction
        );
    }

    #[test]
    fn set_value_refuses_store_behind_non_recompressable() {
        let mut raw_store = mk_node(FfsType::Section, nvar_store_body(), vec![]);
        raw_store.subtype = 0x19;
        let mut guided = mk_node(FfsType::Section, vec![], vec![raw_store]);
        guided.subtype = EFI_SECTION_GUID_DEFINED;
        guided.parsing_data = ParsingData::GuidedSection(crate::types::GuidedSectionParsingData {
            guid: crate::ffs::crc32_guid(),
            dictionary_size: 0,
        });
        let mut nvar_file = mk_node(FfsType::File, vec![], vec![guided]);
        nvar_file.guid = Some(Guid::from_str(NVAR_FV0_GUID_STR).unwrap());

        let mut forms_section = mk_node(FfsType::Section, value_forms_pkg(), vec![]);
        forms_section.subtype = 0x19;
        let mut forms_file = mk_node(FfsType::File, vec![], vec![forms_section]);
        forms_file.guid = Some(Guid::from_str(VENDOR_FORMSET_GUID_STR).unwrap());

        let volume = mk_node(FfsType::Volume, vec![], vec![nvar_file, forms_file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut image = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        assert!(matches!(
            set_value(&mut image, VENDOR_QUESTION_ITEM, 1),
            Err(HiiError::MutationBehindCompression)
        ));
        assert_eq!(
            image.root.children[0].children[0].children[0].children[0].body,
            nvar_store_body()
        );
        assert_eq!(
            image.root.children[0].children[0].children[0].children[0].action,
            Action::NoAction
        );
    }

    #[test]
    fn set_value_repeated_is_noop_report() {
        let mut image = image_with_nvar_stores();
        set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        let fv0_before = fv0_store_body(&image).to_vec();
        let fv2_before = fv2_store_body(&image).to_vec();
        let marks_before = marked_count(&image.root);
        let outcome = set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        assert!(outcome.applied.is_empty());
        assert_eq!(outcome.stores.len(), 2);
        assert_eq!(fv0_store_body(&image), fv0_before);
        assert_eq!(fv2_store_body(&image), fv2_before);
        assert_eq!(marked_count(&image.root), marks_before);
    }

    #[test]
    fn set_value_partial_noop_reports_only_real_changes() {
        let mut image = image_with_nvar_stores();
        image.root.children[0].children[0].body[SETUP_114_DATA_OFF + 0x3A] = 1;
        let fv0_before = fv0_store_body(&image).to_vec();
        let outcome = set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        assert_eq!(outcome.applied.len(), 1);
        assert_eq!(outcome.stores.len(), 2);
        assert!(outcome.applied[0].contains(NVAR_FV2_GUID_STR));
        assert_eq!(fv0_store_body(&image), fv0_before);
        assert_eq!(fv2_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
        assert_eq!(image.root.children[0].children[0].action, Action::NoAction);
        assert_eq!(image.root.children[0].action, Action::NoAction);
        assert_eq!(
            image.root.children[1].children[0].children[0].children[0].action,
            Action::Rebuild
        );
        assert_eq!(image.root.action, Action::Rebuild);
    }

    #[test]
    fn set_value_idempotent_bytes() {
        let mut image = image_with_nvar_stores();
        let fv0_len = fv0_store_body(&image).len();
        let fv2_len = fv2_store_body(&image).len();
        set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        assert_eq!(fv0_store_body(&image).len(), fv0_len);
        assert_eq!(fv2_store_body(&image).len(), fv2_len);
        assert_eq!(fv0_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
        assert_eq!(fv2_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
        set_value(&mut image, VENDOR_QUESTION_ITEM, 1).unwrap();
        assert_eq!(fv0_store_body(&image).len(), fv0_len);
        assert_eq!(fv2_store_body(&image).len(), fv2_len);
        assert_eq!(fv0_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
        assert_eq!(fv2_store_body(&image)[SETUP_114_DATA_OFF + 0x3A], 1);
    }

    mod question_add_tests {
        use super::*;
        use crate::builder::build_image;
        use crate::hii::form_hijack::test_fixtures::SETUPDATA_GUID_STR;
        use crate::hii::question_add_fixtures::*;
        use crate::hii::spf;
        use crate::parser::image::parse_image;

        fn question_add_schema(qid: u16, voff: u16) -> schema::QuestionAddSchema {
            schema::QuestionAddSchema {
                form_id: 10019,
                prompt: "Serial Console".into(),
                help: "Serial console help".into(),
                question_id: qid,
                var_store_id: 1,
                var_offset: voff,
                size: 1,
                options: vec![
                    schema::QuestionAddOption {
                        text: "Disabled".into(),
                        value: 0,
                        default: None,
                    },
                    schema::QuestionAddOption {
                        text: "Enabled".into(),
                        value: 1,
                        default: Some(schema::DefaultClass::Optimized),
                    },
                ],
                defaults: None,
            }
        }

        const ITEM_FORM: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019";
        const ITEM_FORM_BARE: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019";

        fn find_spf_leaf(node: &FfsNode) -> Option<&FfsNode> {
            if node.children.is_empty() && node.body.windows(4).any(|w| w == b"$SPF") {
                return Some(node);
            }
            node.children.iter().find_map(find_spf_leaf)
        }

        fn spf_leaf_of(img: &Image) -> &[u8] {
            let sd_guid = Guid::from_str(SETUPDATA_GUID_STR).unwrap();
            let sd_file = img
                .root
                .children
                .iter()
                .find_map(|v| v.children.iter().find(|f| f.guid == Some(sd_guid)))
                .unwrap();
            &find_spf_leaf(sd_file).unwrap().body
        }

        fn pkg_of(img: &Image, target: &str) -> Vec<u8> {
            let t = crate::parser::target::parse_target(target).unwrap();
            let node = crate::parser::target::find_item(&img.root, &t).unwrap();
            let (off, len) =
                crate::hii::form_add::resource_forms_package(&node.body).expect("forms package");
            node.body[off..off + len].to_vec()
        }

        fn rec_u32(body: &[u8], rec_off: usize, field: usize) -> u32 {
            u32::from_le_bytes(
                body[rec_off + field..rec_off + field + 4]
                    .try_into()
                    .unwrap(),
            )
        }

        fn rec_u16(body: &[u8], rec_off: usize, field: usize) -> u16 {
            u16::from_le_bytes(
                body[rec_off + field..rec_off + field + 2]
                    .try_into()
                    .unwrap(),
            )
        }

        #[test]
        fn add_question_inserts_one_of_into_live_form() {
            let (flash, pkg_before, spf_before) = question_add_flash_image();
            let rec1_before = spf::scan_question_records(&spf_before)
                .into_iter()
                .find(|r| r.question_id == 0x55)
                .unwrap();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let res1 = add_question(&mut img, ITEM_FORM, &question_add_schema(0x200, 0x80))
                .expect("add question");
            assert_eq!(res1.question_id, 0x200);
            let prompt_id = *res1.string_ids.get("Serial Console").unwrap();
            let help_id = *res1.string_ids.get("Serial console help").unwrap();
            let disabled_id = *res1.string_ids.get("Disabled").unwrap();
            let enabled_id = *res1.string_ids.get("Enabled").unwrap();
            assert_ne!(prompt_id, help_id);

            let spf_after1 = spf_leaf_of(&img).to_vec();
            let base1 = spf::container_start(&spf_after1).unwrap();
            assert_eq!(
                rec_u32(&spf_after1, base1, 0x5C),
                (spf_after1.len() - base1) as u32,
                "container length header must track body growth"
            );
            let recs1 = spf::scan_question_records(&spf_after1);
            let new1 = recs1
                .iter()
                .find(|r| r.question_id == 0x200)
                .expect("new $SPF record");
            assert_eq!(new1.offset, base1 + res1.spf_record_offset);
            assert_eq!(
                rec_u16(&spf_after1, new1.offset, spf::SPF_RECORD_HELP_ID),
                help_id
            );
            assert_eq!(
                rec_u16(&spf_after1, new1.offset, spf::SPF_RECORD_PROMPT_ID_OFFSET),
                prompt_id
            );
            assert_eq!(
                rec_u32(&spf_after1, new1.offset, spf::SPF_RECORD_COUNTER_OFFSET),
                0x0001_0067
            );
            assert_eq!(new1.optimal, 1, "optimized default carried into record");
            let slot0 = rec_u32(&spf_after1, base1, spf::SPF_PAGE_TABLE_OFFSET);
            let page = base1 + slot0 as usize;
            assert_eq!(
                u16::from_le_bytes([spf_after1[page + 0xA], spf_after1[page + 0xB]]),
                10019,
                "clone keeps the form id"
            );
            assert_eq!(
                rec_u32(&spf_after1, page, spf::SPF_PAGE_CNT_OFFSET),
                2,
                "clone carries cnt+1"
            );
            assert_eq!(
                rec_u32(&spf_after1, page, spf::SPF_PAGE_LIST_OFFSET),
                0x94,
                "original record stays in the clone list"
            );
            assert_eq!(
                rec_u32(&spf_after1, page, spf::SPF_PAGE_LIST_OFFSET + 4),
                res1.spf_record_offset as u32,
                "new record appended to the clone list"
            );
            assert!(
                spf::scan_string_controls(&spf_after1)
                    .iter()
                    .any(|c| c.string_id == help_id),
                "string control with the new help id exists"
            );
            let rec1_after = recs1.iter().find(|r| r.question_id == 0x55).unwrap();
            let delta1 = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").len()
                - pkg_before.len();
            assert_eq!(
                rec1_after.ifr_offset,
                rec1_before.ifr_offset + delta1 as u32,
                "Setup records above the splice point shift by the op delta"
            );
            let foreign_before = spf::scan_question_records(&spf_before)
                .into_iter()
                .find(|r| r.question_id == 0x66)
                .unwrap();
            let foreign_after = recs1.iter().find(|r| r.question_id == 0x66).unwrap();
            assert_eq!(
                foreign_after.offset, foreign_before.offset,
                "foreign record must stay in place"
            );
            assert_eq!(
                foreign_after.ifr_offset, foreign_before.ifr_offset,
                "foreign (non-resolving) record ifr must not shift"
            );
            assert_eq!(
                &spf_after1[foreign_before.offset..foreign_before.offset + spf::SPF_RECORD_SIZE],
                &spf_before[foreign_before.offset..foreign_before.offset + spf::SPF_RECORD_SIZE],
                "foreign record must stay byte-identical"
            );
            let pkg_after1 = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
            let new_q_off = crate::hii::form_hijack::locate_questions(&pkg_after1, 10019)
                .into_iter()
                .find(|&(_, qid)| qid == 0x200)
                .unwrap()
                .0;
            assert_eq!(
                new1.ifr_offset as usize, new_q_off,
                "record ifr must point at the new one_of opcode"
            );

            let built = build_image(&img).unwrap();
            let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
            let q = crate::hii::question_info(
                &re,
                "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019:0x200",
            )
            .unwrap();
            assert_eq!(q.kind, "one_of");
            assert_eq!(q.question_id, 0x200);
            assert_eq!(q.var_store_id, 1);
            assert_eq!(q.var_offset, 0x80);
            assert_eq!(q.width, 1);
            let vs = q.varstore.as_ref().unwrap();
            assert_eq!(vs.name, "Setup");
            assert_eq!(vs.size, 0x100);
            assert_eq!(
                q.options.iter().map(|o| o.value).collect::<Vec<_>>(),
                vec![0, 1]
            );
            assert_eq!(q.defaults.len(), 1);
            assert_eq!(q.defaults[0].value, 1);
            let strings = crate::hii::strings::collect_strings(&re);
            for (sid, text) in [
                (prompt_id, "Serial Console"),
                (help_id, "Serial console help"),
                (disabled_id, "Disabled"),
                (enabled_id, "Enabled"),
            ] {
                assert!(
                    strings
                        .iter()
                        .any(|s| s.string_id == sid as u32 && s.text == text),
                    "string {sid} must resolve to {text:?}"
                );
            }
            let spf_rebuilt = spf_leaf_of(&re).to_vec();
            let base_r = spf::container_start(&spf_rebuilt).unwrap();
            let recs_r = spf::scan_question_records(&spf_rebuilt);
            assert_eq!(recs_r.len(), 4);
            let new_r = recs_r.iter().find(|r| r.question_id == 0x200).unwrap();
            assert_eq!(new_r.optimal, 1);
            let slot_r = rec_u32(&spf_rebuilt, base_r, spf::SPF_PAGE_TABLE_OFFSET);
            let page_r = base_r + slot_r as usize;
            assert_eq!(rec_u32(&spf_rebuilt, page_r, spf::SPF_PAGE_CNT_OFFSET), 2);
            assert!(
                spf::scan_string_controls(&spf_rebuilt)
                    .iter()
                    .any(|c| c.string_id == help_id)
            );

            let mut img2 = parse_image(&built, ImageMode::Write, "i3", "s3").unwrap();
            let res2 = add_question(&mut img2, ITEM_FORM, &question_add_schema(0x201, 0x81))
                .expect("second question composes");
            assert_eq!(res2.question_id, 0x201);
            let spf_after2 = spf_leaf_of(&img2).to_vec();
            let base2 = spf::container_start(&spf_after2).unwrap();
            let recs2 = spf::scan_question_records(&spf_after2);
            let new1_2 = recs2.iter().find(|r| r.question_id == 0x200).unwrap();
            assert_eq!(
                new1_2.ifr_offset, new1.ifr_offset,
                "first new record must not shift on the second add"
            );
            let new2 = recs2.iter().find(|r| r.question_id == 0x201).unwrap();
            assert_eq!(
                rec_u32(&spf_after2, new2.offset, spf::SPF_RECORD_COUNTER_OFFSET),
                0x0001_0068
            );
            let rec1_2 = recs2.iter().find(|r| r.question_id == 0x55).unwrap();
            let delta2 = pkg_of(&img2, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").len()
                - pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").len();
            assert_eq!(
                rec1_2.ifr_offset,
                rec1_after.ifr_offset + delta2 as u32,
                "older Setup records above the new splice shift again"
            );
            let foreign_2 = recs2.iter().find(|r| r.question_id == 0x66).unwrap();
            assert_eq!(
                foreign_2.ifr_offset, foreign_before.ifr_offset,
                "foreign record must survive the second add untouched"
            );
            assert_eq!(
                &spf_after2[foreign_before.offset..foreign_before.offset + spf::SPF_RECORD_SIZE],
                &spf_before[foreign_before.offset..foreign_before.offset + spf::SPF_RECORD_SIZE],
                "foreign record must stay byte-identical across both adds"
            );
            let slot2 = rec_u32(&spf_after2, base2, spf::SPF_PAGE_TABLE_OFFSET);
            let page2 = base2 + slot2 as usize;
            assert_eq!(rec_u32(&spf_after2, page2, spf::SPF_PAGE_CNT_OFFSET), 3);
            let built2 = build_image(&img2).unwrap();
            let re2 = parse_image(&built2, ImageMode::Read, "i4", "s4").unwrap();
            assert!(
                crate::hii::question_info(
                    &re2,
                    "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019:0x201"
                )
                .is_ok()
            );
            assert!(
                crate::hii::question_info(
                    &re2,
                    "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019:0x200"
                )
                .is_ok()
            );

            let mut img3 = parse_image(&built2, ImageMode::Write, "i5", "s5").unwrap();
            let err =
                add_question(&mut img3, ITEM_FORM, &question_add_schema(0x200, 0x82)).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            assert_eq!(
                build_image(&img3).unwrap(),
                built2,
                "failed repeat add must not mutate"
            );
        }

        #[test]
        fn add_question_bare_channel_inserts_question() {
            let (flash, _, _) = question_add_bare_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let res = add_question(&mut img, ITEM_FORM_BARE, &question_add_schema(0x200, 0x80))
                .expect("bare channel add");
            assert_eq!(res.question_id, 0x200);
            assert!(res.string_ids.contains_key("Serial Console"));
            let built = build_image(&img).unwrap();
            let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
            let q = crate::hii::question_info(
                &re,
                "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#10019:0x200",
            )
            .unwrap();
            assert_eq!(q.kind, "one_of");
            assert_eq!(q.var_offset, 0x80);
        }

        #[test]
        fn add_question_discovers_grandchild_setupdata_behind_guided_wrapper() {
            let (flash, _, _) = question_add_nested_ui_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            assert!(
                matches!(
                    ami_patcher::pfs_payload_path(&img, None),
                    Err(HiiError::AmiFilesNotFound)
                ),
                "direct-children UI lookup must miss the grandchild geometry"
            );
            let res = add_question(&mut img, ITEM_FORM, &question_add_schema(0x200, 0x80))
                .expect("deep discovery must resolve the nested SetupData");
            assert_eq!(res.question_id, 0x200);
            let built = build_image(&img).unwrap();
            let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
            let q = crate::hii::question_info(
                &re,
                "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10019:0x200",
            )
            .unwrap();
            assert_eq!(q.kind, "one_of");
            assert_eq!(q.var_offset, 0x80);
            let spf_body = spf_leaf_of(&re);
            assert!(
                spf::scan_question_records(spf_body)
                    .iter()
                    .any(|r| r.question_id == 0x200),
                "new record must land in the nested $SPF"
            );
        }

        #[test]
        fn add_question_rejects_invalid_requests_without_mutation() {
            let (flash, _, _) = question_add_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = build_image(&img).unwrap();
            let mut dup_qid = question_add_schema(0x200, 0x80);
            dup_qid.question_id = 0x3B;
            let err = add_question(&mut img, ITEM_FORM, &dup_qid).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let overlap = question_add_schema(0x200, 0x3A);
            let err = add_question(&mut img, ITEM_FORM, &overlap).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let mut wide = question_add_schema(0x200, 0x80);
            wide.size = 2;
            let err = add_question(&mut img, ITEM_FORM, &wide).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let mut unknown_varstore = question_add_schema(0x200, 0x80);
            unknown_varstore.var_store_id = 9;
            let err = add_question(&mut img, ITEM_FORM, &unknown_varstore).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let mut defaults_mismatch = question_add_schema(0x200, 0x80);
            defaults_mismatch.defaults = Some(schema::QuestionAddDefaults { optimized: 0 });
            let err = add_question(&mut img, ITEM_FORM, &defaults_mismatch).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let mut form_mismatch = question_add_schema(0x200, 0x80);
            form_mismatch.form_id = 10020;
            let err = add_question(&mut img, ITEM_FORM, &form_mismatch).unwrap_err();
            assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
            let mut unknown_form = question_add_schema(0x200, 0x80);
            unknown_form.form_id = 10099;
            let err = add_question(
                &mut img,
                "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#10099",
                &unknown_form,
            )
            .unwrap_err();
            assert!(matches!(err, HiiError::NotFound), "got {err:?}");
            assert_eq!(
                build_image(&img).unwrap(),
                before,
                "rejections must not mutate the image"
            );
        }

        #[test]
        fn add_question_rejects_size_zero() {
            let (flash, _, _) = question_add_flash_image();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = build_image(&img).unwrap();
            let mut schema = question_add_schema(0x200, 0x80);
            schema.size = 0;
            let err = add_question(&mut img, ITEM_FORM, &schema).unwrap_err();
            assert!(
                matches!(err, HiiError::InvalidSchema(ref m) if m.contains('0')),
                "got {err:?}"
            );
            assert_eq!(
                build_image(&img).unwrap(),
                before,
                "size 0 must be rejected before any mutation"
            );
        }

        fn snapshot_bodies(node: &FfsNode) -> Vec<Vec<u8>> {
            let mut out = vec![node.body.clone()];
            for c in &node.children {
                out.extend(snapshot_bodies(c));
            }
            out
        }

        #[test]
        fn add_question_malformed_ifr_splice_failure_keeps_image_identical() {
            let flash = question_add_malformed_resource_flash();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = snapshot_bodies(&img.root);
            let err =
                add_question(&mut img, ITEM_FORM, &question_add_schema(0x200, 0x80)).unwrap_err();
            assert!(matches!(err, HiiError::NotFound), "got {err:?}");
            assert_eq!(
                snapshot_bodies(&img.root),
                before,
                "splice failure after the strings stage must not leave a half-applied image"
            );
        }

        #[test]
        fn add_question_bare_splice_failure_keeps_image_identical() {
            let flash = question_add_malformed_bare_flash();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = snapshot_bodies(&img.root);
            let err = add_question(&mut img, ITEM_FORM_BARE, &question_add_schema(0x200, 0x80))
                .unwrap_err();
            assert!(matches!(err, HiiError::NotFound), "got {err:?}");
            assert_eq!(
                snapshot_bodies(&img.root),
                before,
                "bare-channel splice failure after strings must not mutate the image"
            );
        }

        #[test]
        fn add_question_growth_failure_after_strings_keeps_image_identical() {
            let flash = question_add_growth_blocked_flash();
            let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = snapshot_bodies(&img.root);
            let err =
                add_question(&mut img, ITEM_FORM, &question_add_schema(0x200, 0x80)).unwrap_err();
            assert!(matches!(err, HiiError::PeGrowthUnsupported), "got {err:?}");
            assert_eq!(
                snapshot_bodies(&img.root),
                before,
                "resource-growth failure after the strings stage must not leave a half-applied image"
            );
        }

        #[test]
        fn check_question_add_validates_whole_list_before_any_apply() {
            let (flash, _, _) = question_add_flash_image();
            let img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
            let before = snapshot_bodies(&img.root);
            let good = vec![
                question_add_schema(0x200, 0x80),
                question_add_schema(0x201, 0x81),
            ];
            assert!(check_question_add(&img, ITEM_FORM, &good).is_ok());
            let mut dup_qid = good.clone();
            dup_qid[1].question_id = 0x200;
            assert!(
                matches!(
                    check_question_add(&img, ITEM_FORM, &dup_qid).unwrap_err(),
                    HiiError::InvalidSchema(_)
                ),
                "intra-list duplicate question id must be rejected"
            );
            let mut overlap = good.clone();
            overlap[1].var_offset = 0x80;
            assert!(
                matches!(
                    check_question_add(&img, ITEM_FORM, &overlap).unwrap_err(),
                    HiiError::InvalidSchema(_)
                ),
                "intra-list var_offset overlap must be rejected"
            );
            let mut unknown_varstore = good.clone();
            unknown_varstore[1].var_store_id = 9;
            assert!(
                matches!(
                    check_question_add(&img, ITEM_FORM, &unknown_varstore).unwrap_err(),
                    HiiError::InvalidSchema(_)
                ),
                "second question varstore must be validated up front"
            );
            let mut base_dup = good;
            base_dup[0].question_id = 0x3B;
            assert!(
                matches!(
                    check_question_add(&img, ITEM_FORM, &base_dup).unwrap_err(),
                    HiiError::InvalidSchema(_)
                ),
                "duplicate of an existing formset question id must be rejected"
            );
            assert_eq!(
                snapshot_bodies(&img.root),
                before,
                "check must not mutate the image"
            );
        }

        mod ref_add_tests {
            use super::*;
            use crate::hii::ifr_builder::{OP_END, OP_REF};

            fn ref_add_schema(qid: u16, dest_form: u16) -> schema::QuestionAddRefSchema {
                schema::QuestionAddRefSchema {
                    form_id: dest_form,
                    prompt: "Goto Page".into(),
                    help: "Goto Page help".into(),
                    question_id: qid,
                    formset_guid: None,
                }
            }

            fn find_ref_op(pkg: &[u8], form_id: u16) -> Option<usize> {
                let span = crate::hii::form_hijack::locate_form(pkg, form_id)?;
                let mut i = span.form_op;
                while i + 2 <= span.next_form_op {
                    let len = (pkg[i + 1] & 0x7F) as usize;
                    if len < 2 {
                        return None;
                    }
                    if pkg[i] == OP_REF {
                        return Some(i);
                    }
                    i += len;
                }
                None
            }

            fn pkg_u16(pkg: &[u8], off: usize) -> u16 {
                u16::from_le_bytes([pkg[off], pkg[off + 1]])
            }

            #[test]
            fn add_ref_inserts_goto_before_form_end() {
                let (flash, pkg_before, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let res =
                    add_ref(&mut img, ITEM_FORM, &ref_add_schema(0x300, 10020)).expect("add ref");
                assert_eq!(res.question_id, 0x300);
                let prompt_id = *res.string_ids.get("Goto Page").unwrap();
                let help_id = *res.string_ids.get("Goto Page help").unwrap();
                assert_ne!(prompt_id, help_id);

                let pkg_after = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
                assert_eq!(pkg_after.len() - pkg_before.len(), 15);
                assert_eq!(
                    crate::hii::ifr::scope_balance(&pkg_after),
                    crate::hii::ifr::scope_balance(&pkg_before),
                    "REF is not a scope op: the splice must not change the IFR scope balance"
                );
                let r = find_ref_op(&pkg_after, 10019).expect("REF op in form 10019");
                assert_eq!(pkg_after[r + 1] & 0x7F, 15, "REF op total length is 15");
                assert_eq!(pkg_u16(&pkg_after, r + 13), 10020, "FormId at +13");
                assert_eq!(pkg_u16(&pkg_after, r + 6), 0x300, "qid at +6");
                assert_eq!(pkg_u16(&pkg_after, r + 2), prompt_id, "prompt at +2");
                assert_eq!(pkg_u16(&pkg_after, r + 4), help_id, "help at +4");
                assert_eq!(pkg_u16(&pkg_after, r + 8), 0, "var store id is zero");
                assert_eq!(
                    pkg_u16(&pkg_after, r + 10),
                    0xFFFF,
                    "var offset carries the stock no-storage sentinel (all 31 stock REFs: 0xFFFF)"
                );
                assert_eq!(
                    pkg_after[r + 15],
                    OP_END,
                    "REF must sit directly before the form END op"
                );
            }

            #[test]
            fn add_ref_fixups_spf_ifr_offsets_without_appends() {
                let (flash, pkg_before, spf_before) = question_add_flash_image();
                let rec0_before = spf::scan_question_records(&spf_before)
                    .into_iter()
                    .find(|r| r.question_id == 0x3B)
                    .unwrap();
                let rec1_before = spf::scan_question_records(&spf_before)
                    .into_iter()
                    .find(|r| r.question_id == 0x55)
                    .unwrap();
                let foreign_before = spf::scan_question_records(&spf_before)
                    .into_iter()
                    .find(|r| r.question_id == 0x66)
                    .unwrap();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                add_ref(&mut img, ITEM_FORM, &ref_add_schema(0x300, 10020)).expect("add ref");

                let spf_after = spf_leaf_of(&img).to_vec();
                assert_eq!(spf_after.len(), spf_before.len(), "no container growth");
                let base_before = spf::container_start(&spf_before).unwrap();
                let base_after = spf::container_start(&spf_after).unwrap();
                assert_eq!(base_after, base_before);
                assert_eq!(
                    rec_u32(&spf_after, base_after, 0x5C),
                    rec_u32(&spf_before, base_before, 0x5C),
                    "container length header must stay untouched"
                );
                assert_eq!(
                    rec_u32(&spf_after, base_after, spf::SPF_PAGE_COUNT_OFFSET),
                    rec_u32(&spf_before, base_before, spf::SPF_PAGE_COUNT_OFFSET),
                    "page count must stay untouched"
                );
                assert_eq!(
                    spf::scan_question_records(&spf_after).len(),
                    spf::scan_question_records(&spf_before).len(),
                    "no new question records"
                );
                assert!(
                    !spf::scan_question_records(&spf_after)
                        .iter()
                        .any(|r| r.question_id == 0x300),
                    "the REF must not gain a $SPF record"
                );
                assert_eq!(
                    spf::scan_string_controls(&spf_after).len(),
                    spf::scan_string_controls(&spf_before).len(),
                    "no new string controls"
                );

                let delta = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").len()
                    - pkg_before.len();
                assert_eq!(delta, 15);
                let recs_after = spf::scan_question_records(&spf_after);
                let rec0_after = recs_after.iter().find(|r| r.question_id == 0x3B).unwrap();
                assert_eq!(
                    rec0_after.ifr_offset, rec0_before.ifr_offset,
                    "record below the splice point must not shift"
                );
                let rec1_after = recs_after.iter().find(|r| r.question_id == 0x55).unwrap();
                assert_eq!(
                    rec1_after.ifr_offset,
                    rec1_before.ifr_offset + delta as u32,
                    "record above the splice point must shift by delta"
                );
                let foreign_after = recs_after.iter().find(|r| r.question_id == 0x66).unwrap();
                assert_eq!(foreign_after.offset, foreign_before.offset);
                assert_eq!(foreign_after.ifr_offset, foreign_before.ifr_offset);

                let shifted_at = rec1_before.offset + spf::SPF_RECORD_IFR_OFFSET;
                let mut expected_spf = spf_before.clone();
                let shifted: [u8; 4] = (rec1_before.ifr_offset + delta as u32).to_le_bytes();
                expected_spf[shifted_at..shifted_at + 4].copy_from_slice(&shifted);
                assert_eq!(
                    spf_after, expected_spf,
                    "only the shifted ifr_offset u32 may differ"
                );
                assert_eq!(
                    rec_u32(&spf_after, rec1_after.offset, spf::SPF_RECORD_IFR_OFFSET),
                    rec1_before.ifr_offset + delta as u32
                );
            }

            #[test]
            fn add_ref_rejects_duplicate_qids_without_mutation() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let before = build_image(&img).unwrap();
                let err = add_ref(&mut img, ITEM_FORM, &ref_add_schema(0x3B, 10020)).unwrap_err();
                assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
                let dup_in_request =
                    vec![ref_add_schema(0x300, 10020), ref_add_schema(0x300, 10019)];
                assert!(
                    matches!(
                        check_ref_add(&img, ITEM_FORM, &dup_in_request, &[]).unwrap_err(),
                        HiiError::InvalidSchema(_)
                    ),
                    "intra-request duplicate ref qid must be rejected"
                );
                let clash_with_question = vec![ref_add_schema(0x200, 10020)];
                assert!(
                    matches!(
                        check_ref_add(&img, ITEM_FORM, &clash_with_question, &[0x200]).unwrap_err(),
                        HiiError::InvalidSchema(_)
                    ),
                    "ref qid clashing with a question from the same request must be rejected"
                );
                assert_eq!(
                    build_image(&img).unwrap(),
                    before,
                    "rejections must not mutate the image"
                );
            }

            #[test]
            fn add_ref_refuses_read_only_mode() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Read, "i", "s").unwrap();
                let err = add_ref(&mut img, ITEM_FORM, &ref_add_schema(0x300, 10020)).unwrap_err();
                assert!(matches!(err, HiiError::NotWritable), "got {err:?}");
                let err = check_ref_add(&img, ITEM_FORM, &[ref_add_schema(0x300, 10020)], &[])
                    .unwrap_err();
                assert!(matches!(err, HiiError::NotWritable), "got {err:?}");
            }

            #[test]
            fn add_ref_allows_dangling_destination_form() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let res = add_ref(&mut img, ITEM_FORM, &ref_add_schema(0x300, 10099))
                    .expect("destination existence is the caller's contract");
                assert_eq!(res.question_id, 0x300);
                let pkg_after = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
                let r = find_ref_op(&pkg_after, 10019).expect("REF op in form 10019");
                assert_eq!(pkg_u16(&pkg_after, r + 13), 10099);
                assert!(
                    crate::hii::form_hijack::locate_form(&pkg_after, 10099).is_none(),
                    "fixture must not contain the dangling destination"
                );
            }

            const CROSS_FORMSET_GUID: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

            #[test]
            fn add_ref_with_formset_guid_emits_ref3_roundtrip() {
                let (flash, pkg_before, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let mut s = ref_add_schema(0x300, 10020);
                s.formset_guid = Some(CROSS_FORMSET_GUID.into());
                add_ref(&mut img, ITEM_FORM, &s).expect("add ref3");

                let pkg_after = pkg_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
                assert_eq!(pkg_after.len() - pkg_before.len(), 33);
                assert_eq!(
                    crate::hii::ifr::scope_balance(&pkg_after),
                    crate::hii::ifr::scope_balance(&pkg_before),
                    "REF3 is not a scope op: the splice must not change the IFR scope balance"
                );
                let r = find_ref_op(&pkg_after, 10019).expect("REF op in form 10019");
                assert_eq!(pkg_after[r + 1] & 0x7F, 33, "REF3 op total length is 33");
                let stmt = &pkg_after[r..r + 33];
                let g = Guid::from_str(CROSS_FORMSET_GUID).unwrap();
                assert_eq!(
                    ref_variant::parse_ref(stmt[0], stmt),
                    Some(ref_variant::RefTarget::Formset {
                        formset_guid: g,
                        form_id: 10020,
                        question_id: 0xFFFF,
                    }),
                    "emitted REF3 must re-parse to a cross-formset target"
                );
            }

            #[test]
            fn add_ref_rejects_unparsable_formset_guid() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let before = build_image(&img).unwrap();
                let mut s = ref_add_schema(0x300, 10020);
                s.formset_guid = Some("not-a-guid".into());
                let err = add_ref(&mut img, ITEM_FORM, &s).unwrap_err();
                assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
                let mut batch = vec![s];
                batch[0].question_id = 0x301;
                assert!(
                    matches!(
                        check_ref_add(&img, ITEM_FORM, &batch, &[]).unwrap_err(),
                        HiiError::InvalidSchema(_)
                    ),
                    "check path must reject unparsable formset_guid too"
                );
                assert_eq!(
                    build_image(&img).unwrap(),
                    before,
                    "rejections must not mutate the image"
                );
            }
        }

        mod page_add_tests {
            use super::*;

            fn page_add_schema(form_id: u16) -> schema::PageAddSchema {
                schema::PageAddSchema {
                    form_id,
                    title: "New Page".into(),
                }
            }

            #[test]
            fn add_page_registers_skeleton_from_parent_clone() {
                let (flash, _, spf_before) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let res = add_page(&mut img, ITEM_FORM, &page_add_schema(10021)).expect("add page");
                assert_eq!(res.form_id, 10021);
                assert_eq!(res.slot, 1, "seq = page count read before the bump");
                let spf_after = spf_leaf_of(&img).to_vec();
                let base = spf::container_start(&spf_after).unwrap();
                assert_eq!(
                    rec_u32(&spf_after, base, spf::SPF_PAGE_COUNT_OFFSET),
                    rec_u32(&spf_before, base, spf::SPF_PAGE_COUNT_OFFSET) + 1,
                    "page count must grow by exactly one"
                );
                assert_eq!(
                    rec_u32(&spf_after, base, spf::SPF_PAGE_TABLE_OFFSET + 4 * res.slot) as usize,
                    res.page_offset,
                    "the new slot must contain the skeleton offset"
                );
                assert_eq!(res.page_offset, spf_before.len() - base);
                let page = base + res.page_offset;
                assert_eq!(
                    rec_u16(&spf_after, page, spf::SPF_PAGE_FORM_ID_OFFSET),
                    10021
                );
                assert_eq!(
                    rec_u16(&spf_after, page, spf::SPF_PAGE_TITLE_ID_OFFSET),
                    res.title_string_id
                );
                assert_eq!(rec_u16(&spf_after, page, spf::SPF_PAGE_SEQ_OFFSET), 1);
                assert_eq!(
                    rec_u16(&spf_after, page, spf::SPF_PAGE_PARENT_OFFSET),
                    0,
                    "B = parent page slot"
                );
                assert_eq!(rec_u32(&spf_after, page, spf::SPF_PAGE_CNT_OFFSET), 0);
                let parent_abs =
                    base + rec_u32(&spf_before, base, spf::SPF_PAGE_TABLE_OFFSET) as usize;
                assert_eq!(
                    spf_after[page + spf::SPF_PAGE_MARKER_OFFSET],
                    spf_before[parent_abs + spf::SPF_PAGE_MARKER_OFFSET],
                    "marker must be cloned from the parent page"
                );
                assert_eq!(
                    rec_u32(&spf_after, page, spf::SPF_PAGE_IMAGE_OFFSET),
                    rec_u32(&spf_before, parent_abs, spf::SPF_PAGE_IMAGE_OFFSET),
                    "image offset must be cloned from the parent page"
                );
                let patched = |i: usize| {
                    (spf::SPF_PAGE_FORM_ID_OFFSET..spf::SPF_PAGE_FORM_ID_OFFSET + 2).contains(&i)
                        || (spf::SPF_PAGE_TITLE_ID_OFFSET..spf::SPF_PAGE_PARENT_OFFSET + 2)
                            .contains(&i)
                        || (spf::SPF_PAGE_CNT_OFFSET..spf::SPF_PAGE_HEADER_SIZE).contains(&i)
                };
                for i in 0..spf::SPF_PAGE_HEADER_SIZE {
                    if !patched(i) {
                        assert_eq!(
                            spf_after[page + i],
                            spf_before[parent_abs + i],
                            "skeleton byte {i:#x} must be cloned from the parent header"
                        );
                    }
                }
                assert_eq!(
                    rec_u32(&spf_after, parent_abs, 0) as usize,
                    res.page_offset,
                    "the zero prefix of the parent page doubles as table slot 1 in this fixture geometry"
                );
                assert_eq!(
                    &spf_after[parent_abs + 4..parent_abs + spf::SPF_PAGE_HEADER_SIZE + 4],
                    &spf_before[parent_abs + 4..parent_abs + spf::SPF_PAGE_HEADER_SIZE + 4],
                    "the parent page past the table-slot prefix must stay byte-identical"
                );
                assert_eq!(spf_after.len(), spf_before.len() + 0x20);
                assert_eq!(
                    rec_u32(&spf_after, base, 0x5C),
                    (spf_after.len() - base) as u32,
                    "the header-region last length must track the grown container"
                );
            }

            #[test]
            fn add_page_survives_rebuild_and_duplicate_is_rejected() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let res = add_page(&mut img, ITEM_FORM, &page_add_schema(10021)).expect("add page");
                let built = build_image(&img).unwrap();
                let mut re = parse_image(&built, ImageMode::Write, "i2", "s2").unwrap();
                let spf_re = spf_leaf_of(&re).to_vec();
                let base = spf::container_start(&spf_re).unwrap();
                assert_eq!(
                    rec_u32(&spf_re, base, spf::SPF_PAGE_COUNT_OFFSET),
                    2,
                    "the registration must survive the rebuild"
                );
                let slot1 = rec_u32(&spf_re, base, spf::SPF_PAGE_TABLE_OFFSET + 4) as usize;
                assert_eq!(slot1, res.page_offset);
                assert_eq!(
                    rec_u16(&spf_re, base + slot1, spf::SPF_PAGE_FORM_ID_OFFSET),
                    10021
                );
                let strings = crate::hii::strings::collect_strings(&re);
                assert!(
                    strings
                        .iter()
                        .any(|s| s.string_id == u32::from(res.title_string_id)
                            && s.text == "New Page"),
                    "the page title string must resolve after the rebuild"
                );
                let err = add_page(&mut re, ITEM_FORM, &page_add_schema(10021)).unwrap_err();
                assert!(
                    matches!(err, HiiError::InvalidSchema(_)),
                    "duplicate registration must be rejected, got {err:?}"
                );
            }

            #[test]
            fn add_page_rejects_empty_title_without_mutation() {
                let (flash, _, _) = question_add_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                let schema = schema::PageAddSchema {
                    form_id: 10021,
                    title: String::new(),
                };
                let err = add_page(&mut img, ITEM_FORM, &schema).unwrap_err();
                assert!(matches!(err, HiiError::InvalidSchema(_)), "got {err:?}");
                assert_eq!(
                    build_image(&img).unwrap(),
                    flash,
                    "rejection must not mutate the image"
                );
            }
        }

        mod form_varstore_tests {
            use super::*;
            use crate::hii::form_add::add_form;

            const ITEM_FORMSET: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0";
            const ITEM_FORMSET_BARE: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0";

            fn varstore_form_schema() -> schema::FormSetSchema {
                schema::FormSetSchema {
                    formset_guid: "11111111-2222-3333-4444-555555555555".into(),
                    title: "T".into(),
                    help: "H".into(),
                    class_guids: vec![],
                    varstores: vec![schema::VarStoreSchema {
                        id: 7,
                        guid: "22222222-3333-4444-5555-666666666666".into(),
                        size: 64,
                        name: "VStore".into(),
                        var_type: schema::VarStoreType::Buffer,
                        attributes: 7,
                    }],
                    default_stores: vec![],
                    forms: vec![schema::FormSchema {
                        id: 42,
                        title: "NewForm".into(),
                        items: vec![schema::ItemSchema::Text(schema::TextItem {
                            prompt: "P".into(),
                            help: "H".into(),
                            text_two: "X".into(),
                        })],
                    }],
                    setupdata_guid: None,
                    amitse_guid: None,
                }
            }

            fn varstore_op_len(pkg: &[u8]) -> usize {
                let at = pkg
                    .windows(6)
                    .position(|w| w == b"VStore")
                    .expect("varstore name in the forms package");
                (pkg[at - 22 + 1] & 0x7F) as usize
            }

            #[test]
            fn add_form_varstore_declaration_fixups_spf_ifr_offsets() {
                let (flash, pkg_before, spf_before) = question_add_flash_image();
                let before_recs = spf::scan_question_records(&spf_before);
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                add_form(&mut img, ITEM_FORMSET, &varstore_form_schema())
                    .expect("form add with a varstore declaration");

                let pkg_after = pkg_of(&img, ITEM_FORMSET);
                let spf_after = spf_leaf_of(&img).to_vec();
                assert_eq!(
                    spf_after.len(),
                    spf_before.len(),
                    "the fixup must not grow the $SPF container"
                );
                let delta = varstore_op_len(&pkg_after);
                assert!(delta > 0);

                let mut expected_spf = spf_before.clone();
                let mut live = 0usize;
                for b in &before_recs {
                    if !spf_record_resolves(&pkg_before, b.question_id, b.ifr_offset) {
                        continue;
                    }
                    live += 1;
                    let at = b.offset + spf::SPF_RECORD_IFR_OFFSET;
                    let shifted: [u8; 4] = (b.ifr_offset + delta as u32).to_le_bytes();
                    expected_spf[at..at + 4].copy_from_slice(&shifted);
                }
                assert_eq!(
                    live, 2,
                    "the fixture carries exactly two live records (q0x3B, q0x55)"
                );
                assert_eq!(
                    spf_after, expected_spf,
                    "round-11 invariant: only the live records' ifr_offset u32s may differ after the varstore insert"
                );
                for b in &before_recs {
                    if !spf_record_resolves(&pkg_before, b.question_id, b.ifr_offset) {
                        continue;
                    }
                    let a = spf::scan_question_records(&spf_after)
                        .into_iter()
                        .find(|r| r.question_id == b.question_id)
                        .expect("stock live record must survive the form add");
                    assert!(
                        spf_record_resolves(&pkg_after, a.question_id, a.ifr_offset),
                        "record q{:#x} must resolve to its own question op after the varstore insert",
                        a.question_id
                    );
                }
            }

            #[test]
            fn add_form_varstore_declaration_fixups_spf_ifr_offsets_bare() {
                let (flash, pkg_before, spf_before) = question_add_bare_flash_image();
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                add_form(&mut img, ITEM_FORMSET_BARE, &varstore_form_schema())
                    .expect("form add with a varstore declaration (bare channel)");

                let t = crate::parser::target::parse_target(ITEM_FORMSET_BARE).unwrap();
                let pkg_after = crate::parser::target::find_item(&img.root, &t)
                    .unwrap()
                    .body
                    .clone();
                let spf_after = spf_leaf_of(&img).to_vec();
                let delta = varstore_op_len(&pkg_after);
                let mut live = 0usize;
                for b in spf::scan_question_records(&spf_before) {
                    if !spf_record_resolves(&pkg_before, b.question_id, b.ifr_offset) {
                        continue;
                    }
                    live += 1;
                    let a = spf::scan_question_records(&spf_after)
                        .into_iter()
                        .find(|r| r.question_id == b.question_id)
                        .unwrap();
                    assert_eq!(
                        a.ifr_offset,
                        b.ifr_offset + delta as u32,
                        "bare channel must shift live record q{:#x} by the varstore length",
                        b.question_id
                    );
                    assert!(spf_record_resolves(&pkg_after, a.question_id, a.ifr_offset));
                }
                assert_eq!(live, 2);
            }

            #[test]
            fn add_form_varstore_fixups_records_in_later_formsets() {
                use crate::hii::form_hijack::test_fixtures::{
                    ffs_file_bytes, file_sections, flash_with_files, section_bytes,
                    string_package_bytes,
                };
                use crate::hii::ifr_builder::{IfrBuilder, TYPE_NUM_SIZE_8};

                let mut b = IfrBuilder::new();
                let g1 = Guid::from_str("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
                let g2 = Guid::from_str("B1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
                b.emit_form_set(&g1, 1, 1, &[]);
                b.emit_var_store(1, &g1, 0x100, "Setup");
                b.emit_form(10019, 1);
                b.emit_one_of(0x01A3, 0x01A4, 0x3B, 1, 0x3A, 0, 1);
                b.emit_one_of_option(4, 0x30, TYPE_NUM_SIZE_8, 0, 1);
                b.emit_end();
                b.emit_end();
                b.emit_end();
                b.emit_form_set(&g2, 2, 2, &[]);
                b.emit_form(10020, 2);
                b.emit_one_of(0x01A5, 0x01A6, 0x55, 1, 0x40, 0, 1);
                b.emit_one_of_option(5, 0x00, TYPE_NUM_SIZE_8, 0, 1);
                b.emit_end();
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

                let spf_body = question_add_spf_body_for(&pkg);
                let before = spf::scan_question_records(&spf_body);
                let flash = flash_with_files(vec![
                    ffs_file_bytes(
                        &Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap(),
                        &file_sections(&[
                            section_bytes(crate::ffs::EFI_SECTION_RAW, &pkg),
                            section_bytes(crate::ffs::EFI_SECTION_RAW, &string_package_bytes()),
                        ]),
                    ),
                    sd_file_direct(&spf_body),
                ]);
                let mut img = parse_image(&flash, ImageMode::Write, "i", "s").unwrap();
                add_form(
                    &mut img,
                    "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0#0",
                    &varstore_form_schema(),
                )
                .expect("form add into formset 0");

                let t = crate::parser::target::parse_target(ITEM_FORMSET_BARE).unwrap();
                let pkg_after = crate::parser::target::find_item(&img.root, &t)
                    .unwrap()
                    .body
                    .clone();
                let spf_after = spf_leaf_of(&img).to_vec();
                let v = varstore_op_len(&pkg_after) as u32;
                let f = (pkg_after.len() - pkg.len() - v as usize) as u32;
                assert!(f > 0);

                let after = spf::scan_question_records(&spf_after);
                let b0 = before.iter().find(|r| r.question_id == 0x3B).unwrap();
                let b1 = before.iter().find(|r| r.question_id == 0x55).unwrap();
                let a0 = after.iter().find(|r| r.question_id == 0x3B).unwrap();
                let a1 = after.iter().find(|r| r.question_id == 0x55).unwrap();
                assert_eq!(
                    a0.ifr_offset,
                    b0.ifr_offset + v,
                    "records inside the targeted formset shift by the varstore length only"
                );
                assert_eq!(
                    a1.ifr_offset,
                    b1.ifr_offset + v + f,
                    "records in later formsets shift by varstore + form lengths"
                );
                assert!(spf_record_resolves(
                    &pkg_after,
                    a0.question_id,
                    a0.ifr_offset
                ));
                assert!(spf_record_resolves(
                    &pkg_after,
                    a1.question_id,
                    a1.ifr_offset
                ));
            }

            #[test]
            fn add_form_varstores_refuse_when_spf_behind_bad_wrapper() {
                let pkg = question_add_forms_pkg();
                let spf_body = question_add_spf_body_for(&pkg);
                let mk = |node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>| FfsNode {
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
                };
                let mut forms_sec = mk(FfsType::Section, pkg.clone(), vec![]);
                forms_sec.subtype = crate::ffs::EFI_SECTION_RAW;
                let mut setup_file = mk(FfsType::File, vec![], vec![forms_sec]);
                setup_file.guid =
                    Some(Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap());
                let mut spf_sec = mk(FfsType::Section, spf_body, vec![]);
                spf_sec.subtype = crate::ffs::EFI_SECTION_FREEFORM_SUBTYPE_GUID;
                let mut tiano = mk(FfsType::Section, vec![], vec![spf_sec]);
                tiano.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
                tiano.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
                    guid: crate::ffs::tiano_guid(),
                    dictionary_size: 0x0080_0000,
                });
                let mut sd_file = mk(FfsType::File, vec![], vec![tiano]);
                sd_file.guid = Some(Guid::from_str(SETUPDATA_GUID_STR).unwrap());
                let volume = mk(FfsType::Volume, vec![], vec![setup_file, sd_file]);
                let root = mk(FfsType::Image, vec![], vec![volume]);
                let mut img = Image {
                    image_id: "i".into(),
                    session_id: "s".into(),
                    root,
                    mode: ImageMode::Write,
                };

                let err =
                    add_form(&mut img, ITEM_FORMSET_BARE, &varstore_form_schema()).unwrap_err();
                assert!(
                    matches!(err, HiiError::MutationBehindCompression),
                    "a reachable-but-blocked $SPF must refuse the varstore insert instead of desyncing it, got {err:?}"
                );
                let forms = &img.root.children[0].children[0].children[0];
                assert_eq!(forms.body, pkg, "the forms package must stay untouched");
                assert_eq!(forms.action, Action::NoAction);
            }
        }
    }
}
