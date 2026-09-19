use std::collections::HashMap;

use r_efi::hii::IFR_NUMERIC_SIZE;

use super::{HiiError, form_package_ranges, ifr, parse_item_id, schema, values};
use crate::types::{FfsNode, FfsType, Image};

/// Запись refs.entries-подсказки (спека hii-form-export §2): prompt/help
/// одного GOTO родителя, ведущего на экспортируемую форму.
pub struct ParentEntry {
    pub prompt: String,
    pub help: String,
}

pub struct FormExport {
    pub schema: schema::FormSetSchema,
    pub formset_guid: String,
    pub parent_form_id: u32,
    pub parent_entries: Vec<ParentEntry>,
    pub lossy: Vec<String>,
}

/// Спека hii-form-export §2: экспорт формы в FormSetSchema-минимум
/// (вопросы OneOf/Numeric/CheckBox + text/action/ref-итемы, lossy-счёт,
/// referenced-only varstores, parent_form_id из ref-рёбер). item_id —
/// `<target>#<form_id>` (грамматика HiiSetFormVisibility для форм);
/// вопрос-суффикс `:qid` и голый target → NotFound. Read-only, любой
/// ImageMode; образ не мутирует.
pub fn export_form(image: &Image, item_id: &str) -> Result<FormExport, HiiError> {
    let (file, node, form_id) = resolve_form(image, item_id)?;
    let pkg = find_form_package(node, form_id).ok_or(HiiError::NotFound)?;
    let texts = super::questions::prompt_texts(file);
    let mut lossy = Vec::new();
    let items = collect_items(pkg, form_id, &texts, &mut lossy);
    let varstores = fill_varstores(&items, pkg, &mut lossy);
    let formset_guid = package_formset_guid(pkg);
    let form = schema::FormSchema {
        id: form_id,
        title: form_title(pkg, form_id, &texts).unwrap_or_else(|| format!("Form {form_id}")),
        items,
    };
    let parent_form_id = parent_form_id(image, &formset_guid, form_id);
    let parent_entries = if parent_form_id != 0 {
        parent_goto_entries(pkg, parent_form_id, form_id, &texts)
    } else {
        Vec::new()
    };
    Ok(FormExport {
        schema: schema::FormSetSchema {
            formset_guid: formset_guid.clone(),
            title: String::new(),
            help: String::new(),
            class_guids: vec![],
            varstores,
            default_stores: vec![],
            forms: vec![form],
            setupdata_guid: None,
            amitse_guid: None,
        },
        formset_guid,
        parent_form_id,
        parent_entries,
        lossy,
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

/// Итем-экспорт формы (спека hii-form-export §2): вопросы полной
/// fidelity + Text/Subtitle/Action/Ref. Непокрытые statement-опкоды
/// считаются в `lossy` по видам: `suppress_if:N`, `grayout_if:N`,
/// `unknown_op_<hex>:N`; REF3/REF4 → skip + `cross_formset_ref:N`,
/// REF5 Dynamic → skip + `dynamic_ref:N`, REF2 → `ref_question_target:N`
/// (target-question в RefItem не выражается). ONE_OF_OPTION/DEFAULT
/// consumed one_of-веткой, FORM — структурный: не считаются.
/// Length-гарды по-веточные (TEXT len 8 / SUBTITLE 6 / вопросы 13).
pub(crate) fn collect_items(
    pkg: &[u8],
    form_id: u16,
    texts: &HashMap<u16, String>,
    lossy: &mut Vec<String>,
) -> Vec<schema::ItemSchema> {
    use r_efi::hii::{
        IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DEFAULT_OP, IFR_FORM_OP, IFR_GRAY_OUT_IF_OP,
        IFR_NUMERIC_OP, IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, IFR_REF_OP, IFR_SUBTITLE_OP,
        IFR_SUPPRESS_IF_OP, IFR_TEXT_OP,
    };
    let mut out = Vec::new();
    let mut counts: Vec<(String, u32)> = Vec::new();
    values::walk_statements(pkg, |op, off, len, current| {
        if current != Some(form_id) {
            return;
        }
        let sid = |pos: usize| u16::from_le_bytes([pkg[off + pos], pkg[off + pos + 1]]);
        let get = |s: u16| texts.get(&s).cloned().unwrap_or_default();
        let op_display = || {
            if len >= 14 {
                display_mode(pkg[off + 13], 0)
            } else {
                schema::DisplayMode::UintDec
            }
        };
        match op {
            IFR_ONE_OF_OP if len >= 13 => {
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
                    prompt: get(sid(2)),
                    help: get(sid(4)),
                    question_id: sid(6),
                    var_store_id: sid(8),
                    var_offset: sid(10),
                    size: values::one_of_width(&options, &defaults),
                    display: op_display(),
                    options: schema_options,
                    defaults: map_defaults(&defaults),
                }));
            }
            IFR_NUMERIC_OP if len >= 13 => {
                let size = if len >= 14 {
                    1u8 << (pkg[off + 13] & IFR_NUMERIC_SIZE)
                } else {
                    0
                };
                let (min, max, step) = numeric_min_max_step(pkg, off, len, size);
                out.push(schema::ItemSchema::Numeric(schema::NumericItem {
                    prompt: get(sid(2)),
                    help: get(sid(4)),
                    question_id: sid(6),
                    var_store_id: sid(8),
                    var_offset: sid(10),
                    size,
                    min,
                    max,
                    step,
                    display: op_display(),
                    defaults: schema::Defaults::default(),
                }));
            }
            IFR_CHECKBOX_OP if len >= 13 => {
                out.push(schema::ItemSchema::CheckBox(schema::CheckBoxItem {
                    prompt: get(sid(2)),
                    help: get(sid(4)),
                    question_id: sid(6),
                    var_store_id: sid(8),
                    var_offset: sid(10),
                    defaults: schema::Defaults::default(),
                }));
            }
            IFR_TEXT_OP if len >= 8 => out.push(schema::ItemSchema::Text(schema::TextItem {
                prompt: get(sid(2)),
                help: get(sid(4)),
                text_two: get(sid(6)),
            })),
            IFR_SUBTITLE_OP if len >= 6 => out.push(schema::ItemSchema::Text(schema::TextItem {
                prompt: get(sid(2)),
                help: get(sid(4)),
                text_two: String::new(),
            })),
            IFR_ACTION_OP if len >= 15 => {
                out.push(schema::ItemSchema::Action(schema::ActionItem {
                    prompt: get(sid(2)),
                    help: get(sid(4)),
                    question_id: sid(6),
                    config: get(sid(13)),
                }));
            }
            IFR_REF_OP => match crate::hii::ref_variant::parse_ref(op, &pkg[off..off + len]) {
                Some(crate::hii::ref_variant::RefTarget::Form { form_id: target }) => {
                    out.push(schema::ItemSchema::Ref(schema::RefItem {
                        prompt: get(sid(2)),
                        help: get(sid(4)),
                        question_id: sid(6),
                        form_id: target,
                    }));
                }
                Some(crate::hii::ref_variant::RefTarget::FormQuestion {
                    form_id: target, ..
                }) => {
                    out.push(schema::ItemSchema::Ref(schema::RefItem {
                        prompt: get(sid(2)),
                        help: get(sid(4)),
                        question_id: sid(6),
                        form_id: target,
                    }));
                    bump(&mut counts, "ref_question_target");
                }
                Some(crate::hii::ref_variant::RefTarget::Formset { .. }) => {
                    bump(&mut counts, "cross_formset_ref");
                }
                Some(crate::hii::ref_variant::RefTarget::Dynamic) => {
                    bump(&mut counts, "dynamic_ref");
                }
                None => bump(&mut counts, &format!("unknown_op_{op:02x}")),
            },
            IFR_SUPPRESS_IF_OP => bump(&mut counts, "suppress_if"),
            IFR_GRAY_OUT_IF_OP => bump(&mut counts, "grayout_if"),
            IFR_FORM_OP | IFR_ONE_OF_OPTION_OP | IFR_DEFAULT_OP => {}
            _ => bump(&mut counts, &format!("unknown_op_{op:02x}")),
        }
    });
    for (key, n) in counts {
        lossy.push(format!("{key}:{n}"));
    }
    out
}

fn bump(counts: &mut Vec<(String, u32)>, key: &str) {
    if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k.as_str() == key) {
        *n += 1;
    } else {
        counts.push((key.to_string(), 1));
    }
}

/// Referenced-only varstore-заполнение (контракт varstore-contract §6,
/// спека hii-form-export §2): декларации только id, на которые ссылаются
/// items формы; name-value → skip + lossy-строка; undeclared id →
/// молчаливый skip (валидация импорта поймает). Buffer → attributes 7
/// (serde-дефолт schema.rs), Efi → атрибуты из декларации.
fn fill_varstores(
    form_items: &[schema::ItemSchema],
    pkg: &[u8],
    lossy: &mut Vec<String>,
) -> Vec<schema::VarStoreSchema> {
    let mut referenced: Vec<u16> = form_items
        .iter()
        .filter_map(|i| match i {
            schema::ItemSchema::OneOf(x) => Some(x.var_store_id),
            schema::ItemSchema::Numeric(x) => Some(x.var_store_id),
            schema::ItemSchema::CheckBox(x) => Some(x.var_store_id),
            schema::ItemSchema::String(x) => Some(x.var_store_id),
            schema::ItemSchema::OrderedList(x) => Some(x.var_store_id),
            _ => None,
        })
        .filter(|id| *id != 0)
        .collect();
    referenced.sort_unstable();
    referenced.dedup();
    let map = values::varstore_map(pkg);
    referenced
        .iter()
        .filter_map(|id| {
            let vs = map.iter().find(|m| m.id == *id)?;
            let base = || schema::VarStoreSchema {
                id: vs.id,
                guid: vs
                    .guid
                    .as_ref()
                    .map(crate::guid_to_upper_string)
                    .unwrap_or_default(),
                size: vs.size,
                name: vs.name.clone(),
                var_type: schema::VarStoreType::Buffer,
                attributes: 7,
            };
            match vs.kind {
                values::VarStoreKind::Buffer => Some(base()),
                values::VarStoreKind::Efi => Some(schema::VarStoreSchema {
                    var_type: schema::VarStoreType::Efi,
                    attributes: vs.attributes,
                    ..base()
                }),
                values::VarStoreKind::NameValue => {
                    lossy.push(format!("varstore {id:#x} is name-value, not exportable"));
                    None
                }
            }
        })
        .collect()
}

/// parent_form_id (спека hii-form-export §2): ровно один same-formset
/// родитель по рёбрам HiiFormTree (form_id == наш, formset совпадает,
/// target_formset_guid пуст); ноль/несколько/кросс-формсетные → 0.
fn parent_form_id(image: &Image, formset_guid: &str, form_id: u16) -> u32 {
    let mut parents: Vec<u32> = super::ref_tree::collect_edges(image)
        .into_iter()
        .filter(|e| {
            e.form_id == u32::from(form_id)
                && e.formset_guid == formset_guid
                && e.target_formset_guid.is_empty()
        })
        .map(|e| e.parent_form_id)
        .collect();
    parents.sort_unstable();
    parents.dedup();
    match parents.as_slice() {
        [only] => *only,
        _ => 0,
    }
}

/// refs.entries из GOTO родителя (спека hii-form-export §2): REF-стейтменты
/// родительской формы (семантика `ref_tree::package_edges`: REF с len ≥ 15,
/// варианты Form/FormQuestion, same-formset), ведущие на form_id
/// экспортируемой формы; prompt/help — через texts-канал. question_id
/// сознательно не пишется (коллизия при реимпорте в тот же образ). Несколько
/// GOTO → по entry на каждый; ноль найдено → пусто (расхождение рёбер/пакета
/// терпимо).
fn parent_goto_entries(
    pkg: &[u8],
    parent: u32,
    form_id: u16,
    texts: &HashMap<u16, String>,
) -> Vec<ParentEntry> {
    let Some(parent) = u16::try_from(parent).ok() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    values::walk_statements(pkg, |op, off, len, current| {
        use r_efi::hii::IFR_REF_OP;
        if op != IFR_REF_OP || len < 15 || current != Some(parent) {
            return;
        }
        let target = match crate::hii::ref_variant::parse_ref(op, &pkg[off..off + len]) {
            Some(crate::hii::ref_variant::RefTarget::Form { form_id })
            | Some(crate::hii::ref_variant::RefTarget::FormQuestion { form_id, .. }) => form_id,
            _ => return,
        };
        if target != form_id {
            return;
        }
        let sid = |pos: usize| u16::from_le_bytes([pkg[off + pos], pkg[off + pos + 1]]);
        out.push(ParentEntry {
            prompt: texts.get(&sid(2)).cloned().unwrap_or_default(),
            help: texts.get(&sid(4)).cloned().unwrap_or_default(),
        });
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
    use crate::types::{Action, Guid, ImageMode, ParsingData};
    use r_efi::hii::{
        IFR_ACTION_OP, IFR_CHECKBOX_OP, IFR_DEFAULT_OP, IFR_END_OP, IFR_FORM_OP, IFR_FORM_SET_OP,
        IFR_GRAY_OUT_IF_OP, IFR_NUMERIC_OP, IFR_ONE_OF_OP, IFR_ONE_OF_OPTION_OP, IFR_PASSWORD_OP,
        IFR_REF_OP, IFR_SUBTITLE_OP, IFR_SUPPRESS_IF_OP, IFR_TEXT_OP, IFR_VARSTORE_EFI_OP,
        IFR_VARSTORE_NAME_VALUE_OP, IFR_VARSTORE_OP, PACKAGE_FORMS,
    };
    use std::str::FromStr;

    const FORMSET_GUID: &str = "7B59104A-C00D-4158-87FF-F04D6396A915";
    const VARSTORE_GUID_STR: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";
    const FILE_GUID_STR: &str = "899407D7-92A6-4174-968F-6F0B47F86A23";
    const CROSS_FORMSET: &str = "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9";

    fn items_of(pkg: &[u8], form_id: u16, texts: &HashMap<u16, String>) -> Vec<schema::ItemSchema> {
        let mut lossy = Vec::new();
        collect_items(pkg, form_id, texts, &mut lossy)
    }

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

    fn checkbox_on_store(prompt: u16, help: u16, qid: u16, store: u16) -> Vec<u8> {
        opcode(
            IFR_CHECKBOX_OP,
            true,
            &question_header(prompt, help, qid, store, 0),
        )
    }

    fn text_op(prompt: u16, help: u16, two: u16) -> Vec<u8> {
        opcode(
            IFR_TEXT_OP,
            false,
            &[prompt.to_le_bytes(), help.to_le_bytes(), two.to_le_bytes()].concat(),
        )
    }

    fn subtitle_op(prompt: u16, help: u16) -> Vec<u8> {
        let mut p = prompt.to_le_bytes().to_vec();
        p.extend_from_slice(&help.to_le_bytes());
        p.push(0);
        opcode(IFR_SUBTITLE_OP, false, &p)
    }

    fn action_op(prompt: u16, help: u16, qid: u16, config: u16) -> Vec<u8> {
        let mut p = question_header(prompt, help, qid, 0, 0);
        p.extend_from_slice(&config.to_le_bytes());
        opcode(IFR_ACTION_OP, false, &p)
    }

    fn ref_op(prompt: u16, qid: u16, target: u16) -> Vec<u8> {
        let mut p = question_header(prompt, 0x01A4, qid, 0xFFFF, 0);
        p.extend_from_slice(&target.to_le_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn ref_op_ph(prompt: u16, help: u16, qid: u16, target: u16) -> Vec<u8> {
        let mut p = question_header(prompt, help, qid, 0xFFFF, 0);
        p.extend_from_slice(&target.to_le_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn ref2_op(prompt: u16, qid: u16, target: u16, target_qid: u16) -> Vec<u8> {
        let mut p = question_header(prompt, 0x01A4, qid, 0xFFFF, 0);
        p.extend_from_slice(&target.to_le_bytes());
        p.extend_from_slice(&target_qid.to_le_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn ref3_op(prompt: u16, qid: u16, target: u16, formset: &str) -> Vec<u8> {
        let g = Guid::from_str(formset).unwrap();
        let mut p = question_header(prompt, 0x01A4, qid, 0xFFFF, 0);
        p.extend_from_slice(&target.to_le_bytes());
        p.extend_from_slice(&0xFFFFu16.to_le_bytes());
        p.extend_from_slice(&g.to_bytes());
        opcode(IFR_REF_OP, false, &p)
    }

    fn ref5_op(prompt: u16, qid: u16) -> Vec<u8> {
        opcode(
            IFR_REF_OP,
            false,
            &question_header(prompt, 0x01A4, qid, 0xFFFF, 0),
        )
    }

    fn gate_op(op_code: u8) -> Vec<u8> {
        opcode(op_code, true, &[])
    }

    fn password_op(prompt: u16, qid: u16) -> Vec<u8> {
        opcode(
            IFR_PASSWORD_OP,
            false,
            &question_header(prompt, 0x01A4, qid, 0, 0),
        )
    }

    fn varstore_buffer(id: u16, size: u16, name: &str) -> Vec<u8> {
        let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&size.to_le_bytes());
        p.extend_from_slice(name.as_bytes());
        p.push(0);
        opcode(IFR_VARSTORE_OP, false, &p)
    }

    fn varstore_efi(id: u16, size: u16, attributes: u32, name: &str) -> Vec<u8> {
        let g = Guid::from_str(VARSTORE_GUID_STR).unwrap();
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        p.extend_from_slice(&g.to_bytes());
        p.extend_from_slice(&attributes.to_le_bytes());
        p.extend_from_slice(&size.to_le_bytes());
        for u in name.encode_utf16().chain(std::iter::once(0)) {
            p.extend_from_slice(&u.to_le_bytes());
        }
        opcode(IFR_VARSTORE_EFI_OP, false, &p)
    }

    fn varstore_name_value(id: u16, name: &str) -> Vec<u8> {
        let mut p = Vec::new();
        p.extend_from_slice(&id.to_le_bytes());
        for u in name.encode_utf16().chain(std::iter::once(0)) {
            p.extend_from_slice(&u.to_le_bytes());
        }
        opcode(IFR_VARSTORE_NAME_VALUE_OP, false, &p)
    }

    fn mk_node(
        guid: Option<Guid>,
        node_type: FfsType,
        subtype: u8,
        body: Vec<u8>,
        children: Vec<FfsNode>,
    ) -> FfsNode {
        FfsNode {
            guid,
            node_type,
            subtype,
            offset: 0,
            header: vec![],
            body,
            tail: vec![],
            children,
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn image_with(pkg: Vec<u8>) -> Image {
        image_with_sections(vec![pkg])
    }

    fn image_with_sections(sections: Vec<Vec<u8>>) -> Image {
        let children = sections
            .into_iter()
            .map(|body| mk_node(None, FfsType::Section, 0x19, body, vec![]))
            .collect();
        let file = mk_node(
            Some(Guid::from_str(FILE_GUID_STR).unwrap()),
            FfsType::File,
            0x07,
            vec![],
            children,
        );
        let volume = mk_node(None, FfsType::Volume, 0, vec![], vec![file]);
        let root = mk_node(None, FfsType::Image, 0, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Read,
        }
    }

    fn string_package(lang: &str, texts: &[&str]) -> Vec<u8> {
        let hdr_size: u32 = (46 + lang.len() + 1) as u32;
        let mut b = vec![0, 0, 0, r_efi::hii::PACKAGE_STRINGS];
        b.extend_from_slice(&hdr_size.to_le_bytes());
        b.extend_from_slice(&hdr_size.to_le_bytes());
        while b.len() < 46 {
            b.push(0);
        }
        b.extend_from_slice(lang.as_bytes());
        b.push(0);
        while b.len() < hdr_size as usize {
            b.push(0);
        }
        for t in texts {
            b.push(0x10);
            b.extend_from_slice(t.as_bytes());
            b.push(0);
        }
        b.push(0x00);
        let len = b.len() as u32;
        b[0] = (len & 0xFF) as u8;
        b[1] = ((len >> 8) & 0xFF) as u8;
        b[2] = ((len >> 16) & 0xFF) as u8;
        b
    }

    fn item_id(form_id: u16) -> String {
        format!("{FILE_GUID_STR}:0x19:0#{form_id}")
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
        let items = items_of(&package(&ifr), 10029, &texts);
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
        let items = items_of(&package(&ifr), 10029, &texts);
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
        let items = items_of(&package(&ifr), 10031, &texts);
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
        assert!(items_of(&package(&ifr), 10030, &HashMap::new()).is_empty());
    }

    #[test]
    fn text_subtitle_action_items() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            text_op(0x0600, 0x0601, 0x0602),
            subtitle_op(0x0603, 0x0604),
            action_op(0x0605, 0x0606, 0x75, 0x0607),
            end(),
            end(),
            end(),
        ]
        .concat();
        let texts: HashMap<u16, String> = [
            (0x0600u16, "Info".to_string()),
            (0x0601, "Info help".to_string()),
            (0x0602, "Second".to_string()),
            (0x0603, "Sub".to_string()),
            (0x0604, "Sub help".to_string()),
            (0x0605, "Act".to_string()),
            (0x0606, "Act help".to_string()),
            (0x0607, "Apply now".to_string()),
        ]
        .into();
        let mut lossy = Vec::new();
        let items = collect_items(&package(&ifr), 10029, &texts, &mut lossy);
        assert_eq!(items.len(), 3);
        match &items[0] {
            schema::ItemSchema::Text(t) => {
                assert_eq!(t.prompt, "Info");
                assert_eq!(t.help, "Info help");
                assert_eq!(t.text_two, "Second");
            }
            _ => panic!("expected Text"),
        }
        match &items[1] {
            schema::ItemSchema::Text(t) => {
                assert_eq!(t.prompt, "Sub");
                assert_eq!(t.help, "Sub help");
                assert_eq!(t.text_two, "");
            }
            _ => panic!("expected Text (subtitle)"),
        }
        match &items[2] {
            schema::ItemSchema::Action(a) => {
                assert_eq!(a.prompt, "Act");
                assert_eq!(a.help, "Act help");
                assert_eq!(a.question_id, 0x75);
                assert_eq!(a.config, "Apply now");
            }
            _ => panic!("expected Action"),
        }
        assert!(lossy.is_empty(), "{lossy:?}");
    }

    #[test]
    fn ref_variants_exported_skipped_and_lossy() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            ref_op(0x0500, 0x71, 10040),
            ref2_op(0x0501, 0x72, 10041, 0x24),
            ref3_op(0x0502, 0x73, 5, CROSS_FORMSET),
            ref5_op(0x0503, 0x74),
            end(),
            end(),
            end(),
        ]
        .concat();
        let texts: HashMap<u16, String> = [
            (0x0500u16, "Goto A".to_string()),
            (0x0501, "Goto Q".to_string()),
            (0x0502, "Goto cross".to_string()),
        ]
        .into();
        let mut lossy = Vec::new();
        let items = collect_items(&package(&ifr), 10029, &texts, &mut lossy);
        assert_eq!(items.len(), 2, "REF1/REF2 exported, REF3/REF5 skipped");
        match &items[0] {
            schema::ItemSchema::Ref(r) => {
                assert_eq!(r.prompt, "Goto A");
                assert_eq!(r.question_id, 0x71);
                assert_eq!(r.form_id, 10040);
            }
            _ => panic!("expected Ref"),
        }
        match &items[1] {
            schema::ItemSchema::Ref(r) => {
                assert_eq!(r.prompt, "Goto Q");
                assert_eq!(r.form_id, 10041);
            }
            _ => panic!("expected Ref"),
        }
        assert_eq!(
            lossy,
            vec![
                "ref_question_target:1".to_string(),
                "cross_formset_ref:1".to_string(),
                "dynamic_ref:1".to_string(),
            ]
        );
    }

    #[test]
    fn lossy_counts_gates_and_unknown_statement_ops() {
        let ifr = [
            form_set(7),
            form(10029, 21),
            gate_op(IFR_SUPPRESS_IF_OP),
            checkbox(0x0700, 0x0701, 0x80),
            end(),
            gate_op(IFR_GRAY_OUT_IF_OP),
            checkbox(0x0702, 0x0703, 0x81),
            end(),
            gate_op(IFR_SUPPRESS_IF_OP),
            end(),
            password_op(0x0704, 0x82),
            end(),
            end(),
            end(),
        ]
        .concat();
        let mut lossy = Vec::new();
        let items = collect_items(&package(&ifr), 10029, &HashMap::new(), &mut lossy);
        assert_eq!(items.len(), 2, "questions under gates are exported");
        assert_eq!(
            lossy,
            vec![
                "suppress_if:2".to_string(),
                "grayout_if:1".to_string(),
                "unknown_op_08:1".to_string(),
            ]
        );
    }

    #[test]
    fn fill_varstores_referenced_only_name_value_and_efi() {
        let ifr = [
            form_set(7),
            varstore_buffer(1, 0x72, "Setup"),
            varstore_name_value(2, "NV"),
            varstore_efi(4, 5, 0x0F, "EfVar"),
            end(),
        ]
        .concat();
        let pkg = package(&ifr);
        let item = |store: u16| {
            schema::ItemSchema::CheckBox(schema::CheckBoxItem {
                prompt: "p".into(),
                help: "h".into(),
                question_id: store,
                var_store_id: store,
                var_offset: 0,
                defaults: schema::Defaults::default(),
            })
        };
        let items = vec![item(1), item(2), item(0), item(4), item(9)];
        let mut lossy = Vec::new();
        let varstores = fill_varstores(&items, &pkg, &mut lossy);
        let expected = vec![
            schema::VarStoreSchema {
                id: 1,
                guid: VARSTORE_GUID_STR.to_string(),
                size: 0x72,
                name: "Setup".into(),
                var_type: schema::VarStoreType::Buffer,
                attributes: 7,
            },
            schema::VarStoreSchema {
                id: 4,
                guid: VARSTORE_GUID_STR.to_string(),
                size: 5,
                name: "EfVar".into(),
                var_type: schema::VarStoreType::Efi,
                attributes: 0x0F,
            },
        ];
        assert_eq!(
            serde_json::to_value(&varstores).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(
            lossy,
            vec!["varstore 0x2 is name-value, not exportable".to_string()]
        );
    }

    #[test]
    fn builder_symmetry_round_trip() {
        use crate::hii::ifr_builder::{
            DEFAULT_ID_STANDARD, IFR_DISPLAY_UINT_DEC, IFR_DISPLAY_UINT_HEX, IFR_OPTION_DEFAULT,
            IfrBuilder, TYPE_NUM_SIZE_8,
        };
        let g = Guid::from_str(FORMSET_GUID).unwrap();
        let vsg = Guid::from_str(VARSTORE_GUID_STR).unwrap();
        let mut b = IfrBuilder::new();
        b.emit_form_set(&g, 7, 8, &[]);
        b.emit_var_store(1, &vsg, 0x72, "Setup");
        b.emit_form(10029, 21);
        b.emit_text(0x0100, 0x0101, 0x0102);
        b.emit_one_of(0x01A3, 0x01A4, 0x003B, 1, 0x003A, IFR_DISPLAY_UINT_DEC, 1);
        b.emit_one_of_option(4, IFR_OPTION_DEFAULT, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_one_of_option(3, 0, TYPE_NUM_SIZE_8, 1, 1);
        b.emit_default(DEFAULT_ID_STANDARD, TYPE_NUM_SIZE_8, 0, 1);
        b.emit_numeric(
            0x0201,
            0x0202,
            0x0055,
            1,
            0x0010,
            IFR_DISPLAY_UINT_HEX,
            1,
            5,
            9,
            2,
        );
        b.emit_check_box(0x0300, 0x0301, 0x0060, 1, 0x0020, 0);
        b.emit_ref(0x0400, 0x0401, 0x0070, 0xFFFF, 0, 10030);
        b.emit_end();
        b.emit_end();
        let pkg = package(&b.build());
        let texts: HashMap<u16, String> = [
            (0x0100u16, "Notice".to_string()),
            (0x0101, "Notice help".to_string()),
            (0x0102, "Second line".to_string()),
            (0x01A3, "Mode".to_string()),
            (0x01A4, "Mode help".to_string()),
            (4, "Disabled".to_string()),
            (3, "Enabled".to_string()),
            (0x0201, "Ratio".to_string()),
            (0x0202, "Ratio help".to_string()),
            (0x0300, "Turbo".to_string()),
            (0x0301, "Turbo help".to_string()),
            (0x0400, "Sub page".to_string()),
            (0x0401, "Sub page help".to_string()),
        ]
        .into();
        let mut lossy = Vec::new();
        let items = collect_items(&pkg, 10029, &texts, &mut lossy);
        assert!(
            lossy.is_empty(),
            "symmetry walk must be loss-free: {lossy:?}"
        );
        let expected = vec![
            schema::ItemSchema::Text(schema::TextItem {
                prompt: "Notice".into(),
                help: "Notice help".into(),
                text_two: "Second line".into(),
            }),
            schema::ItemSchema::OneOf(schema::OneOfItem {
                prompt: "Mode".into(),
                help: "Mode help".into(),
                question_id: 0x003B,
                var_store_id: 1,
                var_offset: 0x003A,
                size: 1,
                display: schema::DisplayMode::UintDec,
                options: vec![
                    schema::OptionSchema {
                        text: "Disabled".into(),
                        value: 0,
                        default: Some(schema::DefaultClass::Optimized),
                    },
                    schema::OptionSchema {
                        text: "Enabled".into(),
                        value: 1,
                        default: None,
                    },
                ],
                defaults: schema::Defaults {
                    optimized: Some(0),
                    failsafe: None,
                },
            }),
            schema::ItemSchema::Numeric(schema::NumericItem {
                prompt: "Ratio".into(),
                help: "Ratio help".into(),
                question_id: 0x0055,
                var_store_id: 1,
                var_offset: 0x0010,
                size: 1,
                min: 5,
                max: 9,
                step: 2,
                display: schema::DisplayMode::UintHex,
                defaults: schema::Defaults::default(),
            }),
            schema::ItemSchema::CheckBox(schema::CheckBoxItem {
                prompt: "Turbo".into(),
                help: "Turbo help".into(),
                question_id: 0x0060,
                var_store_id: 1,
                var_offset: 0x0020,
                defaults: schema::Defaults::default(),
            }),
            schema::ItemSchema::Ref(schema::RefItem {
                prompt: "Sub page".into(),
                help: "Sub page help".into(),
                question_id: 0x0070,
                form_id: 10030,
            }),
        ];
        assert_eq!(
            serde_json::to_value(&items).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        let varstores = fill_varstores(&items, &pkg, &mut lossy);
        let expected_vs = vec![schema::VarStoreSchema {
            id: 1,
            guid: VARSTORE_GUID_STR.to_string(),
            size: 0x72,
            name: "Setup".into(),
            var_type: schema::VarStoreType::Buffer,
            attributes: 7,
        }];
        assert_eq!(
            serde_json::to_value(&varstores).unwrap(),
            serde_json::to_value(&expected_vs).unwrap()
        );
        assert!(lossy.is_empty(), "{lossy:?}");
    }

    #[test]
    fn export_form_end_to_end_parent_varstores_lossy() {
        let ifr = [
            form_set(7),
            varstore_buffer(1, 0x72, "Setup"),
            varstore_name_value(2, "NV"),
            form(10001, 30),
            ref_op(0x10, 0x30, 10019),
            end(),
            form(10019, 21),
            one_of(0x01A3, 0x01A4, 0x003B, 0x10),
            default_op(0, 1, &[0]),
            option(4, r_efi::hii::IFR_OPTION_DEFAULT, &[0]),
            option(3, 0x00, &[1]),
            end(),
            checkbox_on_store(0x0800, 0x0801, 0x90, 2),
            end(),
            ref3_op(0x20, 0x31, 5, CROSS_FORMSET),
            end(),
            end(),
        ]
        .concat();
        let ex = export_form(&image_with(package(&ifr)), &item_id(10019)).unwrap();
        assert_eq!(ex.formset_guid, FORMSET_GUID);
        assert_eq!(ex.parent_form_id, 10001);
        assert_eq!(
            ex.parent_entries.len(),
            1,
            "единственный GOTO родителя → одна entry (текст без strings-пакета пуст)"
        );
        assert_eq!(
            ex.lossy,
            vec![
                "cross_formset_ref:1".to_string(),
                "varstore 0x2 is name-value, not exportable".to_string(),
            ]
        );
        assert_eq!(ex.schema.forms.len(), 1);
        let form = &ex.schema.forms[0];
        assert_eq!(form.id, 10019);
        assert_eq!(form.title, "Form 10019");
        assert_eq!(form.items.len(), 2);
        assert!(matches!(form.items[0], schema::ItemSchema::OneOf(_)));
        assert!(matches!(form.items[1], schema::ItemSchema::CheckBox(_)));
        assert_eq!(ex.schema.varstores.len(), 1);
        assert_eq!(ex.schema.varstores[0].id, 1);
        assert_eq!(ex.schema.varstores[0].name, "Setup");
        assert_eq!(
            ex.schema.varstores[0].var_type,
            schema::VarStoreType::Buffer
        );
        assert_eq!(ex.schema.formset_guid, FORMSET_GUID);
    }

    #[test]
    fn export_form_parent_dedup_same_parent_and_ambiguity() {
        let duplicated = [
            form_set(7),
            form(10001, 30),
            ref_op(0x10, 0x30, 10019),
            ref_op(0x11, 0x33, 10019),
            end(),
            form(10019, 21),
            checkbox(0x0900, 0x0901, 0x91),
            end(),
            end(),
            end(),
        ]
        .concat();
        let ex = export_form(&image_with(package(&duplicated)), &item_id(10019)).unwrap();
        assert_eq!(ex.parent_form_id, 10001, "two GOTOs from one parent dedup");
        assert_eq!(
            ex.parent_entries.len(),
            2,
            "multi-GOTO: по entry на каждый GOTO"
        );

        let ambiguous = [
            form_set(7),
            form(10001, 30),
            ref_op(0x10, 0x30, 10019),
            end(),
            form(10002, 31),
            ref_op(0x11, 0x33, 10019),
            end(),
            form(10019, 21),
            checkbox(0x0900, 0x0901, 0x91),
            end(),
            end(),
            end(),
        ]
        .concat();
        let ex = export_form(&image_with(package(&ambiguous)), &item_id(10019)).unwrap();
        assert_eq!(ex.parent_form_id, 0, "two distinct parents → no parent");
        assert!(ex.parent_entries.is_empty());

        let orphan = [
            form_set(7),
            form(10019, 21),
            checkbox(0x0900, 0x0901, 0x91),
            end(),
            end(),
            end(),
        ]
        .concat();
        let ex = export_form(&image_with(package(&orphan)), &item_id(10019)).unwrap();
        assert_eq!(ex.parent_form_id, 0, "no inbound refs → no parent");
        assert!(ex.parent_entries.is_empty());
        assert!(ex.lossy.is_empty());
    }

    #[test]
    fn export_form_parent_entries_resolve_prompt_help() {
        let ifr = [
            form_set(7),
            form(10001, 30),
            ref_op_ph(1, 2, 0x30, 10019),
            ref_op_ph(3, 4, 0x33, 10019),
            ref_op_ph(5, 6, 0x34, 10020),
            end(),
            form(10019, 21),
            checkbox(0x0900, 0x0901, 0x91),
            end(),
            form(10020, 22),
            end(),
            end(),
            end(),
        ]
        .concat();
        let img = image_with_sections(vec![
            package(&ifr),
            string_package(
                "eng",
                &[
                    "PCI Subsystem Settings",
                    "Open PCI subsystem settings",
                    "Second entry",
                    "Second entry help",
                    "Other target",
                    "Other target help",
                ],
            ),
        ]);
        let ex = export_form(&img, &item_id(10019)).unwrap();
        assert_eq!(ex.parent_form_id, 10001);
        let entries: Vec<(&str, &str)> = ex
            .parent_entries
            .iter()
            .map(|e| (e.prompt.as_str(), e.help.as_str()))
            .collect();
        assert_eq!(
            entries,
            vec![
                ("PCI Subsystem Settings", "Open PCI subsystem settings"),
                ("Second entry", "Second entry help"),
            ],
            "GOTO на 10020 в entries не попадает"
        );
    }
}
