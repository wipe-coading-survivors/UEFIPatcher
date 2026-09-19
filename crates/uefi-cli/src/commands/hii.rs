use crate::client::Client;
use crate::output::OutputFormat;
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
    use super::{ensure_bare_schema, parse_u64_loose, refs_from_parent};

    #[test]
    fn parse_u64_loose_hex_and_dec() {
        assert_eq!(parse_u64_loose("0x1").unwrap(), 1);
        assert_eq!(parse_u64_loose("0xFF").unwrap(), 255);
        assert_eq!(parse_u64_loose("42").unwrap(), 42);
        assert!(parse_u64_loose("0xG").is_err());
        assert!(parse_u64_loose("").is_err());
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
}
