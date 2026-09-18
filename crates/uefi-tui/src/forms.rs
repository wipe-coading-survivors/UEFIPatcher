use std::collections::{HashMap, HashSet};

use uefi_proto::{FormEdge, FormInfo};

use crate::app::FormsData;

pub(crate) fn short_guid(guid: &str) -> &str {
    &guid[..guid.len().min(13)]
}

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
        expanded: bool,
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
                    expanded: false,
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
/// Кросс-формсетные рёбра (target_formset_guid) резолвятся по
/// глобальной карте всех форм. Спека formset-unlock §3 U1c.
pub fn build_tree_rows(
    forms: &[FormInfo],
    edges: &[FormEdge],
    expanded: &HashSet<String>,
) -> Vec<FormsRow> {
    let global: HashMap<(&str, u32), &FormInfo> = forms
        .iter()
        .map(|f| ((f.formset_guid.as_str(), f.form_id_ifr), f))
        .collect();
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
        let mut children: HashMap<u32, Vec<(u32, &str)>> = HashMap::new();
        let mut has_incoming: HashSet<u32> = HashSet::new();
        for e in edges {
            if e.formset_guid != *guid {
                continue;
            }
            let target = e.target_formset_guid.as_str();
            let kids = children.entry(e.parent_form_id).or_default();
            if !kids.contains(&(e.form_id, target)) {
                kids.push((e.form_id, target));
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
                for (child, _) in kids {
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
                &global,
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
        children: &HashMap<u32, Vec<(u32, &str)>>,
        global: &HashMap<(&str, u32), &FormInfo>,
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
        let is_expanded = expanded.contains(&format!("{guid}#{form_id}"));
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
            expanded: is_expanded,
        });
        emitted.insert(form_id);
        if !kids.is_empty() && is_expanded {
            on_path.insert(form_id);
            for (child, target) in kids {
                if by_id.contains_key(&child) {
                    emit_form(
                        by_id,
                        children,
                        global,
                        child,
                        guid,
                        expanded,
                        depth + 1,
                        path,
                        on_path,
                        emitted,
                        rows,
                    );
                } else if !target.is_empty()
                    && let Some(x) = global.get(&(target, child))
                {
                    rows.push(FormsRow::Form {
                        key: FormKey {
                            target: x.form_id.clone(),
                            formset_guid: x.formset_guid.clone(),
                            form_id_ifr: x.form_id_ifr,
                            title: x.title.clone(),
                        },
                        visible: x.visible,
                        depth: depth + 1,
                        path: format!("{} → {}", path.join(" → "), x.title),
                        has_children: false,
                        expanded: expanded
                            .contains(&format!("{}#{}", x.formset_guid, x.form_id_ifr)),
                    });
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

pub struct FormPanel {
    pub header: Vec<String>,
    pub questions: Vec<String>,
    pub gates: Vec<String>,
    pub bottom: Vec<String>,
    pub questions_ready: bool,
}

/// Трёхзонная сборка правой панели Forms-view (спека
/// forms-panel-three-zone §2-§3): шапка формы + колонки-хедер, строки
/// вопросов, хвост гейтов, низ — детали выбранного вопроса. Рендерит
/// ui/forms.rs. НЕ скроллит — скролл списка делает рендер.
pub fn form_panel(forms: &FormsData, rows: &[FormsRow], cursor: usize) -> FormPanel {
    let Some(FormsRow::Form { key, path, .. }) = rows.get(cursor) else {
        return FormPanel {
            header: vec!["no form selected".into()],
            questions: vec![],
            gates: vec![],
            bottom: vec![],
            questions_ready: false,
        };
    };
    let mut header = vec![
        format!("Form:    {}", key.title),
        format!("Form ID: {}", key.form_id_ifr),
        format!("FormSet: {}", short_guid(&key.formset_guid)),
        format!("Target:  {}", key.target),
    ];
    if !path.is_empty() {
        header.push(format!("Path:    {path}"));
    }
    let ready = forms.questions_key.as_ref() == Some(key);
    header.push(String::new());
    header.push(if ready {
        format!("Questions ({}): prompt · qid · kind", forms.questions.len())
    } else {
        "Questions: loading…".into()
    });

    let questions: Vec<String> = if ready {
        forms
            .questions
            .iter()
            .map(|q| {
                let prompt = if q.prompt.is_empty() { "-" } else { &q.prompt };
                format!(
                    "{} {prompt:<28} q{:#x} {}",
                    crate::theme::question_icon(&q.kind),
                    q.question_id,
                    q.kind
                )
            })
            .collect()
    } else {
        vec![]
    };

    let mut gates = Vec::new();
    if ready && !forms.gates.is_empty() {
        gates.push(String::new());
        gates.push(format!("Gates ({}):", forms.gates.len()));
        for g in &forms.gates {
            let flip = if g.flippable { "flippable" } else { "-" };
            let src = if g.source_target.is_empty() {
                String::new()
            } else {
                format!(" @{}", g.source_target)
            };
            gates.push(format!(
                "  {:<8} {:<24} {}{}",
                g.gate_kind, g.expression, flip, src
            ));
        }
    }

    let info_for_this = forms
        .question_info_key
        .as_ref()
        .is_some_and(|(k, _)| k == key);
    let bottom = if info_for_this {
        forms
            .question_info
            .as_ref()
            .map(question_bottom)
            .unwrap_or_else(|| vec!["(loading…)".into()])
    } else {
        vec!["(loading…)".into()]
    };

    FormPanel {
        header,
        questions,
        gates,
        bottom,
        questions_ready: ready,
    }
}

fn question_bottom(qi: &uefi_proto::QuestionInfo) -> Vec<String> {
    let icon = crate::theme::question_icon(&qi.kind);
    let mut lines = vec![
        format!("{icon} Question q{:#x} ({}):", qi.question_id, qi.kind),
        format!(
            "  store {} · offset {:#x} · width {}",
            qi.var_store_id, qi.var_offset, qi.width
        ),
    ];
    match qi.kind.as_str() {
        "numeric" => {
            lines.push(format!("  range {}..={} step {}", qi.min, qi.max, qi.step));
        }
        "one_of" => {
            if qi.options.is_empty() {
                lines.push("  options: (none)".into());
            }
            for (i, o) in qi.options.iter().enumerate() {
                let item = if o.text.is_empty() {
                    format!("{:#x}(sid {})", o.value, o.string_id)
                } else {
                    format!("{:#x} \"{}\"", o.value, o.text)
                };
                let prefix = if i == 0 {
                    "  options: ".to_string()
                } else {
                    " ".repeat(11)
                };
                lines.push(format!("{prefix}{item}"));
            }
        }
        _ => {}
    }
    lines
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

    fn edge(set: &str, parent: u32, child: u32, target: &str) -> uefi_proto::FormEdge {
        uefi_proto::FormEdge {
            formset_guid: set.into(),
            parent_form_id: parent,
            form_id: child,
            target_formset_guid: target.into(),
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
        let edges = vec![
            edge("S", 1, 2, ""),
            edge("S", 2, 3, ""),
            edge("S", 1, 99, ""),
        ];
        let ex = all_row_keys(&forms, &edges);
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(rows.len(), 5);
        assert!(
            matches!(&rows[1], FormsRow::Form { key, depth: 1, path, has_children: true, expanded: true, .. }
                if key.form_id_ifr == 1 && path == "Main")
        );
        assert!(
            matches!(&rows[2], FormsRow::Form { key, depth: 2, path, expanded: true, .. }
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
        let edges = vec![edge("S", 1, 2, ""), edge("S", 2, 1, "")];
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
        let edges = vec![edge("S", 1, 3, ""), edge("S", 2, 3, "")];
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
        let edges = vec![edge("S", 1, 2, "")];
        let mut ex = all_row_keys(&forms, &edges);
        ex.remove("S#1");
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert_eq!(rows.len(), 2, "FormSet + свёрнутая Main");
        assert!(matches!(
            &rows[1],
            FormsRow::Form {
                has_children: true,
                expanded: false,
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
        let edges = vec![edge("X", 9, 1, "")];
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
    fn cross_edge_attaches_foreign_formset_form() {
        let setup = "7B59104A-366D-4C6F-8147-633AA5D8E0D4";
        let chipset = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";
        let forms = vec![
            fi("t1", setup, 10001, "Main", true),
            fi("t2", chipset, 1, "Chipset", true),
        ];
        let edges = vec![edge(setup, 10001, 1, chipset)];
        let ex: HashSet<String> = [setup.to_string(), format!("{setup}#10001")].into();
        let rows = build_tree_rows(&forms, &edges, &ex);
        // EC87D643-форма-1 — ребёнок 10001 в дереве Setup (depth 2),
        // корня в своём (свёрнутом) формсете нет — ровно одна строка
        let cross: Vec<_> = rows
            .iter()
            .filter(|r| {
                matches!(r, FormsRow::Form { key, .. }
                    if key.formset_guid == chipset && key.form_id_ifr == 1)
            })
            .collect();
        assert_eq!(cross.len(), 1, "ровно одна строка кросс-ребёнка");
        assert!(matches!(
            cross[0],
            FormsRow::Form {
                depth: 2,
                path,
                ..
            } if path == "Main → Chipset"
        ));
    }

    #[test]
    fn cross_edge_unresolvable_target_is_dangling() {
        let setup = "7B59104A-366D-4C6F-8147-633AA5D8E0D4";
        let forms = vec![fi("t1", setup, 10001, "Main", true)];
        let edges = vec![edge(
            setup,
            10001,
            7,
            "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9",
        )];
        let ex: HashSet<String> = [setup.to_string(), format!("{setup}#10001")].into();
        let rows = build_tree_rows(&forms, &edges, &ex);
        assert!(matches!(
            rows.last(),
            Some(FormsRow::DanglingRef {
                form_id: 7,
                depth: 2
            })
        ));
    }

    #[test]
    fn all_row_keys_cover_formsets_and_parent_forms() {
        let forms = vec![
            fi("t1", "S", 1, "Main", true),
            fi("t1", "S", 2, "Adv", true),
            fi("t2", "X", 9, "Other", true),
        ];
        let edges = vec![edge("S", 1, 2, "")];
        let keys = all_row_keys(&forms, &edges);
        assert!(keys.contains("S"));
        assert!(keys.contains("X"));
        assert!(
            keys.contains("S#1"),
            "родительский узел развёрнут по умолчанию"
        );
        assert!(!keys.contains("S#2"), "лист не нужен в наборе");
    }

    #[test]
    fn form_panel_builds_zones() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        let rows = app.forms_rows();
        let key = FormKey {
            target: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
        };
        app.forms.questions_key = Some(key.clone());
        app.forms.questions = vec![
            uefi_proto::QuestionSummary {
                question_id: 0x210,
                prompt: "Cores".into(),
                kind: "numeric".into(),
                ..Default::default()
            },
            uefi_proto::QuestionSummary {
                question_id: 0x211,
                prompt: String::new(),
                kind: "one_of".into(),
                ..Default::default()
            },
        ];
        app.forms.gates = vec![uefi_proto::GateInfo {
            gate_kind: "suppress".into(),
            expression: "e0".into(),
            flippable: true,
            ..Default::default()
        }];
        app.forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x210,
            kind: "numeric".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            min: 1,
            max: 8,
            step: 1,
            ..Default::default()
        });
        app.forms.question_info_key = Some((key, 0x210));
        let p = form_panel(&app.forms, &rows, 1);
        assert!(p.questions_ready);
        assert_eq!(
            p.header.last().unwrap(),
            "Questions (2): prompt · qid · kind"
        );
        assert_eq!(p.questions.len(), 2);
        assert!(p.questions[0].contains("Cores"));
        assert!(p.questions[0].contains("q0x210"));
        assert!(p.questions[1].contains('-'), "пустой промпт — дефис");
        assert_eq!(p.gates[1], "Gates (1):");
        assert!(p.bottom[0].contains("Question q0x210"));
        assert!(p.bottom.iter().any(|l| l.contains("range 1..=8 step 1")));
    }

    #[test]
    fn question_bottom_branches_and_path_row() {
        let key = FormKey {
            target: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
        };
        let mut forms = crate::app::FormsData::default();
        forms.questions_key = Some(key.clone());
        let rows = vec![FormsRow::Form {
            key: key.clone(),
            visible: true,
            depth: 1,
            path: "0/1/2".into(),
            has_children: false,
            expanded: false,
        }];
        let p = form_panel(&forms, &rows, 0);
        assert!(p.header.iter().any(|l| l.starts_with("Path:    0/1/2")));

        forms.question_info_key = Some((key.clone(), 0x220));
        forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x220,
            kind: "one_of".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            options: vec![
                uefi_proto::OptionEntry {
                    value: 1,
                    string_id: 0x10,
                    text: "Enabled".into(),
                    ..Default::default()
                },
                uefi_proto::OptionEntry {
                    value: 2,
                    string_id: 0x11,
                    text: String::new(),
                    ..Default::default()
                },
                uefi_proto::OptionEntry {
                    value: 3,
                    string_id: 0x12,
                    text: String::new(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let p = form_panel(&forms, &rows, 0);
        assert!(
            p.bottom.iter().any(|l| l.contains("(sid ")),
            "пустой text — sid-fallback"
        );
        assert!(p.bottom.iter().any(|l| l.contains("0x1 \"Enabled\"")));

        forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x220,
            kind: "one_of".into(),
            var_store_id: 2,
            var_offset: 0x37,
            width: 1,
            ..Default::default()
        });
        let p = form_panel(&forms, &rows, 0);
        assert!(p.bottom.iter().any(|l| l.contains("options: (none)")));

        forms.question_info = Some(uefi_proto::QuestionInfo {
            question_id: 0x221,
            kind: "checkbox".into(),
            var_store_id: 2,
            var_offset: 0x38,
            width: 1,
            ..Default::default()
        });
        let p = form_panel(&forms, &rows, 0);
        assert_eq!(p.bottom.len(), 2, "заголовок + store-строка, без доп-строк");
    }

    #[test]
    fn form_panel_loading_and_no_form() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t1".into(),
            formset_guid: "S".into(),
            form_id_ifr: 1,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.expanded = ["S".into()].into();
        let rows = app.forms_rows();
        let p = form_panel(&app.forms, &rows, 1);
        assert!(!p.questions_ready);
        assert_eq!(p.header.last().unwrap(), "Questions: loading…");
        assert!(p.questions.is_empty());
        assert!(p.gates.is_empty());
        assert_eq!(p.bottom, vec!["(loading…)".to_string()]);
        let p0 = form_panel(&app.forms, &rows, 0);
        assert_eq!(p0.header, vec!["no form selected".to_string()]);
        assert!(!p0.questions_ready);
    }
}
