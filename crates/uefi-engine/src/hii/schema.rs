use super::HiiError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSetSchema {
    pub formset_guid: String,
    pub title: String,
    pub help: String,
    #[serde(default)]
    pub class_guids: Vec<String>,
    pub varstores: Vec<VarStoreSchema>,
    pub default_stores: Vec<DefaultStoreSchema>,
    pub forms: Vec<FormSchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setupdata_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amitse_guid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarStoreSchema {
    pub id: u16,
    pub guid: String,
    pub size: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub var_type: VarStoreType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VarStoreType {
    Buffer,
    Efi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultStoreSchema {
    pub name: String,
    pub id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSchema {
    pub id: u16,
    pub title: String,
    pub items: Vec<ItemSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemSchema {
    OneOf(OneOfItem),
    CheckBox(CheckBoxItem),
    Numeric(NumericItem),
    Text(TextItem),
    Ref(RefItem),
    String(StringItem),
    Action(ActionItem),
    OrderedList(OrderedListItem),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneOfItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub size: u8,
    #[serde(default = "default_display")]
    pub display: DisplayMode,
    pub options: Vec<OptionSchema>,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSchema {
    pub text: String,
    pub value: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultClass>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultClass {
    Optimized,
    Failsafe,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    IntDec,
    UintDec,
    UintHex,
}

fn default_display() -> DisplayMode {
    DisplayMode::UintDec
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimized: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failsafe: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckBoxItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub size: u8,
    pub min: u64,
    pub max: u64,
    pub step: u64,
    #[serde(default = "default_display")]
    pub display: DisplayMode,
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub prompt: String,
    pub help: String,
    pub text_two: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub form_id: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub min_size: u8,
    pub max_size: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub config: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderedListItem {
    pub prompt: String,
    pub help: String,
    pub question_id: u16,
    pub var_store_id: u16,
    pub var_offset: u16,
    pub max_containers: u8,
}

pub fn parse_schema(json: &str) -> Result<FormSetSchema, HiiError> {
    serde_json::from_str(json).map_err(|e| HiiError::InvalidSchema(e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HijackQuestionSchema {
    pub question_id: u16,
    pub prompt: String,
    pub help: String,
    pub failsafe: u8,
    pub optimal: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HijackSchema {
    pub title: String,
    #[serde(default)]
    pub questions: Vec<HijackQuestionSchema>,
}

pub fn parse_hijack_schema(json: &str) -> Result<HijackSchema, HiiError> {
    serde_json::from_str(json).map_err(|e| HiiError::InvalidSchema(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_schema() {
        let json = r#"{
            "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
            "title": "Test", "help": "Test help",
            "class_guids": [],
            "varstores": [{"id": 1, "guid": "11111111-2222-3333-4444-555555555555", "size": 256, "name": "MyVar", "type": "buffer"}],
            "default_stores": [{"name": "Optimized", "id": 0}, {"name": "Failsafe", "id": 1}],
            "forms": [{"id": 1, "title": "Main", "items": [
                {"type": "one_of", "prompt": "P", "help": "H", "question_id": 256, "var_store_id": 1, "var_offset": 0, "size": 1,
                 "options": [{"text": "A", "value": 0, "default": "optimized"}]}
            ]}]
        }"#;
        let s = parse_schema(json).unwrap();
        assert_eq!(s.title, "Test");
        assert_eq!(s.varstores.len(), 1);
        assert_eq!(s.forms[0].items.len(), 1);
        match &s.forms[0].items[0] {
            ItemSchema::OneOf(o) => {
                assert_eq!(o.question_id, 256);
                assert_eq!(o.options.len(), 1);
            }
            _ => panic!("expected OneOf"),
        }
    }

    #[test]
    fn parse_with_ami_guids() {
        let json = r#"{
            "formset_guid": "A1B2C3D4-E5F6-7890-ABCD-EF1234567890",
            "title": "T", "help": "H", "class_guids": [],
            "setupdata_guid": "12345678-90AB-CDEF-1234-567890ABCDEF",
            "amitse_guid": "87654321-FEDC-BA09-8765-432109FEDCBA",
            "varstores": [], "default_stores": [], "forms": []
        }"#;
        let s = parse_schema(json).unwrap();
        assert!(s.setupdata_guid.is_some());
        assert!(s.amitse_guid.is_some());
    }

    #[test]
    fn parse_invalid_json() {
        assert!(parse_schema("{invalid").is_err());
    }

    #[test]
    fn parse_hijack_schema_minimal() {
        let s = parse_hijack_schema(r#"{"title": "UEFIPATCHER E18"}"#).unwrap();
        assert_eq!(s.title, "UEFIPATCHER E18");
        assert!(s.questions.is_empty());
    }

    #[test]
    fn parse_hijack_schema_with_questions() {
        let s = parse_hijack_schema(
            r#"{"title": "T", "questions": [
                {"question_id": 59, "prompt": "P", "help": "H", "failsafe": 1, "optimal": 1}
            ]}"#,
        )
        .unwrap();
        assert_eq!(s.questions.len(), 1);
        assert_eq!(s.questions[0].question_id, 59);
        assert_eq!(s.questions[0].failsafe, 1);
        assert_eq!(s.questions[0].optimal, 1);
    }

    #[test]
    fn parse_hijack_schema_rejects_garbage() {
        assert!(parse_hijack_schema("{").is_err());
        assert!(parse_hijack_schema(r#"{"questions": []}"#).is_err());
    }
}
