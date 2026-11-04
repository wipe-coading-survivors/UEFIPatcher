#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeCmdArgs {
    pub target: Option<String>,
    pub file: Option<String>,
    pub artifact_id: Option<String>,
    pub mode: Option<String>,
    pub body_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    File,
    Artifact,
}

impl NodeCmdArgs {
    pub fn source(&self) -> Result<Option<Source>, String> {
        match (&self.file, &self.artifact_id) {
            (Some(_), None) => Ok(Some(Source::File)),
            (None, Some(_)) => Ok(Some(Source::Artifact)),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => Err("exactly one of --file / --artifact-id is required".into()),
        }
    }
}

/// Hex-only offset (CLI/TUI `--block-offset`): `0x`-префикс срезается
/// однократно; голый hex принят (`16` = 0x16); повтор префикса
/// (`0x0x10`) — ошибка парсинга. Спека tse-followups §4.5.
pub fn parse_hex_offset(s: &str) -> Result<usize, String> {
    let digits = s.strip_prefix("0x").unwrap_or(s);
    usize::from_str_radix(digits, 16).map_err(|e| format!("invalid hex offset {s:?}: {e}"))
}

pub fn parse_node_flags(parts: &[&str]) -> NodeCmdArgs {
    let mut a = NodeCmdArgs::default();
    let mut i = 1;
    while i < parts.len() {
        match parts[i] {
            "--file" => {
                a.file = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--artifact-id" => {
                a.artifact_id = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--mode" => {
                a.mode = parts.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "--body-only" => {
                a.body_only = true;
                i += 1;
            }
            other if !other.starts_with("--") && a.target.is_none() => {
                a.target = Some(other.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_and_target() {
        let a = parse_node_flags(&["insert", "0/3", "--file", "/tmp/x.bin", "--mode", "after"]);
        assert_eq!(a.target.as_deref(), Some("0/3"));
        assert_eq!(a.file.as_deref(), Some("/tmp/x.bin"));
        assert_eq!(a.mode.as_deref(), Some("after"));
        assert_eq!(a.source().unwrap(), Some(Source::File));
    }

    #[test]
    fn both_sources_rejected() {
        let a = parse_node_flags(&["insert", "--file", "a", "--artifact-id", "b"]);
        assert!(a.source().is_err());
    }

    #[test]
    fn none_source_ok_and_body_only() {
        let a = parse_node_flags(&["replace", "0/3", "--body-only"]);
        assert_eq!(a.source().unwrap(), None);
        assert!(a.body_only);
    }

    #[test]
    fn hex_offset_accepts_prefixed_and_bare() {
        assert_eq!(parse_hex_offset("0x1b40").unwrap(), 0x1b40);
        assert_eq!(parse_hex_offset("1b40").unwrap(), 0x1b40);
        assert_eq!(
            parse_hex_offset("16").unwrap(),
            0x16,
            "голый hex принят: 16 = 0x16"
        );
    }

    #[test]
    fn hex_offset_rejects_double_prefix_and_garbage() {
        assert!(parse_hex_offset("0x0x10").is_err());
        assert!(parse_hex_offset("zz").is_err());
        assert!(parse_hex_offset("").is_err());
        assert!(parse_hex_offset("0x").is_err());
    }
}
