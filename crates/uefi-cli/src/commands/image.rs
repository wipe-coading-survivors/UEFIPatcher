use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::{AppError, ErrKind};
use uefi_common::state;

pub async fn open(
    path: &str,
    mode: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    let mode_i = match mode {
        "read" => 0,
        "write" => 1,
        _ => {
            return Err(AppError::new(
                ErrKind::RpcInvalidArgument,
                "mode must be read|write",
            ));
        }
    };
    let mut client = Client::connect(cli_sock, st.clone()).await?;
    let (image_id, root_guid) = client.open_image(path, mode_i).await?;
    st.active_image_id = Some(image_id.clone());
    state::write_state(&st)?;
    match format {
        OutputFormat::Json => {
            println!("{{\"image_id\":\"{image_id}\",\"root_guid\":\"{root_guid}\"}}")
        }
        _ => println!("{image_id}\t{root_guid}"),
    }
    Ok(())
}

pub async fn switch(image_id: &str, _format: OutputFormat) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    st.active_image_id = Some(image_id.into());
    state::write_state(&st)?;
    Ok(())
}

pub async fn close(format: OutputFormat) -> Result<(), AppError> {
    let mut st = state::require_state()?;
    st.active_image_id = None;
    state::write_state(&st)?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn dump(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let items = client.list_items(&image_id, None).await?;
    match format {
        OutputFormat::Text => {
            let rows: Vec<uefi_common::format::TreeRow> = items
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
        OutputFormat::Tsv | OutputFormat::Json => {
            crate::output::print_items(&items, format);
        }
    }
    Ok(())
}

pub async fn list(
    filter: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let items = client.list_items(&image_id, filter).await?;
    crate::output::print_items(&items, format);
    Ok(())
}

pub async fn find(
    target: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    let item_id = client.find_item(&image_id, target).await?;
    crate::output::print_find(&item_id, format);
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
    let items = client.search_items(&image_id, query, modes, limit).await?;
    crate::output::print_items(&items, format);
    Ok(())
}

pub async fn save(
    output: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let image_id = client.active_image()?;
    client.save_image(&image_id, output).await?;
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
        .extract_artifact(&image_id, target, body_only)
        .await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}

pub async fn export(
    artifact_id: &str,
    output_path: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let out = output_path.unwrap_or(artifact_id);
    client.export_artifact(artifact_id, out).await?;
    crate::output::print_ok(format);
    Ok(())
}

pub async fn import(
    file_path: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let artifact_id = client.import_artifact(file_path).await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}

pub async fn artifacts(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let arts = client.list_artifacts().await?;
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(&arts).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("artifact_id\tkind\tsize");
            for a in &arts {
                println!("{}\t{}\t{}", a.artifact_id, a.kind, a.size);
            }
        }
        OutputFormat::Text => {
            for a in &arts {
                println!("{}  kind={} size={}", a.artifact_id, a.kind, a.size);
            }
        }
    }
    Ok(())
}
