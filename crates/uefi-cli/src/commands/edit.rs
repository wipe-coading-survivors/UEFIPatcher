use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state;

pub async fn insert(
    target: &str,
    ffs_path: &str,
    from_artifact: Option<&str>,
    mode: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let mode_i = match mode {
        "into" => 0,
        "before" => 1,
        "after" => 2,
        _ => {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                "mode must be into|before|after",
            ));
        }
    };
    let (eff_path, artifact_id) = match from_artifact {
        Some(aid) => (String::new(), aid.to_string()),
        None => (ffs_path.to_string(), String::new()),
    };
    let item_id = client
        .insert(&image_id, target, &eff_path, &artifact_id, mode_i)
        .await?;
    crate::output::print_find(&item_id, format);
    Ok(())
}

pub async fn remove(
    target: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.remove(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn replace(
    target: &str,
    data_path: &str,
    from_artifact: Option<&str>,
    body_only: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (eff_path, artifact_id) = match from_artifact {
        Some(aid) => (String::new(), aid.to_string()),
        None => (data_path.to_string(), String::new()),
    };
    let item_id = client
        .replace(&image_id, target, &eff_path, &artifact_id, body_only)
        .await?;
    crate::output::print_find(&item_id, format);
    Ok(())
}

pub async fn rebuild(
    target: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.rebuild(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}
