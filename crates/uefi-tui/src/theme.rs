use ratatui::style::Color;

pub const TYPE_IMAGE: u8 = 62;
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

#[allow(clippy::match_same_arms)]
pub fn type_icon(node_type: u8, subtype: u8) -> &'static str {
    match node_type {
        TYPE_IMAGE => "",
        TYPE_VOLUME => "",
        TYPE_FILE => "",
        TYPE_SECTION => match subtype {
            SECTION_PE32 => "",
            SECTION_GUID_DEFINED => "",
            SECTION_COMPRESSION => "",
            SECTION_UI => "",
            SECTION_RAW => "",
            _ => "",
        },
        TYPE_PADDING => "",
        TYPE_FREESPACE => "",
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
        _ => Color::Gray,
    }
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
        assert_eq!(type_icon(TYPE_VOLUME, 0), "");
    }

    #[test]
    fn icon_for_section_pe32() {
        assert_eq!(type_icon(TYPE_SECTION, SECTION_PE32), "");
    }

    #[test]
    fn icon_for_unknown_type() {
        assert_eq!(type_icon(99, 0), "?");
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
}
