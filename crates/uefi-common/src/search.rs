#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Name,
    Utf8,
    Utf16Le,
    Bytes,
}

pub fn match_name(name: &str, query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    name.to_lowercase().contains(&query.to_lowercase())
}

pub fn find_utf8(body: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return false;
    }
    body.windows(needle.len()).any(|w| w == needle)
}

pub fn find_utf16le(body: &[u8], query: &str) -> bool {
    if query.is_empty() {
        return false;
    }
    let needle: Vec<u8> = query.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    find_utf8(body, &needle)
}

pub fn parse_hex_pattern(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(cleaned.len() / 2);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_name_case_insensitive_substring() {
        assert!(match_name("Setup", "set"));
        assert!(match_name("Setup", "UP"));
        assert!(match_name("PchInitDxe", "init"));
        assert!(!match_name("Setup", "menu"));
        assert!(!match_name("Setup", ""));
    }

    #[test]
    fn find_utf8_substring_match() {
        assert!(find_utf8(b"hello world", b"world"));
        assert!(find_utf8(b"abc", b"abc"));
        assert!(!find_utf8(b"abc", b"abcd"));
        assert!(!find_utf8(b"abc", b""));
    }

    #[test]
    fn find_utf16le_encodes_query_as_le_bytes() {
        let body: Vec<u8> = "AB".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert!(find_utf16le(&body, "AB"));
        assert!(!find_utf16le(&body, "AC"));
        assert!(!find_utf16le(b"", ""));
    }

    #[test]
    fn find_utf16le_cyrillic() {
        let body: Vec<u8> = "Привет"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert!(find_utf16le(&body, "рив"));
    }

    #[test]
    fn parse_hex_pattern_no_separator() {
        assert_eq!(
            parse_hex_pattern("DEADBEEF").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xEF]
        );
    }

    #[test]
    fn parse_hex_pattern_with_spaces_and_mixed_case() {
        assert_eq!(
            parse_hex_pattern("DE ad BE f0").unwrap(),
            vec![0xDE, 0xAD, 0xBE, 0xF0]
        );
    }

    #[test]
    fn parse_hex_pattern_invalid() {
        assert!(parse_hex_pattern("XYZW").is_none());
        assert!(parse_hex_pattern("ABC").is_none());
        assert!(parse_hex_pattern("").is_none());
        assert!(parse_hex_pattern("AG").is_none());
    }

    #[test]
    fn search_mode_variants_distinct() {
        assert_ne!(SearchMode::Name, SearchMode::Utf8);
        assert_ne!(SearchMode::Bytes, SearchMode::Utf16Le);
    }
}
