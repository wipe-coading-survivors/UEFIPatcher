use std::collections::{HashMap, HashSet};

use uefi_proto::FormInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormKey {
    pub target: String,
    pub formset_guid: String,
    pub form_id_ifr: u32,
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum FormsRow {
    FormSet { guid: String, expanded: bool },
    Form { key: FormKey, visible: bool },
}

/// Строки левой панели: формсеты в порядке первого появления, формы
/// внутри развёрнутого формсета. Спека tui-forms-view §3.2–3.3.
pub fn build_rows(forms: &[FormInfo], expanded: &HashSet<String>) -> Vec<FormsRow> {
    let mut order: Vec<String> = Vec::new();
    let mut by_set: HashMap<String, Vec<&FormInfo>> = HashMap::new();
    for f in forms {
        if !by_set.contains_key(&f.formset_guid) {
            order.push(f.formset_guid.clone());
        }
        by_set.entry(f.formset_guid.clone()).or_default().push(f);
    }
    let mut rows = Vec::new();
    for guid in &order {
        let is_expanded = expanded.contains(guid);
        rows.push(FormsRow::FormSet {
            guid: guid.clone(),
            expanded: is_expanded,
        });
        if is_expanded && let Some(list) = by_set.get(guid) {
            for f in list {
                rows.push(FormsRow::Form {
                    key: FormKey {
                        target: f.form_id.clone(),
                        formset_guid: f.formset_guid.clone(),
                        form_id_ifr: f.form_id_ifr,
                        title: f.title.clone(),
                    },
                    visible: f.visible,
                });
            }
        }
    }
    rows
}

pub fn selected_key(rows: &[FormsRow], cursor: usize) -> Option<FormKey> {
    match rows.get(cursor) {
        Some(FormsRow::Form { key, .. }) => Some(key.clone()),
        _ => None,
    }
}

pub fn all_formset_guids(forms: &[FormInfo]) -> HashSet<String> {
    forms.iter().map(|f| f.formset_guid.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fi(target: &str, set: &str, id: u32, title: &str, visible: bool) -> FormInfo {
        FormInfo {
            form_id: target.into(),
            formset_guid: set.into(),
            form_id_ifr: id,
            title: title.into(),
            visible,
        }
    }

    fn fixture() -> Vec<FormInfo> {
        vec![
            fi("t1:0x19:0", "SET-A", 10001, "Main", true),
            fi("t1:0x19:0", "SET-A", 10019, "Serial", false),
            fi("t2:0x19:0", "SET-B", 902, "Platform", true),
        ]
    }

    #[test]
    fn build_rows_groups_and_expands() {
        let ex = all_formset_guids(&fixture());
        let rows = build_rows(&fixture(), &ex);
        assert_eq!(rows.len(), 5);
        assert!(matches!(&rows[0], FormsRow::FormSet { guid, expanded: true } if guid == "SET-A"));
        assert!(matches!(&rows[1], FormsRow::Form { key, .. } if key.form_id_ifr == 10001));
        assert!(
            matches!(&rows[2], FormsRow::Form { key, visible: false } if key.form_id_ifr == 10019)
        );
        assert!(matches!(&rows[4], FormsRow::Form { key, .. } if key.form_id_ifr == 902));
    }

    #[test]
    fn build_rows_collapsed_formset_hides_forms() {
        let mut ex = all_formset_guids(&fixture());
        ex.remove("SET-A");
        let rows = build_rows(&fixture(), &ex);
        assert_eq!(rows.len(), 3);
        assert!(matches!(
            &rows[0],
            FormsRow::FormSet {
                expanded: false,
                ..
            }
        ));
        assert!(matches!(&rows[1], FormsRow::FormSet { .. }));
    }

    #[test]
    fn selected_key_only_on_form_rows() {
        let ex = all_formset_guids(&fixture());
        let rows = build_rows(&fixture(), &ex);
        assert!(selected_key(&rows, 0).is_none());
        let key = selected_key(&rows, 2).unwrap();
        assert_eq!(key.form_id_ifr, 10019);
        assert_eq!(key.title, "Serial");
        assert_eq!(key.target, "t1:0x19:0");
    }
}
