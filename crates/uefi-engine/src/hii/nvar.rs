const SIG: [u8; 4] = *b"NVAR";
pub const ATTR_VALID: u8 = 0x80;
pub const ATTR_DATA_ONLY: u8 = 0x08;
pub const ATTR_LOCAL_GUID: u8 = 0x04;
pub const ATTR_ASCII_NAME: u8 = 0x02;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvarRecord {
    pub offset: usize,
    pub size: usize,
    pub attributes: u8,
    pub guid_index: Option<u8>,
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
    Some(NvarRecord {
        offset: off,
        size,
        attributes,
        guid_index,
        name,
        data_offset: p,
        data_len: entry_end - p,
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
}
