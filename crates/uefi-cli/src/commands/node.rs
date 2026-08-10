use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state;

pub async fn list(
    filter: Option<&str>,
    tree: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let nodes = client.image_nodes_list(&image_id, filter).await?;
    if tree {
        match format {
            OutputFormat::Text => {
                let rows: Vec<uefi_common::format::TreeRow> = nodes
                    .iter()
                    .map(|it| uefi_common::format::TreeRow {
                        path: it.path.clone(),
                        type_: it.r#type,
                        subtype: it.subtype as u8,
                        guid: it.guid.clone(),
                        offset: it.offset,
                        size: it.size,
                        name: it.name.clone(),
                    })
                    .collect();
                eprint!("{}", uefi_common::format::format_legend(&rows));
                print!("{}", uefi_common::format::format_tree(&rows));
            }
            _ => crate::output::print_nodes(&nodes, format),
        }
    } else {
        crate::output::print_nodes(&nodes, format);
    }
    Ok(())
}

pub async fn search(
    query: &str,
    modes: &[i32],
    limit: u32,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let nodes = client
        .image_nodes_search(&image_id, query, modes, limit)
        .await?;
    crate::output::print_nodes(&nodes, format);
    Ok(())
}

pub async fn insert(
    target: &str,
    file: Option<&str>,
    artifact: Option<&str>,
    mode: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (ffs_path, artifact_id) = match (file, artifact) {
        (Some(p), None) => (p.to_string(), String::new()),
        (None, Some(a)) => (String::new(), a.to_string()),
        _ => {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                "exactly one of --file or --artifact required",
            ))
        }
    };
    let mode_i = match mode {
        "into" => 0,
        "before" => 1,
        "after" => 2,
        _ => {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                "mode must be into|before|after",
            ))
        }
    };
    let item_id = client
        .image_node_insert(&image_id, target, &ffs_path, &artifact_id, mode_i)
        .await?;
    crate::output::print_node_id(&item_id, format);
    Ok(())
}

pub async fn remove(target: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_node_remove(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn replace(
    target: &str,
    file: Option<&str>,
    artifact: Option<&str>,
    body_only: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let (ffs_path, artifact_id) = match (file, artifact) {
        (Some(p), None) => (p.to_string(), String::new()),
        (None, Some(a)) => (String::new(), a.to_string()),
        _ => {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                "exactly one of --file or --artifact required",
            ))
        }
    };
    let item_id = client
        .image_node_replace(&image_id, target, &ffs_path, &artifact_id, body_only)
        .await?;
    crate::output::print_node_id(&item_id, format);
    Ok(())
}

pub async fn rebuild(target: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.image_node_rebuild(&image_id, target).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn extract(
    target: &str,
    body_only: bool,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let artifact_id = client
        .image_node_extract(&image_id, target, body_only)
        .await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}
