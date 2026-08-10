use uefi_proto::{FormInfo, ImageInfo, Node, SessionInfo, StringInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    #[value(name = "json")]
    Json,
    #[value(name = "text")]
    Text,
    #[value(name = "tsv")]
    Tsv,
}

pub fn print_nodes(nodes: &[Node], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(nodes).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("path\ttype\tsubtype\tguid\toffset\tsize\tname");
            for it in nodes {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    it.path, it.r#type, it.subtype, it.guid, it.offset, it.size, it.name
                );
            }
        }
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
            print!("{}", uefi_common::format::format_tree(&rows));
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

pub fn print_image_info(info: &ImageInfo, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string(info).unwrap_or_else(|_| "{}".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tpath\tmode\tsize\tcreated\tlast_activity");
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                info.image_id, info.name, info.path, info.mode, info.size, info.created_at, info.last_activity
            );
        }
    }
}

pub fn print_images_list(images: &[ImageInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(images).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        _ => {
            println!("image_id\tname\tmode\tsize\tlast_activity");
            for img in images {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    img.image_id, img.name, img.mode, img.size, img.last_activity
                );
            }
        }
    }
}

pub fn print_image_status(info: &ImageInfo, format: OutputFormat) {
    print_image_info(info, format);
}

pub fn print_forms(forms: &[FormInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(forms).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("form_id\tformset_guid\tform_id_ifr\ttitle\tvisible");
            for f in forms {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible
                );
            }
        }
        OutputFormat::Text => {
            for f in forms {
                println!(
                    "{}\t{}\t{}\t{}\tvisible={}",
                    f.form_id, f.formset_guid, f.form_id_ifr, f.title, f.visible
                );
            }
        }
    }
}

pub fn print_strings(strings: &[StringInfo], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let v = serde_json::to_string_pretty(strings).unwrap_or_else(|_| "[]".into());
            println!("{v}");
        }
        OutputFormat::Tsv => {
            println!("language\tstring_id\ttext");
            for s in strings {
                println!("{}\t{}\t{}", s.language, s.string_id, s.text);
            }
        }
        OutputFormat::Text => {
            for s in strings {
                println!("[{}] {}: {}", s.language, s.string_id, s.text);
            }
        }
    }
}

pub fn print_node_id(item_id: &str, format: OutputFormat) {
    match format {
        OutputFormat::Json => println!("{{\"item_id\":\"{item_id}\"}}"),
        _ => println!("{item_id}"),
    }
}

#[allow(dead_code)]
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
    fn session_created_json() {
        print_session_created("s1", "t1", OutputFormat::Json);
    }

    #[test]
    fn node_serializes_type_field() {
        let it = Node {
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
