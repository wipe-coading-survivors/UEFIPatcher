use std::collections::{HashMap, HashSet};

use uefi_proto::{FormEdge, FormInfo};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormKey {
    pub target: String,
    pub formset_guid: String,
    pub form_id_ifr: u32,
    pub title: String,
}

#[derive(Debug, Clone)]
pub enum FormsRow {
    FormSet {
        guid: String,
        expanded: bool,
    },
    Form {
        key: FormKey,
        visible: bool,
        depth: usize,
        path: String,
        has_children: bool,
    },
    DanglingRef {
        form_id: u32,
        depth: usize,
    },
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
                    depth: 1,
                    path: String::new(),
                    has_children: false,
                });
            }
        }
    }
    rows
}

/// Строки левой панели в режиме REF-дерева (спека §3.5): корни —
/// формы без входящих рёбер, затем непоказанные (циклы) в порядке
/// появления; кратные родители — у каждого; висячие REF-цели —
/// DanglingRef-строкой; циклы не зацикливают (множество на-пути).
pub fn build_tree_rows(
    forms: &[FormInfo],
    edges: &[FormEdge],
    expanded: &HashSet<String>,
) -> Vec<FormsRow> {
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
        rows.push(FormsRow::FormSet {
            guid: guid.clone(),
            expanded: expanded.contains(guid),
        });
        if !expanded.contains(guid) {
            continue;
        }
        let Some(list) = by_set.get(guid) else {
            continue;
        };
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut has_incoming: HashSet<u32> = HashSet::new();
        for e in edges {
            if e.formset_guid != *guid {
                continue;
            }
            let kids = children.entry(e.parent_form_id).or_default();
            if !kids.contains(&e.form_id) {
                kids.push(e.form_id);
            }
            has_incoming.insert(e.form_id);
        }
        let by_id: HashMap<u32, &FormInfo> = list.iter().map(|f| (f.form_id_ifr, *f)).collect();
        let mut emitted: HashSet<u32> = HashSet::new();
        let mut queue: Vec<u32> = list
            .iter()
            .filter(|f| !has_incoming.contains(&f.form_id_ifr))
            .map(|f| f.form_id_ifr)
            .collect();
        let mut reachable: HashSet<u32> = HashSet::new();
        let mut stack = queue.clone();
        while let Some(cur) = stack.pop() {
            if !reachable.insert(cur) {
                continue;
            }
            if let Some(kids) = children.get(&cur) {
                for child in kids {
                    if by_id.contains_key(child) {
                        stack.push(*child);
                    }
                }
            }
        }
        for f in list {
            let id = f.form_id_ifr;
            if !reachable.contains(&id) && !queue.contains(&id) {
                queue.push(id);
            }
        }
        for root in queue {
            if emitted.contains(&root) {
                continue;
            }
            let mut path: Vec<String> = Vec::new();
            let mut on_path: HashSet<u32> = HashSet::new();
            emit_form(
                &by_id,
                &children,
                root,
                guid,
                expanded,
                1,
                &mut path,
                &mut on_path,
                &mut emitted,
                &mut rows,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn emit_form(
        by_id: &HashMap<u32, &FormInfo>,
        children: &HashMap<u32, Vec<u32>>,
        form_id: u32,
        guid: &str,
        expanded: &HashSet<String>,
        depth: usize,
        path: &mut Vec<String>,
        on_path: &mut HashSet<u32>,
        emitted: &mut HashSet<u32>,
        rows: &mut Vec<FormsRow>,
    ) {
        if on_path.contains(&form_id) {
            return;
        }
        let Some(f) = by_id.get(&form_id) else { return };
        let kids = children.get(&form_id).cloned().unwrap_or_default();
        path.push(f.title.clone());
        rows.push(FormsRow::Form {
            key: FormKey {
                target: f.form_id.clone(),
                formset_guid: guid.to_string(),
                form_id_ifr: f.form_id_ifr,
                title: f.title.clone(),
            },
            visible: f.visible,
            depth,
            path: path.join(" → "),
            has_children: !kids.is_empty(),
        });
        emitted.insert(form_id);
        if !kids.is_empty() && expanded.contains(&format!("{guid}#{form_id}")) {
            on_path.insert(form_id);
            for child in kids {
                if by_id.contains_key(&child) {
                    emit_form(
                        by_id,
                        children,
                        child,
                        guid,
                        expanded,
                        depth + 1,
                        path,
                        on_path,
                        emitted,
                        rows,
                    );
                } else {
                    rows.push(FormsRow::DanglingRef {
                        form_id: child,
                        depth: depth + 1,
                    });
                }
            }
            on_path.remove(&form_id);
        }
        path.pop();
    }
    rows
}

/// Полный набор ключей развёрнутости для входа во view: гуиды
/// формсетов + "guid#form_id" форм-родителей (полное расширение —
/// паритет с V1, где все формсеты были развёрнуты).
pub fn all_row_keys(forms: &[FormInfo], edges: &[FormEdge]) -> HashSet<String> {
    let mut set = all_formset_guids(forms);
    let parents: HashSet<(String, u32)> = edges
        .iter()
        .map(|e| (e.formset_guid.clone(), e.parent_form_id))
        .collect();
    for f in forms {
        if parents.contains(&(f.formset_guid.clone(), f.form_id_ifr)) {
            set.insert(format!("{}#{}", f.formset_guid, f.form_id_ifr));
        }
    }
    set
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

    fn edge(set: &str, parent: u32, child: u32) -> uefi_proto::FormEdge {
        uefi_proto::FormEdge {
            formset_guid: set.into(),
            parent_form_id: parent,
            form_id: child,
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
            matches!(&rows[2], FormsRow::Form { key, visible: false, .. } if key.form_id_ifr == 10019)
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

    #[test]
    fn tree_rows_nested_path_and_dangling() {
        let forms = vec![
            fi("t1", "S", 1, "Main", true),
            fi("t1", "S", 2, "Advanced", true),
            fi("t1", "S", 3, "Serial", false),
        ];
        let edges = vec![edge("S", 1, 2), edge("S", 2, 3), edge("S", 1, 99)];
        let ex = all_row_keys(&forms, &edges);
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(rows.len(), 5);
        assert!(
            matches!(&rows[1], FormsRow::Form { key, depth: 1, path, has_children: true, .. }
                if key.form_id_ifr == 1 && path == "Main")
        );
        assert!(
            matches!(&rows[2], FormsRow::Form { key, depth: 2, path, .. }
                if key.form_id_ifr == 2 && path == "Main → Advanced")
        );
        assert!(
            matches!(&rows[3], FormsRow::Form { key, depth: 3, path, visible: false, .. }
                if key.form_id_ifr == 3 && path == "Main → Advanced → Serial")
        );
        assert!(matches!(
            &rows[4],
            FormsRow::DanglingRef {
                form_id: 99,
                depth: 2
            }
        ));
    }

    #[test]
    fn tree_rows_cycle_does_not_loop_and_form_still_visible() {
        let forms = vec![fi("t1", "S", 1, "A", true), fi("t1", "S", 2, "B", true)];
        let edges = vec![edge("S", 1, 2), edge("S", 2, 1)];
        let ex = all_row_keys(&forms, &edges);
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(
            rows.len(),
            3,
            "FormSet + A + B ровно один раз каждый (обратное ребро 2->1 пропущено)"
        );
        assert!(matches!(&rows[1], FormsRow::Form { key, .. } if key.form_id_ifr == 1));
        assert!(matches!(&rows[2], FormsRow::Form { key, depth: 2, .. } if key.form_id_ifr == 2));
    }

    #[test]
    fn tree_rows_multi_parent_rendered_under_each() {
        let forms = vec![
            fi("t1", "S", 1, "P1", true),
            fi("t1", "S", 2, "P2", true),
            fi("t1", "S", 3, "Kid", true),
        ];
        let edges = vec![edge("S", 1, 3), edge("S", 2, 3)];
        let ex = all_row_keys(&forms, &edges);
        let rows = build_tree_rows(&forms, &edges, &ex);
        let kid_rows = rows
            .iter()
            .filter(|r| matches!(r, FormsRow::Form { key, .. } if key.form_id_ifr == 3))
            .count();
        assert_eq!(kid_rows, 2, "форма с двумя родителями — у каждого");
    }

    #[test]
    fn tree_rows_collapsed_form_hides_children() {
        let forms = vec![
            fi("t1", "S", 1, "Main", true),
            fi("t1", "S", 2, "Adv", true),
        ];
        let edges = vec![edge("S", 1, 2)];
        let mut ex = all_row_keys(&forms, &edges);
        ex.remove("S#1");
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(rows.len(), 2, "FormSet + свёрнутая Main");
        assert!(matches!(
            &rows[1],
            FormsRow::Form {
                has_children: true,
                ..
            }
        ));
    }

    #[test]
    fn tree_rows_edges_of_other_formset_ignored() {
        let forms = vec![
            fi("t1", "S", 1, "Main", true),
            fi("t2", "X", 9, "Other", true),
        ];
        let edges = vec![edge("X", 9, 1)];
        let ex = all_row_keys(&forms, &edges);
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(rows.len(), 5);
        assert!(
            matches!(&rows[1], FormsRow::Form { depth: 1, .. }),
            "ребро чужого формсета не вкладывает"
        );
        assert!(
            matches!(
                &rows[4],
                FormsRow::DanglingRef {
                    form_id: 1,
                    depth: 2
                }
            ),
            "цель 1 — не форма формсета X: висячая строка под Other"
        );
    }

    #[test]
    fn all_row_keys_cover_formsets_and_parent_forms() {
        let forms = vec![
            fi("t1", "S", 1, "Main", true),
            fi("t1", "S", 2, "Adv", true),
            fi("t2", "X", 9, "Other", true),
        ];
        let edges = vec![edge("S", 1, 2)];
        let keys = all_row_keys(&forms, &edges);
        assert!(keys.contains("S"));
        assert!(keys.contains("X"));
        assert!(
            keys.contains("S#1"),
            "родительский узел развёрнут по умолчанию"
        );
        assert!(!keys.contains("S#2"), "лист не нужен в наборе");
    }
}
