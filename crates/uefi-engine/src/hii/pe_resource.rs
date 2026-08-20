use object::endian::LittleEndian;
use object::read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64, ResourceDirectoryEntryData};
use r_efi::hii::{IFR_FORM_SET_OP, PACKAGE_END, PACKAGE_FORMS};

use crate::hii::ifr::parse_form_package;
use crate::hii::package_list::parse_package_list;

pub fn hii_resource_blobs(pe: &[u8]) -> Vec<&[u8]> {
    hii_resource_ranges(pe)
        .into_iter()
        .filter_map(|(off, len)| pe.get(off..off + len))
        .collect()
}

pub fn hii_resource_ranges(pe: &[u8]) -> Vec<(usize, usize)> {
    hii_entry_locations(pe)
        .into_iter()
        .map(|(_, off, len)| (off, len))
        .collect()
}

pub(crate) fn hii_entry_locations(pe: &[u8]) -> Vec<(usize, usize, usize)> {
    if let Ok(file) = PeFile64::parse(pe) {
        return hii_entries(&file);
    }
    if let Ok(file) = PeFile32::parse(pe) {
        return hii_entries(&file);
    }
    tracing::debug!("not a PE image; no HII resources");
    Vec::new()
}

fn hii_entries<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Vec<(usize, usize, usize)> {
    let sections = file.section_table();
    let Ok(Some(rsrc)) = file
        .data_directories()
        .resource_directory(file.data(), &sections)
    else {
        tracing::debug!("no .rsrc directory; no HII resources");
        return Vec::new();
    };
    let Ok(root) = rsrc.root() else {
        tracing::debug!("invalid .rsrc root; no HII resources");
        return Vec::new();
    };
    let Some(dir_off) = file
        .data_directories()
        .get(object::pe::IMAGE_DIRECTORY_ENTRY_RESOURCE)
        .and_then(|dir| sections.pe_file_range_at(dir.virtual_address.get(LittleEndian)))
        .map(|(off, _)| off as usize)
    else {
        tracing::debug!("resource directory not file-mapped; no HII resources");
        return Vec::new();
    };
    let mut out = Vec::new();
    for type_entry in root.entries {
        let Some(name) = type_entry.name_or_id().name() else {
            continue;
        };
        let Ok(type_name) = name.to_string_lossy(rsrc) else {
            continue;
        };
        if type_name != "HII" {
            continue;
        }
        let Ok(ResourceDirectoryEntryData::Table(name_table)) = type_entry.data(rsrc) else {
            continue;
        };
        for name_entry in name_table.entries {
            let Ok(ResourceDirectoryEntryData::Table(lang_table)) = name_entry.data(rsrc) else {
                continue;
            };
            for lang_entry in lang_table.entries {
                let Ok(ResourceDirectoryEntryData::Data(d)) = lang_entry.data(rsrc) else {
                    continue;
                };
                push_leaf(
                    file,
                    dir_off,
                    lang_entry.data_offset() as usize,
                    d.offset_to_data.get(LittleEndian),
                    d.size.get(LittleEndian),
                    &mut out,
                );
            }
        }
    }
    out
}

fn push_leaf<Pe: ImageNtHeaders>(
    file: &PeFile<'_, Pe>,
    dir_off: usize,
    entry_rel: usize,
    rva: u32,
    size: u32,
    out: &mut Vec<(usize, usize, usize)>,
) {
    let Some((off, avail)) = file.section_table().pe_file_range_at(rva) else {
        tracing::debug!(rva, "resource RVA outside sections; skipped");
        return;
    };
    let Some(entry_off) = dir_off.checked_add(entry_rel) else {
        return;
    };
    let len = (size as usize).min(avail as usize);
    out.push((entry_off, off as usize, len));
}

pub fn bare_form_packages<'a>(pe: &'a [u8], exclude: &[(usize, usize)]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 5 <= pe.len() {
        let plen = pe[pos] as usize | (pe[pos + 1] as usize) << 8 | (pe[pos + 2] as usize) << 16;
        if pe[pos + 3] == PACKAGE_FORMS
            && pe[pos + 4] == IFR_FORM_SET_OP
            && plen >= 24
            && pos + plen <= pe.len()
        {
            let covered = exclude.iter().any(|&(o, l)| o <= pos && pos < o + l);
            if !covered && parse_form_package(&pe[pos..pos + plen]).is_some() {
                out.push(&pe[pos..pos + plen]);
            }
            pos += plen;
        } else {
            pos += 1;
        }
    }
    out
}

struct RsrcGrowPlan {
    section_off: usize,
    size_of_image_off: usize,
    old_virt: u32,
    old_raw: u32,
    pad: u32,
    new_image: Option<u32>,
}

impl RsrcGrowPlan {
    fn apply(self, pe: &mut Vec<u8>) {
        pe.resize(pe.len() + self.pad as usize, 0);
        write_le_u32(pe, self.section_off + 8, self.old_virt + self.pad);
        write_le_u32(pe, self.section_off + 16, self.old_raw + self.pad);
        if let Some(image) = self.new_image {
            write_le_u32(pe, self.size_of_image_off, image);
        }
    }
}

pub fn try_grow_rsrc_tail(pe: &mut Vec<u8>, delta: usize) -> bool {
    if delta == 0 {
        return true;
    }
    let data: &[u8] = pe.as_slice();
    let plan = match PeFile64::parse(data) {
        Ok(file) => rsrc_grow_plan(&file, data, delta),
        Err(_) => match PeFile32::parse(data) {
            Ok(file) => rsrc_grow_plan(&file, data, delta),
            Err(_) => {
                tracing::debug!("not a PE image; refusing rsrc tail growth");
                None
            }
        },
    };
    match plan {
        Some(plan) => {
            plan.apply(pe);
            true
        }
        None => false,
    }
}

fn rsrc_grow_plan<Pe: ImageNtHeaders>(
    file: &PeFile<'_, Pe>,
    pe: &[u8],
    delta: usize,
) -> Option<RsrcGrowPlan> {
    let pe_off = read_le_u32(pe, 0x3c)? as usize;
    let opt_off = pe_off.checked_add(24)?;
    let opt_size = read_le_u16(pe, pe_off.checked_add(20)?)? as usize;
    let table_off = opt_off.checked_add(opt_size)?;
    let rsrc_dir = file
        .data_directories()
        .get(object::pe::IMAGE_DIRECTORY_ENTRY_RESOURCE)?;
    let (rsrc_rva, _) = rsrc_dir.address_range();
    if rsrc_rva == 0 {
        tracing::debug!("no resource data directory; refusing rsrc tail growth");
        return None;
    }
    let mut rsrc = None;
    for (idx, section) in file.section_table().iter().enumerate() {
        let va = section.virtual_address.get(LittleEndian);
        let span = section
            .virtual_size
            .get(LittleEndian)
            .max(section.size_of_raw_data.get(LittleEndian));
        if let Some(off) = rsrc_rva.checked_sub(va)
            && off < span
        {
            rsrc = Some((idx, va));
            break;
        }
    }
    let (rsrc_idx, rsrc_va) = rsrc?;
    let rsrc_header = file.section_table().iter().nth(rsrc_idx)?;
    let raw_ptr = rsrc_header.pointer_to_raw_data.get(LittleEndian) as u64;
    let raw_size = rsrc_header.size_of_raw_data.get(LittleEndian) as u64;
    let virt_size = rsrc_header.virtual_size.get(LittleEndian) as u64;
    let rsrc_end = raw_ptr.checked_add(raw_size)?;
    if pe.len() as u64 != rsrc_end {
        tracing::debug!("rsrc raw data does not end at file end; refusing tail growth");
        return None;
    }
    let mut last_by_rva = true;
    for section in file.section_table().iter() {
        if section.virtual_address.get(LittleEndian) > rsrc_va {
            last_by_rva = false;
        }
        let size = section.size_of_raw_data.get(LittleEndian) as u64;
        if size == 0 {
            continue;
        }
        let end = section.pointer_to_raw_data.get(LittleEndian) as u64 + size;
        if end > rsrc_end {
            tracing::debug!("section raw range extends beyond rsrc; refusing tail growth");
            return None;
        }
    }
    let file_align = pow2_or_one(read_le_u32(pe, opt_off.checked_add(36)?)?);
    let section_align_raw = read_le_u32(pe, opt_off.checked_add(32)?)?;
    let section_align = if section_align_raw.is_power_of_two() {
        section_align_raw
    } else {
        file_align
    };
    let pad = (delta as u64)
        .div_ceil(u64::from(file_align))
        .checked_mul(u64::from(file_align))?;
    let new_raw = raw_size.checked_add(pad)?;
    let new_virt = virt_size.checked_add(pad)?;
    if new_raw > u32::MAX as u64 || new_virt > u32::MAX as u64 {
        return None;
    }
    let new_image = if last_by_rva {
        let end = u64::from(rsrc_va)
            .checked_add(new_virt)?
            .div_ceil(u64::from(section_align))
            .checked_mul(u64::from(section_align))?;
        if end > u32::MAX as u64 {
            return None;
        }
        Some(end as u32)
    } else {
        None
    };
    Some(RsrcGrowPlan {
        section_off: table_off + rsrc_idx * 40,
        size_of_image_off: opt_off + 56,
        old_virt: virt_size as u32,
        old_raw: raw_size as u32,
        pad: pad as u32,
        new_image,
    })
}

fn pow2_or_one(value: u32) -> u32 {
    if value.is_power_of_two() { value } else { 1 }
}

fn read_le_u32(pe: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        pe.get(off..off.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn read_le_u16(pe: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        pe.get(off..off.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn write_le_u32(pe: &mut [u8], off: usize, value: u32) {
    pe[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn write_length_chain(
    pe: &mut [u8],
    entry_off: usize,
    blob_off: usize,
    entry_size: u32,
    total: u32,
) {
    write_le_u32(pe, entry_off + 4, entry_size);
    write_le_u32(pe, blob_off + 16, total);
}

pub fn append_package_to_resource(pe: &mut Vec<u8>, pkg_bytes: &[u8]) -> bool {
    let Some(plan) = append_plan(pe.as_slice(), pkg_bytes) else {
        return false;
    };
    plan.apply(pe, pkg_bytes)
}

fn append_plan(pe: &[u8], pkg_bytes: &[u8]) -> Option<AppendPlan> {
    let pkg_len = package_len(pkg_bytes)?;
    if let Ok(file) = PeFile64::parse(pe) {
        return append_plan_for(&file, pe, pkg_len);
    }
    if let Ok(file) = PeFile32::parse(pe) {
        return append_plan_for(&file, pe, pkg_len);
    }
    tracing::debug!("not a PE image; refusing package append");
    None
}

fn package_len(pkg_bytes: &[u8]) -> Option<usize> {
    let pkg_len = pkg_bytes.len();
    if pkg_len < 4 {
        tracing::debug!("appended package shorter than its 4-byte header");
        return None;
    }
    let declared =
        pkg_bytes[0] as usize | (pkg_bytes[1] as usize) << 8 | (pkg_bytes[2] as usize) << 16;
    if declared != pkg_len {
        tracing::debug!(declared, pkg_len, "appended package length header mismatch");
        return None;
    }
    if pkg_bytes[3] == PACKAGE_END {
        tracing::debug!("refusing to append END package into list");
        return None;
    }
    Some(pkg_len)
}

struct AppendPlan {
    insert_off: usize,
    entry_off: usize,
    blob_off: usize,
    new_entry_size: u32,
    new_total: u32,
    grow: usize,
}

impl AppendPlan {
    fn apply(self, pe: &mut Vec<u8>, pkg_bytes: &[u8]) -> bool {
        if self.grow > 0 && !try_grow_rsrc_tail(pe, self.grow) {
            return false;
        }
        let pkg_len = pkg_bytes.len();
        let tail_off = pe.len() - pkg_len;
        pe.copy_within(self.insert_off..tail_off, self.insert_off + pkg_len);
        pe[self.insert_off..self.insert_off + pkg_len].copy_from_slice(pkg_bytes);
        write_length_chain(
            pe,
            self.entry_off,
            self.blob_off,
            self.new_entry_size,
            self.new_total,
        );
        true
    }
}

fn append_plan_for<Pe: ImageNtHeaders>(
    file: &PeFile<'_, Pe>,
    pe: &[u8],
    pkg_len: usize,
) -> Option<AppendPlan> {
    let (entry_off, blob_off, blob_len) = *hii_entries(file).first()?;
    let blob = pe.get(blob_off..blob_off.checked_add(blob_len)?)?;
    let packages = parse_package_list(blob)?.packages;
    let sum = packages.iter().map(|p| p.bytes.len()).sum::<usize>();
    let insert_off = blob_off.checked_add(20)?.checked_add(sum)?;
    let new_total = 20u64 + sum as u64 + pkg_len as u64 + 4;
    let old_entry_size = u64::from(read_le_u32(pe, entry_off.checked_add(4)?)?);
    let new_entry_size = old_entry_size.checked_add(pkg_len as u64)?;
    if new_total > u32::MAX as u64 || new_entry_size > u32::MAX as u64 {
        tracing::debug!("package list length overflow; refusing append");
        return None;
    }
    let blob_end = blob_off.checked_add(blob_len)?;
    let raw_end = usize::try_from(rsrc_raw_end(file)?).ok()?;
    let grow = blob_end.checked_add(pkg_len)?.saturating_sub(raw_end);
    Some(AppendPlan {
        insert_off,
        entry_off,
        blob_off,
        new_entry_size: new_entry_size as u32,
        new_total: new_total as u32,
        grow,
    })
}

fn rsrc_raw_end<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Option<u64> {
    let rsrc_dir = file
        .data_directories()
        .get(object::pe::IMAGE_DIRECTORY_ENTRY_RESOURCE)?;
    let (rsrc_rva, _) = rsrc_dir.address_range();
    if rsrc_rva == 0 {
        return None;
    }
    for section in file.section_table().iter() {
        let va = section.virtual_address.get(LittleEndian);
        let span = section
            .virtual_size
            .get(LittleEndian)
            .max(section.size_of_raw_data.get(LittleEndian));
        if let Some(off) = rsrc_rva.checked_sub(va)
            && off < span
        {
            return u64::from(section.pointer_to_raw_data.get(LittleEndian))
                .checked_add(u64::from(section.size_of_raw_data.get(LittleEndian)));
        }
    }
    None
}

#[cfg(test)]
fn rsrc_dir_header(named_entries: u16, id_entries: u16) -> [u8; 16] {
    let mut header = [0u8; 16];
    header[12..14].copy_from_slice(&named_entries.to_le_bytes());
    header[14..16].copy_from_slice(&id_entries.to_le_bytes());
    header
}

#[cfg(test)]
pub(crate) fn synth_hii_pe(type_name: &str, blob: &[u8]) -> Vec<u8> {
    let rsrc_rva: u32 = 0x1000;
    let mut rsrc = Vec::new();
    rsrc.extend_from_slice(&rsrc_dir_header(1, 0));
    rsrc.extend_from_slice(&0x8000_0048u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0018u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&1u32.to_le_bytes());
    rsrc.extend_from_slice(&0x8000_0030u32.to_le_bytes());
    rsrc.extend_from_slice(&rsrc_dir_header(0, 1));
    rsrc.extend_from_slice(&0x409u32.to_le_bytes());
    rsrc.extend_from_slice(&0x80u32.to_le_bytes());
    rsrc.extend_from_slice(&(type_name.len() as u16).to_le_bytes());
    for u in type_name.encode_utf16() {
        rsrc.extend_from_slice(&u.to_le_bytes());
    }
    while rsrc.len() < 0x80 {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(&(rsrc_rva + 0x90).to_le_bytes());
    rsrc.extend_from_slice(&(blob.len() as u32).to_le_bytes());
    rsrc.extend_from_slice(&0u32.to_le_bytes());
    rsrc.extend_from_slice(&0u32.to_le_bytes());
    while rsrc.len() < 0x90 {
        rsrc.push(0);
    }
    rsrc.extend_from_slice(blob);
    while rsrc.len() % 16 != 0 {
        rsrc.push(0);
    }

    let size_of_image = (rsrc_rva + rsrc.len() as u32).next_multiple_of(0x1000);
    let mut pe = vec![0u8; 0x170];
    pe[0] = b'M';
    pe[1] = b'Z';
    pe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    pe[0x40..0x44].copy_from_slice(b"PE\0\0");
    pe[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
    pe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    pe[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    pe[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
    pe[0x78..0x7c].copy_from_slice(&0x1000u32.to_le_bytes());
    pe[0x7c..0x80].copy_from_slice(&16u32.to_le_bytes());
    pe[0x90..0x94].copy_from_slice(&size_of_image.to_le_bytes());
    pe[0xc4..0xc8].copy_from_slice(&16u32.to_le_bytes());
    pe[0xd8..0xdc].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0xdc..0xe0].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x148..0x14f].copy_from_slice(b".rsrc\0\0");
    pe[0x150..0x154].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x154..0x158].copy_from_slice(&rsrc_rva.to_le_bytes());
    pe[0x158..0x15c].copy_from_slice(&(rsrc.len() as u32).to_le_bytes());
    pe[0x15c..0x160].copy_from_slice(&0x170u32.to_le_bytes());
    pe[0x16c..0x170].copy_from_slice(&0x4000_0040u32.to_le_bytes());
    pe.extend_from_slice(&rsrc);
    pe
}

#[cfg(test)]
mod tests {
    use super::*;

    const RK3588_STRING_RES: &[u8] =
        include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");

    const RK3588_BARE_FORM: &[u8] = include_bytes!("../../tests/fixtures/hii_rk3588_bare_form.bin");

    #[test]
    fn extracts_hii_blob_from_synthetic_pe() {
        let pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let blobs = hii_resource_blobs(&pe);
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0], RK3588_STRING_RES);
    }

    #[test]
    fn ranges_point_at_blob_bytes() {
        let pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let ranges = hii_resource_ranges(&pe);
        assert_eq!(ranges.len(), 1);
        let (off, len) = ranges[0];
        assert_eq!(&pe[off..off + len], RK3588_STRING_RES);
    }

    #[test]
    fn non_pe_input_returns_empty() {
        assert!(hii_resource_blobs(&[]).is_empty());
        assert!(hii_resource_blobs(b"MZnotape").is_empty());
        assert!(hii_resource_blobs(&[0u8; 64]).is_empty());
    }

    #[test]
    fn pe_without_hii_type_returns_empty() {
        let pe = synth_hii_pe("REGISTRY", RK3588_STRING_RES);
        assert!(hii_resource_blobs(&pe).is_empty());
        assert!(hii_resource_ranges(&pe).is_empty());
    }

    #[test]
    fn pe_without_resource_directory_returns_empty() {
        let mut pe = synth_hii_pe("HII", RK3588_STRING_RES);
        pe[0xd8..0xdc].copy_from_slice(&0u32.to_le_bytes());
        assert!(hii_resource_blobs(&pe).is_empty());
    }

    #[test]
    fn bare_scan_finds_valid_form_package_in_pe_body() {
        let mut body = vec![0x11u8; 64];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(&[0x22; 32]);
        let found = bare_form_packages(&body, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], RK3588_BARE_FORM);
    }

    #[test]
    fn bare_scan_rejects_shallow_false_positive() {
        let mut body = vec![0u8; 16];
        body.extend_from_slice(&[27u8, 0, 0, 0x02, 0x0E]);
        body.extend_from_slice(&[0u8; 22]);
        assert!(bare_form_packages(&body, &[]).is_empty());
    }

    #[test]
    fn bare_scan_drops_candidates_covered_by_resource_ranges() {
        let pe = synth_hii_pe("HII", RK3588_BARE_FORM);
        let exclude = hii_resource_ranges(&pe);
        assert!(!exclude.is_empty());
        let found = bare_form_packages(&pe, &exclude);
        assert!(
            !found.contains(&(RK3588_BARE_FORM as &[u8])),
            "resource-covered copy must be deduplicated"
        );
    }

    #[test]
    fn bare_scan_skips_past_accepted_or_covered_pattern() {
        let mut body = vec![0u8; 8];
        body.extend_from_slice(RK3588_BARE_FORM);
        body.extend_from_slice(&[0x33; 4]);
        assert_eq!(bare_form_packages(&body, &[(0, body.len())]).len(), 0);
        assert_eq!(bare_form_packages(&body, &[]).len(), 1);
    }

    fn le_u32(b: &[u8], off: usize) -> u32 {
        u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
    }

    #[test]
    fn grow_rsrc_tail_updates_headers_and_keeps_ranges() {
        let mut pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let before = pe.clone();
        let ranges_before = hii_resource_ranges(&pe);
        assert_eq!(ranges_before.len(), 1);
        let file_align = le_u32(&pe, 0x7c) as usize;
        let old_virt = le_u32(&pe, 0x150);
        let old_raw = le_u32(&pe, 0x158);
        assert!(try_grow_rsrc_tail(&mut pe, 64));
        let growth = 64usize.next_multiple_of(file_align);
        assert_eq!(pe.len(), before.len() + growth);
        assert_eq!(le_u32(&pe, 0x150), old_virt + growth as u32);
        assert_eq!(le_u32(&pe, 0x158), old_raw + growth as u32);
        assert_eq!(le_u32(&pe, 0x15c), 0x170);
        assert_eq!(
            le_u32(&pe, 0x90),
            (0x1000 + old_virt + growth as u32).next_multiple_of(0x1000)
        );
        assert_eq!(pe[0xd8..0xe0], before[0xd8..0xe0]);
        assert!(pe[before.len()..].iter().all(|&b| b == 0));
        let (blob_off, blob_len) = ranges_before[0];
        assert_eq!(&pe[blob_off..blob_off + blob_len], RK3588_STRING_RES);
        assert_eq!(hii_resource_ranges(&pe), ranges_before);
    }

    #[test]
    fn grow_rsrc_tail_refuses_non_pe_input() {
        let mut empty: Vec<u8> = Vec::new();
        assert!(!try_grow_rsrc_tail(&mut empty, 64));
        assert!(empty.is_empty());
        let mut junk = b"MZnotape".to_vec();
        assert!(!try_grow_rsrc_tail(&mut junk, 64));
        assert_eq!(junk, b"MZnotape");
    }

    #[test]
    fn grow_rsrc_tail_refuses_when_rsrc_not_last_section() {
        let base = synth_hii_pe("HII", RK3588_STRING_RES);
        let raw_size = le_u32(&base, 0x158) as usize;
        let mut pe = Vec::with_capacity(base.len() + 40 + 0x40);
        pe.extend_from_slice(&base[..0x170]);
        pe.extend_from_slice(&[0u8; 40]);
        pe.extend_from_slice(&base[0x170..]);
        pe.extend_from_slice(&[0u8; 0x40]);
        pe[0x46..0x48].copy_from_slice(&2u16.to_le_bytes());
        pe[0x15c..0x160].copy_from_slice(&0x198u32.to_le_bytes());
        pe[0x170..0x174].copy_from_slice(b".dmy");
        pe[0x178..0x17c].copy_from_slice(&0x40u32.to_le_bytes());
        pe[0x17c..0x180].copy_from_slice(&0x2000u32.to_le_bytes());
        pe[0x180..0x184].copy_from_slice(&0x40u32.to_le_bytes());
        pe[0x184..0x188].copy_from_slice(&(0x198u32 + raw_size as u32).to_le_bytes());
        assert_eq!(hii_resource_ranges(&pe).len(), 1);
        let snapshot = pe.clone();
        assert!(!try_grow_rsrc_tail(&mut pe, 64));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn grow_rsrc_tail_zero_delta_is_noop() {
        let mut pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let before = pe.clone();
        assert!(try_grow_rsrc_tail(&mut pe, 0));
        assert_eq!(pe, before);
    }

    #[test]
    fn grow_rsrc_tail_accumulates_across_calls() {
        let mut pe = synth_hii_pe("HII", RK3588_STRING_RES);
        let before = pe.clone();
        let ranges_before = hii_resource_ranges(&pe);
        let file_align = le_u32(&pe, 0x7c) as usize;
        let old_raw = le_u32(&pe, 0x158);
        assert!(try_grow_rsrc_tail(&mut pe, 20));
        assert!(try_grow_rsrc_tail(&mut pe, 30));
        let growth = 20usize.next_multiple_of(file_align) + 30usize.next_multiple_of(file_align);
        assert_eq!(pe.len(), before.len() + growth);
        assert_eq!(le_u32(&pe, 0x158), old_raw + growth as u32);
        assert_eq!(hii_resource_ranges(&pe), ranges_before);
    }

    use crate::hii::package_list::parse_package_list;
    use crate::types::Guid;
    use r_efi::hii::{PACKAGE_END, PACKAGE_STRINGS};
    use std::str::FromStr;

    const LIST_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";

    fn pkg(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            kind,
        ];
        b.extend_from_slice(payload);
        b
    }

    fn list(guid: &Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, PACKAGE_END]);
        b
    }

    #[test]
    fn appends_form_package_to_hii_resource_list() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = pkg(PACKAGE_STRINGS, &[0u8; 8]);
        let form2 = pkg(
            PACKAGE_FORMS,
            &[
                0x0Eu8, 0x17, 0xBB, 0xCC, 0xDD, 0xEE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            ],
        );
        let blob = list(&g, &[&form, &string]);
        let mut pe = synth_hii_pe("HII", &blob);
        let len_before = pe.len();
        let file_align = le_u32(&pe, 0x7c) as usize;
        let (entry_off, blob_off, blob_len) = hii_entry_locations(&pe)[0];
        assert_eq!(le_u32(&pe, entry_off + 4), blob_len as u32);
        assert!(append_package_to_resource(&mut pe, &form2));
        assert_eq!(
            hii_resource_ranges(&pe),
            [(blob_off, blob_len + form2.len())]
        );
        let new_blob = &pe[blob_off..blob_off + blob_len + form2.len()];
        let parsed = parse_package_list(new_blob).unwrap();
        assert_eq!(
            parsed.packages.iter().map(|p| p.kind).collect::<Vec<_>>(),
            [PACKAGE_FORMS, PACKAGE_STRINGS, PACKAGE_FORMS]
        );
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        assert_eq!(parsed.packages[1].bytes, &string[..]);
        assert_eq!(parsed.packages[2].bytes, &form2[..]);
        assert_eq!(
            le_u32(new_blob, 16),
            (20 + form.len() + string.len() + form2.len() + 4) as u32
        );
        assert_eq!(le_u32(&pe, entry_off + 4), (blob_len + form2.len()) as u32);
        assert_eq!(
            &new_blob[new_blob.len() - 4..],
            &[0x04, 0x00, 0x00, PACKAGE_END]
        );
        assert_eq!(
            pe.len() - len_before,
            form2.len().next_multiple_of(file_align)
        );
        assert_eq!(pe.len(), (le_u32(&pe, 0x15c) + le_u32(&pe, 0x158)) as usize);
    }

    #[test]
    fn append_without_hii_resource_returns_false_untouched() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let blob = list(&g, &[&form]);
        let mut pe = synth_hii_pe("REGISTRY", &blob);
        let snapshot = pe.clone();
        assert!(!append_package_to_resource(&mut pe, &form));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn append_to_list_without_end_returns_false_untouched() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xCC]);
        let mut malformed = g.to_bytes().to_vec();
        malformed.extend_from_slice(&24u32.to_le_bytes());
        malformed.extend_from_slice(&form);
        let mut pe = synth_hii_pe("HII", &malformed);
        let snapshot = pe.clone();
        assert!(!append_package_to_resource(&mut pe, &form));
        assert_eq!(pe, snapshot);
    }

    #[test]
    fn append_keeps_old_package_bytes_in_place() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = pkg(PACKAGE_STRINGS, &[0x77u8; 8]);
        let form2 = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xBB]);
        let blob = list(&g, &[&form, &string]);
        let mut pe = synth_hii_pe("HII", &blob);
        let (_, blob_off, _) = hii_entry_locations(&pe)[0];
        assert!(append_package_to_resource(&mut pe, &form2));
        let new_blob = &pe[blob_off..];
        assert_eq!(&new_blob[..16], g.to_bytes().as_slice());
        assert_eq!(&new_blob[20..20 + form.len()], &form[..]);
        assert_eq!(
            &new_blob[20 + form.len()..20 + form.len() + string.len()],
            &string[..]
        );
    }

    #[test]
    fn append_consumes_slack_without_growing_file() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = pkg(PACKAGE_STRINGS, &[0u8; 8]);
        let form2 = pkg(
            PACKAGE_FORMS,
            &[
                0x0Eu8, 0x17, 0xBB, 0xCC, 0xDD, 0xEE, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            ],
        );
        let blob = list(&g, &[&form, &string]);
        let mut pe = synth_hii_pe("HII", &blob);
        assert!(try_grow_rsrc_tail(&mut pe, 1));
        let len_after_pregrow = pe.len();
        let (entry_off, blob_off, blob_len) = hii_entry_locations(&pe)[0];
        assert!(append_package_to_resource(&mut pe, &form2));
        assert_eq!(pe.len(), len_after_pregrow);
        assert_eq!(le_u32(&pe, entry_off + 4), (blob_len + form2.len()) as u32);
        let new_blob = &pe[blob_off..blob_off + blob_len + form2.len()];
        let parsed = parse_package_list(new_blob).unwrap();
        assert_eq!(parsed.packages.len(), 3);
        assert_eq!(
            le_u32(new_blob, 16),
            (20 + form.len() + string.len() + form2.len() + 4) as u32
        );
        assert_eq!(pe.len(), (le_u32(&pe, 0x15c) + le_u32(&pe, 0x158)) as usize);
    }

    #[test]
    fn append_rejects_malformed_package_bytes() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let blob = list(&g, &[&form]);
        let mut pe = synth_hii_pe("HII", &blob);
        let snapshot = pe.clone();
        let short = [0x02u8, 0x00, 0x00, PACKAGE_FORMS];
        assert!(!append_package_to_resource(&mut pe, &short));
        assert_eq!(pe, snapshot);
        let bad_len = [0x09u8, 0x00, 0x00, PACKAGE_FORMS, 0x0E, 0x17];
        assert!(!append_package_to_resource(&mut pe, &bad_len));
        assert_eq!(pe, snapshot);
        let end_pkg = [0x04u8, 0x00, 0x00, PACKAGE_END];
        assert!(!append_package_to_resource(&mut pe, &end_pkg));
        assert_eq!(pe, snapshot);
    }
}
