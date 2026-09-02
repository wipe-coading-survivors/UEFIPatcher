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

pub async fn import(
    path: &str,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let artifact_id = client.artifact_import(path).await?;
    match format {
        OutputFormat::Json => println!("{{\"artifact_id\":\"{artifact_id}\"}}"),
        _ => println!("{artifact_id}"),
    }
    Ok(())
}

fn resolve_output_path(path: &str) -> Result<std::path::PathBuf, AppError> {
    let p = std::path::PathBuf::from(path);
    if p.is_absolute() {
        return Ok(p);
    }
    let cwd = std::env::current_dir()
        .map_err(|e| AppError::new(uefi_common::error::ErrKind::IoError, format!("CWD: {e}")))?;
    Ok(cwd.join(p))
}

pub async fn export(
    artifact_id: &str,
    output_path: Option<&str>,
    cli_sock: Option<&str>,
    format: OutputFormat,
) -> Result<(), AppError> {
    let st = state::require_state()?;
    let mut client = Client::connect(cli_sock, st).await?;
    let out = resolve_output_path(output_path.unwrap_or(artifact_id))?;
    client
        .artifact_export(artifact_id, &out.to_string_lossy())
        .await?;
    crate::output::print_ok(format);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_output_path_is_absolutized_under_cwd() {
        let p = resolve_output_path("out.bin").unwrap();
        assert!(p.is_absolute());
        assert_eq!(p.parent(), std::env::current_dir().ok().as_deref());
        assert_eq!(p.file_name().map(|f| f.to_str()), Some(Some("out.bin")));
    }

    #[test]
    fn absolute_output_path_is_untouched() {
        let p = resolve_output_path("/tmp/x/out.bin").unwrap();
        assert_eq!(p, std::path::PathBuf::from("/tmp/x/out.bin"));
    }
}
