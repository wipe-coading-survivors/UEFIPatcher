use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn set_visibility(
    item_id: &str,
    visible: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client
        .set_setup_visibility(&image_id, item_id, visible)
        .await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn list_items(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let items = client.list_items(&image_id, None).await?;
    let setup_items: Vec<_> = items.into_iter().filter(|it| it.r#type == 67).collect();
    crate::output::print_items(&setup_items, format);
    Ok(())
}
