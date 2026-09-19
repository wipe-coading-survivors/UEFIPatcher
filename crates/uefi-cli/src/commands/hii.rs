use std::collections::HashMap;

use crate::client::Client;
use crate::output::ImportRefBuilt;
use crate::output::OutputFormat;
use uefi_common::envelope::RefsSection;
use uefi_common::error::AppError;
use uefi_common::error::ErrKind;
use uefi_common::state;

pub async fn form_list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let forms = client.hii_list_forms(&image_id).await?;
    if format == OutputFormat::Text {
        let codes = target_section_codes(forms.iter().map(|f| f.form_id.as_str()));
        eprint!(
            "{}",
            uefi_common::format::hii_legend(uefi_common::format::HiiLegendCmd::FormList, &codes)
        );
    }
    crate::output::print_forms(&forms, format);
    Ok(())
}

/// Типы секций из target-частей (`<guid>:<type-hex>:<index>`) для легенды.
fn target_section_codes<'a>(targets: impl Iterator<Item = &'a str>) -> Vec<u8> {
    targets
        .filter_map(|t| t.split(':').nth(1))
        .filter_map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .collect()
}

/// item_id формы: `<target>#<form_id>` (form_id — десятичное; вопросная
/// часть `:qid` допускается и игнорируется). Грамматика target:
/// `<ffs-file-guid>:<section-type-hex>:<index>` — из колонки form_id `hii form list`.
pub async fn question_list(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (target, disc) = item_id.rsplit_once('#').ok_or_else(|| {
        AppError::new(
            ErrKind::RpcInvalidArgument,
            format!("item_id must be <target>#<form_id>: '{item_id}'"),
        )
    })?;
    let form_str = disc.split_once(':').map(|(f, _)| f).unwrap_or(disc);
    let form_id: u32 = form_str.parse().map_err(|e| {
        AppError::new(
            ErrKind::RpcInvalidArgument,
            format!("form_id must be decimal: '{form_str}': {e}"),
        )
    })?;
    let questions = client
        .hii_list_questions(&image_id, target, form_id)
        .await?;
    if format == OutputFormat::Text {
        let codes = target_section_codes(std::iter::once(target));
        eprint!(
            "{}",
            uefi_common::format::hii_legend(
                uefi_common::format::HiiLegendCmd::QuestionList,
                &codes
            )
        );
    }
    crate::output::print_questions(&questions, target, form_id, format);
    Ok(())
}

/// item_id формсета: `<target>` или `<target>#<form_id>` (суффикс после `#`
/// допускается и отбрасывается). Грамматика target:
/// `<ffs-file-guid>:<section-type-hex>:<index>` — из колонки form_id `hii form list`.
pub async fn varstore_list(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let target = item_id.rsplit_once('#').map(|(t, _)| t).unwrap_or(item_id);
    let stores = client.hii_list_varstores(&image_id, target).await?;
    if format == OutputFormat::Text {
        let codes = target_section_codes(std::iter::once(target));
        eprint!(
            "{}",
            uefi_common::format::hii_legend(
                uefi_common::format::HiiLegendCmd::VarstoreList,
                &codes
            )
        );
    }
    crate::output::print_varstores(&stores, format);
    Ok(())
}

pub async fn form_set_visibility(
    form_id: &str,
    visible: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client
        .hii_set_form_visibility(&image_id, form_id, visible)
        .await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn string_list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let strings = client.hii_list_strings(&image_id).await?;
    crate::output::print_strings(&strings, format);
    Ok(())
}

pub async fn formset_add(
    file: &str,
    ffs: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (new_ffs_id, form_ids) = client
        .hii_form_set_add(&image_id, &schema_json, ffs)
        .await?;
    crate::output::print_formset_add(&new_ffs_id, &form_ids, format);
    Ok(())
}

/// Конверт-маршрутизация `form add` (спека hii-form-export §4/§5): файл
/// с верхнеуровневым ключом `meta` — пакет `hii import`, здесь отвергается.
/// Признак конверта: bare-файл `split_envelope` пропускает байт-в-байт (тело
/// совпадает с файлом), а ошибки разбора meta/refs/formset возможны только
/// у конверта; InvalidJson уходит прежним путём — валидирует движок.
fn ensure_bare_schema(schema_json: &str) -> Result<(), AppError> {
    let is_package = match uefi_common::envelope::split_envelope(schema_json) {
        Ok(env) => env.body != schema_json,
        Err(uefi_common::envelope::EnvelopeError::InvalidJson(_)) => false,
        Err(_) => true,
    };
    if is_package {
        return Err(AppError::new(
            ErrKind::RpcInvalidArgument,
            "package file: use hii import",
        ));
    }
    Ok(())
}

pub async fn form_add(
    target: &str,
    file: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    ensure_bare_schema(&schema_json)?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (form_ids, string_ids) = client.hii_form_add(&image_id, target, &schema_json).await?;
    crate::output::print_form_add(&form_ids, &string_ids, format);
    Ok(())
}

/// Спека hii-form-export §4: parent_form_id=0 — корневая форма, refs-секции
/// нет; иначе entries пустые (ссылку синтезирует `hii import`). u32→u16 с
/// явной ошибкой — значение больше u16 в IFR-форме означает битый ответ.
fn refs_from_parent(
    parent_form_id: u32,
) -> Result<Option<uefi_common::envelope::RefsSection>, AppError> {
    if parent_form_id == 0 {
        return Ok(None);
    }
    let id = u16::try_from(parent_form_id).map_err(|_| {
        AppError::new(
            ErrKind::RpcInternal,
            format!("parent_form_id {parent_form_id} does not fit u16"),
        )
    })?;
    Ok(Some(uefi_common::envelope::RefsSection {
        parent_form_id: id,
        entries: vec![],
    }))
}

/// Экспорт формы в конверт (спека hii-form-export §4): RPC отдаёт bare-тело
/// и факты, конверт (meta.source/refs/meta.lossy) собирает клиент — движок
/// мету не видит. stdout — конверт pretty-JSON; при `--out` файл пишется по
/// absolutization-конвенции `resolve_output_path`, на stdout — подтверждение.
pub async fn form_export(
    item_id: &str,
    out: Option<std::path::PathBuf>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let resp = client.hii_form_export(&image_id, item_id).await?;
    let refs = refs_from_parent(resp.parent_form_id)?;
    let envelope = uefi_common::envelope::wrap_export(
        &resp.schema_json,
        Some(uefi_common::envelope::SourceMeta {
            formset_guid: resp.formset_guid,
        }),
        refs,
        resp.lossy,
    );
    match out {
        Some(path) => {
            let p = crate::commands::artifact::resolve_output_path(&path.to_string_lossy())?;
            std::fs::write(&p, format!("{envelope}\n"))
                .map_err(|e| AppError::new(ErrKind::IoError, format!("{}: {e}", p.display())))?;
            crate::output::print_ok(format);
        }
        None => crate::output::print_envelope(&envelope),
    }
    Ok(())
}

fn envelope_err(e: uefi_common::envelope::EnvelopeError) -> AppError {
    AppError::new(ErrKind::RpcInvalidArgument, e.to_string())
}

fn fit_u16(v: u32, what: &str) -> Result<u16, AppError> {
    u16::try_from(v)
        .map_err(|_| AppError::new(ErrKind::RpcInternal, format!("{what} {v} does not fit u16")))
}

/// Pre-check §3.1: родитель refs-секции существует в целевом формсете.
/// Target — грамматика колонки form_id `hii form list` (`<guid>:<type>:<index>`).
fn check_parent(forms: &[uefi_proto::FormInfo], target: &str, parent: u16) -> Result<(), AppError> {
    if forms
        .iter()
        .any(|f| f.form_id.eq_ignore_ascii_case(target) && f.form_id_ifr == parent as u32)
    {
        Ok(())
    } else {
        Err(AppError::new(
            ErrKind::RpcInvalidArgument,
            format!("parent form {parent} not found in target '{target}'; see 'hii form list'"),
        ))
    }
}

/// Pre-check §3.1: авторские question_id из entries не заняты в родителе.
fn check_qids(refs: &RefsSection, busy: &[u16]) -> Result<(), AppError> {
    for qid in refs.entries.iter().filter_map(|e| e.question_id) {
        if busy.contains(&qid) {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                format!(
                    "question_id {qid:#06x} already busy in parent form {}",
                    refs.parent_form_id
                ),
            ));
        }
    }
    Ok(())
}

/// Pre-check §3.1 (анти-dangling): явные form_id-таргеты entries существуют —
/// с formset_guid в том формсете (кросс-формсетный REF3), без — в целевом.
/// Существование цели REF движок не валидирует — только клиент.
fn check_targets(
    forms: &[uefi_proto::FormInfo],
    target: &str,
    refs: &RefsSection,
) -> Result<(), AppError> {
    for e in &refs.entries {
        let Some(id) = e.form_id else { continue };
        let (found, err) = match e.formset_guid.as_deref() {
            Some(guid) => (
                forms.iter().any(|f| {
                    f.form_id_ifr == id as u32 && f.formset_guid.eq_ignore_ascii_case(guid)
                }),
                format!("refs target form {id} not found in formset {guid}"),
            ),
            None => (
                forms
                    .iter()
                    .any(|f| f.form_id_ifr == id as u32 && f.form_id.eq_ignore_ascii_case(target)),
                format!("refs target form {id} not found in target '{target}'"),
            ),
        };
        if !found {
            return Err(AppError::new(ErrKind::RpcInvalidArgument, err));
        }
    }
    Ok(())
}

fn busy_qids(questions: &[uefi_proto::QuestionSummary]) -> Result<Vec<u16>, AppError> {
    questions
        .iter()
        .map(|q| fit_u16(q.question_id, "question_id"))
        .collect()
}

fn varstore_briefs(
    stores: &[uefi_proto::VarStoreInfo],
) -> Result<Vec<uefi_common::envelope::VarstoreBrief>, AppError> {
    stores
        .iter()
        .map(|v| {
            Ok(uefi_common::envelope::VarstoreBrief {
                id: fit_u16(v.id, "varstore id")?,
                guid: v.guid.clone(),
                size: fit_u16(v.size, "varstore size")?,
                name: v.name.clone(),
            })
        })
        .collect()
}

/// Спека hii-form-export §3: макро над `hii form add` + `hii question add`.
/// Pre-check (родитель/qid/явные таргеты) до мутаций; varstore-конфликт и
/// invalid refs — fail fast до RPC-мутаций; refs-only пакеты (body пуст)
/// пропускают form add; ref-фаза после успешной form-фазы не прерывает
/// команду — падение уходит в двухфазный отчёт (автоотката нет, §3.4);
/// warn при импорте в чужой формсет (meta.source ≠ целевого).
pub async fn import(
    target: &str,
    file: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let env = uefi_common::envelope::split_envelope(&text).map_err(envelope_err)?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;

    let mut busy: Vec<u16> = Vec::new();
    if let Some(refs) = env.refs.as_ref() {
        let forms = client.hii_list_forms(&image_id).await?;
        check_parent(&forms, target, refs.parent_form_id)?;
        let questions = client
            .hii_list_questions(&image_id, target, refs.parent_form_id as u32)
            .await?;
        busy = busy_qids(&questions)?;
        check_qids(refs, &busy)?;
        check_targets(&forms, target, refs)?;
        let target_formset = forms
            .iter()
            .find(|f| f.form_id.eq_ignore_ascii_case(target))
            .map(|f| f.formset_guid.clone());
        if let (Some(src), Some(actual)) = (env.meta.source.as_ref(), target_formset.as_deref())
            && !src.formset_guid.eq_ignore_ascii_case(actual)
        {
            eprintln!(
                "warning: importing into foreign formset: package source {}, target formset {actual}",
                src.formset_guid
            );
        }
    }

    let mut body: Option<String> = None;
    let mut inserted: Vec<u32> = Vec::new();
    let mut string_ids: HashMap<String, u32> = HashMap::new();
    if !env.body.is_empty() {
        let stores = client.hii_list_varstores(&image_id, target).await?;
        let planned = uefi_common::envelope::plan_varstores(&env.body, &varstore_briefs(&stores)?)
            .map_err(envelope_err)?;
        let (ids, sids) = client.hii_form_add(&image_id, target, &planned).await?;
        inserted = ids;
        string_ids = sids;
        body = Some(planned);
    }

    let ref_outcome = match env.refs.as_ref() {
        None => None,
        Some(refs) => Some(
            match uefi_common::envelope::plan_ref_step(refs, body.as_deref(), &inserted, &busy) {
                Ok(Some(plan)) => {
                    let schema =
                        serde_json::to_string(&serde_json::json!({ "refs": plan.records }))
                            .map_err(|e| {
                                AppError::new(ErrKind::RpcInternal, format!("refs plan: {e}"))
                            })?;
                    let parent = format!("{}#{}", target, refs.parent_form_id);
                    match client.hii_question_add(&image_id, &parent, &schema).await {
                        Ok(_) => Ok(ImportRefBuilt {
                            parent_form_id: refs.parent_form_id,
                            question_ids: plan.records.iter().map(|r| r.question_id).collect(),
                        }),
                        Err(e) => Err(e.to_string()),
                    }
                }
                Ok(None) => Err("refs plan is empty".to_string()),
                Err(e) => Err(e.to_string()),
            },
        ),
    };
    crate::output::print_import(&inserted, &string_ids, ref_outcome.as_ref(), format);
    Ok(())
}

pub async fn form_hijack(
    target: &str,
    file: &str,
    setupdata_guid: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let resp = client
        .hii_form_hijack(&image_id, target, &schema_json, setupdata_guid)
        .await?;
    crate::output::print_form_hijack(&resp, format);
    Ok(())
}

async fn gates(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let gates = client.hii_gates_list(&image_id, item_id).await?;
    crate::output::print_gates(item_id, &gates, format);
    Ok(())
}

async fn unlock(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (gs, applied) = client.hii_unlock(&image_id, item_id).await?;
    crate::output::print_unlock(item_id, &gs, &applied, format);
    Ok(())
}

pub async fn form_gates(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    gates(item_id, cli_sock, format).await
}

pub async fn form_unlock(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    unlock(item_id, cli_sock, format).await
}

pub async fn question_gates(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    gates(item_id, cli_sock, format).await
}

pub async fn question_unlock(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    unlock(item_id, cli_sock, format).await
}

pub async fn question_info(
    item_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let q = client.hii_question_info(&image_id, item_id).await?;
    crate::output::print_question_info(&q, format);
    Ok(())
}

pub async fn question_set_value(
    item_id: &str,
    value: u64,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (q, applied, stores) = client.hii_set_value(&image_id, item_id, value).await?;
    crate::output::print_set_value(&q, &applied, &stores, format);
    Ok(())
}

pub async fn question_add(
    item_id: &str,
    file: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let resp = client
        .hii_question_add(&image_id, item_id, &schema_json)
        .await?;
    crate::output::print_question_add_result(&resp.questions, &resp.refs, format);
    Ok(())
}

pub async fn page_add(
    target: &str,
    file: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let schema_json = std::fs::read_to_string(file)
        .map_err(|e| AppError::new(ErrKind::IoError, format!("{file}: {e}")))?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let resp = client.hii_page_add(&image_id, target, &schema_json).await?;
    crate::output::print_page_add(&resp, format);
    Ok(())
}

pub fn parse_u64_loose(s: &str) -> Result<u64, AppError> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse::<u64>()
    }
    .map_err(|e| {
        AppError::new(
            ErrKind::RpcInvalidArgument,
            format!("invalid value '{s}': {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        check_parent, check_qids, check_targets, ensure_bare_schema, parse_u64_loose,
        refs_from_parent,
    };
    use uefi_common::envelope::{RefEntry, RefsSection};

    fn form(form_id: &str, formset_guid: &str, form_id_ifr: u32) -> uefi_proto::FormInfo {
        uefi_proto::FormInfo {
            form_id: form_id.into(),
            formset_guid: formset_guid.into(),
            form_id_ifr,
            title: String::new(),
            visible: true,
        }
    }

    #[test]
    fn parse_u64_loose_hex_and_dec() {
        assert_eq!(parse_u64_loose("0x1").unwrap(), 1);
        assert_eq!(parse_u64_loose("0xFF").unwrap(), 255);
        assert_eq!(parse_u64_loose("42").unwrap(), 42);
        assert!(parse_u64_loose("0xG").is_err());
        assert!(parse_u64_loose("").is_err());
    }

    #[test]
    fn check_parent_target_and_form_must_match() {
        let forms = vec![
            form("g:0x19:0", "G-1", 1),
            form("g:0x19:0", "G-1", 2),
            form("other:0x10:0", "G-2", 1),
        ];
        assert!(check_parent(&forms, "g:0x19:0", 2).is_ok());
        assert!(check_parent(&forms, "G:0X19:0", 1).is_ok());
        let miss = check_parent(&forms, "g:0x19:0", 4242).unwrap_err();
        assert!(
            miss.to_string()
                .contains("parent form 4242 not found in target 'g:0x19:0'")
        );
        assert!(check_parent(&forms, "other:0x10:0", 1).is_ok());
    }

    #[test]
    fn check_parent_form_in_other_formset_is_miss() {
        let forms = vec![form("other:0x10:0", "G-2", 7)];
        assert!(check_parent(&forms, "g:0x19:0", 7).is_err());
    }

    #[test]
    fn check_qids_rejects_busy_author_id() {
        let refs = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry {
                question_id: Some(0x22),
                ..Default::default()
            }],
        };
        let err = check_qids(&refs, &[0x20, 0x22]).unwrap_err();
        assert!(err.to_string().contains("question_id 0x0022 already busy"));
        assert!(check_qids(&refs, &[0x23]).is_ok());
    }

    #[test]
    fn check_targets_cross_formset_and_dangling() {
        let forms = vec![
            form("g:0x19:0", "5C60F367-A505-419A-859E-2A4FF6CA6FE5", 1),
            form(
                "899407d7-99fe-43d8-9a21-79ec328cac21:0x10:0",
                "899407D7-99FE-43D8-9A21-79EC328CAC21",
                5002,
            ),
        ];
        let cross = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry {
                form_id: Some(5002),
                formset_guid: Some("899407d7-99fe-43d8-9a21-79ec328cac21".into()),
                ..Default::default()
            }],
        };
        assert!(check_targets(&forms, "g:0x19:0", &cross).is_ok());
        let same_formset = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry {
                form_id: Some(1),
                ..Default::default()
            }],
        };
        assert!(check_targets(&forms, "g:0x19:0", &same_formset).is_ok());
        let dangling = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry {
                form_id: Some(6000),
                ..Default::default()
            }],
        };
        let err = check_targets(&forms, "g:0x19:0", &dangling).unwrap_err();
        assert!(
            err.to_string()
                .contains("refs target form 6000 not found in target 'g:0x19:0'")
        );
        let wrong_guid = RefsSection {
            parent_form_id: 1,
            entries: vec![RefEntry {
                form_id: Some(5002),
                formset_guid: Some("00000000-0000-0000-0000-000000000000".into()),
                ..Default::default()
            }],
        };
        let err = check_targets(&forms, "g:0x19:0", &wrong_guid).unwrap_err();
        assert!(err.to_string().contains(
            "refs target form 5002 not found in formset 00000000-0000-0000-0000-000000000000"
        ));
    }

    #[test]
    fn refs_from_parent_zero_is_none() {
        assert!(refs_from_parent(0).unwrap().is_none());
        let refs = refs_from_parent(10001).unwrap().unwrap();
        assert_eq!(refs.parent_form_id, 10001);
        assert!(refs.entries.is_empty());
        assert!(refs_from_parent(u16::MAX as u32 + 1).is_err());
    }

    #[test]
    fn bare_schema_passthrough_envelope_rejected() {
        let bare = r#"{"formset_guid":"G","forms":[]}"#;
        assert!(ensure_bare_schema(bare).is_ok());
        assert!(ensure_bare_schema("not json at all").is_ok());
        let pkg = r#"{"meta":{"source":{"formset_guid":"G"}},"formset":{"forms":[]}}"#;
        assert!(ensure_bare_schema(pkg).is_err());
        let empty_meta = r#"{"meta":{},"formset":{"forms":[]}}"#;
        assert!(ensure_bare_schema(empty_meta).is_err());
        let no_body = r#"{"meta":{"lossy":["x"]}}"#;
        assert!(ensure_bare_schema(no_body).is_err());
    }

    #[test]
    fn refs_only_package_rejected_by_form_add_router() {
        let refs_only = r#"{"refs":{"parent_form_id":1,"entries":[{"form_id":2}]}}"#;
        assert!(ensure_bare_schema(refs_only).is_err());
    }
}
