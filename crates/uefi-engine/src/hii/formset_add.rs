use std::collections::HashMap;

use crate::ops;
use crate::types::*;

use super::HiiError;
use super::ami_patcher::{self, QuestionAmiRecord};
use super::ffs_assembler;
use super::ifr_builder::*;
use super::schema;
use super::string_pack;

pub struct AddSetupResult {
    pub new_ffs_guid: Guid,
    pub inserted_form_ids: Vec<u16>,
    pub string_ids: HashMap<String, u16>,
}

pub fn add_setup_formset(
    image: &mut Image,
    schema: &schema::FormSetSchema,
    target_ffs_guid: Option<&Guid>,
) -> Result<AddSetupResult, HiiError> {
    let formset_guid: Guid = Guid::try_parse(&schema.formset_guid)
        .map_err(|e| HiiError::InvalidSchema(e.to_string()))?;
    let new_ffs_guid = Guid::try_parse(&format!(
        "{:08X}-BEEF-1234-8000-000000000001",
        0xB00B0000 + image.root.children.len() as u32,
    ))
    .map_err(|e| HiiError::IfrBuildError(e.to_string()))?;
    let mut strings: Vec<String> = Vec::new();
    strings.push(schema.title.clone());
    strings.push(schema.help.clone());
    for vs in &schema.varstores {
        strings.push(vs.name.clone());
    }
    for form in &schema.forms {
        strings.push(form.title.clone());
        for item in &form.items {
            collect_item_strings(item, &mut strings);
        }
    }
    let string_ids = string_pack::add_strings(image, target_ffs_guid, &strings)?;
    let ifr_bytes = build_ifr(schema, &string_ids)?;
    let (sp_vi, sp_fi, sp_si) = find_string_package_section(image, target_ffs_guid)?;
    let strpkg_bytes = image.root.children[sp_vi].children[sp_fi].children[sp_si]
        .body
        .clone();
    let ffs_bytes = ffs_assembler::assemble_ffs(&ifr_bytes, &strpkg_bytes, &new_ffs_guid)?;
    let (form_ids, questions) = extract_form_ids_and_questions(schema);
    let setupdata_guid = schema
        .setupdata_guid
        .as_ref()
        .and_then(|s| Guid::try_parse(s).ok());
    let amitse_guid = schema
        .amitse_guid
        .as_ref()
        .and_then(|s| Guid::try_parse(s).ok());
    ami_patcher::patch_ami(
        image,
        &formset_guid,
        &form_ids,
        &questions,
        setupdata_guid.as_ref(),
        amitse_guid.as_ref(),
    )?;
    let target = Target::Path(vec![sp_vi]);
    ops::insert(&mut image.root, &target, &ffs_bytes, ops::InsertMode::Into)
        .map_err(|e| HiiError::FfsAssemblyError(e.to_string()))?;
    Ok(AddSetupResult {
        new_ffs_guid,
        inserted_form_ids: form_ids,
        string_ids,
    })
}

fn collect_item_strings(item: &schema::ItemSchema, strings: &mut Vec<String>) {
    match item {
        schema::ItemSchema::OneOf(o) => {
            strings.push(o.prompt.clone());
            strings.push(o.help.clone());
            for opt in &o.options {
                strings.push(opt.text.clone());
            }
        }
        schema::ItemSchema::CheckBox(c) => {
            strings.push(c.prompt.clone());
            strings.push(c.help.clone());
        }
        schema::ItemSchema::Numeric(n) => {
            strings.push(n.prompt.clone());
            strings.push(n.help.clone());
        }
        schema::ItemSchema::Text(t) => {
            strings.push(t.prompt.clone());
            strings.push(t.help.clone());
            strings.push(t.text_two.clone());
        }
        schema::ItemSchema::Ref(r) => {
            strings.push(r.prompt.clone());
            strings.push(r.help.clone());
        }
        schema::ItemSchema::String(s) => {
            strings.push(s.prompt.clone());
            strings.push(s.help.clone());
        }
        schema::ItemSchema::Action(a) => {
            strings.push(a.prompt.clone());
            strings.push(a.help.clone());
            strings.push(a.config.clone());
        }
        schema::ItemSchema::OrderedList(o) => {
            strings.push(o.prompt.clone());
            strings.push(o.help.clone());
        }
    }
}

fn build_ifr(
    schema: &schema::FormSetSchema,
    string_ids: &HashMap<String, u16>,
) -> Result<Vec<u8>, HiiError> {
    let mut b = IfrBuilder::new();
    let formset_guid: Guid = Guid::try_parse(&schema.formset_guid)
        .map_err(|e| HiiError::InvalidSchema(e.to_string()))?;
    let title_id = string_ids[&schema.title];
    let help_id = string_ids[&schema.help];
    let class_guids: Vec<Guid> = schema
        .class_guids
        .iter()
        .filter_map(|s| Guid::try_parse(s).ok())
        .collect();
    b.emit_form_set(&formset_guid, title_id, help_id, &class_guids);
    for vs in &schema.varstores {
        let g: Guid =
            Guid::try_parse(&vs.guid).map_err(|e| HiiError::InvalidSchema(e.to_string()))?;
        b.emit_var_store(vs.id, &g, vs.size, &vs.name);
    }
    for ds in &schema.default_stores {
        b.emit_default_store(string_ids[&ds.name], ds.id);
    }
    for form in &schema.forms {
        let title_id = string_ids[&form.title];
        b.emit_form(form.id, title_id);
        for item in &form.items {
            emit_item(&mut b, item, string_ids);
        }
        b.emit_end();
    }
    b.emit_end();
    Ok(b.build())
}

fn emit_item(b: &mut IfrBuilder, item: &schema::ItemSchema, string_ids: &HashMap<String, u16>) {
    let display_flags = |d: schema::DisplayMode| -> u8 {
        match d {
            schema::DisplayMode::IntDec => IFR_DISPLAY_INT_DEC,
            schema::DisplayMode::UintDec => IFR_DISPLAY_UINT_DEC,
            schema::DisplayMode::UintHex => IFR_DISPLAY_UINT_HEX,
        }
    };
    match item {
        schema::ItemSchema::OneOf(o) => {
            let pid = string_ids[&o.prompt];
            let hid = string_ids[&o.help];
            b.emit_one_of(
                pid,
                hid,
                o.question_id,
                o.var_store_id,
                o.var_offset,
                display_flags(o.display),
                o.size,
            );
            for opt in &o.options {
                let tid = string_ids[&opt.text];
                let mut flags = 0u8;
                match opt.default {
                    Some(schema::DefaultClass::Optimized) => flags |= IFR_OPTION_DEFAULT,
                    Some(schema::DefaultClass::Failsafe) => flags |= IFR_OPTION_DEFAULT_MFG,
                    None => {}
                }
                b.emit_one_of_option(tid, flags, o.size - 1, opt.value, o.size);
            }
            if let Some(v) = o.defaults.optimized {
                b.emit_default(DEFAULT_ID_STANDARD, o.size - 1, v, o.size);
            }
            if let Some(v) = o.defaults.failsafe {
                b.emit_default(DEFAULT_ID_MANUFACTURING, o.size - 1, v, o.size);
            }
            b.emit_end();
        }
        schema::ItemSchema::CheckBox(c) => {
            let pid = string_ids[&c.prompt];
            let hid = string_ids[&c.help];
            let mut cbflags = 0u8;
            if c.defaults.optimized.unwrap_or(0) != 0 {
                cbflags |= IFR_CHECKBOX_DEFAULT;
            }
            if c.defaults.failsafe.unwrap_or(0) != 0 {
                cbflags |= IFR_CHECKBOX_DEFAULT_MFG;
            }
            b.emit_check_box(
                pid,
                hid,
                c.question_id,
                c.var_store_id,
                c.var_offset,
                cbflags,
            );
            b.emit_end();
        }
        schema::ItemSchema::Numeric(n) => {
            let pid = string_ids[&n.prompt];
            let hid = string_ids[&n.help];
            b.emit_numeric(
                pid,
                hid,
                n.question_id,
                n.var_store_id,
                n.var_offset,
                display_flags(n.display),
                n.size,
                n.min,
                n.max,
                n.step,
            );
            if let Some(v) = n.defaults.optimized {
                b.emit_default(DEFAULT_ID_STANDARD, n.size - 1, v, n.size);
            }
            if let Some(v) = n.defaults.failsafe {
                b.emit_default(DEFAULT_ID_MANUFACTURING, n.size - 1, v, n.size);
            }
            b.emit_end();
        }
        schema::ItemSchema::Text(t) => {
            let pid = string_ids[&t.prompt];
            let hid = string_ids[&t.help];
            let t2 = string_ids[&t.text_two];
            b.emit_text(pid, hid, t2);
        }
        schema::ItemSchema::Ref(r) => {
            let pid = string_ids[&r.prompt];
            let hid = string_ids[&r.help];
            b.emit_ref(pid, hid, r.question_id, 0, 0, r.form_id);
        }
        schema::ItemSchema::String(_)
        | schema::ItemSchema::Action(_)
        | schema::ItemSchema::OrderedList(_) => {
            b.emit_end();
        }
    }
}

fn extract_form_ids_and_questions(
    schema: &schema::FormSetSchema,
) -> (Vec<u16>, Vec<QuestionAmiRecord>) {
    let form_ids: Vec<u16> = schema.forms.iter().map(|f| f.id).collect();
    let mut questions = Vec::new();
    for form in &schema.forms {
        for item in &form.items {
            if let Some((qid, page_id, failsafe, optimal)) = extract_question(item) {
                questions.push(QuestionAmiRecord {
                    question_id: qid,
                    page_id,
                    access_level: ami_patcher::AMI_DEFAULT_ACCESS_LEVEL,
                    failsafe,
                    optimal,
                });
            }
        }
    }
    (form_ids, questions)
}

fn extract_question(item: &schema::ItemSchema) -> Option<(u16, Option<u16>, u8, u8)> {
    match item {
        schema::ItemSchema::OneOf(o) => Some((
            o.question_id,
            None,
            o.defaults.failsafe.unwrap_or(0) as u8,
            o.defaults.optimized.unwrap_or(0) as u8,
        )),
        schema::ItemSchema::CheckBox(c) => Some((
            c.question_id,
            None,
            c.defaults.failsafe.unwrap_or(0) as u8,
            c.defaults.optimized.unwrap_or(0) as u8,
        )),
        schema::ItemSchema::Numeric(n) => Some((
            n.question_id,
            None,
            n.defaults.failsafe.unwrap_or(0) as u8,
            n.defaults.optimized.unwrap_or(0) as u8,
        )),
        schema::ItemSchema::Ref(r) => Some((r.question_id, Some(r.form_id), 0, 0)),
        _ => None,
    }
}

fn find_string_package_section(
    image: &Image,
    ffs_guid: Option<&Guid>,
) -> Result<(usize, usize, usize), HiiError> {
    for (vi, vol) in image.root.children.iter().enumerate() {
        for (fi, file) in vol.children.iter().enumerate() {
            if let Some(g) = ffs_guid
                && file.guid != Some(*g)
            {
                continue;
            }
            for (si, sec) in file.children.iter().enumerate() {
                if string_pack::is_string_package(&sec.body) {
                    return Ok((vi, fi, si));
                }
            }
        }
    }
    Err(HiiError::StringPackageNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::build_image;
    use crate::ffs::{EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE, EFI_SECTION_RAW, size_to_uint24};
    use crate::parser::image::parse_image;
    use crate::parser::target::find_item_path;

    const STR_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    const SETUPDATA_GUID: &str = "12345678-90AB-CDEF-1234-567890ABCDEF";
    const AMITSE_GUID: &str = "87654321-FEDC-BA09-8765-432109FEDCBA";

    fn section_bytes(stype: u8, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + body.len());
        v.extend_from_slice(&size_to_uint24((4 + body.len()) as u32));
        v.push(stype);
        v.extend_from_slice(body);
        v
    }

    fn ffs_file_bytes(guid: &Guid, content: &[u8]) -> Vec<u8> {
        let mut buf = vec![0u8; 24];
        buf[0..16].copy_from_slice(&guid.to_bytes());
        buf[18] = crate::ffs::EFI_FV_FILETYPE_RAW;
        buf[20..23].copy_from_slice(&size_to_uint24((24 + content.len()) as u32));
        buf.extend_from_slice(content);
        buf
    }

    fn string_package_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, string_pack::PACKAGE_STRINGS]);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.push(0x10);
        buf.extend_from_slice(b"first");
        buf.push(0x00);
        buf.push(0x00);
        let len = buf.len() as u32;
        buf[0] = (len & 0xFF) as u8;
        buf[1] = ((len >> 8) & 0xFF) as u8;
        buf[2] = ((len >> 16) & 0xFF) as u8;
        buf
    }

    fn gap_aware_image() -> Vec<u8> {
        let string_file = ffs_file_bytes(
            &Guid::try_parse(STR_GUID).unwrap(),
            &section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
        );
        let setupdata = ffs_file_bytes(&Guid::try_parse(SETUPDATA_GUID).unwrap(), &[0u8; 108]);
        let amitse = ffs_file_bytes(&Guid::try_parse(AMITSE_GUID).unwrap(), &[0u8; 108]);

        let mut body = Vec::new();
        for f in [string_file, setupdata, amitse] {
            let aligned = (body.len() + 7) & !7;
            body.resize(aligned, 0xFF);
            body.extend_from_slice(&f);
        }
        body.extend_from_slice(&[0xFF; 1024]);

        let total = 56 + body.len();
        let mut buf = vec![0xFFu8; 32 + total];
        let fv = &mut buf[32..];
        fv[32..40].copy_from_slice(&(total as u64).to_le_bytes());
        fv[40..44].copy_from_slice(&EFI_FVH_SIGNATURE.to_le_bytes());
        fv[44..48].copy_from_slice(&EFI_FVB2_ERASE_POLARITY.to_le_bytes());
        fv[48..50].copy_from_slice(&56u16.to_le_bytes());
        fv[55] = 2;
        fv[56..].copy_from_slice(&body);
        buf
    }

    fn test_schema() -> schema::FormSetSchema {
        schema::FormSetSchema {
            formset_guid: "A1B2C3D4-E5F6-7890-ABCD-EF1234567890".into(),
            title: "T".into(),
            help: "H".into(),
            class_guids: vec![],
            varstores: vec![],
            default_stores: vec![],
            forms: vec![schema::FormSchema {
                id: 1,
                title: "Main".into(),
                items: vec![schema::ItemSchema::Text(schema::TextItem {
                    prompt: "P1".into(),
                    help: "H1".into(),
                    text_two: "X1".into(),
                })],
            }],
            setupdata_guid: Some(SETUPDATA_GUID.into()),
            amitse_guid: Some(AMITSE_GUID.into()),
        }
    }

    #[test]
    fn add_setup_formset_inserts_into_volume_on_gap_aware_image() {
        let data = gap_aware_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        assert_eq!(img.root.children[0].node_type, FfsType::Padding);
        assert_eq!(img.root.children[1].node_type, FfsType::Volume);

        let res = add_setup_formset(&mut img, &test_schema(), None).unwrap();
        let built = build_image(&img).unwrap();
        assert_eq!(built.len(), data.len());
        let guid_bytes = res.new_ffs_guid.to_bytes();
        assert!(
            built.windows(16).any(|w| w == guid_bytes),
            "new FFS must be present in built image"
        );

        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let path = find_item_path(&re.root, &Target::Guid(res.new_ffs_guid)).unwrap();
        assert_eq!(
            path[0], 1,
            "new FFS must live inside the Volume, not the Padding"
        );
        assert_eq!(res.inserted_form_ids, vec![1]);
    }
}
