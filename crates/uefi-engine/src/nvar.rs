use crate::types::Guid;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
