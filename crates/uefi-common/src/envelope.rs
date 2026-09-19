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
        None => return Err(EnvelopeError::MissingFormsetBody),
    };
    Ok(Envelope { meta, refs, body })
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
}
