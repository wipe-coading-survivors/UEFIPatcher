use std::collections::HashMap;

use crate::ffs::{
    EFI_SECTION_COMPRESSION, EFI_SECTION_GUID_DEFINED, EFI_SECTION_PE32, EFI_SECTION_RAW,
};
use crate::ops;
use crate::types::*;

use super::HiiError;
use super::formset_add;
use super::ifr;
use super::ifr_builder::IfrBuilder;
use super::package_list::parse_package_list;
use super::pe_resource::{
    hii_entry_locations, plan_rsrc_blob_growth, try_grow_rsrc_tail, write_length_chain,
};
use super::schema;
use super::spf;
use super::string_pack;
use super::{ami_patcher, node_at, node_at_mut, question_forms_package, select_resolving_records};
use r_efi::hii::PACKAGE_FORMS;

#[derive(Debug)]
pub struct AddFormResult {
    pub inserted_form_ids: Vec<u16>,
    pub string_ids: HashMap<String, u16>,
}

#[tracing::instrument(level = "debug", skip(image, schema), fields(item_id = %item_id), err)]
pub fn add_form(
    image: &mut Image,
    item_id: &str,
    schema: &schema::FormSetSchema,
) -> Result<AddFormResult, HiiError> {
    if image.mode != ImageMode::Write {
        return Err(HiiError::NotWritable);
    }
    let (target_str, formset_idx) = match item_id.rsplit_once('#') {
        Some((t, n)) => match n.parse::<u16>() {
            Ok(idx) => (t, idx as usize),
            Err(_) => return Err(HiiError::NotFound),
        },
        None => (item_id, 0),
    };
    let target = crate::parser::target::parse_target(target_str).map_err(|_| HiiError::NotFound)?;
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut ancestor = &image.root;
    for &i in &path[..path.len() - 1] {
        ancestor = &ancestor.children[i];
        if ancestor.node_type == FfsType::Section
            && (ancestor.subtype == EFI_SECTION_COMPRESSION
                || ancestor.subtype == EFI_SECTION_GUID_DEFINED)
            && !matches!(
                &ancestor.parsing_data,
                crate::types::ParsingData::GuidedSection(d)
                    if crate::ffs::is_recompressable_lzma_guid(&d.guid)
            )
        {
            return Err(HiiError::MutationBehindCompression);
        }
    }
    let bare_channel = {
        let node = crate::parser::target::find_item(&image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if node.subtype == EFI_SECTION_RAW && ifr::is_form_package(&node.body) {
            if !ifr::formset_at(&node.body, formset_idx) {
                return Err(HiiError::NotFound);
            }
            true
        } else if node.subtype == EFI_SECTION_PE32 {
            match resource_forms_package(&node.body) {
                None => return Err(HiiError::NotASetupItem),
                Some((off, len)) => {
                    if !node
                        .body
                        .get(off..off + len)
                        .is_some_and(|pkg| ifr::formset_at(pkg, formset_idx))
                    {
                        return Err(HiiError::NotFound);
                    }
                    false
                }
            }
        } else {
            return Err(HiiError::NotASetupItem);
        }
    };
    let owner_file_guid = owner_guid_by_path(&image.root, &path);
    let mut strings: Vec<String> = Vec::new();
    for vs in &schema.varstores {
        strings.push(vs.name.clone());
    }
    for form in &schema.forms {
        strings.push(form.title.clone());
        for item in &form.items {
            formset_add::collect_item_strings(item, &mut strings);
        }
    }
    let inserted_form_ids: Vec<u16> = schema.forms.iter().map(|f| f.id).collect();
    let varstores = build_varstores(schema)?;
    let spf_fixup = if varstores.is_empty() {
        None
    } else {
        let pkg_pre = question_forms_package(&image.root, &target, bare_channel)?.to_vec();
        let (after_header, formset_end) =
            ifr::formset_insert_points(&pkg_pre, formset_idx).ok_or(HiiError::InvalidIfr)?;
        match ami_patcher::discover_pfs_payload_path(image) {
            Ok(sd_path) => {
                let spf_body = node_at(&image.root, &sd_path).body.clone();
                Some((
                    sd_path,
                    after_header as u32,
                    formset_end as u32,
                    select_resolving_records(&spf::scan_question_records(&spf_body), &pkg_pre),
                ))
            }
            Err(HiiError::AmiFilesNotFound) => None,
            Err(e) => return Err(e),
        }
    };
    let string_ids = if bare_channel {
        string_pack::add_strings(image, owner_file_guid.as_ref(), &strings)?
    } else {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        match string_pack::add_strings_to_resource(&mut node.body, &strings) {
            Ok(ids) => ids,
            Err(string_pack::AddStringsToResourceError::NotFound) => {
                return Err(HiiError::StringPackageNotFound);
            }
            Err(string_pack::AddStringsToResourceError::GrowthUnsupported) => {
                return Err(HiiError::PeGrowthUnsupported);
            }
        }
    };
    let mut b = IfrBuilder::new();
    formset_add::emit_forms(&mut b, schema, &string_ids);
    let form_ifr = b.build();
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if bare_channel {
            if !ifr::insert_form_into_package(&mut node.body, formset_idx, &form_ifr, &varstores) {
                return Err(HiiError::InvalidIfr);
            }
        } else {
            insert_form_into_resource(&mut node.body, formset_idx, &form_ifr, &varstores)?;
        }
    }
    if let Some((sd_path, after_header, formset_end, selected)) = spf_fixup {
        let node = node_at_mut(&mut image.root, &sd_path);
        spf::fixup_selected_record_ifr_offsets(
            &mut node.body,
            &selected,
            formset_end,
            form_ifr.len() as u32,
        );
        spf::fixup_selected_record_ifr_offsets(
            &mut node.body,
            &selected,
            after_header,
            varstores.len() as u32,
        );
        ops::mark_rebuild_to_root_by_path(&mut image.root, &sd_path);
    }
    ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    tracing::debug!(?inserted_form_ids, "add_form done");
    Ok(AddFormResult {
        inserted_form_ids,
        string_ids,
    })
}

pub(crate) fn owner_guid_by_path(root: &FfsNode, path: &[usize]) -> Option<Guid> {
    let mut node = root;
    let mut guid = None;
    for &i in path {
        node = node.children.get(i)?;
        if node.node_type == FfsType::File {
            guid = node.guid;
        }
    }
    guid
}

fn build_varstores(schema: &schema::FormSetSchema) -> Result<Vec<u8>, HiiError> {
    let mut b = IfrBuilder::new();
    for vs in &schema.varstores {
        let g: Guid =
            Guid::try_parse(&vs.guid).map_err(|e| HiiError::InvalidSchema(e.to_string()))?;
        b.emit_var_store(vs.id, &g, vs.size, &vs.name);
    }
    Ok(b.build())
}

pub(crate) fn resource_forms_package(pe: &[u8]) -> Option<(usize, usize)> {
    let (_, blob_off, blob_len) = hii_entry_locations(pe).first().copied()?;
    let end = blob_off.checked_add(blob_len)?;
    let blob = pe.get(blob_off..end)?;
    let list = parse_package_list(blob)?;
    let mut prefix = 0usize;
    for pkg in &list.packages {
        if pkg.kind == PACKAGE_FORMS {
            return Some((blob_off + 20 + prefix, pkg.bytes.len()));
        }
        prefix += pkg.bytes.len();
    }
    None
}

fn insert_form_into_resource(
    pe: &mut Vec<u8>,
    formset_idx: usize,
    form_ifr: &[u8],
    varstores: &[u8],
) -> Result<(), HiiError> {
    let (pkg_off, old_len) = resource_forms_package(pe).ok_or(HiiError::InvalidIfr)?;
    let mut pkg = pe[pkg_off..pkg_off + old_len].to_vec();
    if !ifr::insert_form_into_package(&mut pkg, formset_idx, form_ifr, varstores) {
        return Err(HiiError::InvalidIfr);
    }
    let delta = pkg.len() - old_len;
    let (_, blob_off, blob_len) = hii_entry_locations(pe)
        .first()
        .copied()
        .ok_or(HiiError::InvalidIfr)?;
    let blob_end = blob_off.checked_add(blob_len).ok_or(HiiError::InvalidIfr)?;
    let blob = pe.get(blob_off..blob_end).ok_or(HiiError::InvalidIfr)?;
    let list = parse_package_list(blob).ok_or(HiiError::InvalidIfr)?;
    let sum: usize = list.packages.iter().map(|p| p.bytes.len()).sum();
    let new_blob_len = blob_len
        .checked_add(delta)
        .ok_or(HiiError::PeGrowthUnsupported)?;
    let new_total = 20u64 + sum as u64 + delta as u64 + 4;
    if new_total > u32::MAX as u64 || new_blob_len > u32::MAX as usize {
        return Err(HiiError::PeGrowthUnsupported);
    }
    let plan = plan_rsrc_blob_growth(pe, delta).ok_or(HiiError::PeGrowthUnsupported)?;
    if plan.grow > 0 && !try_grow_rsrc_tail(pe, plan.grow) {
        return Err(HiiError::PeGrowthUnsupported);
    }
    pe.copy_within(pkg_off + old_len..blob_end, pkg_off + pkg.len());
    pe[pkg_off..pkg_off + pkg.len()].copy_from_slice(&pkg);
    write_length_chain(
        pe,
        plan.entry_off,
        plan.blob_off,
        new_blob_len as u32,
        new_total as u32,
    );
    Ok(())
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
    use crate::hii::ifr_builder::{IfrBuilder, OP_VARSTORE};
    use crate::hii::package_list::parse_package_list;
    use crate::hii::pe_resource::{hii_entry_locations, synth_hii_pe, synth_reloc_hii_pe};
    use crate::hii::strings::parse_string_package;
    use crate::parser::image::parse_image;
    use crate::types::{Action, GuidedSectionParsingData, ParsingData};
    use r_efi::hii::{PACKAGE_END, PACKAGE_FORMS, PACKAGE_STRINGS};

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

    const FILE_GUID: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";
    const FORMSET_GUID: &str = "A1B2C3D4-E5F6-7890-ABCD-EF1234567890";
    const FORMSET2_GUID: &str = "B1B2C3D4-E5F6-7890-ABCD-EF1234567890";
    const LIST_GUID: &str = "899407D7-99FE-43D8-9A21-79EC328CAC21";

    fn string_package_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0x00, 0x00, 0x00, PACKAGE_STRINGS]);
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

    fn formset_ifr(guid: &Guid, form_id: u16) -> Vec<u8> {
        let mut b = IfrBuilder::new();
        b.emit_form_set(guid, 1, 1, &[]);
        b.emit_form(form_id, 1);
        b.emit_end();
        b.emit_end();
        b.build()
    }

    fn bare_form_package() -> Vec<u8> {
        hii_pkg(
            PACKAGE_FORMS,
            &formset_ifr(&Guid::try_parse(FORMSET_GUID).unwrap(), 1),
        )
    }

    fn two_formset_package() -> Vec<u8> {
        let mut b = IfrBuilder::new();
        b.emit_form_set(&Guid::try_parse(FORMSET_GUID).unwrap(), 1, 1, &[]);
        b.emit_form(1, 1);
        b.emit_end();
        b.emit_end();
        b.emit_form_set(&Guid::try_parse(FORMSET2_GUID).unwrap(), 2, 2, &[]);
        b.emit_form(2, 2);
        b.emit_end();
        b.emit_end();
        hii_pkg(PACKAGE_FORMS, &b.build())
    }

    fn add_form_schema() -> schema::FormSetSchema {
        schema::FormSetSchema {
            formset_guid: "11111111-2222-3333-4444-555555555555".into(),
            title: "T".into(),
            help: "H".into(),
            class_guids: vec![],
            varstores: vec![schema::VarStoreSchema {
                id: 7,
                guid: "22222222-3333-4444-5555-666666666666".into(),
                size: 64,
                name: "VStore".into(),
                var_type: schema::VarStoreType::Buffer,
                attributes: 7,
            }],
            default_stores: vec![],
            forms: vec![schema::FormSchema {
                id: 42,
                title: "NewForm".into(),
                items: vec![schema::ItemSchema::Text(schema::TextItem {
                    prompt: "P".into(),
                    help: "H".into(),
                    text_two: "X".into(),
                })],
            }],
            setupdata_guid: None,
            amitse_guid: None,
        }
    }

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

    fn bare_flash_image() -> Vec<u8> {
        let content = [
            section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            section_bytes(EFI_SECTION_RAW, &bare_form_package()),
        ]
        .concat();
        flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &content,
        )])
    }

    fn two_formset_flash_image() -> Vec<u8> {
        let content = [
            section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            section_bytes(EFI_SECTION_RAW, &two_formset_package()),
        ]
        .concat();
        flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &content,
        )])
    }

    fn resource_blob() -> Vec<u8> {
        hii_list(
            &Guid::try_parse(LIST_GUID).unwrap(),
            &[&bare_form_package(), &string_package_bytes()],
        )
    }

    fn resource_flash_image(pe: Vec<u8>) -> Vec<u8> {
        flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_PE32, &pe),
        )])
    }

    fn section_of<'a>(img: &'a Image, target: &str) -> &'a FfsNode {
        let t = crate::parser::target::parse_target(target).unwrap();
        crate::parser::target::find_item(&img.root, &t).unwrap()
    }

    fn resource_blob_of(pe: &[u8]) -> &[u8] {
        let (_, off, len) = hii_entry_locations(pe).first().copied().unwrap();
        &pe[off..off + len]
    }

    #[test]
    fn add_form_bare_channel_inserts_form_and_strings() {
        let data = bare_flash_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let res = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1",
            &add_form_schema(),
        )
        .unwrap();
        assert_eq!(res.inserted_form_ids, vec![42]);
        assert_eq!(res.string_ids["VStore"], 2);
        assert_eq!(res.string_ids["NewForm"], 3);
        assert_eq!(res.string_ids["P"], 4);

        let form_sec = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1");
        assert_eq!(form_sec.action, Action::Rebuild);
        let fs = ifr::parse_form_package(&form_sec.body).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 42]
        );
        assert!(form_sec.body.windows(6).any(|w| w == b"VStore"));

        let str_sec = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0");
        assert_eq!(str_sec.action, Action::Rebuild);
        let sp = parse_string_package(&str_sec.body).unwrap();
        assert!(sp.strings.iter().any(|(_, s)| s == "NewForm"));

        let built = build_image(&img).unwrap();
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let forms = collect_forms(&re);
        assert!(
            forms
                .iter()
                .any(|f| f.form_id_ifr == 42 && f.form_id.ends_with(":0x19:1"))
        );
    }

    #[test]
    fn add_form_resource_channel_inserts_form_and_strings() {
        let data = resource_flash_image(synth_hii_pe("HII", &resource_blob()));
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let res = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            &add_form_schema(),
        )
        .unwrap();
        assert_eq!(res.inserted_form_ids, vec![42]);
        assert_eq!(res.string_ids["VStore"], 2);
        assert_eq!(res.string_ids["NewForm"], 3);

        let pe_sec = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
        assert_eq!(pe_sec.action, Action::Rebuild);
        let blob = resource_blob_of(&pe_sec.body);
        let list = parse_package_list(blob).unwrap();
        assert_eq!(
            list.packages.iter().map(|p| p.kind).collect::<Vec<_>>(),
            vec![PACKAGE_FORMS, PACKAGE_STRINGS]
        );
        let fs = ifr::parse_form_package(list.packages[0].bytes).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 42]
        );
        assert!(list.packages[0].bytes.windows(6).any(|w| w == b"VStore"));
        let sp = parse_string_package(list.packages[1].bytes).unwrap();
        assert!(sp.strings.iter().any(|(_, s)| s == "NewForm"));
        let (_, _, entry_len) = hii_entry_locations(&pe_sec.body)[0];
        assert_eq!(entry_len, blob.len());
        let total = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
        assert_eq!(total, blob.len());

        let built = build_image(&img).unwrap();
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        let forms = collect_forms(&re);
        assert!(
            forms
                .iter()
                .any(|f| f.form_id_ifr == 42 && f.form_id.ends_with(":0x10:0"))
        );
    }

    #[test]
    fn add_form_resource_channel_on_reloc_bearing_pe() {
        let pe = synth_reloc_hii_pe("HII", &resource_blob());
        let data = resource_flash_image(pe);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let res = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            &add_form_schema(),
        )
        .unwrap();
        assert_eq!(res.inserted_form_ids, vec![42]);

        let pe_sec = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0");
        let blob = resource_blob_of(&pe_sec.body);
        let list = parse_package_list(blob).unwrap();
        let fs = ifr::parse_form_package(list.packages[0].bytes).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 42]
        );

        let built = build_image(&img).unwrap();
        let re = parse_image(&built, ImageMode::Read, "i2", "s2").unwrap();
        assert!(collect_forms(&re).iter().any(|f| f.form_id_ifr == 42));
    }

    #[test]
    fn add_form_bare_channel_targets_formset_ordinal() {
        let data = two_formset_flash_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pkg_before = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1")
            .body
            .clone();
        let fs2_off = 4 + (23 + 6 + 2 + 2);
        add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1#1",
            &add_form_schema(),
        )
        .unwrap();
        let pkg_after = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1")
            .body
            .clone();
        assert_eq!(&pkg_after[4..fs2_off], &pkg_before[4..fs2_off]);
        let plen_after =
            pkg_after[0] as usize | (pkg_after[1] as usize) << 8 | (pkg_after[2] as usize) << 16;
        let plen_before =
            pkg_before[0] as usize | (pkg_before[1] as usize) << 8 | (pkg_before[2] as usize) << 16;
        assert_eq!(plen_after, pkg_after.len());
        assert_eq!(plen_before, pkg_before.len());
        assert!(plen_after > plen_before);
        assert!(pkg_after[fs2_off..].windows(6).any(|w| w == b"VStore"));
        assert_eq!(pkg_after[fs2_off + 23], OP_VARSTORE);
        let fs = ifr::parse_form_package(&pkg_after).unwrap();
        assert_eq!(
            fs.forms.iter().map(|f| f.form_id).collect::<Vec<_>>(),
            vec![1, 2, 42]
        );
    }

    #[test]
    fn add_form_rejects_read_only_mode() {
        let mut img = parse_image(&bare_flash_image(), ImageMode::Read, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotWritable));
    }

    #[test]
    fn add_form_rejects_malformed_ordinal_discriminator() {
        let mut img = parse_image(&bare_flash_image(), ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1#nope",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn add_form_rejects_unknown_target() {
        let mut img = parse_image(&bare_flash_image(), ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "00000000-0000-0000-0000-000000000001:0x19:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }

    #[test]
    fn add_form_rejects_mutation_behind_compression() {
        let mut inner = mk_node(FfsType::Section, bare_form_package(), vec![]);
        inner.subtype = EFI_SECTION_RAW;
        let mut wrapper = mk_node(FfsType::Section, vec![], vec![inner]);
        wrapper.subtype = crate::ffs::EFI_SECTION_GUID_DEFINED;
        wrapper.parsing_data = ParsingData::GuidedSection(GuidedSectionParsingData {
            guid: crate::ffs::tiano_guid(),
            dictionary_size: 0x0080_0000,
        });
        let mut file = mk_node(FfsType::File, vec![], vec![wrapper]);
        file.guid = Some(Guid::try_parse(FILE_GUID).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        let mut img = Image {
            image_id: "i".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        };
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::MutationBehindCompression));
        let inner = &img.root.children[0].children[0].children[0].children[0];
        assert_eq!(inner.body, bare_form_package());
        assert_eq!(inner.action, Action::NoAction);
    }

    #[test]
    fn add_form_rejects_non_form_raw_section() {
        let content = [
            section_bytes(EFI_SECTION_RAW, &string_package_bytes()),
            section_bytes(EFI_SECTION_RAW, &[0xAA; 16]),
        ]
        .concat();
        let data = flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &content,
        )]);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn add_form_rejects_pe32_without_forms_package() {
        let data = resource_flash_image(synth_hii_pe("HII", &string_package_bytes()));
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn add_form_bare_out_of_range_ordinal_is_not_found() {
        let data = bare_flash_image();
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pkg_before = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1")
            .body
            .clone();
        let str_before = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0")
            .body
            .clone();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1#1",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1").body,
            pkg_before
        );
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0").body,
            str_before
        );
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0").action,
            Action::NoAction
        );
    }

    #[test]
    fn add_form_resource_out_of_range_ordinal_is_not_found() {
        let data = resource_flash_image(synth_hii_pe("HII", &resource_blob()));
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pe_before = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0")
            .body
            .clone();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0#1",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").body,
            pe_before
        );
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").action,
            Action::NoAction
        );
    }

    #[test]
    fn add_form_resource_insert_stage_growth_refusal_is_pe_growth_unsupported() {
        let mut pe = synth_hii_pe("HII", &resource_blob());
        assert!(crate::hii::pe_resource::try_grow_rsrc_tail(&mut pe, 32));
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let data = resource_flash_image(pe);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::PeGrowthUnsupported));
    }

    #[test]
    fn add_form_bare_without_string_package_fails() {
        let data = flash_with_files(vec![ffs_file_bytes(
            &Guid::try_parse(FILE_GUID).unwrap(),
            &section_bytes(EFI_SECTION_RAW, &bare_form_package()),
        )]);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let pkg_before = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0")
            .body
            .clone();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::StringPackageNotFound));
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0").body,
            pkg_before
        );
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0").action,
            Action::NoAction
        );
    }

    #[test]
    fn add_form_resource_growth_refusal_is_pe_growth_unsupported() {
        let mut pe = synth_hii_pe("HII", &resource_blob());
        pe[0xe8..0xec].copy_from_slice(&0x5000u32.to_le_bytes());
        let data = resource_flash_image(pe);
        let mut img = parse_image(&data, ImageMode::Write, "i", "s").unwrap();
        let snapshot = section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0")
            .body
            .clone();
        let err = add_form(
            &mut img,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0",
            &add_form_schema(),
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::PeGrowthUnsupported));
        assert_eq!(
            section_of(&img, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x10:0").body,
            snapshot
        );
    }
}
