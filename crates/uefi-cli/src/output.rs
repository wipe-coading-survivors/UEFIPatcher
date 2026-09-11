use uefi_proto::{
    FormInfo, GateInfo, HiiFormHijackResponse, HiiPageAddResponse, HiiQuestionAddOutcome,
    ImageInfo, Node, QuestionInfo, QuestionSummary, SessionInfo, StringInfo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    #[value(name = "json")]
    Json,
    #[value(name = "text")]
    Text,
    #[value(name = "tsv")]
    Tsv,
}

pub fn print_nodes(nodes: &[Node], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(nodes).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("path\ttype\tsubtype\tguid\toffset\tsize\tname");
            for it in nodes {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    it.path, it.r#type, it.subtype, it.guid, it.offset, it.size, it.name
                );
            }
        }
        OutputFormat::Text => {
            let rows: Vec<uefi_common::format::TreeRow> = nodes
                .iter()
                .map(|it| uefi_common::format::TreeRow {
                    path: it.path.clone(),
                    type_: it.r#type,
                    subtype: it.subtype as u8,
                    guid: it.guid.clone(),
                    offset: it.offset,
                    size: it.size,
                    name: it.name.clone(),
                })
                .collect();
            print!("{}", uefi_common::format::format_tree(&rows));
        }
    }
}

pub fn print_session_created(session_id: &str, token: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"session_id\":\"{session_id}\",\"token\":\"{token}\"}}"),
        _ => println!("{session_id}\t{token}"),
    }
}

pub fn print_sessions(rows: &[SessionInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(rows).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        _ => {
            println!("session_id\tcreated_at\tlast_activity");
            for r in rows {
                println!("{}\t{}\t{}", r.session_id, r.created_at, r.last_activity);
            }
        }
    }
}

pub fn print_image_info(info: &ImageInfo, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string(info).unwrap_or_else(|_| "{}".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity");
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                info.image_id,
                info.name,
                info.path,
                info.mode,
                info.size,
                info.created_at,
                info.last_activity
            );
        }
    }
}

pub fn print_images_list(images: &[ImageInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(images).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tmode\tsize\tlast_activity");
            for img in images {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    img.image_id, img.name, img.mode, img.size, img.last_activity
                );
            }
        }
    }
}

pub fn print_image_status(info: &ImageInfo, format: OutputFormat) {
    print_image_info(info, format);
}

pub fn print_questions(
    questions: &[QuestionSummary],
    target: &str,
    form_id: u32,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(questions).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("item_id\tquestion_id\tkind\tprompt\tvar_store_id\tvar_offset\twidth");
            for q in questions {
                println!(
                    "{target}#{form_id}:{:#x}\t{:#x}\t{}\t{}\t{}\t{:#x}\t{}",
                    q.question_id,
                    q.question_id,
                    q.kind,
                    q.prompt,
                    q.var_store_id,
                    q.var_offset,
                    q.width
                );
            }
        }
        OutputFormat::Text => {
            for q in questions {
                let prompt = if q.prompt.is_empty() {
                    "-".to_string()
                } else {
                    format!("\"{}\"", q.prompt)
                };
                println!(
                    "{target}#{form_id}:{:#x}  {}  {prompt}",
                    q.question_id, q.kind
                );
            }
        }
    }
}

pub fn print_forms(forms: &[FormInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(forms).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("form_id\tformset_guid\tform_id_ifr\ttitle\tvisible");
            for f in forms {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible
                );
            }
        }
        OutputFormat::Text => {
            for f in forms {
                println!(
                    "{}\t{}\t{}\t{}\tvisible={}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible
                );
            }
        }
    }
}

pub fn print_strings(strings: &[StringInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(strings).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("language\tstring_id\ttext");
            for s in strings {
                println!("{}\t{}\t{}", s.language, s.string_id, s.text);
            }
        }
        OutputFormat::Text => {
            for s in strings {
                println!("[{}] {}: {}", s.language, s.string_id, s.text);
            }
        }
    }
}

pub fn print_gates(item_id: &str, gates: &[GateInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(gates).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!(
                "gate_kind\twraps\tform_id\thost_form_id\tquestion_id\texpression\tflippable\tflip\tscope_offset"
            );
            for g in gates {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    g.gate_kind,
                    g.wraps,
                    g.form_id,
                    g.host_form_id,
                    g.question_id,
                    g.expression,
                    g.flippable,
                    g.flip,
                    g.scope_offset
                );
            }
        }
        OutputFormat::Text => {
            if gates.is_empty() {
                println!("no gates for {item_id}");
            }
            for g in gates {
                println!(
                    "{:<8} {:<8} form {} host {} qid {} expr '{}' flip '{}' @pkg+{:#x}",
                    g.gate_kind,
                    g.wraps,
                    g.form_id,
                    g.host_form_id,
                    g.question_id,
                    g.expression,
                    if g.flippable { g.flip.as_str() } else { "-" },
                    g.scope_offset
                );
            }
        }
    }
}

pub fn print_unlock(item_id: &str, gates: &[GateInfo], applied: &[String], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let gates_json = serde_json::to_string(gates).unwrap_or_else(|_| "[]".into());
            let applied_json = serde_json::to_string(applied).unwrap_or_else(|_| "[]".into());
            println!(
                "{{\"item_id\":\"{item_id}\",\"gates\":{gates_json},\"applied\":{applied_json}}}"
            );
        }
        _ => {
            print_gates(item_id, gates, format);
            if applied.is_empty() {
                println!("nothing to unlock for {item_id}");
            }
            for f in applied {
                println!("applied {f}");
            }
        }
    }
}

pub fn print_question_info(q: &QuestionInfo, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(q).unwrap_or_else(|_| "{}".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!(
                "form_id\tquestion_id\tkind\tvar_store_id\tvarstore\tvar_offset\twidth\tmin\tmax\tstep"
            );
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                q.form_id,
                q.question_id,
                q.kind,
                q.var_store_id,
                q.varstore.as_ref().map(|v| v.name.as_str()).unwrap_or(""),
                q.var_offset,
                q.width,
                q.min,
                q.max,
                q.step
            );
            for o in &q.options {
                println!(
                    "option\t{}\t{}\t{}\t{}",
                    o.string_id, o.value, o.flags, o.text
                );
            }
        }
        OutputFormat::Text => print!("{}", question_info_text(q)),
    }
}

fn question_info_text(q: &QuestionInfo) -> String {
    let mut s = format!("question {} #{}:{:#x}\n", q.kind, q.form_id, q.question_id);
    match &q.varstore {
        Some(vs) => {
            s.push_str(&format!(
                "varstore {} ({}) id {} size {:#x}\n",
                vs.name, vs.guid, vs.id, vs.size
            ));
        }
        None => s.push_str(&format!("varstore id {} (undeclared)\n", q.var_store_id)),
    }
    s.push_str(&format!(
        "width {}, offset {:#x} ({})\n",
        q.width, q.var_offset, q.var_offset
    ));
    if q.options.is_empty() {
        s.push_str("no options\n");
    }
    for o in &q.options {
        if o.text.is_empty() {
            s.push_str(&format!(
                "value = {} (string {}, flags {:#x})\n",
                o.value, o.string_id, o.flags
            ));
        } else {
            s.push_str(&format!(
                "value = {} \"{}\" (string {}, flags {:#x})\n",
                o.value, o.text, o.string_id, o.flags
            ));
        }
    }
    for d in &q.defaults {
        s.push_str(&format!(
            "default = {} (id {}, type {})\n",
            d.value, d.default_id, d.r#type
        ));
    }
    s
}

pub fn print_set_value(
    q: &QuestionInfo,
    applied: &[String],
    stores: &[String],
    format: OutputFormat,
) {
    match format {
        OutputFormat::Json => {
            let q_json = serde_json::to_string(q).unwrap_or_else(|_| "{}".into());
            let applied_json = serde_json::to_string(applied).unwrap_or_else(|_| "[]".into());
            let stores_json = serde_json::to_string(stores).unwrap_or_else(|_| "[]".into());
            println!(
                "{{\"question\":{q_json},\"applied\":{applied_json},\"stores\":{stores_json}}}"
            );
        }
        _ => {
            print_question_info(q, format);
            for f in applied {
                println!("applied {f}");
            }
            if matches!(format, OutputFormat::Text) {
                println!("stores: {}", stores.len());
            }
        }
    }
}

pub fn print_node_id(item_id: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"item_id\":\"{item_id}\"}}"),
        _ => println!("{item_id}"),
    }
}

pub fn print_formset_add(new_ffs_id: &str, form_ids: &[u32], format: OutputFormat) {
    let ids = form_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    match format {
        OutputFormat::Json => {
            println!("{{\"new_ffs_id\":\"{new_ffs_id}\",\"inserted_form_ids\":[{ids}]}}")
        }
        _ => println!("{new_ffs_id}\t{ids}"),
    }
}

pub fn print_form_add(
    form_ids: &[u32],
    string_ids: &std::collections::HashMap<String, u32>,
    format: OutputFormat,
) {
    let ids = form_ids
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    match format {
        OutputFormat::Json => {
            let sids = serde_json::to_string(string_ids).unwrap_or_else(|_| "{}".into());
            println!("{{\"inserted_form_ids\":[{ids}],\"string_ids\":{sids}}}")
        }
        _ => {
            println!("inserted_form_ids\t{ids}");
            for (name, sid) in string_ids {
                println!("string_id\t{name}\t{sid}");
            }
        }
    }
}

pub fn print_form_hijack(resp: &HiiFormHijackResponse, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let sids = serde_json::to_string(&resp.string_ids).unwrap_or_else(|_| "{}".into());
            let flips = serde_json::to_string(&resp.unlock_flips).unwrap_or_else(|_| "[]".into());
            let ctrls = resp
                .help_controls
                .iter()
                .map(|c| {
                    format!(
                        "{{\"question_id\":{},\"offset\":{},\"old_string_id\":{},\"new_string_id\":{}}}",
                        c.question_id, c.offset, c.old_string_id, c.new_string_id
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let recs = resp
                .help_records
                .iter()
                .map(|r| {
                    format!(
                        "{{\"question_id\":{},\"record_offset\":{},\"old_string_id\":{},\"new_string_id\":{}}}",
                        r.question_id, r.record_offset, r.old_string_id, r.new_string_id
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "{{\"string_ids\":{sids},\"unlock_flips\":{flips},\"help_controls\":[{ctrls}],\"help_records\":[{recs}],\"form_ifr_start\":{},\"form_ifr_end\":{}}}",
                resp.form_ifr_start, resp.form_ifr_end
            );
        }
        _ => {
            println!(
                "hijack\tstring_ids\t{}\tunlock_flips\t{}\thelp_controls\t{}\thelp_records\t{}",
                resp.string_ids.len(),
                resp.unlock_flips.len(),
                resp.help_controls.len(),
                resp.help_records.len()
            );
            for (name, sid) in &resp.string_ids {
                println!("string_id\t{name}\t{sid}");
            }
            for f in &resp.unlock_flips {
                println!("unlock\t{f}");
            }
            for c in &resp.help_controls {
                println!(
                    "help_control\tqid=0x{:X}\tstr@0x{:X}\t{:X}→{:X}",
                    c.question_id, c.offset, c.old_string_id, c.new_string_id
                );
            }
            for r in &resp.help_records {
                println!(
                    "help_record\tqid=0x{:X}\trec@0x{:X}\t{:X}→{:X}",
                    r.question_id, r.record_offset, r.old_string_id, r.new_string_id
                );
            }
            println!(
                "ifr [0x{:X}..0x{:X})",
                resp.form_ifr_start, resp.form_ifr_end
            );
        }
    }
}

pub fn question_add_json(
    questions: &[HiiQuestionAddOutcome],
    refs: &[HiiQuestionAddOutcome],
) -> String {
    fn outcomes_json(outcomes: &[HiiQuestionAddOutcome], with_spf_record: bool) -> Vec<String> {
        outcomes
            .iter()
            .map(|o| {
                let sids = serde_json::to_string(&o.string_ids).unwrap_or_else(|_| "{}".into());
                if with_spf_record {
                    format!(
                        "{{\"question_id\":{},\"string_ids\":{sids},\"spf_record_offset\":{}}}",
                        o.question_id, o.spf_record_offset
                    )
                } else {
                    format!(
                        "{{\"question_id\":{},\"string_ids\":{sids}}}",
                        o.question_id
                    )
                }
            })
            .collect()
    }
    let questions = outcomes_json(questions, true).join(",");
    let refs = outcomes_json(refs, false).join(",");
    format!("{{\"questions\":[{questions}],\"refs\":[{refs}]}}")
}

pub fn print_question_add_result(
    questions: &[HiiQuestionAddOutcome],
    refs: &[HiiQuestionAddOutcome],
    format: OutputFormat,
) {
    fn print_table(outcomes: &[HiiQuestionAddOutcome]) {
        if outcomes.is_empty() {
            return;
        }
        println!("question_id\tspf_record_offset\tstring_id\tname");
        for o in outcomes {
            println!("{:#X}\t{:#X}\t-\t-", o.question_id, o.spf_record_offset);
            let mut sids: Vec<(&String, &u32)> = o.string_ids.iter().collect();
            sids.sort_by_key(|&(name, sid)| (*sid, name));
            for (name, sid) in sids {
                println!(
                    "{:#X}\t{:#X}\t{sid}\t{name}",
                    o.question_id, o.spf_record_offset
                );
            }
        }
    }
    match format {
        OutputFormat::Json => println!("{}", question_add_json(questions, refs)),
        _ => {
            let sectioned = !questions.is_empty() && !refs.is_empty();
            if sectioned {
                println!("questions");
            }
            print_table(questions);
            if sectioned {
                println!("refs");
            }
            print_table(refs);
        }
    }
}

pub fn print_page_add(resp: &HiiPageAddResponse, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!(
            "{{\"form_id\":{},\"slot\":{},\"page_offset\":{},\"title_string_id\":{}}}",
            resp.form_id, resp.slot, resp.page_offset, resp.title_string_id
        ),
        _ => {
            println!("form_id\tslot\tpage_offset\ttitle_string_id");
            println!(
                "{}\t{}\t{}\t{}",
                resp.form_id, resp.slot, resp.page_offset, resp.title_string_id
            );
        }
    }
}

#[allow(dead_code)]
pub fn print_text(text: &str) {
    print!("{text}");
}

pub fn print_ok(format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"ok\":true}}"),
        _ => println!("ok"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uefi_proto::{
        DefaultEntry, HiiHelpControlEdit, HiiHelpRecordEdit, OptionEntry, VarStoreInfo,
    };

    #[test]
    fn session_created_json() {
        print_session_created("s1", "t1", OutputFormat::Json);
    }

    #[test]
    fn question_add_json_is_one_document_with_refs() {
        let mut sids = std::collections::HashMap::new();
        sids.insert("Serial Console".to_string(), 9u32);
        let q = HiiQuestionAddOutcome {
            question_id: 600,
            string_ids: sids,
            spf_record_offset: 0x1A4,
        };
        let mut rsids = std::collections::HashMap::new();
        rsids.insert("UEFIPatcher Setup".to_string(), 11u32);
        let r = HiiQuestionAddOutcome {
            question_id: 528,
            string_ids: rsids,
            spf_record_offset: 0,
        };
        let doc: serde_json::Value = serde_json::from_str(&question_add_json(&[q], &[r])).unwrap();
        assert_eq!(doc["questions"][0]["question_id"].as_u64(), Some(600));
        assert_eq!(
            doc["questions"][0]["spf_record_offset"].as_u64(),
            Some(0x1A4)
        );
        assert_eq!(
            doc["questions"][0]["string_ids"]["Serial Console"].as_u64(),
            Some(9)
        );
        assert_eq!(doc["refs"][0]["question_id"].as_u64(), Some(528));
        assert!(
            doc["refs"][0].get("spf_record_offset").is_none(),
            "refs carry no $SPF record and must not report a bogus offset"
        );
    }

    #[test]
    fn formset_add_print_smoke() {
        print_formset_add(
            "B00B0002-BEEF-1234-8000-000000000001",
            &[1, 2],
            OutputFormat::Json,
        );
        print_formset_add("X", &[], OutputFormat::Text);
    }

    #[test]
    fn form_add_print_smoke() {
        let mut string_ids = std::collections::HashMap::new();
        string_ids.insert("NewForm".to_string(), 3u32);
        print_form_add(&[42], &string_ids, OutputFormat::Json);
        print_form_add(&[], &string_ids, OutputFormat::Text);
    }

    #[test]
    fn form_hijack_print_smoke() {
        let mut string_ids = std::collections::HashMap::new();
        string_ids.insert("HijackTitle".to_string(), 7u32);
        let resp = HiiFormHijackResponse {
            string_ids,
            unlock_flips: vec!["pkg+0x10: 01→02".into()],
            help_controls: vec![HiiHelpControlEdit {
                question_id: 0x3B,
                offset: 0x40,
                old_string_id: 0x2A,
                new_string_id: 7,
            }],
            help_records: vec![HiiHelpRecordEdit {
                question_id: 0x3B,
                record_offset: 0x1F4,
                old_string_id: 0x1A4,
                new_string_id: 7,
            }],
            form_ifr_start: 0x5A,
            form_ifr_end: 0x8C,
        };
        print_form_hijack(&resp, OutputFormat::Json);
        print_form_hijack(&resp, OutputFormat::Text);
    }

    #[test]
    fn question_add_print_smoke() {
        let mut string_ids = std::collections::HashMap::new();
        string_ids.insert("Serial Console".to_string(), 2u32);
        string_ids.insert("Enabled".to_string(), 5u32);
        let outcomes = vec![HiiQuestionAddOutcome {
            question_id: 0x200,
            string_ids,
            spf_record_offset: 0x13C,
        }];
        let mut ref_sids = std::collections::HashMap::new();
        ref_sids.insert("UEFIPatcher Setup".to_string(), 7u32);
        let refs = vec![HiiQuestionAddOutcome {
            question_id: 528,
            string_ids: ref_sids,
            spf_record_offset: 0,
        }];
        print_question_add_result(&outcomes, &refs, OutputFormat::Json);
        print_question_add_result(&outcomes, &refs, OutputFormat::Text);
        print_question_add_result(&[], &[], OutputFormat::Tsv);
        print_question_add_result(&outcomes, &[], OutputFormat::Text);
    }

    #[test]
    fn page_add_print_smoke() {
        let resp = HiiPageAddResponse {
            form_id: 10021,
            slot: 1,
            page_offset: 0x178,
            title_string_id: 0x1A7,
        };
        print_page_add(&resp, OutputFormat::Json);
        print_page_add(&resp, OutputFormat::Text);
        print_page_add(&resp, OutputFormat::Tsv);
    }

    fn mock_question() -> QuestionInfo {
        QuestionInfo {
            form_id: 10029,
            question_id: 0x3B,
            kind: "one_of".into(),
            var_store_id: 1,
            varstore: Some(uefi_proto::VarStoreInfo {
                id: 1,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x72,
                name: "Setup".into(),
            }),
            var_offset: 0x3A,
            width: 1,
            min: 0,
            max: 0,
            step: 0,
            options: vec![
                uefi_proto::OptionEntry {
                    string_id: 4,
                    value: 0,
                    flags: 0x30,
                    ..Default::default()
                },
                uefi_proto::OptionEntry {
                    string_id: 3,
                    value: 1,
                    flags: 0x00,
                    ..Default::default()
                },
            ],
            defaults: vec![],
        }
    }

    #[test]
    fn question_info_print_smoke() {
        let q = mock_question();
        print_question_info(&q, OutputFormat::Json);
        print_question_info(&q, OutputFormat::Tsv);
        print_question_info(&q, OutputFormat::Text);
    }

    #[test]
    fn question_info_no_options_print_smoke() {
        let mut q = mock_question();
        q.options.clear();
        q.varstore = None;
        print_question_info(&q, OutputFormat::Text);
        print_question_info(&q, OutputFormat::Tsv);
    }

    #[test]
    fn question_info_text_prints_defaults_and_varstore() {
        let q = QuestionInfo {
            form_id: 10029,
            question_id: 0x3B,
            kind: "one_of".into(),
            var_store_id: 1,
            varstore: Some(VarStoreInfo {
                id: 1,
                guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                size: 0x72,
                name: "Setup".into(),
            }),
            var_offset: 0x3A,
            width: 1,
            min: 0,
            max: 0,
            step: 0,
            options: vec![OptionEntry {
                string_id: 3,
                value: 1,
                flags: 0x00,
                text: "Enabled".into(),
            }],
            defaults: vec![DefaultEntry {
                default_id: 0,
                r#type: 0,
                value: 1,
            }],
        };
        let text = question_info_text(&q);
        assert!(
            text.contains("varstore Setup (EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9) id 1 size 0x72")
        );
        assert!(text.contains("value = 1 \"Enabled\" (string 3, flags 0x0)"));
        assert!(
            !text.contains("value = 1 (string 3"),
            "резолвленная опция печатается с текстом, не только со string id"
        );
        assert!(text.contains("default = 1 (id 0, type 0)"));
    }

    #[test]
    fn question_info_text_keeps_sid_fallback_when_text_empty() {
        let q = QuestionInfo {
            form_id: 10029,
            question_id: 0x3B,
            kind: "one_of".into(),
            var_store_id: 1,
            varstore: None,
            var_offset: 0x3A,
            width: 1,
            min: 0,
            max: 0,
            step: 0,
            options: vec![OptionEntry {
                string_id: 9,
                value: 2,
                flags: 0x00,
                ..Default::default()
            }],
            defaults: vec![],
        };
        let text = question_info_text(&q);
        assert!(text.contains("value = 2 (string 9, flags 0x0)"));
    }

    #[test]
    fn set_value_print_smoke() {
        let q = mock_question();
        let applied = vec!["file …raw body store+0x62: 00 -> 01".to_string()];
        let stores = vec!["mock store".to_string()];
        print_set_value(&q, &applied, &stores, OutputFormat::Json);
        print_set_value(&q, &applied, &stores, OutputFormat::Tsv);
        print_set_value(&q, &applied, &stores, OutputFormat::Text);
    }

    #[test]
    fn node_serializes_type_field() {
        let it = Node {
            path: "0".into(),
            r#type: 65,
            subtype: 0,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: String::new(),
            action: 0,
            region: String::new(),
        };
        let j = serde_json::to_string(&it).unwrap();
        assert!(
            j.contains("\"type\":65"),
            "expected `\"type\"` field, got: {j}"
        );
    }
}
