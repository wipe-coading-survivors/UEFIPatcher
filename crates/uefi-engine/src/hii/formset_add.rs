use std::collections::HashMap;

use crate::ffs::{EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32};
use crate::ops;
use crate::types::*;

use super::HiiError;
use super::ami_patcher::{self, QuestionAmiRecord};
use super::ffs_assembler;
use super::ifr_builder::*;
use super::pe_resource;
use super::schema;
use super::string_pack;
use r_efi::hii::PACKAGE_FORMS;

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
    let setupdata_guid = schema
        .setupdata_guid
        .as_ref()
        .and_then(|s| Guid::try_parse(s).ok());
    let amitse_guid = schema
        .amitse_guid
        .as_ref()
        .and_then(|s| Guid::try_parse(s).ok());
    let (form_ids, questions) = extract_form_ids_and_questions(schema);
    let sp_path = string_pack::string_package_section_path(&image.root, target_ffs_guid)
        .filter(|p| p.len() == 3);
    if let Some(sp_path) = sp_path {
        ami_patcher::precheck_ami_modules(image, setupdata_guid.as_ref(), amitse_guid.as_ref())?;
        let (sp_vi, sp_fi, sp_si) = (sp_path[0], sp_path[1], sp_path[2]);
        let string_ids = string_pack::add_strings(image, target_ffs_guid, &strings)?;
        let ifr_bytes = build_ifr(schema, &string_ids)?;
        let strpkg_bytes = image.root.children[sp_vi].children[sp_fi].children[sp_si]
            .body
            .clone();
        let new_ffs_guid = Guid::try_parse(&format!(
            "{:08X}-BEEF-1234-8000-000000000001",
            0xB00B0000 + image.root.children.len() as u32,
        ))
        .map_err(|e| HiiError::IfrBuildError(e.to_string()))?;
        let ffs_bytes = ffs_assembler::assemble_ffs(&ifr_bytes, &strpkg_bytes, &new_ffs_guid)?;
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
    } else {
        let pe_path = pe_resource_channel_path(&image.root, target_ffs_guid)?;
        let Some(pe_path) = pe_path else {
            return Err(HiiError::StringPackageNotFound);
        };
        ami_patcher::precheck_ami_modules(image, setupdata_guid.as_ref(), amitse_guid.as_ref())?;
        let string_ids = {
            let mut node = &mut image.root;
            for &i in &pe_path {
                node = &mut node.children[i];
            }
            let string_ids = string_pack::add_strings_to_resource(&mut node.body, &strings)
                .map_err(|e| match e {
                    string_pack::AddStringsToResourceError::NotFound => {
                        HiiError::StringPackageNotFound
                    }
                    string_pack::AddStringsToResourceError::GrowthUnsupported => {
                        HiiError::PeGrowthUnsupported
                    }
                })?;
            let ifr_bytes = build_ifr(schema, &string_ids)?;
            let form_pkg_len = 4 + ifr_bytes.len();
            let mut form_pkg = Vec::with_capacity(form_pkg_len);
            form_pkg.push((form_pkg_len & 0xFF) as u8);
            form_pkg.push(((form_pkg_len >> 8) & 0xFF) as u8);
            form_pkg.push(((form_pkg_len >> 16) & 0xFF) as u8);
            form_pkg.push(PACKAGE_FORMS);
            form_pkg.extend_from_slice(&ifr_bytes);
            if !pe_resource::append_package_to_resource(&mut node.body, &form_pkg) {
                return Err(HiiError::PeGrowthUnsupported);
            }
            string_ids
        };
        ami_patcher::patch_ami(
            image,
            &formset_guid,
            &form_ids,
            &questions,
            setupdata_guid.as_ref(),
            amitse_guid.as_ref(),
        )?;
        ops::mark_rebuild_to_root_by_path(&mut image.root, &pe_path);
        Ok(AddSetupResult {
            new_ffs_guid: formset_guid,
            inserted_form_ids: form_ids,
            string_ids,
        })
    }
}

fn pe_resource_channel_path(
    root: &FfsNode,
    ffs_guid: Option<&Guid>,
) -> Result<Option<Vec<usize>>, HiiError> {
    let mut skipped_behind_bad_wrapper = false;
    let found = walk_for_pe_resource_channel(
        root,
        ffs_guid,
        None,
        false,
        &mut Vec::new(),
        &mut skipped_behind_bad_wrapper,
    );
    match found {
        Some(path) => Ok(Some(path)),
        None if skipped_behind_bad_wrapper => Err(HiiError::MutationBehindCompression),
        None => Ok(None),
    }
}

fn walk_for_pe_resource_channel(
    node: &FfsNode,
    ffs_guid: Option<&Guid>,
    owner: Option<&Guid>,
    behind_bad_wrapper: bool,
    path: &mut Vec<usize>,
    skipped_behind_bad_wrapper: &mut bool,
) -> Option<Vec<usize>> {
    for (i, child) in node.children.iter().enumerate() {
        let child_owner = if child.node_type == FfsType::File {
            child.guid.as_ref()
        } else {
            owner
        };
        if child.node_type == FfsType::Section
            && child.children.is_empty()
            && child.subtype == EFI_SECTION_PE32
            && ffs_guid.is_none_or(|g| owner == Some(g))
            && string_pack::pe_resource_has_string_package(&child.body)
        {
            if behind_bad_wrapper {
                *skipped_behind_bad_wrapper = true;
                continue;
            }
            path.push(i);
            return Some(path.clone());
        }
        path.push(i);
        if let Some(found) = walk_for_pe_resource_channel(
            child,
            ffs_guid,
            child_owner,
            behind_bad_wrapper || is_non_recompressable_wrapper(child),
            path,
            skipped_behind_bad_wrapper,
        ) {
            return Some(found);
        }
        path.pop();
    }
    None
}

pub(crate) fn is_non_recompressable_wrapper(node: &FfsNode) -> bool {
    node.node_type == FfsType::Section
        && (node.subtype == EFI_SECTION_COMPRESSION || node.subtype == EFI_SECTION_GUID_DEFINED)
        && !matches!(
            &node.parsing_data,
            crate::types::ParsingData::GuidedSection(d)
                if crate::ffs::is_recompressable_lzma_guid(&d.guid)
        )
}

pub(crate) fn collect_item_strings(item: &schema::ItemSchema, strings: &mut Vec<String>) {
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
    emit_forms(&mut b, schema, string_ids);
    b.emit_end();
    Ok(b.build())
}

pub(crate) fn emit_forms(
    b: &mut IfrBuilder,
    schema: &schema::FormSetSchema,
    string_ids: &HashMap<String, u16>,
) {
    for form in &schema.forms {
        let title_id = string_ids[&form.title];
        b.emit_form(form.id, title_id);
        for item in &form.items {
            emit_item(b, item, string_ids);
        }
        b.emit_end();
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::build_image;
    use crate::ffs::{
        EFI_FVB2_ERASE_POLARITY, EFI_FVH_SIGNATURE, EFI_SECTION_PE32, EFI_SECTION_RAW,
        size_to_uint24,
    };
    use crate::hii::forms::collect_forms;
    use crate::hii::package_list::parse_package_list;
    use crate::hii::pe_resource::hii_entry_locations;
    use crate::hii::strings::parse_string_package;
    use crate::parser::image::parse_image;
    use crate::parser::target::find_item_path;
    use crate::types::GuidedSectionParsingData;
    use r_efi::hii::{PACKAGE_END, PACKAGE_FORMS};

    fn mk_node(node_type: FfsType, body: Vec<u8>, children: Vec<FfsNode>) -> FfsNode {
        FfsNode {
            guid: None,
            node_type,
            subtype: 0,
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

    fn flash_with_files(files: Vec<Vec<u8>>) -> Vec<u8> {
        let mut body = Vec::new();
        for f in files {
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

    fn gap_aware_image(with_ami: bool) -> Vec<u8> {
        let string_file = ffs_file_bytes(
            &Guid::try_parse(STR_GUID).unwrap(),
            &section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
        );
        let mut files = vec![string_file];
        if with_ami {
            files.push(ffs_file_bytes(
                &Guid::try_parse(SETUPDATA_GUID).unwrap(),
                &[0u8; 108],
            ));
            files.push(ffs_file_bytes(
                &Guid::try_parse(AMITSE_GUID).unwrap(),
                &[0u8; 108],
            ));
        }
        flash_with_files(files)
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
        let data = gap_aware_image(true);
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

    #[test]
    fn add_setup_formset_fails_cleanly_when_package_is_wrapped() {
        let pkg = string_package_bytes();
        let pkg_len = pkg.len();
        let inner = mk_node(FfsType::Section, pkg, vec![]);
        let wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        let mut file_node = mk_node(FfsType::File, vec![], vec![wrapper]);
        file_node.guid = Some(Guid::try_parse(STR_GUID).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file_node]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut img = Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };

        assert!(matches!(
            add_setup_formset(&mut img, &test_schema(), None),
            Err(HiiError::StringPackageNotFound)
        ));
        let inner = &img.root.children[0].children[0].children[0].children[0];
        assert_eq!(inner.body.len(), pkg_len);
        assert_eq!(inner.action, Action::NoAction);
        assert_eq!(img.root.children[0].children[0].action, Action::NoAction);
        assert_eq!(img.root.children[0].action, Action::NoAction);
        assert_eq!(img.root.action, Action::NoAction);
    }

    const LIST_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";
    const FORMSET_GUID: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";

    fn hii_pkg(kind: u8, payload: &[u8]) -> Vec<u8> {
        let len = 4 + payload.len();
        let mut b = vec![
            (len & 0xFF) as u8,
            ((len >> 8) & 0xFF) as u8,
            ((len >> 16) & 0xFF) as u8,
            kind,
        ];
        b.extend_from_slice(payload);
        b
    }

    fn hii_list(guid: &Guid, pkgs: &[&[u8]]) -> Vec<u8> {
        let total = 20 + 4 + pkgs.iter().map(|p| p.len()).sum::<usize>();
        let mut b = guid.to_bytes().to_vec();
        b.extend_from_slice(&(total as u32).to_le_bytes());
        for p in pkgs {
            b.extend_from_slice(p);
        }
        b.extend_from_slice(&[0x04, 0x00, 0x00, PACKAGE_END]);
        b
    }

    fn resource_hii_pe() -> Vec<u8> {
        let g = Guid::try_parse(LIST_GUID).unwrap();
        let form = hii_pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = string_package_bytes();
        let blob = hii_list(&g, &[&form, &string]);
        crate::hii::pe_resource::synth_hii_pe("HII", &blob)
    }

    fn resource_reloc_hii_pe() -> Vec<u8> {
        let g = Guid::try_parse(LIST_GUID).unwrap();
        let form = hii_pkg(PACKAGE_FORMS, &[0x0Eu8, 0x17, 0xAA]);
        let string = string_package_bytes();
        let blob = hii_list(&g, &[&form, &string]);
        crate::hii::pe_resource::synth_reloc_hii_pe("HII", &blob)
    }

    fn resource_flash_image(with_ami: bool) -> Vec<u8> {
        let mut files = vec![ffs_file_bytes(
            &Guid::try_parse(STR_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &resource_hii_pe()),
        )];
        if with_ami {
            files.push(ffs_file_bytes(
                &Guid::try_parse(SETUPDATA_GUID).unwrap(),
                &[0u8; 108],
            ));
            files.push(ffs_file_bytes(
                &Guid::try_parse(AMITSE_GUID).unwrap(),
                &[0u8; 108],
            ));
        }
        flash_with_files(files)
    }

    fn resource_pe_section(img: &Image) -> &FfsNode {
        &img.root.children[1].children[0].children[0]
    }

    fn resource_forms_count(pe: &[u8]) -> usize {
        let (_, blob_off, blob_len) = hii_entry_locations(pe)[0];
        let blob = &pe[blob_off..blob_off + blob_len];
        parse_package_list(blob).unwrap().packages.len()
    }

    #[test]
    fn add_setup_formset_appends_to_pe_resource_channel() {
        let data = resource_flash_image(true);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let files_before = img.root.children[1].children.len();
        let pe_before = resource_pe_section(&img).body.clone();
        let packages_before = resource_forms_count(&pe_before);

        let res = add_setup_formset(&mut img, &test_schema(), None).unwrap();
        assert_eq!(
            res.new_ffs_guid,
            Guid::try_parse(FORMSET_GUID).unwrap(),
            "resource branch reports the added formset id"
        );
        assert_eq!(res.inserted_form_ids, vec![1]);
        assert_eq!(
            img.root.children[1].children.len(),
            files_before,
            "resource branch must not insert a new FFS"
        );

        let vol = &img.root.children[1];
        let pe_sec = resource_pe_section(&img);
        assert_eq!(pe_sec.action, Action::Rebuild);
        assert_eq!(vol.children[0].action, Action::Rebuild);
        assert_eq!(vol.action, Action::Rebuild);
        assert!(pe_sec.body.len() > pe_before.len());

        let (_, blob_off, blob_len) = hii_entry_locations(&pe_sec.body)[0];
        let blob = &pe_sec.body[blob_off..blob_off + blob_len];
        let parsed = parse_package_list(blob).unwrap();
        assert_eq!(parsed.packages.len(), packages_before + 1);
        assert_eq!(parsed.packages.last().unwrap().kind, PACKAGE_FORMS);
        let sp = parsed
            .packages
            .iter()
            .find(|p| p.kind == string_pack::PACKAGE_STRINGS)
            .unwrap();
        let strings = parse_string_package(sp.bytes).unwrap();
        for want in ["T", "H", "Main", "P1", "H1", "X1"] {
            assert!(
                strings.strings.iter().any(|(_, s)| s == want),
                "string {want} missing from resource string package"
            );
        }

        let built = build_image(&img).unwrap();
        assert_eq!(built.len(), data.len());
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let forms = collect_forms(&re);
        let added = forms
            .iter()
            .find(|f| f.formset_guid == FORMSET_GUID)
            .expect("added formset must be visible after round-trip");
        assert_eq!(added.title, "Main");
        assert_eq!(added.form_id_ifr, 1);
    }

    #[test]
    fn add_setup_formset_grows_reloc_bearing_pe_resource_channel() {
        let mut files = vec![ffs_file_bytes(
            &Guid::try_parse(STR_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &resource_reloc_hii_pe()),
        )];
        files.push(ffs_file_bytes(
            &Guid::try_parse(SETUPDATA_GUID).unwrap(),
            &[0u8; 108],
        ));
        files.push(ffs_file_bytes(
            &Guid::try_parse(AMITSE_GUID).unwrap(),
            &[0u8; 108],
        ));
        let data = flash_with_files(files);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pe_before = resource_pe_section(&img).body.clone();
        let packages_before = resource_forms_count(&pe_before);

        let res = add_setup_formset(&mut img, &test_schema(), None).unwrap();
        assert_eq!(res.inserted_form_ids, vec![1]);
        let pe_sec = resource_pe_section(&img);
        assert_eq!(pe_sec.action, Action::Rebuild);
        assert!(pe_sec.body.len() > pe_before.len());
        assert_eq!(
            resource_forms_count(&pe_sec.body),
            packages_before + 1,
            "strings + formset append must compose on a .reloc-bearing PE"
        );

        let built = build_image(&img).unwrap();
        assert_eq!(built.len(), data.len());
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let forms = collect_forms(&re);
        let added = forms
            .iter()
            .find(|f| f.formset_guid == FORMSET_GUID)
            .expect("added formset must survive rebuild in reloc-bearing PE");
        assert_eq!(added.title, "Main");
        assert_eq!(added.form_id_ifr, 1);
    }

    #[test]
    fn add_setup_formset_ami_precheck_blocks_resource_mutation() {
        let data = resource_flash_image(false);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pe_snapshot = resource_pe_section(&img).body.clone();

        assert!(matches!(
            add_setup_formset(&mut img, &test_schema(), None),
            Err(HiiError::AmiFilesNotFound)
        ));
        assert_eq!(resource_pe_section(&img).body, pe_snapshot);
        assert_eq!(img.root.children[1].children[0].action, Action::NoAction);
        assert_eq!(img.root.children[1].action, Action::NoAction);
        assert_eq!(img.root.action, Action::NoAction);
    }

    #[test]
    fn add_setup_formset_ami_precheck_blocks_bare_mutation() {
        let data = gap_aware_image(false);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let sp_snapshot = img.root.children[1].children[0].children[0].body.clone();

        assert!(matches!(
            add_setup_formset(&mut img, &test_schema(), None),
            Err(HiiError::AmiFilesNotFound)
        ));
        assert_eq!(
            img.root.children[1].children[0].children[0].body,
            sp_snapshot
        );
        assert_eq!(img.root.children[1].children[0].action, Action::NoAction);
        assert_eq!(img.root.action, Action::NoAction);
    }

    fn wrapped_resource_node_image(wrapper_guid: Guid) -> Image {
        let mut inner = mk_node(FfsType::Section, resource_hii_pe(), vec![]);
        inner.subtype = EFI_SECTION_PE32;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
        wrapper.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
            guid: wrapper_guid,
            dictionary_size: 0x0080_0000,
        });
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::try_parse(STR_GUID).unwrap());
        let mut setupdata = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        setupdata.guid = Some(Guid::try_parse(SETUPDATA_GUID).unwrap());
        let mut amitse = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        amitse.guid = Some(Guid::try_parse(AMITSE_GUID).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file, setupdata, amitse]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn add_setup_formset_refuses_non_recompressable_wrapper() {
        let mut img = wrapped_resource_node_image(crate::ffs::tiano_guid());
        let pe_snapshot = img.root.children[0].children[0].children[0].children[0]
            .body
            .clone();

        assert!(matches!(
            add_setup_formset(&mut img, &test_schema(), None),
            Err(HiiError::MutationBehindCompression)
        ));
        assert_eq!(
            img.root.children[0].children[0].children[0].children[0].body,
            pe_snapshot
        );
    }

    #[test]
    fn add_setup_formset_resource_channel_behind_lzma_wrapper() {
        let mut img = wrapped_resource_node_image(crate::ffs::lzma_guid());
        let pe_before = img.root.children[0].children[0].children[0].children[0]
            .body
            .clone();

        let res = add_setup_formset(&mut img, &test_schema(), None).unwrap();
        assert_eq!(res.inserted_form_ids, vec![1]);

        let pe_sec = &img.root.children[0].children[0].children[0].children[0];
        assert_eq!(pe_sec.action, Action::Rebuild);
        assert!(pe_sec.body.len() > pe_before.len());
        assert_eq!(
            resource_forms_count(&pe_sec.body),
            resource_forms_count(&pe_before) + 1
        );
    }

    fn plain_resource_node_image(pe_body: Vec<u8>) -> Image {
        let mut pe_sec = mk_node(FfsType::Section, pe_body, vec![]);
        pe_sec.subtype = EFI_SECTION_PE32;
        let mut file = mk_node(FfsType::File, vec![], vec![pe_sec]);
        file.guid = Some(Guid::try_parse(STR_GUID).unwrap());
        let mut setupdata = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        setupdata.guid = Some(Guid::try_parse(SETUPDATA_GUID).unwrap());
        let mut amitse = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        amitse.guid = Some(Guid::try_parse(AMITSE_GUID).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file, setupdata, amitse]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn add_setup_formset_maps_string_growth_refusal_to_pe_growth_unsupported() {
        let mut pe = resource_hii_pe();
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let mut img = plain_resource_node_image(pe);

        assert!(matches!(
            add_setup_formset(&mut img, &test_schema(), None),
            Err(HiiError::PeGrowthUnsupported)
        ));
    }

    fn two_candidate_resource_image(wrapper_guid: Guid) -> Image {
        let mut inner = mk_node(FfsType::Section, resource_hii_pe(), vec![]);
        inner.subtype = EFI_SECTION_PE32;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
        wrapper.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
            guid: wrapper_guid,
            dictionary_size: 0x0080_0000,
        });
        let mut blocked_file = mk_node(FfsType::File, vec![], vec![wrapper]);
        blocked_file.guid = Some(Guid::try_parse("11111111-1111-1111-1111-111111111111").unwrap());
        let mut good_pe = mk_node(FfsType::Section, resource_hii_pe(), vec![]);
        good_pe.subtype = EFI_SECTION_PE32;
        let mut good_file = mk_node(FfsType::File, vec![], vec![good_pe]);
        good_file.guid = Some(Guid::try_parse("22222222-2222-2222-2222-222222222222").unwrap());
        let mut setupdata = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        setupdata.guid = Some(Guid::try_parse(SETUPDATA_GUID).unwrap());
        let mut amitse = mk_node(FfsType::File, vec![0u8; 108], vec![]);
        amitse.guid = Some(Guid::try_parse(AMITSE_GUID).unwrap());
        let volume = mk_node(
            FfsType::Volume,
            vec![],
            vec![blocked_file, good_file, setupdata, amitse],
        );
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn add_setup_formset_skips_blocked_candidate_and_uses_later_good_one() {
        let mut img = two_candidate_resource_image(crate::ffs::tiano_guid());

        let res = add_setup_formset(&mut img, &test_schema(), None).unwrap();
        assert_eq!(res.inserted_form_ids, vec![1]);

        let blocked_pe = &img.root.children[0].children[0].children[0].children[0];
        assert_eq!(blocked_pe.action, Action::NoAction);
        assert_eq!(blocked_pe.body, resource_hii_pe());
        let good_pe = &img.root.children[0].children[1].children[0];
        assert_eq!(good_pe.action, Action::Rebuild);
        assert_eq!(
            resource_forms_count(&good_pe.body),
            resource_forms_count(&resource_hii_pe()) + 1
        );
    }
}
