use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lossy: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceMeta {
    pub formset_guid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefsSection {
    pub parent_form_id: u16,
    #[serde(default)]
    pub entries: Vec<RefEntry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_id: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formset_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_id: Option<u16>,
}

pub struct Envelope {
    pub meta: Meta,
    pub refs: Option<RefsSection>,
    pub body: String,
}

/// Спека hii-form-export §1: bare-файл проходит насквозь байт-в-байт;
/// конверт режется на типизированную мету/refs и bare-тело для RPC.
/// Refs-only пакет (Task 7): объект верхнего уровня с ключом `refs`
/// (значение-объект) без ключа `formset`, `meta` опциональна — все entries
/// обязаны нести явный `form_id` (спека §1/§5, invalid package до RPC);
/// `body` такого пакета — пустая строка (мутации формсета нет). Массивный
/// np_ref-wire `{"refs":[…]}` — по-прежнему bare для question add.
pub fn split_envelope(text: &str) -> Result<Envelope, EnvelopeError> {
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?;
    let Some(obj) = v.as_object() else {
        return Ok(Envelope {
            meta: Meta::default(),
            refs: None,
            body: text.to_string(),
        });
    };
    if !obj.contains_key("meta") {
        if let Some(raw) = obj.get("refs").filter(|r| r.is_object()) {
            return refs_only(Meta::default(), raw.clone());
        }
        return Ok(Envelope {
            meta: Meta::default(),
            refs: None,
            body: text.to_string(),
        });
    }
    let meta: Meta = serde_json::from_value(obj["meta"].clone())
        .map_err(|e| EnvelopeError::InvalidMeta(e.to_string()))?;
    let refs = match obj.get("refs") {
        Some(r) => Some(
            serde_json::from_value(r.clone())
                .map_err(|e| EnvelopeError::InvalidRefs(e.to_string()))?,
        ),
        None => None,
    };
    let body = match obj.get("formset") {
        Some(b) => {
            serde_json::to_string(b).map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?
        }
        None => {
            return match refs {
                Some(_) => refs_only(meta, obj["refs"].clone()),
                None => Err(EnvelopeError::MissingFormsetBody),
            };
        }
    };
    Ok(Envelope { meta, refs, body })
}

fn refs_only(meta: Meta, raw: serde_json::Value) -> Result<Envelope, EnvelopeError> {
    let refs: RefsSection =
        serde_json::from_value(raw).map_err(|e| EnvelopeError::InvalidRefs(e.to_string()))?;
    if refs.entries.is_empty() || refs.entries.iter().any(|e| e.form_id.is_none()) {
        return Err(EnvelopeError::InvalidRefs(
            "refs-only package entries must be non-empty with explicit form_id".to_string(),
        ));
    }
    Ok(Envelope {
        meta,
        refs: Some(refs),
        body: String::new(),
    })
}

/// Сборка конверта на экспорте. Body — уже сериализованная FormSetSchema.
pub fn wrap_export(
    body: &str,
    source: Option<SourceMeta>,
    refs: Option<RefsSection>,
    lossy: Vec<String>,
) -> String {
    let mut obj = serde_json::json!({ "formset": serde_json::from_str::<serde_json::Value>(body).expect("body is valid json") });
    let meta = Meta { source, lossy };
    obj["meta"] = serde_json::to_value(&meta).expect("meta serializable");
    if let Some(r) = refs {
        obj["refs"] = serde_json::to_value(&r).expect("refs serializable");
    }
    serde_json::to_string_pretty(&obj).expect("envelope serializable")
}

/// Спека §3: резолв refs-секций в wire-формат question add.
/// Entry с явным form_id — как есть; без — из inserted_form_ids[0].
/// Дефолты: prompt=help=title формы (reach-in forms[0].title), qid=max(0x7F00, max(busy)+1).
pub fn plan_ref_step(
    refs: &RefsSection,
    body: Option<&str>,
    inserted: &[u32],
    busy_qids: &[u16],
) -> Result<Option<RefStepPlan>, EnvelopeError> {
    let mut records = Vec::new();
    let entries: Vec<RefEntry> = if refs.entries.is_empty() {
        vec![RefEntry::default()]
    } else {
        refs.entries.clone()
    };
    let default_title = body.and_then(body_form_title).unwrap_or_default();
    for e in entries {
        let form_id = match e.form_id {
            Some(id) => id,
            None => {
                let Some(first) = inserted.first() else {
                    return Err(EnvelopeError::FormNotInserted);
                };
                *first as u16
            }
        };
        let fallback = if default_title.is_empty() {
            format!("Form {form_id}")
        } else {
            default_title.clone()
        };
        let qid = e.question_id.unwrap_or_else(|| next_qid(busy_qids));
        records.push(RefRecord {
            form_id,
            prompt: e.prompt.unwrap_or_else(|| fallback.clone()),
            help: e.help.unwrap_or(fallback),
            question_id: qid,
            formset_guid: e.formset_guid,
        });
    }
    Ok(Some(RefStepPlan {
        parent_form_id: refs.parent_form_id,
        records,
    }))
}

/// Мягкий reach-in заголовка формы (спека §3); None → планировщик подставит `Form <id>`.
pub fn body_form_title(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("forms")?
        .get(0)?
        .get("title")?
        .as_str()
        .map(String::from)
}

fn next_qid(busy: &[u16]) -> u16 {
    busy.iter()
        .copied()
        .max()
        .map_or(0x7F00, |m| m.saturating_add(1).max(0x7F00))
}

#[derive(Debug, Clone, Serialize)]
pub struct RefRecord {
    pub form_id: u16,
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub formset_guid: Option<String>,
}

pub struct RefStepPlan {
    pub parent_form_id: u16,
    pub records: Vec<RefRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarstoreBrief {
    pub id: u16,
    pub guid: String,
    pub size: u16,
    pub name: String,
}

/// Спека §3.6 / varstore-contract §6. Возвращает bare-тело с отфильтрованными varstores.
pub fn plan_varstores(body: &str, target: &[VarstoreBrief]) -> Result<String, EnvelopeError> {
    let mut v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?;
    let Some(declared) = v.get_mut("varstores").and_then(|x| x.as_array_mut()) else {
        return Ok(body.to_string());
    };
    declared.retain(|d| {
        let id = d["id"].as_u64().unwrap_or(0) as u16;
        let guid = d["guid"].as_str().unwrap_or("").to_ascii_uppercase();
        let size = d["size"].as_u64().unwrap_or(0) as u16;
        let name = d["name"].as_str().unwrap_or("");
        match target.iter().find(|t| t.id == id) {
            None => true,
            Some(t) => !(t.guid.to_ascii_uppercase() == guid && t.size == size && t.name == name),
        }
    });
    for d in v["varstores"].as_array().unwrap() {
        let id = d["id"].as_u64().unwrap_or(0) as u16;
        let guid = d["guid"].as_str().unwrap_or("").to_ascii_uppercase();
        let size = d["size"].as_u64().unwrap_or(0) as u16;
        let name = d["name"].as_str().unwrap_or("");
        if let Some(t) = target.iter().find(|t| t.id == id) {
            let identical = t.guid.to_ascii_uppercase() == guid && t.size == size && t.name == name;
            if !identical {
                return Err(EnvelopeError::VarstoreConflict { id });
            }
        }
    }
    Ok(serde_json::to_string(&v).expect("body re-serializable"))
}

#[derive(Debug, thiserror::Error)]
pub enum EnvelopeError {
    #[error("invalid json: {0}")]
    InvalidJson(String),
    #[error("invalid meta: {0}")]
    InvalidMeta(String),
    #[error("invalid refs section: {0}")]
    InvalidRefs(String),
    #[error("envelope with refs entries has no formset body")]
    MissingFormsetBody,
    #[error("refs entries reference the inserted form, but no form was inserted")]
    FormNotInserted,
    #[error("varstore id {id:#x} already exists with different definition")]
    VarstoreConflict { id: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;

    const BARE: &str = r#"{"formset_guid":"G","title":"T","help":"","varstores":[],"default_stores":[],"forms":[]}"#;

    #[test]
    fn bare_passthrough_byte_identical() {
        let e = split_envelope(BARE).unwrap();
        assert_eq!(e.body, BARE);
        assert!(e.refs.is_none());
        assert!(e.meta.source.is_none());
    }

    #[test]
    fn np_ref_json_is_bare() {
        let np = r#"{"refs":[{"form_id":10101,"prompt":"p","help":"h","question_id":528}]}"#;
        let e = split_envelope(np).unwrap();
        assert_eq!(e.body, np);
    }

    #[test]
    fn envelope_splits_meta_refs_body() {
        let text = r#"{"meta":{"source":{"formset_guid":"G"},"lossy":["suppress_if:2"]},"formset":{"forms":[{"id":7}]},"refs":{"parent_form_id":10001,"entries":[{"prompt":"P","help":"H"}]}}"#;
        let e = split_envelope(text).unwrap();
        assert_eq!(e.meta.source.unwrap().formset_guid, "G");
        assert_eq!(e.meta.lossy, vec!["suppress_if:2".to_string()]);
        let r = e.refs.unwrap();
        assert_eq!(r.parent_form_id, 10001);
        assert_eq!(r.entries.len(), 1);
        assert!(r.entries[0].form_id.is_none());
        assert!(e.body.contains(r#""forms":[{"id":7}]"#));
    }

    #[test]
    fn unknown_meta_keys_ignored() {
        let text = r#"{"meta":{"future_field":1},"formset":{}}"#;
        assert!(split_envelope(text).is_ok());
    }

    #[test]
    fn refs_without_parent_rejected() {
        let text = r#"{"meta":{},"formset":{},"refs":{"entries":[]}}"#;
        assert!(matches!(
            split_envelope(text),
            Err(EnvelopeError::InvalidRefs(_))
        ));
    }

    #[test]
    fn meta_without_formset_is_error_only_when_refs_present() {
        let text = r#"{"meta":{"lossy":[]}}"#;
        assert!(matches!(
            split_envelope(text),
            Err(EnvelopeError::MissingFormsetBody)
        ));
    }

    #[test]
    fn wrap_export_roundtrips_through_split() {
        let body = r#"{"formset_guid":"G"}"#;
        let text = wrap_export(
            body,
            Some(SourceMeta {
                formset_guid: "G".into(),
            }),
            Some(RefsSection {
                parent_form_id: 9,
                entries: vec![],
            }),
            vec!["cross_formset_ref:1".into()],
        );
        let e = split_envelope(&text).unwrap();
        assert_eq!(e.meta.source.unwrap().formset_guid, "G");
        assert_eq!(e.refs.unwrap().parent_form_id, 9);
        assert_eq!(e.meta.lossy.len(), 1);
    }

    #[test]
    fn plan_empty_entries_synthesizes_from_inserted() {
        let refs = RefsSection {
            parent_form_id: 10001,
            entries: vec![],
        };
        let body = r#"{"forms":[{"title":"Serial"}]}"#;
        let plan = plan_ref_step(&refs, Some(body), &[10077], &[5])
            .unwrap()
            .unwrap();
        assert_eq!(plan.records.len(), 1);
        assert_eq!(plan.records[0].form_id, 10077);
        assert_eq!(plan.records[0].prompt, "Serial");
        assert_eq!(plan.records[0].help, "Serial");
        assert_eq!(plan.records[0].question_id, 0x7F00);
    }

    #[test]
    fn plan_busy_qids_push_high() {
        let refs = RefsSection {
            parent_form_id: 1,
            entries: vec![],
        };
        let plan = plan_ref_step(&refs, None, &[2], &[0x7F10])
            .unwrap()
            .unwrap();
        assert_eq!(plan.records[0].question_id, 0x7F11);
    }

    #[test]
    fn plan_explicit_target_passthrough_with_override() {
        let refs = RefsSection {
            parent_form_id: 5,
            entries: vec![RefEntry {
                form_id: Some(5002),
                formset_guid: Some("EC87D643-0000-0000-0000-000000000000".into()),
                prompt: Some("IntelRC".into()),
                help: Some("rc setup".into()),
                question_id: Some(528),
            }],
        };
        let plan = plan_ref_step(&refs, None, &[], &[528]).unwrap().unwrap();
        assert_eq!(plan.records[0].form_id, 5002);
        assert_eq!(plan.records[0].prompt, "IntelRC");
        assert_eq!(plan.records[0].question_id, 528);
    }

    #[test]
    fn plan_missing_inserted_is_error() {
        let refs = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry::default()],
        };
        assert!(matches!(
            plan_ref_step(&refs, Some(r#"{"forms":[]}"#), &[], &[]),
            Err(EnvelopeError::FormNotInserted)
        ));
    }

    #[test]
    fn plan_title_unavailable_falls_back_to_form_id() {
        let refs = RefsSection {
            parent_form_id: 1,
            entries: vec![],
        };
        let plan = plan_ref_step(&refs, Some(r#"{"forms":[]}"#), &[9], &[])
            .unwrap()
            .unwrap();
        assert_eq!(plan.records[0].prompt, "Form 9");
    }

    #[test]
    fn plan_varstores_keeps_free_drops_identical_rejects_different() {
        let body = r#"{"varstores":[{"id":21,"guid":"A","size":8,"name":"V1"},{"id":1,"guid":"EC87D643-","size":114,"name":"Setup"}],"forms":[]}"#;
        let target = vec![VarstoreBrief {
            id: 1,
            guid: "ec87d643-".into(),
            size: 0x72,
            name: "Setup".into(),
        }];
        let out = plan_varstores(body, &target).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let ids: Vec<u64> = v["varstores"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_u64().unwrap())
            .collect();
        assert!(ids.contains(&21));
        assert!(!ids.contains(&1));

        let conflict = vec![VarstoreBrief {
            id: 1,
            guid: "OTHER".into(),
            size: 4,
            name: "Setup".into(),
        }];
        assert!(matches!(
            plan_varstores(body, &conflict),
            Err(EnvelopeError::VarstoreConflict { id: 1 })
        ));
    }

    #[test]
    fn plan_varstores_no_varstores_passthrough() {
        let body = r#"{"forms":[]}"#;
        assert_eq!(plan_varstores(body, &[]).unwrap(), body);
    }

    #[test]
    fn refs_only_bare_object_is_refs_package() {
        let text = r#"{"refs":{"parent_form_id":10001,"entries":[{"form_id":5002,"prompt":"P","help":"H"}]}}"#;
        let e = split_envelope(text).unwrap();
        assert!(e.body.is_empty());
        let r = e.refs.unwrap();
        assert_eq!(r.parent_form_id, 10001);
        assert_eq!(r.entries.len(), 1);
        assert_eq!(r.entries[0].form_id, Some(5002));
    }

    #[test]
    fn refs_only_with_meta_and_without_formset() {
        let text = r#"{"meta":{"source":{"formset_guid":"G"}},"refs":{"parent_form_id":9,"entries":[{"form_id":1}]}}"#;
        let e = split_envelope(text).unwrap();
        assert!(e.body.is_empty());
        assert_eq!(e.refs.unwrap().parent_form_id, 9);
        assert_eq!(e.meta.source.unwrap().formset_guid, "G");
    }

    #[test]
    fn refs_only_entry_without_form_id_rejected() {
        let no_id = r#"{"refs":{"parent_form_id":1,"entries":[{"prompt":"p"}]}}"#;
        assert!(matches!(
            split_envelope(no_id),
            Err(EnvelopeError::InvalidRefs(_))
        ));
        let empty = r#"{"refs":{"parent_form_id":1,"entries":[]}}"#;
        assert!(matches!(
            split_envelope(empty),
            Err(EnvelopeError::InvalidRefs(_))
        ));
        let absent = r#"{"refs":{"parent_form_id":1}}"#;
        assert!(matches!(
            split_envelope(absent),
            Err(EnvelopeError::InvalidRefs(_))
        ));
    }

    #[test]
    fn refs_only_np_wire_array_still_bare() {
        let np = r#"{"refs":[{"form_id":10101,"prompt":"p","help":"h","question_id":528}]}"#;
        let e = split_envelope(np).unwrap();
        assert_eq!(e.body, np);
        assert!(e.refs.is_none());
    }
}
