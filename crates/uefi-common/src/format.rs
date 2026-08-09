use crate::names::{file_type_name_or_raw, node_type_name, section_type_name_or_raw};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    pub path: String,
    pub type_: u32,
    pub subtype: u8,
    pub guid: String,
    pub offset: u64,
    pub size: u64,
    pub name: String,
}

fn depth(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.matches('/').count() + 1
    }
}

fn subtype_name_for(type_: u32, subtype: u8) -> String {
    match type_ {
        66 => file_type_name_or_raw(subtype),
        67 => section_type_name_or_raw(subtype),
        _ => String::new(),
    }
}

pub fn format_tree(rows: &[TreeRow]) -> String {
    let mut out = String::new();
    for r in rows {
        let indent = "  ".repeat(depth(&r.path));
        let node_label = node_type_name(r.type_);
        let sub = subtype_name_for(r.type_, r.subtype);
        let sub_part = if sub.is_empty() {
            String::new()
        } else {
            format!("({sub})")
        };
        let name_part = if r.name.is_empty() {
            String::new()
        } else {
            format!(" name={}", r.name)
        };
        out.push_str(&format!(
            "{indent}{}{sub_part} subtype={:02X} guid={} off={} size={}{name_part}\n",
            node_label, r.subtype, r.guid, r.offset, r.size,
        ));
    }
    out
}

pub fn format_legend(rows: &[TreeRow]) -> String {
    let mut file_codes: BTreeMap<u8, ()> = BTreeMap::new();
    let mut section_codes: BTreeMap<u8, ()> = BTreeMap::new();
    for r in rows {
        if r.type_ == 66 {
            file_codes.insert(r.subtype, ());
        } else if r.type_ == 67 {
            section_codes.insert(r.subtype, ());
        }
    }
    if file_codes.is_empty() && section_codes.is_empty() {
        return String::new();
    }
    let mut out = String::from("Legend:\n");
    if !file_codes.is_empty() {
        out.push_str("  File types:\n");
        for code in file_codes.keys() {
            out.push_str(&format!(
                "    {:02X} = {}\n",
                code,
                file_type_name_or_raw(*code)
            ));
        }
    }
    if !section_codes.is_empty() {
        out.push_str("  Section types:\n");
        for code in section_codes.keys() {
            out.push_str(&format!(
                "    {:02X} = {}\n",
                code,
                section_type_name_or_raw(*code)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, type_: u32, subtype: u8, name: &str) -> TreeRow {
        TreeRow {
            path: path.into(),
            type_,
            subtype,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: name.into(),
        }
    }

    #[test]
    fn format_tree_renders_named_ffs_and_ui_section() {
        let rows = vec![
            row("", 62, 0, ""),
            row("0", 65, 0, ""),
            row("0/0", 66, 0x07, "Setup"),
            row("0/0/0", 67, 0x15, ""),
        ];
        let out = format_tree(&rows);
        assert!(out.contains("Image subtype=00"));
        assert!(out.contains("  Volume subtype=00"));
        assert!(out.contains("    File(DXE driver) subtype=07 guid= off=0 size=0 name=Setup"));
        assert!(out.contains("      Section(UI) subtype=15"));
    }

    #[test]
    fn format_tree_skips_subtype_name_for_non_file_section() {
        let rows = vec![row("0", 65, 0x42, "")];
        let out = format_tree(&rows);
        assert!(out.contains("Volume subtype=42"));
        assert!(!out.contains("Volume("));
    }

    #[test]
    fn format_legend_groups_codes_by_kind() {
        let rows = vec![
            row("0/0", 66, 0x07, ""),
            row("0/0/0", 67, 0x10, ""),
            row("0/0/1", 67, 0x15, ""),
            row("0/1", 66, 0xF0, ""),
        ];
        let leg = format_legend(&rows);
        assert!(leg.contains("File types:"));
        assert!(leg.contains("07 = DXE driver"));
        assert!(leg.contains("F0 = Pad"));
        assert!(leg.contains("Section types:"));
        assert!(leg.contains("10 = PE32 image"));
        assert!(leg.contains("15 = UI"));
    }

    #[test]
    fn format_legend_empty_when_no_file_or_section() {
        let rows = vec![row("", 62, 0, ""), row("0", 65, 0, "")];
        assert_eq!(format_legend(&rows), "");
    }

    #[test]
    fn depth_returns_tree_level() {
        assert_eq!(depth(""), 0);
        assert_eq!(depth("0"), 1);
        assert_eq!(depth("0/1"), 2);
        assert_eq!(depth("0/1/2/3"), 4);
    }
}
