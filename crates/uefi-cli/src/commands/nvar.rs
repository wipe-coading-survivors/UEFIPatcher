use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn list(
    path: Option<&str>,
    var: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let stores = client.nvar_list(&image_id, path, false).await?;
    if format == OutputFormat::Text {
        eprint!(
            "{}",
            uefi_common::format::nvar_legend(uefi_common::format::NvarLegendCmd::VarList)
        );
    }
    crate::output::print_nvar_list(&stores, format, var);
    Ok(())
}

pub async fn set(
    name: &str,
    guid: Option<&str>,
    offset: u64,
    value: u64,
    width: u32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    if !matches!(width, 1 | 2 | 4 | 8) {
        return Err(AppError::new(
            uefi_common::error::ErrKind::RpcInvalidArgument,
            "width must be one of 1, 2, 4, 8",
        ));
    }
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (applied, stores) = client
        .nvar_set(&image_id, name, guid, offset, value, width)
        .await?;
    crate::output::print_nvar_set(&applied, &stores, format);
    Ok(())
}
