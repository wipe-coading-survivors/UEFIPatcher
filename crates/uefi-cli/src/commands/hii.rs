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
    crate::output::print_forms(&forms, format);
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

pub async fn form_add(
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
    let (form_ids, string_ids) = client.hii_form_add(&image_id, target, &schema_json).await?;
    crate::output::print_form_add(&form_ids, &string_ids, format);
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
    let outcomes = client
        .hii_question_add(&image_id, item_id, &schema_json)
        .await?;
    crate::output::print_question_add(&outcomes, format);
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
    use super::parse_u64_loose;

    #[test]
    fn parse_u64_loose_hex_and_dec() {
        assert_eq!(parse_u64_loose("0x1").unwrap(), 1);
        assert_eq!(parse_u64_loose("0xFF").unwrap(), 255);
        assert_eq!(parse_u64_loose("42").unwrap(), 42);
        assert!(parse_u64_loose("0xG").is_err());
        assert!(parse_u64_loose("").is_err());
    }
}
