use crate::client::Client;
use crate::output::OutputFormat;
use uefi_common::error::AppError;
use uefi_common::state;

pub async fn list(cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let arts = client.artifacts_list().await?;
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

pub async fn import(path: &str, cli_sock: Option<&str>, format: OutputFormat) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let artifact_id = client.artifact_import(path).await?;
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
    client.artifact_export(artifact_id, out).await?;
    crate::output::print_ok(format);
    Ok(())
}
