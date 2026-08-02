use uefi_common::error::{AppError, ErrKind};
use uefi_proto::{Item, SessionInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Text,
    Tsv,
}

pub fn parse_format(s: &str) -> Result<OutputFormat, AppError> {
    match s {
        "json" => Ok(OutputFormat::Json),
        "text" => Ok(OutputFormat::Text),
        "tsv" => Ok(OutputFormat::Tsv),
        _ => Err(AppError::new(
            ErrKind::RpcInvalidArgument,
            format!("unknown format: {s}"),
        )),
    }
}

pub fn print_items(items: &[Item], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("path\ttype\tsubtype\tguid\toffset\tsize\tname");
            for it in items {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    it.path, it.r#type, it.subtype, it.guid, it.offset, it.size, it.name
                );
            }
        }
        OutputFormat::Text => {
            for it in items {
                println!(
                    "{}  type={} subtype={:02X} guid={} offset={} size={} name={}",
                    it.path, it.r#type, it.subtype, it.guid, it.offset, it.size, it.name
                );
            }
        }
    }
}

pub fn print_session_created(session_id: &str, token: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"session_id\":\"{session_id}\",\"token\":\"{token}\"}}"),
        _ => println!("{session_id}\t{token}"),
    }
}

pub fn print_sessions(rows: &[SessionInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(rows).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        _ => {
            println!("session_id\tcreated_at\tlast_activity");
            for r in rows {
                println!("{}\t{}\t{}", r.session_id, r.created_at, r.last_activity);
            }
        }
    }
}

pub fn print_find(item_id: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"item_id\":\"{item_id}\"}}"),
        _ => println!("{item_id}"),
    }
}

pub fn print_text(text: &str) {
    print!("{text}");
}

pub fn print_ok(format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"ok\":true}}"),
        _ => println!("ok"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_format_valid() {
        assert_eq!(parse_format("json").unwrap(), OutputFormat::Json);
        assert_eq!(parse_format("tsv").unwrap(), OutputFormat::Tsv);
        assert!(parse_format("yaml").is_err());
    }

    #[test]
    fn session_created_json() {
        print_session_created("s1", "t1", OutputFormat::Json);
    }

    #[test]
    fn item_serializes_type_field() {
        let it = Item {
            path: "0".into(),
            r#type: 65,
            subtype: 0,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: String::new(),
        };
        let j = serde_json::to_string(&it).unwrap();
        assert!(
            j.contains("\"type\":65"),
            "expected `\"type\"` field, got: {j}"
        );
    }
}
