use crate::types::{FfsNode, FfsType, Guid, Image, ImageMode};
use thiserror::Error;

const SIG: [u8; 4] = *b"NVAR";
pub const ATTR_VALID: u8 = 0x80;
pub const ATTR_EXTENDED_HEADER: u8 = 0x10;
pub const ATTR_DATA_ONLY: u8 = 0x08;
pub const ATTR_LOCAL_GUID: u8 = 0x04;
pub const ATTR_ASCII_NAME: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvarRecord {
    pub offset: usize,
    pub size: usize,
    pub attributes: u8,
    pub guid_index: Option<u8>,
    pub guid: Option<Guid>,
    pub next: Option<u32>,
    pub extended_size: u16,
    pub name: Option<String>,
    pub data_offset: usize,
    pub data_len: usize,
}

pub fn parse_entry(buf: &[u8], off: usize) -> Option<NvarRecord> {
    if buf.get(off..off + 4)? != &SIG[..] {
        return None;
    }
    let size = u16::from_le_bytes([*buf.get(off + 4)?, *buf.get(off + 5)?]) as usize;
    if size < 10 || off + size > buf.len() {
        return None;
    }
    let attributes = buf[off + 9];
    let next_raw =
        (buf[off + 6] as u32) | ((buf[off + 7] as u32) << 8) | ((buf[off + 8] as u32) << 16);
    let next = if next_raw == 0xFF_FFFF {
        None
    } else {
        Some(next_raw)
    };
    let entry_end = off + size;
    let mut p = off + 10;
    let mut guid_index = None;
    if attributes & ATTR_VALID != 0 && attributes & ATTR_DATA_ONLY == 0 {
        if attributes & ATTR_LOCAL_GUID != 0 {
            p += 16;
        } else {
            guid_index = Some(*buf.get(p)?);
            p += 1;
        }
        if p > entry_end {
            return None;
        }
    }
    let mut name = None;
    if attributes & ATTR_VALID != 0 && attributes & ATTR_DATA_ONLY == 0 {
        if attributes & ATTR_ASCII_NAME != 0 {
            let end = buf[p..entry_end].iter().position(|&b| b == 0)? + p;
            name = Some(String::from_utf8_lossy(&buf[p..end]).into_owned());
            p = end + 1;
        } else {
            let mut end = p;
            while end + 1 < entry_end && !(buf[end] == 0 && buf[end + 1] == 0) {
                end += 2;
            }
            if end + 1 >= entry_end {
                return None;
            }
            let units: Vec<u16> = buf[p..end]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            name = Some(String::from_utf16_lossy(&units));
            p = end + 2;
        }
        if p > entry_end {
            return None;
        }
    }
    let mut data_end = entry_end;
    let mut extended_size = 0u16;
    if attributes & ATTR_EXTENDED_HEADER != 0 && size > 14 {
        let esz = u16::from_le_bytes([buf[entry_end - 2], buf[entry_end - 1]]);
        if esz >= 3 && (esz as usize) <= size - 10 {
            extended_size = esz;
            data_end -= esz as usize;
        }
    }
    if p > data_end {
        return None;
    }
    Some(NvarRecord {
        offset: off,
        size,
        attributes,
        guid_index,
        guid: None,
        next,
        extended_size,
        name,
        data_offset: p,
        data_len: data_end - p,
    })
}

/// File/Section-лист, чьё тело начинается с валидной NVAR-записи
/// (проб первого заголовка). Критерий метки «NVRAM store» и сбора
/// сторов для nvar list/set (спека nvar-op §2/§3).
pub fn is_nvar_body(node: &crate::types::FfsNode) -> bool {
    matches!(
        node.node_type,
        crate::types::FfsType::File | crate::types::FfsType::Section
    ) && node.children.is_empty()
        && parse_entry(&node.body, 0).is_some()
}

pub fn walk(buf: &[u8]) -> Vec<NvarRecord> {
    let mut out = Vec::new();
    let mut off = 0usize;
    while let Some(r) = parse_entry(buf, off) {
        off += r.size;
        out.push(r);
    }
    out
}

pub fn std_defaults_data(buf: &[u8]) -> Option<(usize, &[u8])> {
    let first = parse_entry(buf, 0)?;
    if first.name.as_deref() != Some("StdDefaults") {
        return None;
    }
    Some((
        first.data_offset,
        buf.get(first.data_offset..first.data_offset + first.data_len)?,
    ))
}

pub fn is_std_defaults(body: &[u8]) -> bool {
    std_defaults_data(body).is_some()
}

/// Первая NVAR-запись, совпавшая по (имя, data_len). Дубликаты имени и
/// длины в одном сторе не дискриминируются (осознанный контракт; на
/// живых образах AMI уникальны — спека varstore-contract §8).
pub fn find_varstore_record<'a>(
    body: &'a [u8],
    name: &str,
    data_len: usize,
) -> Option<(usize, &'a [u8])> {
    let (inner_off, inner) = std_defaults_data(body)?;
    for r in walk(inner) {
        if r.name.as_deref() == Some(name) && r.data_len == data_len {
            return Some((
                inner_off + r.data_offset,
                &inner[r.data_offset..r.data_offset + r.data_len],
            ));
        }
    }
    None
}

#[derive(Debug, Error)]
pub enum NvarError {
    #[error("not found")]
    NotFound,
    #[error("image is not writable (open in Write mode first)")]
    NotWritable,
    #[error("target is behind a compressed/guided section that cannot be recompressed")]
    MutationBehindCompression,
    #[error("value operation not supported: {0}")]
    ValueOpUnsupported(String),
    #[error("ambiguous variable name '{0}'")]
    AmbiguousName(String),
    #[error("invalid NVAR store: {0}")]
    InvalidStore(String),
}

/// Резолв guid_index по хвосту блоба: k → [len−16·(k+1) .. len−16·k]
/// (UEFITool nvramparser.cpp:209). Выход за хвост → None — деградация
/// косметическая, правка по имени остаётся рабочей (спека nvar-op §1).
pub fn resolve_guid(buf: &[u8], guid_index: u8) -> Option<Guid> {
    let idx = guid_index as usize;
    let start = buf.len().checked_sub(16 * (idx + 1))?;
    let end = buf.len() - 16 * idx;
    let mut arr = [0u8; 16];
    arr.copy_from_slice(buf.get(start..end)?);
    Some(Guid::from_bytes(arr))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatRecord {
    pub record: NvarRecord,
    pub depth: u32,
    pub data_off: usize,
}

pub const MAX_DEPTH: u32 = 4;

/// Pre-order обход записей с рекурсией во вложенные сторы (данные записи
/// начинаются с валидного NVAR-заголовка) и резолвом GUID по хвосту буфера
/// каждого уровня. Глубина ≤ MAX_DEPTH (спека nvar-op §1).
pub fn flatten(buf: &[u8]) -> Vec<FlatRecord> {
    let mut out = Vec::new();
    flatten_inner(buf, 0, 0, &mut out);
    out
}

fn flatten_inner(buf: &[u8], base: usize, depth: u32, out: &mut Vec<FlatRecord>) {
    for r in walk(buf) {
        let mut r = r;
        if let Some(gi) = r.guid_index {
            r.guid = resolve_guid(buf, gi);
        }
        let (data_offset, data_len) = (r.data_offset, r.data_len);
        out.push(FlatRecord {
            data_off: base + data_offset,
            depth,
            record: r,
        });
        if depth >= MAX_DEPTH || data_len < 10 {
            continue;
        }
        let inner = &buf[data_offset..data_offset + data_len];
        if parse_entry(inner, 0).is_some() {
            flatten_inner(inner, base + data_offset, depth + 1, out);
        }
    }
}

/// Поиск записи по имени (GUID опционален). Неоднозначность имени без GUID —
/// Err с перечнем кандидатов «имя GUID (N bytes)» (спека nvar-op §1/§3).
pub fn find_var(
    buf: &[u8],
    name: &str,
    guid: Option<Guid>,
) -> Result<Option<FlatRecord>, NvarError> {
    let mut hits: Vec<FlatRecord> = flatten(buf)
        .into_iter()
        .filter(|f| f.record.name.as_deref() == Some(name))
        .filter(|f| guid.is_none_or(|g| f.record.guid == Some(g)))
        .collect();
    match hits.len() {
        0 => Ok(None),
        1 => Ok(Some(hits.swap_remove(0))),
        _ => {
            let cands = hits
                .iter()
                .map(|f| {
                    let g = f
                        .record
                        .guid
                        .map(|g| crate::guid_to_upper_string(&g))
                        .unwrap_or_else(|| "-".into());
                    format!(
                        "{} {} ({} bytes)",
                        f.record.name.clone().unwrap_or_default(),
                        g,
                        f.record.data_len
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(NvarError::AmbiguousName(format!("{name}: {cands}")))
        }
    }
}

pub struct NvarVarRow {
    pub name: String,
    pub guid: Option<Guid>,
    pub offset: usize,
    pub size: usize,
    pub attributes: u8,
    pub depth: u32,
    pub data: Vec<u8>,
}

pub struct NvarStoreListing {
    pub path: String,
    pub desc: String,
    pub rows: Vec<NvarVarRow>,
    pub records: usize,
    pub free_tail: usize,
    pub guid_store_size: usize,
}

pub(crate) struct StoreNodeHit {
    pub path: Vec<usize>,
    pub desc: String,
}

/// Сбор NVAR-сторов: File/Section-лист с NVAR-телом; спуск через развёрнутое
/// содержимое (LZMA-копии включаются). Стор за non-recompressable секцией —
/// запечённый слепок дефолтов (226D2IL/C275: StdDefaults-копия за Tiano),
/// вне листинга/правок: пропускается (спека nvar-op §3).
pub(crate) fn collect_stores(
    node: &FfsNode,
    path: &mut Vec<usize>,
    barrier: bool,
    file_guid: Option<&Guid>,
    out: &mut Vec<StoreNodeHit>,
) {
    let own_file_guid = if node.node_type == FfsType::File {
        node.guid.as_ref()
    } else {
        file_guid
    };
    if is_nvar_body(node) {
        if !barrier {
            out.push(StoreNodeHit {
                path: path.clone(),
                desc: store_desc(node, own_file_guid),
            });
        }
        return;
    }
    let child_barrier = barrier || section_blocks_mutation(node);
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_stores(child, path, child_barrier, own_file_guid, out);
        path.pop();
    }
}

/// Section-барьер мутаций v5: COMPRESSION/GUID_DEFINED без гарантированного
/// пересжатия (не recompressable-LZMA). Единая точка для nvar_set и
/// collect_std_defaults_hits (hii/mod.rs) — спека nvar-op §3.
pub(crate) fn section_blocks_mutation(node: &FfsNode) -> bool {
    node.node_type == FfsType::Section
        && (node.subtype == crate::ffs::EFI_SECTION_COMPRESSION
            || node.subtype == crate::ffs::EFI_SECTION_GUID_DEFINED)
        && !matches!(
            &node.parsing_data,
            crate::types::ParsingData::GuidedSection(d)
                if crate::ffs::is_recompressable_lzma_guid(&d.guid)
        )
}

/// Листинг NVAR-сторов образа: без path — все сторы; с path — один
/// (грамматика target: tree-path «0/2» или GUID-форма, parse_target).
/// offset строки = абсолютный офсет данных в образе
/// node.offset + header.len() + data_off (спека nvar-op §3).
pub fn nvar_list(
    image: &Image,
    path: Option<&str>,
    include_data: bool,
) -> Result<Vec<NvarStoreListing>, NvarError> {
    let mut out = Vec::new();
    match path {
        Some(p) => {
            let target = crate::parser::target::parse_target(p)
                .map_err(|_| NvarError::InvalidStore(format!("bad path: {p}")))?;
            let node = crate::parser::target::find_item(&image.root, &target)
                .map_err(|_| NvarError::InvalidStore(format!("path {p} not found")))?;
            if !is_nvar_body(node) {
                return Err(NvarError::InvalidStore(format!(
                    "node {p} is not an NVAR store"
                )));
            }
            let indices =
                crate::parser::target::find_item_path(&image.root, &target).unwrap_or_default();
            let path_str = indices
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join("/");
            let desc = store_desc(node, owning_file_guid(&image.root, &indices).as_ref());
            out.push(listing_of(node, &path_str, desc, include_data));
        }
        None => {
            let mut hits = Vec::new();
            let mut p = Vec::new();
            collect_stores(&image.root, &mut p, false, None, &mut hits);
            for h in hits {
                let node = crate::hii::node_at(&image.root, &h.path);
                let path_str = h
                    .path
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(listing_of(node, &path_str, h.desc.clone(), include_data));
            }
        }
    }
    Ok(out)
}

fn owning_file_guid(root: &FfsNode, indices: &[usize]) -> Option<Guid> {
    let mut node = root;
    let mut guid = None;
    for &i in indices {
        node = node.children.get(i)?;
        if node.node_type == FfsType::File {
            guid = node.guid;
        }
    }
    guid
}

fn listing_of(
    node: &FfsNode,
    path_str: &str,
    desc: String,
    include_data: bool,
) -> NvarStoreListing {
    let buf = &node.body;
    let flats = flatten(buf);
    let body_base = node.offset as usize + node.header.len();
    let rows: Vec<NvarVarRow> = flats
        .iter()
        .map(|f| NvarVarRow {
            name: f
                .record
                .name
                .clone()
                .unwrap_or_else(|| "(data-only)".into()),
            guid: f.record.guid,
            offset: body_base + f.data_off,
            size: f.record.data_len,
            attributes: f.record.attributes,
            depth: f.depth,
            data: if include_data {
                buf[f.data_off..f.data_off + f.record.data_len].to_vec()
            } else {
                Vec::new()
            },
        })
        .collect();
    let top = walk(buf);
    let consumed = top.last().map(|r| r.offset + r.size).unwrap_or(0);
    let guid_count = top
        .iter()
        .filter_map(|r| r.guid_index)
        .map(|g| g as usize + 1)
        .max()
        .unwrap_or(0);
    let guid_store_size = 16 * guid_count;
    NvarStoreListing {
        path: path_str.to_string(),
        desc,
        records: rows.len(),
        free_tail: buf.len().saturating_sub(consumed + guid_store_size),
        guid_store_size,
        rows,
    }
}

/// Описание стора для диагностик: «file <GUID> (raw body)» либо
/// «file <GUID> section <type> raw body» (перенесена из hii/mod.rs:742).
pub(crate) fn store_desc(node: &FfsNode, file_guid: Option<&Guid>) -> String {
    let guid_text = file_guid
        .map(crate::guid_to_upper_string)
        .unwrap_or_else(|| "unknown".into());
    if node.node_type == FfsType::File {
        format!("file {guid_text} (raw body)")
    } else {
        format!("file {guid_text} section {:#04x} raw body", node.subtype)
    }
}

#[derive(Debug)]
pub struct NvarSetOutcome {
    pub applied: Vec<String>,
    pub stores: Vec<String>,
}

struct VarHit {
    path: Vec<usize>,
    desc: String,
    data_off: usize,
}

struct VarHits {
    hits: Vec<VarHit>,
    var_behind_barrier: bool,
}

#[allow(clippy::too_many_arguments)]
fn collect_var_hits(
    node: &FfsNode,
    path: &mut Vec<usize>,
    barrier: bool,
    file_guid: Option<&Guid>,
    name: &str,
    guid: Option<Guid>,
    offset: usize,
    width: usize,
    out: &mut VarHits,
) -> Result<(), NvarError> {
    let own_file_guid = if node.node_type == FfsType::File {
        node.guid.as_ref()
    } else {
        file_guid
    };
    if is_nvar_body(node) {
        if barrier {
            if find_var(&node.body, name, guid).ok().flatten().is_some() {
                out.var_behind_barrier = true;
            }
            return Ok(());
        }
        if let Some(f) = find_var(&node.body, name, guid)? {
            if f.record.extended_size > 0 {
                return Err(NvarError::ValueOpUnsupported(
                    "record has an extended header; edit refused".into(),
                ));
            }
            if offset + width > f.record.data_len {
                return Err(NvarError::ValueOpUnsupported(format!(
                    "offset {offset:#x} + width {width} exceeds data size {}",
                    f.record.data_len
                )));
            }
            out.hits.push(VarHit {
                path: path.clone(),
                desc: store_desc(node, own_file_guid),
                data_off: f.data_off,
            });
        }
        return Ok(());
    }
    let child_barrier = barrier || section_blocks_mutation(node);
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        collect_var_hits(
            child,
            path,
            child_barrier,
            own_file_guid,
            name,
            guid,
            offset,
            width,
            out,
        )?;
        path.pop();
    }
    Ok(())
}

/// Правка данных переменной во ВСЕХ копиях во всех сторах образа (философия
/// согласованного флипа v5). Адресация: имя; GUID обязателен при
/// неоднозначности имени в сторе. Запечённые копии за non-recompressable
/// секциями пропускаются; отказ MutationBehindCompression — только если
/// переменная найдена лишь за барьером. Мутация in-place +
/// ops::mark_rebuild_to_root_by_path (LZMA-копии пересобираются билдером,
/// как у set_value). Спека nvar-op §3.
pub fn nvar_set(
    image: &mut Image,
    name: &str,
    guid: Option<&str>,
    offset: u64,
    value: u64,
    width: u32,
) -> Result<NvarSetOutcome, NvarError> {
    if image.mode != ImageMode::Write {
        return Err(NvarError::NotWritable);
    }
    if !matches!(width, 1 | 2 | 4 | 8) {
        return Err(NvarError::ValueOpUnsupported(format!(
            "width {width} must be one of 1, 2, 4, 8"
        )));
    }
    let width = width as usize;
    if width < 8 && value >= 1u64 << (8 * width) {
        return Err(NvarError::ValueOpUnsupported(format!(
            "value {value} does not fit width {width}"
        )));
    }
    let guid = match guid {
        Some(s) => Some(
            Guid::try_parse(s)
                .map_err(|e| NvarError::ValueOpUnsupported(format!("invalid guid '{s}': {e}")))?,
        ),
        None => None,
    };
    let offset = offset as usize;
    let mut found = VarHits {
        hits: Vec::new(),
        var_behind_barrier: false,
    };
    let mut path = Vec::new();
    collect_var_hits(
        &image.root,
        &mut path,
        false,
        None,
        name,
        guid,
        offset,
        width,
        &mut found,
    )?;
    if found.hits.is_empty() {
        if found.var_behind_barrier {
            return Err(NvarError::MutationBehindCompression);
        }
        return Err(NvarError::ValueOpUnsupported(format!(
            "no NVAR store in the image has a variable named '{name}'"
        )));
    }
    let hits = found.hits;
    struct Plan {
        hit_index: usize,
        off: usize,
        from: Vec<u8>,
        to: Vec<u8>,
    }
    let mut plans = Vec::new();
    for (i, hit) in hits.iter().enumerate() {
        let off = hit.data_off + offset;
        let from = crate::hii::node_at(&image.root, &hits[i].path)
            .body
            .get(off..off + width)
            .ok_or_else(|| NvarError::ValueOpUnsupported("record data out of bounds".into()))?
            .to_vec();
        let mut to = value.to_le_bytes().to_vec();
        to.truncate(width);
        plans.push(Plan {
            hit_index: i,
            off,
            from,
            to,
        });
    }
    let mut applied = Vec::new();
    for plan in &plans {
        if plan.from == plan.to {
            continue;
        }
        let hit = &hits[plan.hit_index];
        {
            let node = crate::hii::node_at_mut(&mut image.root, &hit.path);
            node.body[plan.off..plan.off + width].copy_from_slice(&plan.to);
        }
        crate::ops::mark_rebuild_to_root_by_path(&mut image.root, &hit.path);
        applied.push(format!(
            "{} store+{:#x}: {} -> {}",
            hit.desc,
            plan.off,
            hex_bytes(&plan.from),
            hex_bytes(&plan.to)
        ));
    }
    Ok(NvarSetOutcome {
        applied,
        stores: hits.iter().map(|h| h.desc.clone()).collect(),
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode, ParsingData};

    fn entry(name: Option<&str>, data: &[u8], attributes: u8, guid_index: Option<u8>) -> Vec<u8> {
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
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]); // next
        e.push(attributes);
        e.extend_from_slice(&body);
        e
    }

    fn inner_fixture() -> Vec<u8> {
        let mut inner = Vec::new();
        inner.extend_from_slice(&entry(Some("Setup"), &[0u8; 114], 0x82, Some(0)));
        inner.extend_from_slice(&entry(Some("Timeout"), &[0, 0], 0x83, Some(1)));
        inner.extend_from_slice(&entry(Some("Setup"), &[0x11; 6], 0x82, Some(4))); // вторая Setup
        inner
    }

    fn nvar_dup_records_fixture() -> Vec<u8> {
        let mut inner = entry(Some("Setup"), &[0x11u8; 6], 0x82, Some(0));
        inner.extend_from_slice(&entry(Some("Setup"), &[0x22u8; 6], 0x82, Some(0)));
        inner
    }

    fn store_fixture() -> Vec<u8> {
        let mut store = entry(Some("StdDefaults"), &inner_fixture(), 0x82, Some(0));
        store.extend_from_slice(&[0xFF; 8]); // свободный хвост
        store
    }

    #[test]
    fn parse_entry_reads_std_defaults_geometry() {
        let store = store_fixture();
        let inner = inner_fixture();
        let r = parse_entry(&store, 0).expect("StdDefaults entry parses");
        assert_eq!(r.name.as_deref(), Some("StdDefaults"));
        assert_eq!(r.size, store.len() - 8);
        assert_eq!(r.data_offset, 23);
        assert_eq!(r.data_len, inner.len());
    }

    #[test]
    fn parse_entry_rejects_bad_signature_and_bounds() {
        assert_eq!(parse_entry(&[0xFFu8; 32], 0), None);
        assert_eq!(parse_entry(b"XXXX", 0), None);
        let mut truncated = store_fixture();
        truncated.truncate(100);
        assert_eq!(parse_entry(&truncated, 0), None);
        let mut tiny = Vec::new();
        tiny.extend_from_slice(b"NVAR");
        tiny.extend_from_slice(&9u16.to_le_bytes());
        tiny.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        tiny.push(0x82);
        assert_eq!(parse_entry(&tiny, 0), None);
    }

    #[test]
    fn walk_is_sequential_by_size() {
        let inner = inner_fixture();
        let records = walk(&inner);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].name.as_deref(), Some("Setup"));
        assert_eq!(records[0].data_offset, 17);
        assert_eq!(records[0].data_len, 114);
        assert_eq!(records[1].name.as_deref(), Some("Timeout"));
        assert_eq!(records[1].data_len, 2);
        assert_eq!(records[2].name.as_deref(), Some("Setup"));
        assert_eq!(records[2].data_len, 6);
        assert_eq!(records[1].offset, records[0].offset + records[0].size);
        assert_eq!(records[2].offset, records[1].offset + records[1].size);
    }

    #[test]
    fn std_defaults_data_returns_inner() {
        let store = store_fixture();
        let inner = inner_fixture();
        assert_eq!(std_defaults_data(&store), Some((23, inner.as_slice())));
    }

    #[test]
    fn std_defaults_data_rejects_non_std_defaults_store() {
        let store = entry(Some("Setup"), &[0u8; 4], 0x82, Some(0));
        assert_eq!(std_defaults_data(&store), None);
    }

    #[test]
    fn find_varstore_record_matches_name_and_len() {
        let store = store_fixture();
        let (off, data) = find_varstore_record(&store, "Setup", 114).expect("first Setup matches");
        assert_eq!(off, 23 + 17);
        assert_eq!(data, &[0u8; 114]);
    }

    #[test]
    fn find_varstore_record_ignores_same_name_wrong_len() {
        let store = store_fixture();
        let (off, data) = find_varstore_record(&store, "Setup", 6).expect("second Setup matches");
        assert_eq!(off, 23 + 131 + 21 + 17);
        assert_eq!(data, &[0x11u8; 6]);
        assert_eq!(find_varstore_record(&store, "Setup", 7), None);
    }

    #[test]
    fn find_varstore_record_missing_is_none() {
        let store = store_fixture();
        assert_eq!(find_varstore_record(&store, "Missing", 114), None);
    }

    #[test]
    fn find_varstore_record_pins_first_match_on_duplicate_name_and_len() {
        let inner = nvar_dup_records_fixture();
        let store = entry(Some("StdDefaults"), &inner, 0x82, Some(0));
        let (off, data) = find_varstore_record(&store, "Setup", 6).unwrap();
        assert_eq!(data, &[0x11u8; 6], "first record wins");
        assert_eq!(&store[off..off + 6], &[0x11u8; 6]);
    }

    #[test]
    fn parse_entry_data_only_has_no_name() {
        let e = entry(None, &[0xAA, 0xBB], ATTR_VALID | ATTR_DATA_ONLY, None);
        let r = parse_entry(&e, 0).expect("data-only entry parses");
        assert_eq!(r.name, None);
        assert_eq!(r.guid_index, None);
        assert_eq!(r.data_offset, 10);
        assert_eq!(r.data_len, 2);
    }

    #[test]
    fn parse_entry_local_guid_skips_16_bytes() {
        let mut body = Vec::new();
        body.extend_from_slice(&[0x5Du8; 16]);
        body.extend_from_slice(b"Boot");
        body.push(0);
        body.extend_from_slice(&[0x01, 0x02]);
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        e.extend_from_slice(&((10 + body.len()) as u16).to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(ATTR_VALID | ATTR_LOCAL_GUID | ATTR_ASCII_NAME);
        e.extend_from_slice(&body);
        let r = parse_entry(&e, 0).expect("local-guid entry parses");
        assert_eq!(r.guid_index, None);
        assert_eq!(r.name.as_deref(), Some("Boot"));
        assert_eq!(r.data_offset, 10 + 16 + 5);
        assert_eq!(r.data_len, 2);
    }

    fn raw_entry(size: u16, attributes: u8, body: &[u8]) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        e.extend_from_slice(&size.to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(attributes);
        e.extend_from_slice(body);
        e
    }

    #[test]
    fn parse_entry_malformed_geometries_return_none() {
        let e = raw_entry(10, ATTR_VALID, &[]);
        assert_eq!(parse_entry(&e, 0), None);
        let e = raw_entry(
            12,
            ATTR_VALID | ATTR_LOCAL_GUID | ATTR_ASCII_NAME,
            &[0x00, 0x00],
        );
        assert_eq!(parse_entry(&e, 0), None);
        let e = raw_entry(15, ATTR_VALID, &[0x00, 0x42, 0x00, 0x43, 0x00]);
        assert_eq!(parse_entry(&e, 0), None);
    }

    #[test]
    fn parse_entry_ucs2_name_reads_name() {
        let mut body = Vec::new();
        body.push(0);
        for u in "Bo".encode_utf16() {
            body.extend_from_slice(&u.to_le_bytes());
        }
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&[0xEE, 0xFF]);
        let size = (10 + body.len()) as u16;
        let e = raw_entry(size, ATTR_VALID, &body);
        let r = parse_entry(&e, 0).expect("ucs2-name entry parses");
        assert_eq!(r.guid_index, Some(0));
        assert_eq!(r.name.as_deref(), Some("Bo"));
        assert_eq!(r.data_offset, 17);
        assert_eq!(r.data_len, 2);
    }

    #[test]
    fn walk_stops_at_ff_tail() {
        let store = store_fixture();
        let records = walk(&store);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name.as_deref(), Some("StdDefaults"));
        assert_eq!(records[0].size, store.len() - 8);
    }

    #[test]
    fn parse_entry_reads_next_link() {
        let mut e = entry(Some("Boot"), &[0x01], ATTR_VALID | ATTR_ASCII_NAME, Some(0));
        e[6] = 0x34;
        e[7] = 0x12;
        e[8] = 0x00;
        let r = parse_entry(&e, 0).expect("entry parses");
        assert_eq!(r.next, Some(0x1234));
        let r2 = parse_entry(&entry(Some("Boot"), &[0x01], 0x82, Some(0)), 0).unwrap();
        assert_eq!(r2.next, None, "0xFFFFFF = нет ссылки");
    }

    #[test]
    fn parse_entry_extended_header_shrinks_data() {
        let data = vec![0xAA, 0xBB, 0xCC];
        let ext = vec![0x00, 0x03, 0x00]; // attrs u8 + size u16 = 3 (поле size считает весь ext-хвост)
        let mut body = Vec::new();
        body.push(0);
        body.extend_from_slice(b"Boot");
        body.push(0);
        body.extend_from_slice(&data);
        body.extend_from_slice(&ext);
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        e.extend_from_slice(&((10 + body.len()) as u16).to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(ATTR_VALID | ATTR_ASCII_NAME | ATTR_EXTENDED_HEADER);
        e.extend_from_slice(&body);
        let r = parse_entry(&e, 0).expect("ext-header entry parses");
        assert_eq!(r.extended_size, 3);
        assert_eq!(r.data_len, 3, "данные не включают ext-хвост");
    }

    #[test]
    fn parse_entry_extended_header_garbage_size_treated_as_absent() {
        let mut body = Vec::new();
        body.push(0);
        body.extend_from_slice(b"Boot");
        body.push(0);
        body.extend_from_slice(&[0xAA]);
        body.extend_from_slice(&[0x00, 0x02, 0x00]); // size=2 < 3 → невалиден
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        e.extend_from_slice(&((10 + body.len()) as u16).to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(ATTR_VALID | ATTR_ASCII_NAME | ATTR_EXTENDED_HEADER);
        e.extend_from_slice(&body);
        let r = parse_entry(&e, 0).expect("entry parses");
        assert_eq!(r.extended_size, 0);
        assert_eq!(r.data_len, 4, "хвост считается данными");
    }

    /// Внутренний стор: записи + свободный хвост + GUID-стор в хвосте
    /// ВНУТРЕННЕГО блоба (как на живых образах: GUID-резолв записей — по
    /// хвосту буфера своего уровня).
    fn guids_inner_fixture() -> Vec<u8> {
        let ec87 = Guid::try_parse("ec87d643-eba4-4bb5-a1e5-3f3e36b20da9").unwrap();
        let g80e1 = Guid::try_parse("80e1202e-2697-4264-9cc9-80762c3e5863").unwrap();
        let lu = Guid::try_parse("8be4df61-93ca-11d2-aa0d-00e098032b8c").unwrap();
        let mut inner = Vec::new();
        inner.extend_from_slice(&entry(Some("Setup"), &[0x11; 4], 0x82, Some(0)));
        inner.extend_from_slice(&entry(Some("Lang"), &[0x22; 2], 0x83, Some(1)));
        inner.extend_from_slice(&entry(Some("Setup"), &[0x33; 2], 0x82, Some(2)));
        inner.extend_from_slice(&[0xFF; 4]);
        inner.extend_from_slice(&ec87.to_bytes()); // gi=2
        inner.extend_from_slice(&g80e1.to_bytes()); // gi=1
        inner.extend_from_slice(&lu.to_bytes()); // gi=0
        inner
    }

    fn guids_fixture() -> Vec<u8> {
        entry(Some("StdDefaults"), &guids_inner_fixture(), 0x82, Some(0))
    }

    #[test]
    fn resolve_guid_maps_index_to_tail() {
        let buf = guids_inner_fixture();
        let ec87 = Guid::try_parse("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        let lu = Guid::try_parse("8be4df61-93ca-11d2-aa0d-00e098032b8c").unwrap();
        assert_eq!(resolve_guid(&buf, 0), Some(lu), "gi=0 — последние 16 байт");
        assert_eq!(resolve_guid(&buf, 2), Some(ec87));
        assert_eq!(
            resolve_guid(&buf, 6),
            None,
            "индекс за хвостом (16·7=112 > 110 байт фикстуры)"
        );
    }

    #[test]
    fn flatten_recurses_nested_store_and_resolves_guids() {
        let buf = guids_fixture();
        let flats = flatten(&buf);
        assert_eq!(flats.len(), 4, "StdDefaults + 3 вложенных");
        assert_eq!(flats[0].depth, 0);
        assert_eq!(flats[0].record.name.as_deref(), Some("StdDefaults"));
        assert_eq!(flats[1].depth, 1);
        assert_eq!(flats[1].record.name.as_deref(), Some("Setup"));
        let lu = Guid::try_parse("8be4df61-93ca-11d2-aa0d-00e098032b8c").unwrap();
        let ec87 = Guid::try_parse("ec87d643-eba4-4bb5-a1e5-3f3e36b20da9").unwrap();
        assert_eq!(
            flats[1].record.guid,
            Some(lu),
            "gi=0 → последний GUID хвоста"
        );
        assert_eq!(
            flats[1].data_off,
            flats[0].record.data_offset + 17,
            "data_off — в координатах топ-буфера"
        );
        assert_eq!(flats[3].record.name.as_deref(), Some("Setup"));
        assert_ne!(
            flats[1].record.guid, flats[3].record.guid,
            "две Setup различимы GUID'ом"
        );
        assert_eq!(flats[3].record.guid, Some(ec87), "gi=2 → EC87D643");
    }

    #[test]
    fn flatten_depth_is_capped_at_four() {
        let mut leaf = entry(Some("V"), &[0x00], 0x82, Some(0));
        for _ in 0..8 {
            leaf = entry(Some("V"), &leaf, 0x82, Some(0));
        }
        let flats = flatten(&leaf);
        assert!(flats.iter().all(|f| f.depth <= 4));
        assert!(
            flats.iter().any(|f| f.depth == 4),
            "кап не раньше 4 уровней"
        );
    }

    #[test]
    fn find_var_name_and_guid_is_unique() {
        let buf = guids_fixture();
        let ec87 = Guid::try_parse("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        let hit = find_var(&buf, "Setup", Some(ec87)).unwrap().expect("hit");
        assert_eq!(hit.record.data_len, 2, "GUID выбирает вторую Setup");
        let miss = find_var(&buf, "Nope", None).unwrap();
        assert!(miss.is_none());
    }

    #[test]
    fn find_var_duplicate_name_without_guid_is_error_with_candidates() {
        let buf = guids_fixture();
        let err = find_var(&buf, "Setup", None).unwrap_err();
        assert!(matches!(err, NvarError::AmbiguousName(_)));
        let msg = err.to_string();
        assert!(
            msg.contains("8BE4DF61-93CA-11D2-AA0D-00E098032B8C"),
            "{msg}"
        );
        assert!(
            msg.contains("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
            "{msg}"
        );
        assert!(msg.contains("bytes"), "{msg}");
    }

    fn zero_node(node_type: FfsType, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn empty_image() -> Image {
        Image {
            image_id: "i1".into(),
            session_id: "s1".into(),
            root: zero_node(FfsType::Image, vec![]),
            mode: ImageMode::Read,
        }
    }

    fn image_with_store(offset: u32, header_len: usize, body: Vec<u8>) -> Image {
        let mut file = zero_node(FfsType::File, vec![]);
        file.guid = Some(Guid::try_parse("CEF5B9A3-476D-497F-9FDC-E98143E0422C").unwrap());
        file.subtype = 0x01;
        file.offset = offset;
        file.header = vec![0x11; header_len];
        file.body = body;
        let mut img = empty_image();
        img.root.children = vec![zero_node(FfsType::Volume, vec![file])];
        img
    }

    #[test]
    fn nvar_list_reports_rows_offsets_and_summary() {
        let store = guids_fixture();
        let img = image_with_store(0x1000, 4, store.clone());
        let out = nvar_list(&img, None, true).unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.records, 4, "StdDefaults + 3 вложенных");
        assert_eq!(
            s.guid_store_size, 16,
            "сводка — по верхнему уровню (gi=0 у StdDefaults)"
        );
        assert_eq!(s.free_tail, 0, "внешний блоб занят записью целиком");
        assert_eq!(s.path, "0/0");
        assert!(s.desc.contains("CEF5B9A3"), "{}", s.desc);
        let setup_main = s
            .rows
            .iter()
            .find(|r| r.name == "Setup" && r.size == 4)
            .unwrap();
        let lu = Guid::try_parse("8be4df61-93ca-11d2-aa0d-00e098032b8c").unwrap();
        assert_eq!(setup_main.guid, Some(lu), "gi=0 → GUID из хвоста");
        assert_eq!(setup_main.depth, 1);
        assert_eq!(setup_main.data, vec![0x11; 4]);
        assert_eq!(
            setup_main.offset,
            0x1000 + 4 + s_body_setup_abs(&store),
            "абсолютный офсет = node.offset + header + data_off"
        );
        let ec87 = Guid::try_parse("ec87d643-eba4-4bb5-a1e5-3f3e36b20da9").unwrap();
        let setup_second = s
            .rows
            .iter()
            .find(|r| r.name == "Setup" && r.size == 2)
            .unwrap();
        assert_eq!(setup_second.guid, Some(ec87), "gi=2 → EC87D643");
    }

    fn s_body_setup_abs(store: &[u8]) -> usize {
        flatten(store)
            .into_iter()
            .find(|f| f.record.name.as_deref() == Some("Setup") && f.record.data_len == 4)
            .unwrap()
            .data_off
    }

    #[test]
    fn nvar_list_data_only_row_and_no_data_mode() {
        let mut inner = entry(Some("Setup"), &[0x11; 4], 0x82, Some(0));
        inner.extend_from_slice(&raw_entry(
            (10 + 2) as u16,
            ATTR_VALID | ATTR_DATA_ONLY,
            &[0xAA, 0xBB],
        ));
        let store = entry(Some("StdDefaults"), &inner, 0x82, Some(0));
        let img = image_with_store(0x100, 4, store);
        let out = nvar_list(&img, None, false).unwrap();
        let data_only = out[0]
            .rows
            .iter()
            .find(|r| r.name == "(data-only)")
            .unwrap();
        assert_eq!(data_only.size, 2);
        assert!(data_only.guid.is_none());
        assert!(
            out[0].rows.iter().all(|r| r.data.is_empty()),
            "include_data=false"
        );
    }

    #[test]
    fn nvar_list_targeted_path_and_rejections() {
        let store = guids_fixture();
        let img = image_with_store(0x1000, 4, store);
        let one = nvar_list(&img, Some("0/0"), false).unwrap();
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].path, "0/0");
        assert!(matches!(
            nvar_list(&img, Some("0/9"), false),
            Err(NvarError::InvalidStore(_))
        ));
        let empty = empty_image();
        assert!(matches!(
            nvar_list(&empty, Some("0"), false),
            Err(NvarError::InvalidStore(_))
        ));
    }

    fn image_live_store_plus_blocked_copy() -> Image {
        let mk_store = |seed: u8| {
            let inner = entry(Some("Setup"), &[seed; 8], 0x82, Some(0));
            entry(Some("StdDefaults"), &inner, 0x82, Some(0))
        };
        let mk_file = |offset: u32, seed: u8| FfsNode {
            guid: Some(Guid::try_parse("CEF5B9A3-476D-497F-9FDC-E98143E0422C").unwrap()),
            node_type: FfsType::File,
            subtype: 0x01,
            offset,
            header: vec![0x11; 4],
            body: mk_store(seed),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let guided = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: crate::ffs::EFI_SECTION_GUID_DEFINED,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![mk_file(0x3000, 0x00)],
            action: Action::NoAction,
            parsing_data: ParsingData::GuidedSection(crate::types::GuidedSectionParsingData {
                guid: Guid::try_parse("00000000-0000-0000-0000-000000000000").unwrap(),
                dictionary_size: 0,
            }),
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let mut img = empty_image();
        img.root.children = vec![
            zero_node(FfsType::Volume, vec![mk_file(0x1000, 0x00)]),
            guided,
        ];
        img
    }

    #[test]
    fn nvar_list_skips_store_behind_hard_compression() {
        let mut img = image_live_store_plus_blocked_copy();
        img.mode = ImageMode::Read;
        let out = nvar_list(&img, None, false).unwrap();
        assert_eq!(
            out.len(),
            1,
            "запечённая копия за non-recompressable секцией не листится"
        );
        assert_eq!(out[0].path, "0/0");
    }

    #[test]
    fn nvar_set_skips_blocked_copy_when_live_store_exists() {
        let mut img = image_live_store_plus_blocked_copy();
        img.mode = ImageMode::Write;
        let out = nvar_set(&mut img, "Setup", None, 0, 1, 1).unwrap();
        assert_eq!(out.applied.len(), 1, "только живой стор");
        let live = flatten(&img.root.children[0].children[0].body)
            .into_iter()
            .find(|x| x.record.name.as_deref() == Some("Setup"))
            .unwrap();
        assert_eq!(img.root.children[0].children[0].body[live.data_off], 1);
        let blocked = flatten(&img.root.children[1].children[0].body)
            .into_iter()
            .find(|x| x.record.name.as_deref() == Some("Setup"))
            .unwrap();
        assert_eq!(
            img.root.children[1].children[0].body[blocked.data_off], 0,
            "запечённая копия не тронута"
        );
    }

    fn image_two_stores_write() -> Image {
        let mk = |seed: u8| {
            let mut inner = entry(Some("Setup"), &[seed; 8], 0x82, Some(0));
            inner.extend_from_slice(&[0u8; 6]);
            entry(Some("StdDefaults"), &inner, 0x82, Some(0))
        };
        let f1 = FfsNode {
            guid: Some(Guid::try_parse("CEF5B9A3-476D-497F-9FDC-E98143E0422C").unwrap()),
            node_type: FfsType::File,
            subtype: 0x01,
            offset: 0x1000,
            header: vec![0x11; 4],
            body: mk(0x00),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let mut f2 = f1.clone();
        f2.guid = Some(Guid::try_parse("AA000000-0000-0000-0000-000000000001").unwrap());
        f2.offset = 0x2000;
        let mut img = empty_image();
        img.mode = ImageMode::Write;
        img.root.children = vec![f1, f2];
        img
    }

    fn ext_header_setup_store() -> Vec<u8> {
        let mut body = Vec::new();
        body.push(0);
        body.extend_from_slice(b"Setup");
        body.push(0);
        body.extend_from_slice(&[0x00; 4]);
        body.extend_from_slice(&[0x00, 0x03, 0x00]);
        let mut e = Vec::new();
        e.extend_from_slice(b"NVAR");
        e.extend_from_slice(&((10 + body.len()) as u16).to_le_bytes());
        e.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        e.push(ATTR_VALID | ATTR_ASCII_NAME | ATTR_EXTENDED_HEADER);
        e.extend_from_slice(&body);
        entry(Some("StdDefaults"), &e, 0x82, Some(0))
    }

    #[test]
    fn nvar_set_flips_all_copies_and_reports() {
        let mut img = image_two_stores_write();
        let out = nvar_set(&mut img, "Setup", None, 2, 0xAB, 1).unwrap();
        assert_eq!(out.stores.len(), 2);
        assert_eq!(out.applied.len(), 2);
        assert!(out.applied[0].contains("store+"), "{}", out.applied[0]);
        assert!(out.applied[0].contains("00 -> ab"), "{}", out.applied[0]);
        for f in &img.root.children {
            let flat = flatten(&f.body)
                .into_iter()
                .find(|x| x.record.name.as_deref() == Some("Setup"))
                .unwrap();
            assert_eq!(f.body[flat.data_off + 2], 0xAB);
            assert_eq!(f.body[flat.data_off + 3], 0x00, "width=1 трогает один байт");
        }
    }

    #[test]
    fn nvar_set_noop_copy_is_reported_via_stores() {
        let mut img = image_two_stores_write();
        let out = nvar_set(&mut img, "Setup", None, 0, 0x00, 1).unwrap();
        assert_eq!(out.stores.len(), 2);
        assert!(out.applied.is_empty(), "нет фактических флипов");
    }

    #[test]
    fn nvar_set_validates_width_value_offset_and_mode() {
        let mut img = image_two_stores_write();
        assert!(matches!(
            nvar_set(&mut img, "Setup", None, 0, 0, 3),
            Err(NvarError::ValueOpUnsupported(_))
        ));
        assert!(matches!(
            nvar_set(&mut img, "Setup", None, 0, 0x1FF, 1),
            Err(NvarError::ValueOpUnsupported(_))
        ));
        assert!(matches!(
            nvar_set(&mut img, "Setup", None, 8, 0, 1),
            Err(NvarError::ValueOpUnsupported(_))
        ));
        assert!(matches!(
            nvar_set(&mut img, "Setup", Some("not-a-guid"), 0, 0, 1),
            Err(NvarError::ValueOpUnsupported(_))
        ));
        assert!(matches!(
            nvar_set(&mut img, "Missing", None, 0, 0, 1),
            Err(NvarError::ValueOpUnsupported(_))
        ));
        let mut ro = image_two_stores_write();
        ro.mode = ImageMode::Read;
        assert!(matches!(
            nvar_set(&mut ro, "Setup", None, 0, 0, 1),
            Err(NvarError::NotWritable)
        ));
    }

    #[test]
    fn nvar_set_refuses_extended_header_record() {
        let mut img = empty_image();
        img.mode = ImageMode::Write;
        img.root.children = vec![FfsNode {
            guid: Some(Guid::try_parse("CEF5B9A3-476D-497F-9FDC-E98143E0422C").unwrap()),
            node_type: FfsType::File,
            subtype: 0x01,
            offset: 0,
            header: vec![],
            body: ext_header_setup_store(),
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }];
        let err = nvar_set(&mut img, "Setup", None, 0, 1, 1).unwrap_err();
        assert!(matches!(err, NvarError::ValueOpUnsupported(_)));
        assert!(err.to_string().contains("extended header"));
    }

    #[test]
    fn nvar_set_width_two_writes_le() {
        let mut img = image_two_stores_write();
        nvar_set(&mut img, "Setup", None, 0, 0x1234, 2).unwrap();
        let f = &img.root.children[0];
        let flat = flatten(&f.body)
            .into_iter()
            .find(|x| x.record.name.as_deref() == Some("Setup"))
            .unwrap();
        assert_eq!(&f.body[flat.data_off..flat.data_off + 2], &[0x34, 0x12]);
    }

    #[test]
    fn nvar_set_behind_hard_compression_is_refused() {
        let mut img = empty_image();
        img.mode = ImageMode::Write;
        img.root.children = vec![FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: crate::ffs::EFI_SECTION_GUID_DEFINED,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![FfsNode {
                guid: Some(Guid::try_parse("CEF5B9A3-476D-497F-9FDC-E98143E0422C").unwrap()),
                node_type: FfsType::File,
                subtype: 0x01,
                offset: 0,
                header: vec![],
                body: {
                    let inner = entry(Some("Setup"), &[0x00; 4], 0x82, Some(0));
                    entry(Some("StdDefaults"), &inner, 0x82, Some(0))
                },
                tail: vec![],
                children: vec![],
                action: Action::NoAction,
                parsing_data: ParsingData::None,
                fixed: false,
                compressed: false,
                alignment_bytes: vec![],
            }],
            action: Action::NoAction,
            parsing_data: ParsingData::GuidedSection(crate::types::GuidedSectionParsingData {
                guid: Guid::try_parse("00000000-0000-0000-0000-000000000000").unwrap(),
                dictionary_size: 0,
            }),
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }];
        assert!(matches!(
            nvar_set(&mut img, "Setup", None, 0, 1, 1),
            Err(NvarError::MutationBehindCompression)
        ));
    }
}
