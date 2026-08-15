use object::endian::LittleEndian;
use object::read::pe::{ImageNtHeaders, PeFile, PeFile32, PeFile64, ResourceDirectoryEntryData};
use r_efi::hii::{IFR_FORM_SET_OP, PACKAGE_FORMS};

use crate::hii::ifr::parse_form_package;

pub fn hii_resource_blobs(pe: &[u8]) -> Vec<&[u8]> {
    hii_resource_ranges(pe)
        .into_iter()
        .filter_map(|(off, len)| pe.get(off..off + len))
        .collect()
}

pub fn hii_resource_ranges(pe: &[u8]) -> Vec<(usize, usize)> {
    if let Ok(file) = PeFile64::parse(pe) {
        return hii_entries(&file);
    }
    if let Ok(file) = PeFile32::parse(pe) {
        return hii_entries(&file);
    }
    tracing::debug!("not a PE image; no HII resources");
    Vec::new()
}

fn hii_entries<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Vec<(usize, usize)> {
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
    rva: u32,
    size: u32,
    out: &mut Vec<(usize, usize)>,
) {
    let Some((off, avail)) = file.section_table().pe_file_range_at(rva) else {
        tracing::debug!(rva, "resource RVA outside sections; skipped");
        return;
    };
    let len = (size as usize).min(avail as usize);
    out.push((off as usize, len));
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

    let mut pe = vec![0u8; 0x170];
    pe[0] = b'M';
    pe[1] = b'Z';
    pe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    pe[0x40..0x44].copy_from_slice(b"PE\0\0");
    pe[0x44..0x46].copy_from_slice(&0x8664u16.to_le_bytes());
    pe[0x46..0x48].copy_from_slice(&1u16.to_le_bytes());
    pe[0x54..0x56].copy_from_slice(&240u16.to_le_bytes());
    pe[0x58..0x5a].copy_from_slice(&0x20bu16.to_le_bytes());
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
            !found.iter().any(|p| *p == RK3588_BARE_FORM),
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
}
