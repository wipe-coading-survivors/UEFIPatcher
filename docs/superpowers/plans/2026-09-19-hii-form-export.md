# hii-form-export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Round-trip JSON-экспорт HII-форм (`hii form export` → конверт `{"meta","formset","refs"}`) + макро `hii import` (агрегирует `form add` + `question add`, включая refs-only пакеты) + TUI-клавиши `e`/`I`/`R`/`A`.

**Architecture:** Экспорт — движковый walker-модуль `hii/form_export.rs` (полная fidelity `ItemSchema` + varstore-заполнение по контракту varstore-contract §6 + `parent_form_id` из ref-рёбер) за новым RPC `HiiFormExport`. Конверт и планировщики — чистый client-слой `uefi-common::envelope` (движок мету не видит). `hii import` (CLI+TUI) исполняет pre-check → form add → question add; конверт-файлы в `hii form add` отвергаются. Тесты: TDD на каждом слое, паритет CLI/TUI — равенством RPC-журналов, real-image гейт на HNX99TF.

**Tech Stack:** Rust workspace (edition 2024), serde/serde_json, tonic/uefi-proto, ratatui/crossterm (TUI), clap (CLI).

**Spec:** `docs/superpowers/specs/2026-09-19-hii-form-export-design.md` — план спорит от спеки, исполнитель читает обе. Смежная: `2026-09-19-varstore-contract-design.md` §6 (контракт varstore-правил).

## Global Constraints

- Спека — источник истины; при расхождении плана с реальностью — коммит `docs: fix Task N in cycle hii-form-export plan (...)` ДО реализации (AGENTS.md правило 11).
- **Module-first rule:** `pub mod X;` в `lib.rs`/`main.rs` в ТОМ ЖЕ шаге, ДО `cargo test`.
- Без narration-комментариев в коде; разрешены короткие rustdoc `///` на pub-функциях (контракт + ссылка на спеку).
- TDD: failing test → реализация → pass → commit. После каждой задачи: `cargo test -p <crate>` и `cargo clippy -p <crate> -- -D warnings`.
- Мета — Rust-only (`uefi-common`), движок её не читает/не пишет ни в одном пути.
- Никаких плейсхолдерных id новой формы в конверте (спека §1); `question_id`-базис `0x7F00` вычисляется планировщиком.
- Bare-файлы (без ключа `meta`) проходят все пути байт-в-байт как сегодня.
- Формат item_id форм — тот же, что у `HiiSetFormVisibilityRequest.item_id` (строка `FormInfo.form_id`).
- Реальные образцы: HNX `refs/fw/HNX99TF_200525_original_E5C88C6F.bin`, таргет корневого Setup `899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0` (varstore-карта: Setup id 1, GUID EC87D643…, size 0x72).

## Файлы плана

| Файл | Назначение |
|---|---|
| `crates/uefi-common/src/envelope.rs` | конверт: типы, `split_envelope`, `wrap_export`, `plan_ref_step`, `plan_varstores` |
| `crates/uefi-engine/src/hii/form_export.rs` | walker-экспорт: `export_form` |
| `crates/uefi-proto/proto/engine.proto` | RPC `HiiFormExport` + сообщения |
| `crates/uefi-engine/src/rpc/server.rs` | хендлер `hii_form_export` |
| `crates/uefi-cli/src/client.rs`, `commands/hii.rs`, `main.rs`, `output.rs` | `hii form export`, `hii import` |
| `crates/uefi-tui/src/commands.rs`, `main.rs` | `:hii form export`, `:hii import`, клавиши, completion |
| `crates/uefi-cli/tests/mock_server.rs`, `crates/uefi-tui/tests/mock_server.rs` | мок `HiiFormExport` + журнал вызовов |

---

### Task 1: uefi-common::envelope — типы, split_envelope, wrap_export

**Files:**
- Modify: `crates/uefi-common/Cargo.toml` (добавить `serde_json.workspace = true`)
- Modify: `crates/uefi-common/src/lib.rs` (добавить `pub mod envelope;`)
- Create: `crates/uefi-common/src/envelope.rs`

**Interfaces:**
- Produces (для Tasks 2, 6–9):
  - `pub struct Meta { pub source: Option<SourceMeta>, pub lossy: Vec<String> }`
  - `pub struct SourceMeta { pub formset_guid: String }`
  - `pub struct RefsSection { pub parent_form_id: u16, pub entries: Vec<RefEntry> }`
  - `pub struct RefEntry { pub form_id: Option<u16>, pub formset_guid: Option<String>, pub prompt: Option<String>, pub help: Option<String>, pub question_id: Option<u16> }`
  - `pub struct Envelope { pub meta: Meta, pub refs: Option<RefsSection>, pub body: String }`
  - `pub fn split_envelope(text: &str) -> Result<Envelope, EnvelopeError>`
  - `pub fn wrap_export(body: &str, source: Option<SourceMeta>, refs: Option<RefsSection>, lossy: Vec<String>) -> String`
  - `pub enum EnvelopeError { InvalidJson(String), MissingFormsetBody, … }` (thiserror)

- [ ] **Step 1: Добавить зависимость и модуль**

`crates/uefi-common/Cargo.toml` `[dependencies]`:
```toml
serde_json.workspace = true
```
`crates/uefi-common/src/lib.rs`: `pub mod envelope;` (module-first, до тестов).

- [ ] **Step 2: Написать failing tests**

`crates/uefi-common/src/envelope.rs`:
```rust
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
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?;
    let Some(obj) = v.as_object() else {
        return Ok(Envelope { meta: Meta::default(), refs: None, body: text.to_string() });
    };
    if !obj.contains_key("meta") {
        return Ok(Envelope { meta: Meta::default(), refs: None, body: text.to_string() });
    }
    let meta: Meta = serde_json::from_value(obj["meta"].clone())
        .map_err(|e| EnvelopeError::InvalidMeta(e.to_string()))?;
    let refs = match obj.get("refs") {
        Some(r) => Some(serde_json::from_value(r.clone())
            .map_err(|e| EnvelopeError::InvalidRefs(e.to_string()))?),
        None => None,
    };
    let body = match obj.get("formset") {
        Some(b) => serde_json::to_string(b)
            .map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?,
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
```
Плюс `#[cfg(test)] mod tests` (в том же файле):
```rust
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
        assert!(matches!(split_envelope(text), Err(EnvelopeError::InvalidRefs(_))));
    }

    #[test]
    fn meta_without_formset_is_error_only_when_refs_present() {
        let text = r#"{"meta":{"lossy":[]}}"#;
        assert!(matches!(split_envelope(text), Err(EnvelopeError::MissingFormsetBody)));
    }

    #[test]
    fn wrap_export_roundtrips_through_split() {
        let body = r#"{"formset_guid":"G"}"#;
        let text = wrap_export(
            body,
            Some(SourceMeta { formset_guid: "G".into() }),
            Some(RefsSection { parent_form_id: 9, entries: vec![] }),
            vec!["cross_formset_ref:1".into()],
        );
        let e = split_envelope(&text).unwrap();
        assert_eq!(e.meta.source.unwrap().formset_guid, "G");
        assert_eq!(e.refs.unwrap().parent_form_id, 9);
        assert_eq!(e.meta.lossy.len(), 1);
    }
}
```
Примечание к тесту `meta_without_formset…`: конверт с `meta` обязан нести `formset` ИЛИ быть refs-only — refs-only не содержит `meta` вообще (это bare для question add), поэтому `meta` без `formset` — ошибка.

- [ ] **Step 3: Запустить тесты (упадут — нет файла/модуля, если писать тесты до impl — стандартный цикл уже соблюдён порядком выше; при написании файла целиком убедиться, что `cargo test -p uefi-common envelope` проходит)**

Run: `cargo test -p uefi-common envelope`
Expected: PASS (7 тестов)

- [ ] **Step 4: clippy**

Run: `cargo clippy -p uefi-common -- -D warnings`
Expected: без warnings

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-common/
git commit -m "feat(common): envelope module — meta/refs/formset split + wrap_export (спека hii-form-export §1)"
```

---

### Task 2: uefi-common::envelope — plan_ref_step + plan_varstores

**Files:**
- Modify: `crates/uefi-common/src/envelope.rs`

**Interfaces:**
- Consumes: `RefsSection`/`RefEntry` (Task 1)
- Produces (для Tasks 7–9):
  - `pub struct RefRecord { pub form_id: u16, pub prompt: String, pub help: String, pub question_id: u16, pub formset_guid: Option<String> }` — wire-формат `parse_question_add_schema`
  - `pub struct RefStepPlan { pub parent_form_id: u16, pub records: Vec<RefRecord> }`
  - `pub fn plan_ref_step(refs: &RefsSection, body: Option<&str>, inserted: &[u32], busy_qids: &[u16]) -> Result<Option<RefStepPlan>, EnvelopeError>`
  - `pub struct VarstoreBrief { pub id: u16, pub guid: String, pub size: u16, pub name: String }`
  - `pub fn plan_varstores(body: &str, target: &[VarstoreBrief]) -> Result<String, EnvelopeError>` — возвращает отфильтрованное bare-тело

- [ ] **Step 1: Failing tests + реализация plan_ref_step**

Добавить в `envelope.rs`:
```rust
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
    Ok(Some(RefStepPlan { parent_form_id: refs.parent_form_id, records }))
}

/// Мягкий reach-in заголовка формы (спека §3); None → планировщик подставит `Form <id>`.
pub fn body_form_title(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("forms")?.get(0)?.get("title")?.as_str().map(String::from)
}

fn next_qid(busy: &[u16]) -> u16 {
    busy.iter().copied().max().map_or(0x7F00, |m| m.saturating_add(1).max(0x7F00))
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
```
Расширить `EnvelopeError`:
```rust
    #[error("refs entries reference the inserted form, but no form was inserted")]
    FormNotInserted,
```
Тесты:
```rust
    #[test]
    fn plan_empty_entries_synthesizes_from_inserted() {
        let refs = RefsSection { parent_form_id: 10001, entries: vec![] };
        let body = r#"{"forms":[{"title":"Serial"}]}"#;
        let plan = plan_ref_step(&refs, Some(body), &[10077], &[5]).unwrap().unwrap();
        assert_eq!(plan.records.len(), 1);
        assert_eq!(plan.records[0].form_id, 10077);
        assert_eq!(plan.records[0].prompt, "Serial");
        assert_eq!(plan.records[0].help, "Serial");
        assert_eq!(plan.records[0].question_id, 0x7F00);
    }

    #[test]
    fn plan_busy_qids_push_high() {
        let refs = RefsSection { parent_form_id: 1, entries: vec![] };
        let plan = plan_ref_step(&refs, None, &[2], &[0x7F10]).unwrap().unwrap();
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
        let refs = RefsSection { parent_form_id: 1, entries: vec![RefEntry::default()] };
        assert!(matches!(
            plan_ref_step(&refs, Some(r#"{"forms":[]}"#), &[], &[]),
            Err(EnvelopeError::FormNotInserted)
        ));
    }

    #[test]
    fn plan_title_unavailable_falls_back_to_form_id() {
        let refs = RefsSection { parent_form_id: 1, entries: vec![] };
        let plan = plan_ref_step(&refs, Some(r#"{"forms":[]}"#), &[9], &[]).unwrap().unwrap();
        assert_eq!(plan.records[0].prompt, "Form 9");
    }
```

- [ ] **Step 2: Запустить**

Run: `cargo test -p uefi-common envelope`
Expected: PASS

- [ ] **Step 3: Failing tests + реализация plan_varstores**

Контракт varstore-contract §6: id свободен → декларация остаётся; занят и guid+size+name идентичны → drop (round-trip в тот же формсет); занят и отличается → ошибка.
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarstoreBrief {
    pub id: u16,
    pub guid: String,
    pub size: u16,
    pub name: String,
}

/// Спека §3.6 / varstore-contract §6. Возвращает bare-тело с отфильтрованными varstores.
pub fn plan_varstores(body: &str, target: &[VarstoreBrief]) -> Result<String, EnvelopeError> {
    let mut v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| EnvelopeError::InvalidJson(e.to_string()))?;
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
            Some(t) => {
                if t.guid.to_ascii_uppercase() == guid && t.size == size && t.name == name {
                    false
                } else {
                    // отличающееся определение — ошибка ниже; retain не умеет Err,
                    // поэтому проверку делаем вторым проходом
                    true
                }
            }
        }
    });
    // второй проход: конфликт определений
    for d in v["varstores"].as_array().unwrap() {
        let id = d["id"].as_u64().unwrap_or(0) as u16;
        let guid = d["guid"].as_str().unwrap_or("").to_ascii_uppercase();
        let size = d["size"].as_u64().unwrap_or(0) as u16;
        let name = d["name"].as_str().unwrap_or("");
        if let Some(t) = target.iter().find(|t| t.id == id) {
            let identical = t.guid.to_ascii_uppercase() == guid
                && t.size == size
                && t.name == name;
            if !identical {
                return Err(EnvelopeError::VarstoreConflict { id });
            }
        }
    }
    Ok(serde_json::to_string(&v).expect("body re-serializable"))
}
```
Расширить `EnvelopeError`:
```rust
    #[error("varstore id {id:#x} already exists with different definition")]
    VarstoreConflict { id: u16 },
```
Тесты:
```rust
    #[test]
    fn plan_varstores_keeps_free_drops_identical_rejects_different() {
        let body = r#"{"varstores":[{"id":21,"guid":"A","size":8,"name":"V1"},{"id":1,"guid":"EC87D643-","size":114,"name":"Setup"}],"forms":[]}"#;
        let target = vec![VarstoreBrief { id: 1, guid: "ec87d643-".into(), size: 0x72, name: "Setup".into() }];
        let out = plan_varstores(body, &target).unwrap();
        assert!(out.contains(r#""id":21"#));
        assert!(!out.contains(r#""id":1,"guid":"EC87D643-""#));

        let conflict = vec![VarstoreBrief { id: 1, guid: "OTHER".into(), size: 4, name: "Setup".into() }];
        assert!(matches!(plan_varstores(body, &conflict), Err(EnvelopeError::VarstoreConflict { id: 1 })));
    }

    #[test]
    fn plan_varstores_no_varstores_passthrough() {
        let body = r#"{"forms":[]}"#;
        assert_eq!(plan_varstores(body, &[]).unwrap(), body);
    }
```
Примечание: тест `keeps_free…` рассчитан на pretty/compact-сериализацию `serde_json::to_string` (compact, без пробелов); при расхождении формата — фиксировать ассерты на `serde_json::from_str::<Value>`-сравнении, не на подстроках (см. docs:fix при необходимости).

- [ ] **Step 4: Запустить + clippy**

Run: `cargo test -p uefi-common envelope && cargo clippy -p uefi-common -- -D warnings`
Expected: PASS

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-common/src/envelope.rs
git commit -m "feat(common): envelope planners — plan_ref_step (дефолты/явные таргеты) + plan_varstores (контракт varstore-contract §6)"
```

---

### Task 3: engine — form_export.rs: вопросы полной fidelity

**Files:**
- Create: `crates/uefi-engine/src/hii/form_export.rs`
- Modify: `crates/uefi-engine/src/hii/mod.rs` (`pub mod form_export;`)

**Interfaces:**
- Consumes: `values::{walk_statements, scan_options, varstore_map, QuestionKind}`, `questions::prompt_texts`, `schema::*`, `ifr::parse_form_package`, `crate::types::{FfsNode, Guid}`
- Produces (для Tasks 4–7):
  - `pub struct FormExport { pub schema: schema::FormSetSchema, pub formset_guid: String, pub parent_form_id: u32, pub lossy: Vec<String> }`
  - `pub(crate) fn question_items(pkg: &[u8], form_id: u16, texts: &HashMap<u16, String>) -> Vec<schema::ItemSchema>`
  - `pub(crate) fn display_mode(flags: u8, size: u8) -> schema::DisplayMode`

- [ ] **Step 1: Каркас + резолв item_id**

`item_id` — грамматика `set_item_visibility` для форм-таргетов: `<target>#<form_id>` (form_id — десятичный `FormInfo.form_id_ifr`, как строит TUI `fmt_item` `{}#{}`); вопрос-суффикс `:qid` и голый `<target>` без `#` → `HiiError::NotFound` (экспорт требует конкретную форму). Резолв: `super::parse_item_id` (mod.rs) → `find_item_path`/`find_item` (как `form_add.rs:40-46`, но read-only, без writability-барьеров); тексты — `questions::prompt_texts(file)` по FFS-файлу (секция — `path[..len-1]`, образец `find_question_map`, mod.rs). Поиск form-пакета — готовый `super::form_package_ranges(node)` (mod.rs: RAW-тело = пакет, PE32 = все forms-пакеты ресурса); пакет выбирается тот, где `ifr::parse_form_package` видит форму `form_id`. formset_guid — `ifr::parse_form_package(pkg) → FormSetInfo.guid` → `crate::guid_to_upper_string`.
```rust
use std::collections::HashMap;

use super::{form_package_ranges, ifr, parse_item_id, schema, values, HiiError};
use crate::types::{FfsNode, FfsType, Image};

pub struct FormExport {
    pub schema: schema::FormSetSchema,
    pub formset_guid: String,
    pub parent_form_id: u32,
    pub lossy: Vec<String>,
}

/// Спека hii-form-export §2: экспорт формы в FormSetSchema-минимум.
/// item_id — `<target>#<form_id>` (грамматика HiiSetFormVisibility для форм).
pub fn export_form(image: &Image, item_id: &str) -> Result<FormExport, HiiError> {
    let (file, node, form_id) = resolve_form(image, item_id)?;
    let pkg = find_form_package(node, form_id).ok_or(HiiError::NotFound)?;
    let texts = super::questions::prompt_texts(file);
    let items = question_items(pkg, form_id, &texts);
    // Task 4 дополнит: text/action/ref-итемы, lossy, varstores, parent.
    let form = schema::FormSchema {
        id: form_id,
        title: form_title(pkg, form_id, &texts).unwrap_or_else(|| format!("Form {form_id}")),
        items,
    };
    let formset_guid = package_formset_guid(pkg);
    Ok(FormExport {
        schema: schema::FormSetSchema {
            formset_guid: formset_guid.clone(),
            title: String::new(),
            help: String::new(),
            class_guids: vec![],
            varstores: vec![],
            default_stores: vec![],
            forms: vec![form],
            setupdata_guid: None,
            amitse_guid: None,
        },
        formset_guid,
        parent_form_id: 0,
        lossy: vec![],
    })
}
```
`resolve_form`/`find_form_package`/`package_formset_guid`/`form_title` — приватные хелперы этого файла: `resolve_form(image, item_id) -> (file: &FfsNode, section: &FfsNode, form_id: u16)` (parse_item_id + find_item + Section-проверка как у list_varstores), `find_form_package(node, form_id) -> Option<&[u8]>` (form_package_ranges + parse_form_package), `package_formset_guid`/`form_title` — через `ifr::parse_form_package`.

- [ ] **Step 2: question_items — полная fidelity вопросов**

```rust
/// Вопросы формы → ItemSchema (help_sid +4/+5, display-флаги, options, defaults).
/// Display/size-флаги живут в собственном Flags-байте опкода (off+13,
/// EDK2 EFI_IFR_ONE_OF/EFI_IFR_NUMERIC = Question + UINT8 Flags +
/// MINMAXSTEP_DATA), НЕ в question-header Flags (off+12 — там
/// EFI_IFR_FLAG_*, где RESET_REQUIRED=0x10/REST_STYLE=0x20 коллидируют
/// с display-маской). len < 14 → флагов нет: display = UintDec, size = 0.
pub(crate) fn question_items(
    pkg: &[u8],
    form_id: u16,
    texts: &HashMap<u16, String>,
) -> Vec<schema::ItemSchema> {
    use r_efi::hii::{IFR_CHECKBOX_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP, IFR_NUMERIC_SIZE};
    let mut out = Vec::new();
    values::walk_statements(pkg, |op, off, len, current| {
        if current != Some(form_id) || len < 13 {
            return;
        }
        let prompt_sid = u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]);
        let help_sid = u16::from_le_bytes([pkg[off + 4], pkg[off + 5]]);
        let question_id = u16::from_le_bytes([pkg[off + 6], pkg[off + 7]]);
        let var_store_id = u16::from_le_bytes([pkg[off + 8], pkg[off + 9]]);
        let var_offset = u16::from_le_bytes([pkg[off + 10], pkg[off + 11]]);
        let get = |sid: u16| texts.get(&sid).cloned().unwrap_or_default();
        match op {
            IFR_ONE_OF_OP => {
                let mut options = Vec::new();
                let mut defaults = Vec::new();
                values::scan_options(pkg, off + len, &mut options, &mut defaults);
                let schema_options = options
                    .iter()
                    .map(|o| schema::OptionSchema {
                        text: get(o.string_id),
                        value: o.value,
                        default: if o.flags & r_efi::hii::IFR_OPTION_DEFAULT != 0 {
                            Some(schema::DefaultClass::Optimized)
                        } else if o.flags & r_efi::hii::IFR_OPTION_DEFAULT_MFG != 0 {
                            Some(schema::DefaultClass::Failsafe)
                        } else {
                            None
                        },
                    })
                    .collect();
                out.push(schema::ItemSchema::OneOf(schema::OneOfItem {
                    prompt: get(prompt_sid),
                    help: get(help_sid),
                    question_id,
                    var_store_id,
                    var_offset,
                    size: values::one_of_width(&options, &defaults),
                    display: if len >= 14 {
                        display_mode(pkg[off + 13], 0)
                    } else {
                        schema::DisplayMode::UintDec
                    },
                    options: schema_options,
                    defaults: map_defaults(&defaults),
                }));
            }
            IFR_NUMERIC_OP => {
                let size = if len >= 14 {
                    1u8 << (pkg[off + 13] & IFR_NUMERIC_SIZE)
                } else {
                    0
                };
                let (min, max, step) = numeric_min_max_step(pkg, off, len, size);
                out.push(schema::ItemSchema::Numeric(schema::NumericItem {
                    prompt: get(prompt_sid),
                    help: get(help_sid),
                    question_id,
                    var_store_id,
                    var_offset,
                    size,
                    min,
                    max,
                    step,
                    display: if len >= 14 {
                        display_mode(pkg[off + 13], size)
                    } else {
                        schema::DisplayMode::UintDec
                    },
                    defaults: schema::Defaults::default(),
                }));
            }
            IFR_CHECKBOX_OP => out.push(schema::ItemSchema::CheckBox(schema::CheckBoxItem {
                prompt: get(prompt_sid),
                help: get(help_sid),
                question_id,
                var_store_id,
                var_offset,
                defaults: schema::Defaults::default(),
            })),
            _ => {}
        }
    });
    out
}

/// IFR_DISPLAY-флаги (маска 0x30) → DisplayMode; r-efi/EDK2:
/// 0x00=IntDec, 0x10=UintDec, 0x20=UintHex; default = UintDec
/// (0x30 — только TIME/DATE). Спека hii-form-export §2.
pub(crate) fn display_mode(flags: u8, _size: u8) -> schema::DisplayMode {
    match flags & r_efi::hii::IFR_DISPLAY {
        r_efi::hii::IFR_DISPLAY_INT_DEC => schema::DisplayMode::IntDec,
        r_efi::hii::IFR_DISPLAY_UINT_HEX => schema::DisplayMode::UintHex,
        r_efi::hii::IFR_DISPLAY_UINT_DEC => schema::DisplayMode::UintDec,
        _ => schema::DisplayMode::UintDec,
    }
}

/// MINMAXSTEP_DATA-хвост Numeric: min/max/step по `size` байт на значение,
/// clamp к длине опкода (образец — values.rs::find_question; read_le_u64
/// приватен в values.rs — локальная копия в form_export.rs).
fn numeric_min_max_step(pkg: &[u8], off: usize, len: usize, size: u8) -> (u64, u64, u64) {
    let w = size as usize;
    let read = |pos: usize| {
        let start = off + 14 + pos * w;
        let avail = (off + len).saturating_sub(start);
        read_le_u64(pkg, start, w.min(avail))
    };
    (read(0), read(1), read(2))
}

fn map_defaults(entries: &[values::DefaultEntry]) -> schema::Defaults {
    let mut d = schema::Defaults::default();
    for e in entries {
        match e.default_id {
            1 => d.failsafe = Some(e.value),
            _ => d.optimized = d.optimized.or(Some(e.value)),
        }
    }
    d
}
```
Выверено по фактическому коду (бывшие docs:fix-кандидаты): поля `values::OptionEntry{string_id, flags, value}`/`DefaultEntry{default_id, type_, value}` — как в skeleton; default-флаги опций — `IFR_OPTION_DEFAULT=0x10`→Optimized, `IFR_OPTION_DEFAULT_MFG=0x20`→Failsafe (bit 0x1 — не default-флаг; round-trip с `build_question_ops`, mod.rs); `map_defaults`: default_id 1=manufacturing→failsafe, 0/прочие→optimized (EDK2 `EFI_HII_DEFAULT_CLASS_*`); `r_efi::hii::IFR_NUMERIC_SIZE` существует (маска 0x03, r-efi 7.0 hii.rs:776) — использование в skeleton верно; display-маска skeleton'а (0x10→IntDec) противоречила r-efi (`IFR_DISPLAY_UINT_DEC=0x10`) — исправлено выше.

- [ ] **Step 3: Синтетика-тесты (фикстуры в духе questions.rs:159-230)**

Тесты: (1) one_of с двумя опциями и текстами → `ItemSchema::OneOf` с options/defaults; (2) numeric с flags=0x20 → `UintHex`, size по IFR_NUMERIC_SIZE; (3) checkbox с help_sid → help-текст; (4) форма без вопросов → пустой items. Каждый — собрать IFR-байты хелперами тестов `questions.rs`, прогнать `question_items`.

- [ ] **Step 4: Запустить + clippy + коммит**

Run: `cargo test -p uefi-engine form_export && cargo clippy -p uefi-engine -- -D warnings`
```bash
git add crates/uefi-engine/src/hii/
git commit -m "feat(engine): form_export — вопросы полной fidelity (help_sid, display, options, defaults)"
```

---

### Task 4: engine — Text/Action/Ref-итемы, lossy, varstore-заполнение, parent, симметрия

**Files:**
- Modify: `crates/uefi-engine/src/hii/form_export.rs`

**Interfaces:**
- Consumes: Task 3; `values::varstore_map`; `ref_tree::collect_edges` (`ref_tree.rs:44`, возвращает `Vec<uefi_proto::FormEdge>` — engine-зависимость от proto уже есть)
- Produces: заполненные `parent_form_id`/`lossy`/`schema.varstores` в `FormExport`

- [ ] **Step 1: Text/Action/Ref + lossy-подсчёт в export_form**

В обход `walk_statements` добавить ветки: `IFR_TEXT_OP`/`IFR_SUBTITLE_OP` → `ItemSchema::Text` (TextItem = `{prompt, help, text_two}`; TEXT: prompt@+2/help@+4/text_two@+6, len≥8; SUBTITLE: prompt/help, len≥6, text_two=""). `IFR_ACTION_OP` → `Action` (question-header@+2..+13, config StringId@+13, len≥15). `IFR_REF_OP` → `ItemSchema::Ref` — вариант различает `ref_variant::parse_ref`: REF1 (len 15) → RefItem чисто; REF2 (len 17, FormQuestion) → RefItem, но schema не выражает target-question → lossy `ref_question_target:N`; REF3/REF4 (Formset, `target_formset_guid`) → **skip + lossy** `cross_formset_ref:N`; REF5 (len 13, Dynamic) целевого form_id не несёт вовсе (план ошибочно приписывал ему target_formset_guid) → **skip + lossy** `dynamic_ref:N`; неканоническая длина REF → `unknown_op_0f:N`. Length-гарды — по-веточные: ранний `len < 13` из Task 3 выкинул бы TEXT (len 8)/SUBTITLE (len 7).

Lossy-счётчики: walker-функция получает `lossy: &mut Vec<String>` (Task 3 `question_items` переименовывается в `collect_items`; consumer'ов вне файла нет, Task 5+ ходят через `export_form`). `walk_statements` НЕ визитирует `IFR_SUPPRESS_IF_OP`/`IFR_GRAY_OUT_IF_OP` (это не statement-опкоды — счётчики из замыкания недоступны) → walker расширяется: gate-опкоды визитируются с current-form охватывающей формы, БЕЗ expr_end-маркировки (все существующие потребители фильтруют по op/is_question_op — совместимо; тест в values.rs). Считаются только внутриформенные стейтменты (`current == Some(form_id)`); formset-уровневые gates (обёртки вокруг FORM, current=None) НЕ считаются. `IFR_ONE_OF_OPTION_OP`/`IFR_DEFAULT_OP` consumed one_of-веткой и `IFR_FORM_OP` — не считаются; прочие непокрытые (PASSWORD/ORDERED_LIST/STRING/DATE/TIME/…) — `unknown_op_<hex2>:N`.

- [ ] **Step 2: Varstore-заполнение (контракт varstore-contract §6)**

`VarStoreMap` (values.rs) несёт `{id, guid, size, name}` — БЕЗ типа и атрибутов (`is_name_value` из ранней редакции шага не существует). Расширение отдельным коммитом ДО реализации (санкционировано этим шагом): `kind: VarStoreKind {Buffer, Efi, NameValue}` (по опкоду декларации VARSTORE/VARSTORE_EFI/VARSTORE_NAME_VALUE) + `attributes: u32` (из тела `IFR_VARSTORE_EFI_OP` @+20; существующие consumer'ы — `list_varstores`, RPC — поле не читают, компилируются как есть).

```rust
fn fill_varstores(
    form_items: &[schema::ItemSchema],
    pkg: &[u8],
    lossy: &mut Vec<String>,
) -> Vec<schema::VarStoreSchema> {
    let mut referenced: Vec<u16> = form_items
        .iter()
        .filter_map(|i| match i {
            schema::ItemSchema::OneOf(x) => Some(x.var_store_id),
            schema::ItemSchema::Numeric(x) => Some(x.var_store_id),
            schema::ItemSchema::CheckBox(x) => Some(x.var_store_id),
            schema::ItemSchema::String(x) => Some(x.var_store_id),
            schema::ItemSchema::OrderedList(x) => Some(x.var_store_id),
            _ => None,
        })
        .filter(|id| *id != 0)
        .collect();
    referenced.sort_unstable();
    referenced.dedup();
    let map = values::varstore_map(pkg);
    referenced
        .iter()
        .filter_map(|id| {
            let vs = map.iter().find(|m| m.id == *id)?;
            let guid = || vs.guid.as_ref().map(crate::guid_to_upper_string).unwrap_or_default();
            match vs.kind {
                values::VarStoreKind::Buffer => Some(schema::VarStoreSchema {
                    id: vs.id,
                    guid: guid(),
                    size: vs.size,
                    name: vs.name.clone(),
                    var_type: schema::VarStoreType::Buffer,
                    attributes: 7,
                }),
                values::VarStoreKind::Efi => Some(schema::VarStoreSchema {
                    id: vs.id,
                    guid: guid(),
                    size: vs.size,
                    name: vs.name.clone(),
                    var_type: schema::VarStoreType::Efi,
                    attributes: vs.attributes,
                }),
                values::VarStoreKind::NameValue => {
                    lossy.push(format!("varstore {id:#x} is name-value, not exportable"));
                    None
                }
            }
        })
        .collect()
}
```
Примечания по фактическому коду: `schema::ItemSchema::String` существует и несёт `var_store_id` (walker его не порождает — arm валиден для произвольного списка); добавлен и `OrderedListItem` (тоже несёт var_store_id). guid — через `crate::guid_to_upper_string` (рабочая конвенция), не `g.to_string().to_ascii_uppercase()`. Buffer → attributes 7 (serde-дефолт schema.rs «только для type=efi»), Efi → реальные атрибуты из карты. Undeclared vsid (нет декларации в карте) — молчаливый skip (движковая валидация импорта его поймает).
- [ ] **Step 3: parent_form_id из рёбер**

В `export_form` (после резолва): `let edges = super::ref_tree::collect_edges(image);` → родители = edges с `form_id == form_id` и `formset_guid == наш` (обе — upper-строки `guid_to_upper_string`) и пустым `target_formset_guid` (FormEdge-поля: `{formset_guid, parent_form_id, form_id, target_formset_guid}` — имя родителя `parent_form_id`); после dedup ровно один → `parent_form_id`, иначе 0. entries-подсказку (prompt/help родительского GOTO) экспорт НЕ заполняет по question_id (спека §2) — только `parent_form_id`.
- [ ] **Step 4: Симметрия с билдером (главный инвариант)**

Тест: собрать schema → `ifr_builder` (emit_form_set/var_store/form/text/one_of/one_of_option/default/numeric/check_box/ref) → `export_form` на полученном пакете → сравнить семантически (id/тексты/options/varstores) с исходной schema. Расхождения — чинить walker, пока тест не зелёный. По фактическому инвентарю ifr_builder: `emit_action`/`emit_subtitle` НЕ существуют — Action/Subtitle-ветки покрываются ручными байтовыми fixtures (паттерн Task 3), в симметрию входит TEXT через `emit_text`. Сравнение ItemSchema/VarStoreSchema — через `serde_json::to_value` (PartialEq у schema-типов нет); тексты — явной texts-картой на pkg-уровне (`collect_items` + `fill_varstores`), плюс отдельный end-to-end `export_form`-тест на синтетическом Image (паттерн `sample_image_with_ifr_guid`: RAW-секция 0x19 + target `<guid>:0x19:0#<form_id>`) — parent-рёбра и lossy.
- [ ] **Step 5: Запустить + clippy + коммит**

Run: `cargo test -p uefi-engine form_export && cargo clippy -p uefi-engine -- -D warnings`
```bash
git add crates/uefi-engine/src/hii/form_export.rs crates/uefi-engine/src/hii/values.rs
git commit -m "feat(engine): form_export — Text/Action/Ref + lossy + referenced varstores + parent edge (контракт varstore-contract §6)"
```

---

### Task 5: proto + RPC HiiFormExport + моки

**Files:**
- Modify: `crates/uefi-proto/proto/engine.proto` (rpc-строка рядом с `HiiFormAdd`, ~:33; сообщения рядом с `HiiFormAddRequest`, ~:211)
- Modify: `crates/uefi-engine/src/rpc/server.rs` (хендлер по образцу `hii_form_add`, :873)
- Modify: `crates/uefi-tui/tests/mock_server.rs`, `crates/uefi-cli/tests/mock_server.rs` (мок-метод), `crates/uefi-gateway/tests/mock_server.rs` (если есть EngineService-мок — заглушка)

**Interfaces:**
- Produces: RPC `HiiFormExport(HiiFormExportRequest{image_id, item_id}) → HiiFormExportResponse{schema_json, formset_guid, parent_form_id, lossy}`

- [ ] **Step 1: proto**

```proto
  rpc HiiFormExport(HiiFormExportRequest)                 returns (HiiFormExportResponse);
```
```proto
message HiiFormExportRequest  { string image_id = 1; string item_id = 2; }
message HiiFormExportResponse {
  string schema_json = 1;
  string formset_guid = 2;
  uint32 parent_form_id = 3;
  repeated string lossy = 4;
}
```
Пересобрать: `cargo build -p uefi-proto`.
- [ ] **Step 2: Хендлер (по образцу :873 — get_or_load_image, lock, hii_error_status_ctx)**

```rust
    async fn hii_form_export(&self, req: Request<HiiFormExportRequest>) -> RpcResult<HiiFormExportResponse> {
        let r = req.into_inner();
        let img = self.get_or_load_image(&r.image_id).await?;
        let images = self.images.lock().await;
        let img_slot = images.get(&r.image_id).ok_or_else(|| Status::not_found("image not found"))?;
        let export = crate::hii::form_export::export_form(img_slot, &r.item_id)
            .map_err(|e| hii_error_status_ctx(e, &r.item_id))?;
        let schema_json = serde_json::to_string(&export.schema)
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(HiiFormExportResponse {
            schema_json,
            formset_guid: export.formset_guid,
            parent_form_id: export.parent_form_id,
            lossy: export.lossy,
        }))
    }
```
- [ ] **Step 3: Моки** — в TUI/CLI mock: `hii_form_export` возвращает фикстурный ответ (schema_json из строковой константы) и пишет в журнал `SchemaCall { rpc: "HiiFormExport", .. }`.
- [ ] **Step 4: Тест хендлера (server.rs tests, по образцу :1686) + `cargo test --all` + clippy**

- [ ] **Step 5: Коммит**

```bash
git add crates/uefi-proto/ crates/uefi-engine/src/rpc/ crates/uefi-tui/tests/ crates/uefi-cli/tests/ crates/uefi-gateway/
git commit -m "feat(proto,rpc): HiiFormExport RPC + хендлер + моки (спека hii-form-export §2)"
```

---

### Task 6: CLI — hii form export + маршрутизация конверта

**Files:**
- Modify: `crates/uefi-cli/src/client.rs` (обёртка `hii_form_export` по образцу `hii_question_add`, :412)
- Modify: `crates/uefi-cli/src/main.rs` (clap: `HiiFormCmd::Export { item_id, out }`)
- Modify: `crates/uefi-cli/src/commands/hii.rs` (`form_export`), `output.rs` (печать JSON)
- Modify: `crates/uefi-cli/src/commands/hii.rs::form_add` (конверт-маршрутизация)

- [ ] **Step 1: client-обёртка + команда export**

```rust
    pub async fn hii_form_export(
        &self,
        image_id: &str,
        item_id: &str,
    ) -> Result<HiiFormExportResponse, AppError> { … }
```
`form_export(item_id, out: Option<PathBuf>, sock, format)`: RPC → конверт собирает клиент (`uefi_common::envelope::wrap_export(body, Some(SourceMeta{formset_guid}), refs_from_parent, lossy)`; refs = parent_form_id != 0 → `Some(RefsSection{parent_form_id, entries: vec![]})`) → stdout pretty ИЛИ `--out` с absolutization по образцу `resolve_output_path` (`commands/artifact.rs`).
- [ ] **Step 2: Маршрутизация в form_add**

В начале `form_add`: прочитать файл, `split_envelope` → если конверт (был ключ `meta`): `Err` «package file: use hii import». Bare — прежний путь.
- [ ] **Step 3: Тесты**: parse-тест clap (`parse_hii_form_export_args` по образцу :647); integration с mock — export пишет конверт с meta.source + refs при parent≠0; form add на конверте → ошибка.
- [ ] **Step 4: `cargo test -p uefi-cli` + clippy + коммит**

```bash
git add crates/uefi-cli/
git commit -m "feat(cli): hii form export (конверт на клиенте) + маршрутизация конвертов из form add"
```

---

### Task 7: CLI — hii import (макро)

**Files:**
- Modify: `crates/uefi-cli/src/main.rs` (новый подкомандный узел `HiiCmd::Import { target, file }`)
- Modify: `crates/uefi-cli/src/commands/hii.rs` (`import`)
- Modify: `crates/uefi-cli/tests/mock_server.rs` (журнал вызовов — добавить `Arc<Mutex<Vec<String>>>` rpc-имён, если ещё нет)

- [ ] **Step 1: Реализация import (порядок — спека §3)**

```text
fn import(target, file):
  text = read(file)
  env = split_envelope(text)?            // bare без meta → тоже валидный пакет (refs нет)
  refs = env.refs
  busy_qids = hii_list_questions(target, refs.parent_form_id)  // при refs
  pre-check: parent ∈ hii_list_forms(target); явные form_id таргеты ∈ hii_list_forms(image);
             авторские question_id ∉ busy_qids           // fail fast, ноль мутаций
  body = refs-only? none : plan_varstores(env.body, hii_list_varstores(target))?
  form_add_outcome = body? hii_form_add(target, body)     // шаг пропущен для refs-only
  plan = refs? plan_ref_step(refs, body, inserted, busy_qids)
  plan? hii_question_add("<target>#<parent>", serialize(plan.records))
  отчёт: «форма вставлена (id X); ref построен» / «…ref не построен: …» / warn formset≠source
```
`hii_list_forms`/`hii_list_questions`/`hii_list_varstores`-обёртки уже есть (client.rs). `HiiQuestionAddRequest.schema_json` — `serde_json::to_string(&QuestionAddList-совместимый json {"refs":[…]})` (формат `np_ref.json`: записи с form_id/prompt/help/question_id[/formset_guid]).
- [ ] **Step 2: Integration-тест с журналом mock'а**

Полный пакет (formset+refs): журнал == `[HiiListForms, HiiListQuestions, HiiListVarstores, HiiFormAdd, HiiQuestionAdd]` (HiiListVarstores — карта для `plan_varstores`, до FormAdd; порядок list_forms → list_questions — pre-check §3.1); refs-only: `[HiiListForms, HiiListQuestions, HiiQuestionAdd]` (varstore-free путь, без карты); bad parent: `[HiiListForms]` + ошибка, ноль мутаций.
- [ ] **Step 3: `cargo test -p uefi-cli` + clippy + коммит**

```bash
git add crates/uefi-cli/
git commit -m "feat(cli): hii import — макро pre-check→form add→question add (спека §3), refs-only пакеты"
```

---

### Task 8: TUI — :hii form export / :hii import / клавиши e,I,R,A / completion

**Files:**
- Modify: `crates/uefi-tui/src/commands.rs` (ветки `form export`/`import` в execute; `export_prefill`; расширение `add_prefill`→`A`; `complete()`-грамматика)
- Modify: `crates/uefi-tui/src/main.rs` (клавиши ~:374-432, паттерн `AppEvent::Key('x') if !app.forms.show_strings && !app.forms.show_varstores`)

- [ ] **Step 1: Команды**: `:hii form export <item_id> [--out FILE]` — RPC → wrap_export → clipboard-да нет: писать в файл/stdout-панель (status_msg с путём; stdout вне TUI не используют — `--out` обязателен в TUI, без него писать в `$TMPDIR/uefipatcher-export-<form_id>.json` и показать путь в статусе); `:hii import <target> --file` — тот же порядок Task 7 (переиспользовать plan-функции; RPC — клиент TUI).
- [ ] **Step 2: Клавиши** (в main.rs, рядом с 'a'-arm :405):
  - `e` на FormsRow::Form → `enter_insert_mode("hii", format!("hii form export {item} --out "))`
  - `I` → `enter_insert_mode("hii", "hii import ".into())` (target из FormSet-строки под курсором, если стоит на ней)
  - `A` на Form → `enter_insert_mode("hii", format!("hii question add {target}#{form} "))`
  - `R` на Form → `enter_insert_mode("hii", format!("hii import {target} --file refs.json"))` (import не принимает `#parent` — родитель берётся из `refs.parent_form_id` пакета; target — формсет строки под курсором, автор правит) + в status_msg подсказка цели (`form_id`+`formset_guid` строки под курсором — автор вписывает их в пакет)
- [ ] **Step 3: Completion** (`complete()`, тесты-образцы :1744-1878): `form export` → item_id-кандидаты (переиспользовать список form add); `import` → target-кандидаты + `--file` → path completion (`complete_path`).
- [ ] **Step 4: Тесты**: unit — префиллы e/I/R/A (по образцу `add_prefill_formset_form_and_dangling`, :2356); execute-ветки на mock-клиенте невозможны (unit) — покрытие в Task 9.
- [ ] **Step 5: `cargo test -p uefi-tui` + clippy + коммит**

```bash
git add crates/uefi-tui/src/
git commit -m "feat(tui): :hii form export/:hii import + клавиши e/I/R/A + completion (спека §4)"
```

---

### Task 9: TUI integration — журнал, refs-only, паритет CLI/TUI

**Files:**
- Modify: `crates/uefi-tui/tests/tui_integration.rs` (или новый `hii_import.rs`)

- [ ] **Step 1: Тесты через `start_mock` + `SchemaCall`-журнал (:16-39, :575)**: полный пакет → журнал `["HiiListForms","HiiListQuestions","HiiListVarstores","HiiFormAdd","HiiQuestionAdd"]`, schema_json question add содержит form_id из ответа form add; refs-only → без HiiFormAdd; конверт в `:hii form add` → ошибка-маршрутизатор.
- [ ] **Step 2: Паритет**: один и тот же файл пакета прогоняется через CLI-команду (Task 7 integration) и через TUI `execute_command` — ассерт: последовательности rpc-имён журналов идентичны (спека §6).
- [ ] **Step 3: `cargo test -p uefi-tui --test tui_integration` + коммит**

```bash
git add crates/uefi-tui/tests/
git commit -m "test(tui): import-журналы (полный/refs-only/маршрутизация) + паритет CLI/TUI"
```

---

### Task 10: Real-image гейт + финальные проверки

**Files:**
- Modify: `crates/uefi-engine/tests/real_image.rs` (новые `#[ignore]`-тесты)

- [ ] **Step 1: Экспорт известной формы HNX** (таргет `899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0`, форма Processor Configuration): ассерты — число вопросов, непустые prompt/help, options-тексты, `varstores` содержит Setup id 1 (EC87D643…, size 0x72), `meta.lossy` ⊆ ожидаемого множества (suppress_if/grayout_if/…).
- [ ] **Step 2: Round-trip гейт**: export → `add_form` (write-копия образа) → `export_form` новой формы → семантическое равенство (кроме form_id/question_id/string_id).
- [ ] **Step 3: `cargo test --all` + `cargo clippy --all -- -D warnings` + `cargo fmt --all -- --check`**
- [ ] **Step 4: Коммит**

```bash
git add crates/uefi-engine/tests/real_image.rs
git commit -m "test(engine): real-image гейты hii-form-export (HNX: конкретика + round-trip)"
```

**Живой гейт владельца** (вне плана задач, вердикт-аддендум в спеку §6 по паттерну tui-forms): в TUI на живом образе `e` → файл → правка → `I` (import с refs) → форма под родителем; `R`-сценарий — кросс-формсетный REF3 (IntelRCSetup на HNX открыт).

---

## Само-проверка плана

**Спека-покрытие:** §1 конверт (T1) · §2 walker+RPC+varstore-fill+parent (T3–T5) · §3 планировщики+порядок+varstore-контракт (T2, T7, T8) · §4 клиентские поверхности+маршрутизация (T6–T8) · §5 ошибки (T1, T2, T6–T9 — каждый кейс отображён в тестах) · §6 тесты и гейты (T1–T10 + живой гейт). Пробелов нет.

**Placeholder-скан:** код в каждом шаге; два места честно помечены как docs:fix-кандидаты при расхождении с фактическими полями (`values::OptionEntry`-поля, min/max/step Numeric, типы varstore-карты) — это правило 11 AGENTS, не placeholder.

**Type-consistency:** `Envelope{meta,refs,body}`/`RefsSection`/`RefEntry` — T1→T2/T7/T8; `RefStepPlan.records → QuestionAddList-json` — T2→T7/T9; `FormExport` — T3→T4→T5; proto-имена — T5→T6–T9. `question_items`/`display_mode` pub(crate) — только внутри engine.
