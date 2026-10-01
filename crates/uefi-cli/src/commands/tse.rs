use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn report(
    image_id: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let resp = client.tse_report(image_id).await?;
    crate::output::print_tse_report(&resp, format);
    Ok(())
}

pub async fn unhide(
    image_id: &str,
    formset_guid: &str,
    form_id: u16,
    block_offset: Option<usize>,
    cli_sock: Option<&str>,
) -> Result<(), AppError> {
    uguid::Guid::try_parse(formset_guid).map_err(|e| {
        AppError::new(
            uefi_common::error::ErrKind::RpcInvalidArgument,
            format!("invalid formset guid: {e}"),
        )
    })?;
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let block_pe_offset = match block_offset {
        None => None,
        Some(off) => Some(u32::try_from(off).map_err(|_| {
            AppError::new(
                uefi_common::error::ErrKind::RpcInvalidArgument,
                "--block-offset exceeds u32",
            )
        })?),
    };
    let resp = client
        .tse_unhide(image_id, formset_guid, u32::from(form_id), block_pe_offset)
        .await?;
    crate::output::print_tse_unhide(formset_guid, form_id, &resp);
    Ok(())
}
