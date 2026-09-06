use crate::types::Guid;
use r_efi::hii::PACKAGE_END;

pub struct HiiPackage<'a> {
    pub kind: u8,
    pub bytes: &'a [u8],
}

pub struct HiiPackageList<'a> {
    pub guid: Guid,
    pub packages: Vec<HiiPackage<'a>>,
}

pub fn parse_package_list(bytes: &[u8]) -> Option<HiiPackageList<'_>> {
    if bytes.len() < 20 {
        tracing::debug!("HII package list shorter than 20-byte header");
        return None;
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes[0..16]);
    let guid = Guid::from_bytes(arr);
    let mut packages = Vec::new();
    let mut pos = 20usize;
    while pos + 4 <= bytes.len() {
        let plen =
            bytes[pos] as usize | (bytes[pos + 1] as usize) << 8 | (bytes[pos + 2] as usize) << 16;
        let kind = bytes[pos + 3];
        if kind == PACKAGE_END {
            return Some(HiiPackageList { guid, packages });
        }
        if plen < 4 || pos + plen > bytes.len() {
            tracing::warn!(
                pos,
                plen,
                kind,
                "malformed HII package; dropping whole list"
            );
            return None;
        }
        packages.push(HiiPackage {
            kind,
            bytes: &bytes[pos..pos + plen],
        });
        pos += plen;
    }
    tracing::warn!("HII package list without END terminator; dropping whole list");
    None
}

#[cfg(test)]
use r_efi::hii::{PACKAGE_FORMS, PACKAGE_STRINGS};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Guid;
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

    fn list(guid: &Guid, total: u32, pkgs: &[&[u8]]) -> Vec<u8> {
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&total.to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, PACKAGE_END]);
        b
    }

    #[test]
    fn parses_form_and_string_packages() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = pkg(PACKAGE_STRINGS, &[0x00; 8]);
        let bytes = list(&g, 0, &[&form, &string]);
        let parsed = parse_package_list(&bytes).unwrap();
        assert_eq!(parsed.guid, g);
        assert_eq!(parsed.packages.len(), 2);
        assert_eq!(parsed.packages[0].kind, PACKAGE_FORMS);
        assert_eq!(parsed.packages[0].bytes, &form[..]);
        assert_eq!(parsed.packages[1].kind, PACKAGE_STRINGS);
    }

    #[test]
    fn empty_list_with_only_end_terminator_is_valid() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let bytes = list(&g, 20, &[]);
        let parsed = parse_package_list(&bytes).unwrap();
        assert!(parsed.packages.is_empty());
    }

    #[test]
    fn total_is_a_hint_and_not_validated() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xBB]);
        let total = (20 + form.len() + 4) as u32 - 1;
        let bytes = list(&g, total, &[&form]);
        assert!(parse_package_list(&bytes).is_some());
    }

    #[test]
    fn truncated_mid_package_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let form = pkg(
            PACKAGE_FORMS,
            &[0x0Eu8, 0x17, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x22],
        );
        let mut bytes = list(&g, 0, &[&form]);
        bytes.truncate(bytes.len() - 10);
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn package_length_below_header_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let mut bytes = g.to_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[0x02, 0x00, 0x00, PACKAGE_FORMS]);
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn missing_end_terminator_returns_none() {
        let g = Guid::from_str(LIST_GUID).unwrap();
        let mut bytes = g.to_bytes().to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xDD]));
        assert!(parse_package_list(&bytes).is_none());
    }

    #[test]
    fn short_input_returns_none() {
        assert!(parse_package_list(&[0u8; 19]).is_none());
    }

    #[test]
    fn parses_real_rk3588_string_resource() {
        let bytes = include_bytes!("../../tests/fixtures/hii_rk3588_string_res.bin");
        let parsed = parse_package_list(bytes).unwrap();
        assert_eq!(
            parsed.guid,
            Guid::from_str("A487A478-51EF-48AA-8794-7BEE2A0562F1").unwrap()
        );
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].kind, PACKAGE_STRINGS);
        let len = parsed.packages[0].bytes[0] as usize
            | (parsed.packages[0].bytes[1] as usize) << 8
            | (parsed.packages[0].bytes[2] as usize) << 16;
        assert_eq!(len, 6852);
    }
}
