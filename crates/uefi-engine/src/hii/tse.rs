//! AMITSE setup browser: stride-таблицы `{GUID, u64 form-id, u64 0}`
//! в PE-теле (P2 analyzer-patterns) и операция снятия hide-маркера.
//! Спека 2026-10-02-tse-unhide-design.md.

use std::collections::HashSet;

use uguid::Guid;

pub const AMITSE_GUID_STR: &str = "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A";
pub const STRIDE_ENTRY_SIZE: usize = 0x20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrideEntry {
    pub guid: Guid,
    pub form_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrideBlock {
    pub pe_offset: usize,
    pub entries: Vec<StrideEntry>,
}

fn stride_series_at(pe: &[u8], off: usize) -> Option<Vec<StrideEntry>> {
    let mut entries = Vec::new();
    let mut p = off;
    while p + STRIDE_ENTRY_SIZE <= pe.len() {
        let rec = &pe[p..p + STRIDE_ENTRY_SIZE];
        if rec.iter().all(|&b| b == 0) {
            return Some(entries);
        }
        let form_id = u64::from_le_bytes(rec[16..24].try_into().unwrap());
        let tail = u64::from_le_bytes(rec[24..32].try_into().unwrap());
        if tail != 0 || form_id == 0 || form_id >= 0x10000 {
            return None;
        }
        entries.push(StrideEntry {
            guid: Guid::from_bytes(rec[..16].try_into().unwrap()),
            form_id,
        });
        p += STRIDE_ENTRY_SIZE;
    }
    None
}

/// Серии ≥2 записей `{GUID, u64 form-id, u64 0}` шагом 0x20 с нулевым
/// терминатором; блок валиден, если ВСЕ его GUID-ы — формсеты образа
/// (known_formsets). Скан по границам 0x20 от нуля; блоки могут
/// перекрываться (хвост бара = динамика) — это отражается в инвентаре.
/// Спека tse-unhide §2.1.
pub fn scan_stride_blocks(pe: &[u8], known_formsets: &HashSet<Guid>) -> Vec<StrideBlock> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + STRIDE_ENTRY_SIZE <= pe.len() {
        if let Some(entries) = stride_series_at(pe, off)
            && entries.len() >= 2
            && entries.iter().all(|e| known_formsets.contains(&e.guid))
        {
            out.push(StrideBlock {
                pe_offset: off,
                entries,
            });
        }
        off += STRIDE_ENTRY_SIZE;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(guid: &str, fid: u64) -> Vec<u8> {
        let g = Guid::try_parse(guid).unwrap();
        let mut b = g.to_bytes().to_vec();
        b.extend_from_slice(&fid.to_le_bytes());
        b.extend_from_slice(&[0u8; 8]);
        b
    }

    fn term() -> Vec<u8> {
        vec![0u8; 0x20]
    }

    fn guid_set(items: &[&str]) -> HashSet<Guid> {
        items.iter().map(|s| Guid::try_parse(s).unwrap()).collect()
    }

    const A: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";
    const B: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";

    #[test]
    fn scan_finds_block_with_terminator() {
        let mut pe = vec![0x11u8; 0x40];
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        pe.extend(vec![0x22u8; 0x10]);
        let blocks = scan_stride_blocks(&pe, &guid_set(&[A, B]));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].pe_offset, 0x40);
        assert_eq!(blocks[0].entries.len(), 2);
        assert_eq!(blocks[0].entries[0].form_id, 1);
    }

    #[test]
    fn scan_rejects_foreign_guid_block() {
        let mut pe = Vec::new();
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2));
        pe.extend(term());
        assert!(scan_stride_blocks(&pe, &guid_set(&[A])).is_empty());
    }

    #[test]
    fn scan_requires_two_entries_and_terminator() {
        let mut one = entry(A, 1);
        one.extend(term());
        assert!(scan_stride_blocks(&one, &guid_set(&[A])).is_empty());
        let mut no_term = Vec::new();
        no_term.extend(entry(A, 1));
        no_term.extend(entry(A, 2));
        assert!(scan_stride_blocks(&no_term, &guid_set(&[A])).is_empty());
        let mut bad_tail = Vec::new();
        bad_tail.extend(entry(A, 1));
        bad_tail.extend(entry(A, 2));
        let mut last = bad_tail.split_off(0x20);
        last[24] = 1;
        bad_tail.extend(last);
        bad_tail.extend(term());
        assert!(scan_stride_blocks(&bad_tail, &guid_set(&[A])).is_empty());
        let mut big_fid = Vec::new();
        big_fid.extend(entry(A, 0x10000));
        big_fid.extend(entry(A, 2));
        big_fid.extend(term());
        assert!(scan_stride_blocks(&big_fid, &guid_set(&[A])).is_empty());
    }

    #[test]
    fn scan_reports_overlapping_tail_block() {
        let mut pe = Vec::new();
        pe.extend(entry(B, 10001));
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2));
        pe.extend(term());
        let blocks = scan_stride_blocks(&pe, &guid_set(&[A, B]));
        assert_eq!(blocks.len(), 2, "бар с 0 и хвост-блок с 0x20");
        assert_eq!(blocks[0].pe_offset, 0);
        assert_eq!(blocks[1].pe_offset, 0x20);
    }
}
