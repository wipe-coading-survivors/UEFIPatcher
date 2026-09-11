use ratatui::style::Color;

pub const TYPE_IMAGE: u8 = 62;
pub const TYPE_REGION: u8 = 63;
pub const TYPE_VOLUME: u8 = 65;
pub const TYPE_FILE: u8 = 66;
pub const TYPE_SECTION: u8 = 67;
pub const TYPE_PADDING: u8 = 64;
pub const TYPE_FREESPACE: u8 = 68;

pub const ACTION_NO: u8 = 50;
pub const ACTION_INSERT: u8 = 52;
pub const ACTION_REPLACE: u8 = 53;
pub const ACTION_REMOVE: u8 = 54;
pub const ACTION_REBUILD: u8 = 55;

pub const SECTION_PE32: u8 = 0x10;
pub const SECTION_GUID_DEFINED: u8 = 0x02;
pub const SECTION_COMPRESSION: u8 = 0x01;
pub const SECTION_UI: u8 = 0x15;
pub const SECTION_RAW: u8 = 0x19;

pub fn type_icon(node_type: u8, subtype: u8) -> &'static str {
    match node_type {
        TYPE_IMAGE => "\u{F2DB}",
        TYPE_VOLUME => "\u{F1C0}",
        TYPE_FILE => "\u{F15B}",
        TYPE_SECTION => match subtype {
            SECTION_PE32 => "\u{F1C9}",
            SECTION_GUID_DEFINED => "\u{F023}",
            SECTION_COMPRESSION => "\u{F1C6}",
            SECTION_UI => "\u{F031}",
            SECTION_RAW => "\u{EAE8}",
            _ => "\u{F016}",
        },
        TYPE_PADDING => "\u{EB7D}",
        TYPE_FREESPACE => "\u{F10C}",
        TYPE_REGION => "\u{F492}",
        _ => "?",
    }
}

pub fn type_color(node_type: u8) -> Color {
    match node_type {
        TYPE_IMAGE => Color::Cyan,
        TYPE_VOLUME => Color::Blue,
        TYPE_FILE => Color::Green,
        TYPE_SECTION => Color::Yellow,
        TYPE_PADDING | TYPE_FREESPACE => Color::DarkGray,
        TYPE_REGION => Color::DarkGray,
        _ => Color::Gray,
    }
}

pub fn immutable_style() -> ratatui::style::Style {
    ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray)
}

pub fn action_marker(action: u8) -> &'static str {
    match action {
        ACTION_INSERT => "+",
        ACTION_REMOVE => "-",
        ACTION_REBUILD => "~",
        ACTION_REPLACE => "*",
        _ => " ",
    }
}

pub fn action_color(action: u8) -> Color {
    match action {
        ACTION_INSERT => Color::Green,
        ACTION_REMOVE => Color::Red,
        ACTION_REBUILD => Color::Yellow,
        ACTION_REPLACE => Color::Blue,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_for_volume() {
        assert_eq!(type_icon(TYPE_VOLUME, 0), "\u{F1C0}");
    }

    #[test]
    fn icon_for_section_pe32() {
        assert_eq!(type_icon(TYPE_SECTION, SECTION_PE32), "\u{F1C9}");
    }

    #[test]
    fn icon_for_unknown_type() {
        assert_eq!(type_icon(99, 0), "?");
    }

    #[test]
    fn icons_are_distinct_per_type() {
        assert_ne!(type_icon(TYPE_IMAGE, 0), type_icon(TYPE_VOLUME, 0));
        assert_ne!(type_icon(TYPE_FILE, 0), type_icon(TYPE_PADDING, 0));
        assert_ne!(
            type_icon(TYPE_SECTION, SECTION_PE32),
            type_icon(TYPE_SECTION, SECTION_COMPRESSION)
        );
    }

    #[test]
    fn action_marker_insert() {
        assert_eq!(action_marker(ACTION_INSERT), "+");
    }

    #[test]
    fn action_marker_no_action() {
        assert_eq!(action_marker(ACTION_NO), " ");
    }

    #[test]
    fn color_for_remove() {
        assert_eq!(action_color(ACTION_REMOVE), Color::Red);
    }

    #[test]
    fn immutable_style_is_dark_gray() {
        assert_eq!(immutable_style().fg, Some(Color::DarkGray));
    }

    #[test]
    fn region_icon_distinct_from_other_types() {
        let region = type_icon(TYPE_REGION, 0);
        for other in [
            TYPE_IMAGE,
            TYPE_VOLUME,
            TYPE_FILE,
            TYPE_SECTION,
            TYPE_PADDING,
            TYPE_FREESPACE,
        ] {
            assert_ne!(region, type_icon(other, 0));
        }
    }
}
