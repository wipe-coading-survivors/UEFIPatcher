use std::collections::HashMap;

use r_efi::hii::IFR_NUMERIC_SIZE;

use super::{HiiError, form_package_ranges, ifr, parse_item_id, schema, values};
use crate::types::{FfsNode, FfsType, Image};

pub struct FormExport {
    pub schema: schema::FormSetSchema,
    pub formset_guid: String,
    pub parent_form_id: u32,
    pub lossy: Vec<String>,
}

/// Спека hii-form-export §2: экспорт формы в FormSetSchema-минимум
/// (вопросы OneOf/Numeric/CheckBox полной fidelity; text/action/ref-итемы,
/// lossy, varstores, parent — Task 4). item_id — `<target>#<form_id>`
/// (грамматика HiiSetFormVisibility для форм); вопрос-суффикс `:qid` и
/// голый target → NotFound. Read-only, любой ImageMode; образ не мутирует.
pub fn export_form(image: &Image, item_id: &str) -> Result<FormExport, HiiError> {
    let (file, node, form_id) = resolve_form(image, item_id)?;
    let pkg = find_form_package(node, form_id).ok_or(HiiError::NotFound)?;
    let texts = super::questions::prompt_texts(file);
    let form = schema::FormSchema {
        id: form_id,
        title: form_title(pkg, form_id, &texts).unwrap_or_else(|| format!("Form {form_id}")),
        items: question_items(pkg, form_id, &texts),
    };
    let formset_guid = package_formset_guid(pkg);
    Ok(FormExport {
        schema: schema::FormSetSchema {
            formset_guid: formset_guid.clone(),
            title: String::new(),
            help: String::new(),
            class_guids: vec![],
            varstores: vec![],
            default_stores: vec![],
            forms: vec![form],
            setupdata_guid: None,
            amitse_guid: None,
        },
        formset_guid,
        parent_form_id: 0,
        lossy: vec![],
    })
}

fn resolve_form<'a>(
    image: &'a Image,
    item_id: &str,
) -> Result<(&'a FfsNode, &'a FfsNode, u16), HiiError> {
    let (target, form_id, question_id) = parse_item_id(item_id)?;
    if question_id.is_some() {
        return Err(HiiError::NotFound);
    }
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let node =
        crate::parser::target::find_item(&image.root, &target).map_err(|_| HiiError::NotFound)?;
    if node.node_type != FfsType::Section {
        return Err(HiiError::NotASetupItem);
    }
    let mut file = &image.root;
    for &i in &path[..path.len() - 1] {
        file = &file.children[i];
    }
    Ok((file, node, form_id))
}

fn find_form_package(node: &FfsNode, form_id: u16) -> Option<&[u8]> {
    for (start, len) in form_package_ranges(node) {
        let pkg = &node.body[start..start + len];
        if let Some(fs) = ifr::parse_form_package(pkg)
            && fs.forms.iter().any(|f| f.form_id == form_id)
        {
            return Some(pkg);
        }
    }
    None
}

fn package_formset_guid(pkg: &[u8]) -> String {
    ifr::parse_form_package(pkg)
        .map(|fs| crate::guid_to_upper_string(&fs.guid))
        .unwrap_or_default()
}

fn form_title(pkg: &[u8], form_id: u16, texts: &HashMap<u16, String>) -> Option<String> {
    let fs = ifr::parse_form_package(pkg)?;
    let form = fs.forms.iter().find(|f| f.form_id == form_id)?;
    texts.get(&form.title).cloned()
}

/// Вопросы формы → ItemSchema (help_sid +4/+5, display-флаги, options,
/// defaults). Display/size-флаги — в собственном Flags-байте опкода
/// (off+13), НЕ в question-header Flags (off+12 — там EFI_IFR_FLAG_*,
/// где RESET_REQUIRED=0x10/REST_STYLE=0x20 коллидируют с display-маской).
/// len < 14 → флагов нет: display = UintDec, Numeric size = 0.
/// Спека hii-form-export §2.
pub(crate) fn question_items(
    pkg: &[u8],
    form_id: u16,
    texts: &HashMap<u16, String>,
) -> Vec<schema::ItemSchema> {
    use r_efi::hii::{IFR_CHECKBOX_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP};
    let mut out = Vec::new();
    values::walk_statements(pkg, |op, off, len, current| {
        if current != Some(form_id) || len < 13 {
            return;
        }
        let prompt_sid = u16::from_le_bytes([pkg[off + 2], pkg[off + 3]]);
        let help_sid = u16::from_le_bytes([pkg[off + 4], pkg[off + 5]]);
        let question_id = u16::from_le_bytes([pkg[off + 6], pkg[off + 7]]);
        let var_store_id = u16::from_le_bytes([pkg[off + 8], pkg[off + 9]]);
        let var_offset = u16::from_le_bytes([pkg[off + 10], pkg[off + 11]]);
        let get = |sid: u16| texts.get(&sid).cloned().unwrap_or_default();
        let op_display = || {
            if len >= 14 {
                display_mode(pkg[off + 13], 0)
            } else {
                schema::DisplayMode::UintDec
            }
        };
        match op {
            IFR_ONE_OF_OP => {
                let mut options = Vec::new();
                let mut defaults = Vec::new();
                values::scan_options(pkg, off + len, &mut options, &mut defaults);
                let schema_options = options
                    .iter()
                    .map(|o| schema::OptionSchema {
                        text: get(o.string_id),
                        value: o.value,
                        default: if o.flags & r_efi::hii::IFR_OPTION_DEFAULT != 0 {
                            Some(schema::DefaultClass::Optimized)
                        } else if o.flags & r_efi::hii::IFR_OPTION_DEFAULT_MFG != 0 {
                            Some(schema::DefaultClass::Failsafe)
                        } else {
                            None
                        },
                    })
                    .collect();
                out.push(schema::ItemSchema::OneOf(schema::OneOfItem {
                    prompt: get(prompt_sid),
                    help: get(help_sid),
                    question_id,
                    var_store_id,
                    var_offset,
                    size: values::one_of_width(&options, &defaults),
                    display: op_display(),
                    options: schema_options,
                    defaults: map_defaults(&defaults),
                }));
            }
            IFR_NUMERIC_OP => {
                let size = if len >= 14 {
                    1u8 << (pkg[off + 13] & IFR_NUMERIC_SIZE)
                } else {
                    0
                };
                let (min, max, step) = numeric_min_max_step(pkg, off, len, size);
                out.push(schema::ItemSchema::Numeric(schema::NumericItem {
                    prompt: get(prompt_sid),
                    help: get(help_sid),
                    question_id,
                    var_store_id,
                    var_offset,
                    size,
                    min,
                    max,
                    step,
                    display: if len >= 14 {
                        display_mode(pkg[off + 13], size)
                    } else {
                        schema::DisplayMode::UintDec
                    },
                    defaults: schema::Defaults::default(),
                }));
            }
            IFR_CHECKBOX_OP => out.push(schema::ItemSchema::CheckBox(schema::CheckBoxItem {
                prompt: get(prompt_sid),
                help: get(help_sid),
                question_id,
                var_store_id,
                var_offset,
                defaults: schema::Defaults::default(),
            })),
            _ => {}
        }
    });
    out
}

/// IFR_DISPLAY-флаги (маска 0x30) Flags-байта опкода → DisplayMode:
/// 0x00=IntDec, 0x10=UintDec, 0x20=UintHex (r-efi/EDK2); default =
/// UintDec (0x30 — только TIME/DATE). Спека hii-form-export §2.
pub(crate) fn display_mode(flags: u8, _size: u8) -> schema::DisplayMode {
    match flags & r_efi::hii::IFR_DISPLAY {
        r_efi::hii::IFR_DISPLAY_INT_DEC => schema::DisplayMode::IntDec,
        r_efi::hii::IFR_DISPLAY_UINT_HEX => schema::DisplayMode::UintHex,
        _ => schema::DisplayMode::UintDec,
    }
}

/// MINMAXSTEP_DATA-хвост Numeric: min/max/step по `size` байт на
/// значение, clamp к длине опкода (образец — values.rs::find_question).
fn numeric_min_max_step(pkg: &[u8], off: usize, len: usize, size: u8) -> (u64, u64, u64) {
    let w = size as usize;
    let read = |pos: usize| {
        let start = off + 14 + pos * w;
        let avail = (off + len).saturating_sub(start);
        read_le_u64(pkg, start, w.min(avail))
    };
    (read(0), read(1), read(2))
}

fn read_le_u64(pkg: &[u8], off: usize, count: usize) -> u64 {
    if off >= pkg.len() {
        return 0;
    }
    let mut buf = [0u8; 8];
    let n = count.min(8).min(pkg.len().saturating_sub(off));
    buf[..n].copy_from_slice(&pkg[off..off + n]);
    u64::from_le_bytes(buf)
}

/// IFR_DEFAULT-записи скоупа вопроса → Defaults: default_id 1 =
/// manufacturing → failsafe, 0/прочие → optimized (первый wins).
fn map_defaults(entries: &[values::DefaultEntry]) -> schema::Defaults {
    let mut d = schema::Defaults::default();
    for e in entries {
        match e.default_id {
            1 => d.failsafe = Some(e.value),
            _ => d.optimized = d.optimized.or(Some(e.value)),
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Guid;
    use r_efi::hii::{
        IFR_CHECKBOX_OP, IFR_DEFAULT_OP, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP, IFR_NUMERIC_OP,
        IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, PACKAGE_FORMS,
    };
    use std::str::FromStr;

    const FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";

    fn opcode(op_code: u8, scope: bool, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(payload.len() + 2);
        v.push(op_code);
        v.push(((payload.len() + 2) as u8) | if scope { 0x80 } else { 0 });
        v.extend_from_slice(payload);
        v
    }

    fn end() -> Vec<u8> {
        vec![IFR_END_OP, 0x02]
    }

    fn package(ifr: &[u8]) -> Vec<u8> {
        let total = 4 + ifr.len();
        let mut b = vec![
            (total & 0xFF) as u8,
            ((total >> 8) & 0xFF) as u8,
            ((total >> 16) & 0xFF) as u8,
            PACKAGE_FORMS,
        ];
        b.extend_from_slice(ifr);
        b
    }

    fn form_set(title: u16) -> Vec<u8> {
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&title.to_le_bytes());
        p.extend_from_slice(&0u16.to_le_bytes());
        p.push(0u8);
        opcode(IFR_FORM_SET_OP, true, &p)
    }

    fn form(id: u16, title: u16) -> Vec<u8> {
        opcode(
            IFR_FORM_OP,
            true,
            &[id.to_le_bytes(), title.to_le_bytes()].concat(),
        )
    }

    fn question_header(prompt: u16, help: u16, qid: u16, store: u16, offset: u16) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&prompt.to_le_bytes());
        p.extend_from_slice(&help.to_le_bytes());
        p.extend_from_slice(&qid.to_le_bytes());
        p.extend_from_slice(&store.to_le_bytes());
        p.extend_from_slice(&offset.to_le_bytes());
        p.push(0x00);
        p
    }

    fn one_of(prompt: u16, help: u16, qid: u16, op_flags: u8) -> Vec<u8> {
        let mut p = question_header(prompt, help, qid, 1, 0x003A);
        p.push(op_flags);
        p.extend_from_slice(&[0x00, 0x01, 0x00]);
        opcode(IFR_ONE_OF_OP, true, &p)
    }

    fn option(string_id: u16, flags: u8, value_bytes: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&string_id.to_le_bytes());
        p.push(flags);
        p.push(0x00);
        p.extend_from_slice(value_bytes);
        opcode(IFR_ONE_OF_OPTION_OP, false, &p)
    }

    fn default_op(default_id: u16, type_: u8, value_bytes: &[u8]) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&default_id.to_le_bytes());
        p.push(type_);
        p.extend_from_slice(value_bytes);
        opcode(IFR_DEFAULT_OP, false, &p)
    }

    fn numeric(
        prompt: u16,
        help: u16,
        qid: u16,
        size_flags: u8,
        min: u8,
        max: u8,
        step: u8,
    ) -> Vec<u8> {
        let mut p = question_header(prompt, help, qid, 1, 0x0010);
        p.push(size_flags);
        p.extend_from_slice(&[min, max, step]);
        opcode(IFR_NUMERIC_OP, true, &p)
    }

    fn checkbox(prompt: u16, help: u16, qid: u16) -> Vec<u8> {
        opcode(
            IFR_CHECKBOX_OP,
            true,
            &question_header(prompt, help, qid, 1, 0x0020),
        )
    }

    #[test]
    fn one_of_with_two_options_and_texts() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            one_of(0x01A3, 0x01A4, 0x003B, 0x10),
            default_op(0, 1, &[0]),
            option(4, r_efi::hii::IFR_OPTION_DEFAULT, &[0]),
            option(3, 0x00, &[1]),
            end(),
            end(),
            end(),
        ]
        .concat();
        let texts: HashMap<u16, String> = [
            (0x01A3u16, "4G Sync Mode".to_string()),
            (0x01A4, "Enable 4G decoding".to_string()),
            (4, "Disabled".to_string()),
            (3, "Enabled".to_string()),
        ]
        .into();
        let items = question_items(&package(&ifr), 10029, &texts);
        assert_eq!(items.len(), 1);
        match &items[0] {
            schema::ItemSchema::OneOf(q) => {
                assert_eq!(q.prompt, "4G Sync Mode");
                assert_eq!(q.help, "Enable 4G decoding");
                assert_eq!(q.question_id, 0x003B);
                assert_eq!(q.var_store_id, 1);
                assert_eq!(q.var_offset, 0x003A);
                assert_eq!(q.size, 1);
                assert_eq!(q.display, schema::DisplayMode::UintDec);
                assert_eq!(q.options.len(), 2);
                assert_eq!(q.options[0].text, "Disabled");
                assert_eq!(q.options[0].value, 0);
                assert_eq!(q.options[0].default, Some(schema::DefaultClass::Optimized));
                assert_eq!(q.options[1].text, "Enabled");
                assert_eq!(q.options[1].value, 1);
                assert_eq!(q.options[1].default, None);
                assert_eq!(q.defaults.optimized, Some(0));
                assert_eq!(q.defaults.failsafe, None);
            }
            _ => panic!("expected OneOf"),
        }
    }

    #[test]
    fn numeric_flags_0x20_is_uint_hex_with_min_max_step() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            numeric(
                0x0201,
                0x0202,
                0x0055,
                r_efi::hii::IFR_NUMERIC_SIZE_1 | r_efi::hii::IFR_DISPLAY_UINT_HEX,
                5,
                9,
                2,
            ),
            end(),
            end(),
            end(),
        ]
        .concat();
        let texts: HashMap<u16, String> = [
            (0x0201u16, "Ratio".to_string()),
            (0x0202, "CPU Core Ratio".to_string()),
        ]
        .into();
        let items = question_items(&package(&ifr), 10029, &texts);
        assert_eq!(items.len(), 1);
        match &items[0] {
            schema::ItemSchema::Numeric(q) => {
                assert_eq!(q.prompt, "Ratio");
                assert_eq!(q.help, "CPU Core Ratio");
                assert_eq!(q.question_id, 0x0055);
                assert_eq!(q.var_store_id, 1);
                assert_eq!(q.var_offset, 0x0010);
                assert_eq!(q.size, 1);
                assert_eq!(q.display, schema::DisplayMode::UintHex);
                assert_eq!(q.min, 5);
                assert_eq!(q.max, 9);
                assert_eq!(q.step, 2);
                assert_eq!(q.defaults.optimized, None);
            }
            _ => panic!("expected Numeric"),
        }
        assert_eq!(display_mode(0x00, 1), schema::DisplayMode::IntDec);
        assert_eq!(display_mode(0x10, 1), schema::DisplayMode::UintDec);
        assert_eq!(display_mode(0x20, 1), schema::DisplayMode::UintHex);
    }

    #[test]
    fn checkbox_resolves_help_text() {
        let ifr = [
            form_set(7),
            form(10031, 22),
            checkbox(0x0300, 0x0301, 0x0060),
            end(),
            end(),
            end(),
        ]
        .concat();
        let texts: HashMap<u16, String> = [
            (0x0300u16, "Turbo".to_string()),
            (0x0301, "Enable Turbo Mode".to_string()),
        ]
        .into();
        let items = question_items(&package(&ifr), 10031, &texts);
        assert_eq!(items.len(), 1);
        match &items[0] {
            schema::ItemSchema::CheckBox(q) => {
                assert_eq!(q.prompt, "Turbo");
                assert_eq!(q.help, "Enable Turbo Mode");
                assert_eq!(q.question_id, 0x0060);
                assert_eq!(q.var_store_id, 1);
                assert_eq!(q.var_offset, 0x0020);
                assert_eq!(q.defaults.optimized, None);
            }
            _ => panic!("expected CheckBox"),
        }
    }

    #[test]
    fn form_without_questions_yields_empty_items() {
        let ifr = [form_set(7), form(10030, 21), end(), end(), end()].concat();
        assert!(question_items(&package(&ifr), 10030, &HashMap::new()).is_empty());
    }
}
