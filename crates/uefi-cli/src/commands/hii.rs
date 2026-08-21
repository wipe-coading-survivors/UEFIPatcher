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
