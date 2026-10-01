//! AMITSE setup browser: stride-таблицы `{GUID, u64 form-id, u64 0}`
//! в PE-теле (P2 analyzer-patterns) и операция снятия hide-маркера.
//! Спека 2026-10-02-tse-unhide-design.md.

use std::collections::{HashMap, HashSet};

use crate::hii::HiiError;
use crate::types::{FfsNode, FfsType, Image, ImageMode};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnhideOutcome {
    pub pe_offset: usize,
    pub entry_pe_offset: usize,
    pub formset_guid: Guid,
    pub form_id: u16,
}

/// Корневая форма формсета = минимальный form_id по IFR-формам
/// формсета (collect_forms). Спека tse-unhide §2.2 п.3.
fn formset_roots(image: &Image) -> HashMap<Guid, u16> {
    let mut roots: HashMap<Guid, u16> = HashMap::new();
    for f in crate::hii::forms::collect_forms(image) {
        let Ok(g) = Guid::try_parse(&f.formset_guid) else {
            continue;
        };
        let Ok(fid) = u16::try_from(f.form_id_ifr) else {
            continue;
        };
        roots
            .entry(g)
            .and_modify(|r| *r = (*r).min(fid))
            .or_insert(fid);
    }
    roots
}

/// Формсеты образа для GUID-фильтра скана: union IFR collect_forms +
/// $SPF formset-blob (спека §2.1).
fn known_formsets(image: &Image) -> HashSet<Guid> {
    let mut s: HashSet<Guid> = formset_roots(image).into_keys().collect();
    if let Ok((_, file)) = amitse_file(image)
        && let Some(body) = spf_section_body(file)
    {
        s.extend(
            crate::hii::spf::scan_formsets(body)
                .into_iter()
                .map(|f| f.guid),
        );
    }
    s
}

fn find_file<'a>(
    node: &'a FfsNode,
    guid: &Guid,
    path: &mut Vec<usize>,
) -> Option<(Vec<usize>, &'a FfsNode)> {
    if node.node_type == FfsType::File && node.guid.as_ref() == Some(guid) {
        return Some((path.clone(), node));
    }
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(found) = find_file(c, guid, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Путь (относительно файла) к первому PE32-листу.
fn pe32_rel(file: &FfsNode) -> Option<Vec<usize>> {
    fn walk(node: &FfsNode, path: &mut Vec<usize>) -> Option<Vec<usize>> {
        if node.node_type == FfsType::Section
            && node.subtype == crate::ffs::EFI_SECTION_PE32
            && node.children.is_empty()
        {
            return Some(path.clone());
        }
        for (i, c) in node.children.iter().enumerate() {
            path.push(i);
            if let Some(p) = walk(c, path) {
                return Some(p);
            }
            path.pop();
        }
        None
    }
    walk(file, &mut Vec::new())
}

fn amitse_file(image: &Image) -> Result<(Vec<usize>, &FfsNode), HiiError> {
    let g = Guid::try_parse(AMITSE_GUID_STR).map_err(|e| HiiError::InvalidItemId(e.to_string()))?;
    find_file(&image.root, &g, &mut Vec::new()).ok_or(HiiError::NotFound)
}

fn walk_spf<'a>(node: &'a FfsNode, path: &mut Vec<usize>) -> Option<(&'a [u8], Vec<usize>)> {
    if node.node_type == FfsType::Section
        && node.children.is_empty()
        && crate::hii::spf::container_start(&node.body).is_some()
    {
        return Some((&node.body, path.clone()));
    }
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(found) = walk_spf(c, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

/// Тело секции $SPF в файле AMITSE (детект по magic; спека §2.3).
fn spf_section_body(file: &FfsNode) -> Option<&[u8]> {
    walk_spf(file, &mut Vec::new()).map(|(b, _)| b)
}

fn entry_bytes_match(pe: &[u8], off: usize, guid: &Guid, form_id: u16) -> bool {
    let Some(rec) = pe.get(off..off + STRIDE_ENTRY_SIZE) else {
        return false;
    };
    rec[..16] == guid.to_bytes()
        && rec[16..24] == u64::from(form_id).to_le_bytes()
        && rec[24..32] == [0u8; 8]
}

/// Зануление hide-записи `{formset_guid, form_id}` в stride-таблице TSE
/// PE. Кандидат = блок содержит запись И все записи блока — корневые
/// формы (дискриминация hide/бар; спека tse-unhide §2.2, правка
/// 2026-10-02). `block_pe_offset` — экспертный обход выбора блока,
/// byte-guard остаётся; в экспертном режиме `pe_offset == entry_pe_offset`.
/// Ошибки: NotFound (модуль/запись) → NotWritable →
/// InvalidSchema (ambiguous/guard). Мутация + Rebuild по цепочке предков,
/// snapshot-rollback при Err.
pub fn tse_unhide(
    image: &mut Image,
    formset_guid: &Guid,
    form_id: u16,
    block_pe_offset: Option<usize>,
) -> Result<UnhideOutcome, HiiError> {
    crate::hii::with_rollback(image, |image| {
        let (file_path, file) = amitse_file(image)?;
        let rel = pe32_rel(file).ok_or(HiiError::NotFound)?;
        let path: Vec<usize> = [file_path, rel].concat();
        if image.mode != ImageMode::Write {
            return Err(HiiError::NotWritable);
        }
        let (block_off, entry_pe_offset) =
            match block_pe_offset {
                Some(off) => {
                    let pe = &crate::hii::node_at(&image.root, &path).body;
                    if !entry_bytes_match(pe, off, formset_guid, form_id) {
                        return Err(HiiError::InvalidSchema(format!(
                            "guard mismatch at pe+{off:#x}"
                        )));
                    }
                    (off, off)
                }
                None => {
                    let roots = formset_roots(image);
                    let known = known_formsets(image);
                    let pe = &crate::hii::node_at(&image.root, &path).body;
                    let blocks = scan_stride_blocks(pe, &known);
                    let mut cands: Vec<(usize, usize)> = Vec::new();
                    for b in &blocks {
                        let all_roots = b.entries.iter().all(|e| {
                            roots
                                .get(&e.guid)
                                .is_some_and(|r| u64::from(*r) == e.form_id)
                        });
                        if !all_roots {
                            continue;
                        }
                        if let Some(pos) = b.entries.iter().position(|e| {
                            e.guid == *formset_guid && e.form_id == u64::from(form_id)
                        }) {
                            cands.push((b.pe_offset, b.pe_offset + STRIDE_ENTRY_SIZE * pos));
                        }
                    }
                    tracing::warn!(
                        blocks = blocks.len(),
                        candidates = cands.len(),
                        "tse unhide scan"
                    );
                    match cands.as_slice() {
                        [] => return Err(HiiError::NotFound),
                        [single] => *single,
                        many => {
                            return Err(HiiError::InvalidSchema(format!(
                                "ambiguous: entry in {} hide-candidate blocks at {:?}",
                                many.len(),
                                many.iter()
                                    .map(|(b, _)| format!("{b:#x}"))
                                    .collect::<Vec<_>>()
                            )));
                        }
                    }
                }
            };
        let node = crate::hii::node_at_mut(&mut image.root, &path);
        node.body[entry_pe_offset..entry_pe_offset + STRIDE_ENTRY_SIZE].fill(0);
        crate::ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
        Ok(UnhideOutcome {
            pe_offset: block_off,
            entry_pe_offset,
            formset_guid: *formset_guid,
            form_id,
        })
    })
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

    use crate::hii::HiiError;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode};

    const PE_GUID: &str = "B1DA0ADF-4F77-4070-A88E-BFFE1C60529A";

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
            parsing_data: crate::types::ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    /// PE-тело: [pad 0x40][hide: {A#1, B#10000}][term]
    /// [бар: {B#10001, A#1, B#2049}][term]. Скан даёт 3 блока
    /// (hide @0x40, бар @0xA0, перекрывающийся хвост @0xC0).
    fn tse_pe_body() -> Vec<u8> {
        let mut pe = vec![0x11u8; 0x40];
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        pe.extend(entry(B, 10001));
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 2049));
        pe.extend(term());
        pe
    }

    fn g_opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn g_form(id: u16) -> Vec<u8> {
        g_opcode(
            r_efi::hii::IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), 7u16.to_le_bytes()].concat(),
        )
    }

    fn g_end() -> Vec<u8> {
        vec![r_efi::hii::IFR_END_OP, 0x02]
    }

    /// Форм-пакет формсета `guid` с формами `forms`; закрывающий
    /// FORM_SET END обязателен (иначе formset_spans None — прецедент
    /// f7f6500). По образцу forms_pkg в hii/mod.rs:3552.
    fn forms_pkg_for(guid: &str, forms: &[u16]) -> Vec<u8> {
        let g = Guid::try_parse(guid).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&7u16.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0);
        let mut ifr = g_opcode(r_efi::hii::IFR_FORM_SET_OP, true, &p);
        for &f in forms {
            ifr.extend(g_form(f));
            ifr.extend(g_end());
        }
        ifr.extend(g_end());
        let total = 4 + ifr.len();
        let mut pkg = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            r_efi::hii::PACKAGE_FORMS,
        ];
        pkg.extend_from_slice(&ifr);
        pkg
    }

    /// Образ: Volume > File(B1DA0ADF) > [PE32 leaf, Section 0x19
    /// (forms_pkg_for(A, &[1,5])), Section 0x19 (forms_pkg_for(B,
    /// &[10000,10001]))]. Root-карта: A→1, B→10000. Форма 2049 —
    /// только в stride-баре PE, не в IFR формсета B (как на 450x:
    /// иначе min-правило дало бы корнем 2049, спека §2.2 п.3).
    fn root_image(pe: Vec<u8>, mode: ImageMode) -> Image {
        let pe32 = {
            let mut n = mk_node(FfsType::Section, pe, vec![]);
            n.subtype = crate::ffs::EFI_SECTION_PE32;
            n
        };
        let fs_a = {
            let mut n = mk_node(FfsType::Section, forms_pkg_for(A, &[1, 5]), vec![]);
            n.subtype = 0x19;
            n
        };
        let fs_b = {
            let mut n = mk_node(FfsType::Section, forms_pkg_for(B, &[10000, 10001]), vec![]);
            n.subtype = 0x19;
            n
        };
        let file = {
            let mut n = mk_node(FfsType::File, vec![], vec![pe32, fs_a, fs_b]);
            n.guid = Some(Guid::try_parse(PE_GUID).unwrap());
            n
        };
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root: mk_node(
                FfsType::Image,
                vec![],
                vec![mk_node(FfsType::Volume, vec![], vec![file])],
            ),
            mode,
        }
    }

    #[test]
    fn formset_roots_from_forms_packages() {
        let img = root_image(tse_pe_body(), ImageMode::Read);
        let roots = formset_roots(&img);
        assert_eq!(roots.get(&Guid::try_parse(A).unwrap()), Some(&1));
        assert_eq!(roots.get(&Guid::try_parse(B).unwrap()), Some(&10000));
    }

    #[test]
    fn tse_unhide_zeroes_hide_entry_and_marks_rebuild() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let out = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap();
        assert_eq!(
            out.pe_offset, 0x40,
            "hide-блок (бар отфильтрован root-правилом)"
        );
        assert_eq!(out.entry_pe_offset, 0x40);
        let file = &img.root.children[0].children[0];
        let pe32 = &file.children[0];
        assert!(
            pe32.body[0x40..0x60].iter().all(|&b| b == 0),
            "запись занулена"
        );
        assert_eq!(pe32.action, Action::Rebuild, "leaf помечен");
        assert_eq!(file.action, Action::Rebuild, "цепочка до файла");
    }

    #[test]
    fn tse_unhide_read_mode_refused() {
        let mut img = root_image(tse_pe_body(), ImageMode::Read);
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(matches!(err, HiiError::NotWritable));
    }

    #[test]
    fn tse_unhide_no_amitse_not_found() {
        let mut img = Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root: mk_node(FfsType::Image, vec![], vec![]),
            mode: ImageMode::Write,
        };
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn tse_unhide_visible_tab_not_found() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10001, None).unwrap_err();
        assert!(matches!(err, HiiError::NotFound), "{err:?}");
    }

    #[test]
    fn tse_unhide_explicit_offset_bypasses_candidates() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let out = tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10000, Some(0x60)).unwrap();
        assert_eq!(out.entry_pe_offset, 0x60);
        assert_eq!(
            out.pe_offset, 0x60,
            "экспертный режим: pe_offset == entry_pe_offset"
        );
    }

    #[test]
    fn tse_unhide_guard_mismatch_refused() {
        let mut img = root_image(tse_pe_body(), ImageMode::Write);
        let err =
            tse_unhide(&mut img, &Guid::try_parse(B).unwrap(), 10000, Some(0x80)).unwrap_err();
        assert!(matches!(err, HiiError::InvalidSchema(_)), "{err:?}");
        let file = &img.root.children[0].children[0];
        assert_eq!(
            file.children[0].action,
            Action::NoAction,
            "rollback: мутаций нет"
        );
    }

    #[test]
    fn tse_unhide_ambiguous_root_blocks_refused() {
        let mut pe = tse_pe_body();
        pe.extend(entry(A, 1));
        pe.extend(entry(B, 10000));
        pe.extend(term());
        let mut img = root_image(pe, ImageMode::Write);
        let err = tse_unhide(&mut img, &Guid::try_parse(A).unwrap(), 1, None).unwrap_err();
        assert!(
            matches!(err, HiiError::InvalidSchema(ref m) if m.contains("ambiguous")),
            "{err:?}"
        );
    }
}
