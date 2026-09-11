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
            "{}  {}{sub_part} type={} subtype={:02X} guid={} off={} size={}{name_part}\n",
            r.path, node_label, r.type_, r.subtype, r.guid, r.offset, r.size,
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

/// Какая hii-команда печатает легенду — определяет блок колонок/подсказок.
pub enum HiiLegendCmd {
    FormList,
    QuestionList,
}

/// Легенда грамматики item_id и колонок для hii-вывода CLI (в stderr, по
/// образцу format_legend). section_codes — типы секций из target-частей item_id.
pub fn hii_legend(cmd: HiiLegendCmd, section_codes: &[u8]) -> String {
    let mut out = String::from(
        "Legend:\n  item_id = <ffs-file-guid>:<section-type>:<index>#<form_id-dec>[:<question-id-hex>]\n",
    );
    let mut codes = section_codes.to_vec();
    codes.sort_unstable();
    codes.dedup();
    if !codes.is_empty() {
        out.push_str("  Section types:\n");
        for code in codes {
            out.push_str(&format!(
                "    {:02X} = {}\n",
                code,
                section_type_name_or_raw(code)
            ));
        }
    }
    match cmd {
        HiiLegendCmd::FormList => out.push_str(
            "  Columns:\n    form_id = target of the form package (left part of item_id)\n    formset_guid = IFR formset GUID\n    form_id_ifr = IFR form id (decimal; append as #<form_id> to form_id)\n    title = form title string\n    visible = suppression state (false = hidden by a suppress gate)\n",
        ),
        HiiLegendCmd::QuestionList => out.push_str(
            "  kind: one_of = pick an option · checkbox = 0/1 · numeric = range · other = not settable\n  set-value accepts decimal or 0x-hex values\n  prompt \"-\" = string id not resolved\n",
        ),
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
        assert!(out.contains("Image type=62 subtype=00"));
        assert!(out.contains("0  Volume type=65 subtype=00"));
        assert!(
            out.contains("0/0  File(DXE driver) type=66 subtype=07 guid= off=0 size=0 name=Setup")
        );
        assert!(out.contains("0/0/0  Section(UI) type=67 subtype=15"));
    }

    #[test]
    fn format_tree_skips_subtype_name_for_non_file_section() {
        let rows = vec![row("0", 65, 0x42, "")];
        let out = format_tree(&rows);
        assert!(out.contains("Volume type=65 subtype=42"));
        assert!(!out.contains("Volume("));
    }

    #[test]
    fn hii_legend_item_id_grammar_and_section_codes() {
        let leg = hii_legend(HiiLegendCmd::FormList, &[0x19, 0x10, 0x19]);
        assert!(leg.contains(
            "item_id = <ffs-file-guid>:<section-type>:<index>#<form_id-dec>[:<question-id-hex>]"
        ));
        assert!(leg.contains("10 = PE32 image"));
        assert!(leg.contains("19 = Raw"));
        let ten = leg.find("10 =").unwrap();
        let nineteen = leg.find("19 =").unwrap();
        assert!(ten < nineteen, "коды отсортированы");
        assert!(!leg.contains("kind:"));
        assert!(leg.contains("form_id = target of the form package"));
        assert!(leg.contains("formset_guid = IFR formset GUID"));
        assert!(leg.contains("form_id_ifr = IFR form id (decimal"));
        assert!(leg.contains("visible = suppression state"));
    }

    #[test]
    fn hii_legend_questions_variant() {
        let leg = hii_legend(HiiLegendCmd::QuestionList, &[]);
        assert!(leg.contains("item_id ="));
        assert!(!leg.contains("Section types:"));
        assert!(leg.contains("kind: one_of = pick an option"));
        assert!(leg.contains("set-value accepts decimal or 0x-hex"));
        assert!(leg.contains("prompt \"-\" = string id not resolved"));
        assert!(!leg.contains("Columns:"));
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
}
