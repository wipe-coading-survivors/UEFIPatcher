pub mod ami_patcher;
pub mod ffs_assembler;
pub mod forms;
pub mod formset_add;
pub mod ifr;
pub mod ifr_builder;
pub mod package_list;
pub mod schema;
pub mod string_pack;
pub mod strings;

use crate::ops;
use crate::types::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HiiError {
    #[error("not found")]
    NotFound,
    #[error("not a setup item")]
    NotASetupItem,
    #[error("invalid IFR")]
    InvalidIfr,
    #[error("invalid schema: {0}")]
    InvalidSchema(String),
    #[error("string package not found")]
    StringPackageNotFound,
    #[error("AMI files not found (setupdataBin/amitseSct)")]
    AmiFilesNotFound,
    #[error("IFR build error: {0}")]
    IfrBuildError(String),
    #[error("FFS assembly error: {0}")]
    FfsAssemblyError(String),
}

#[tracing::instrument(level = "debug", skip(image), fields(item_id = %item_id, visible), err)]
pub fn set_item_visibility(
    image: &mut Image,
    item_id: &str,
    visible: bool,
) -> Result<(), HiiError> {
    let target = crate::parser::target::parse_target(item_id).map_err(|_| HiiError::NotFound)?;
    let path =
        crate::parser::target::find_item_path(&image.root, &target).ok_or(HiiError::NotFound)?;
    let mut changed = false;
    {
        let node = crate::parser::target::find_item_mut(&mut image.root, &target)
            .map_err(|_| HiiError::NotFound)?;
        if node.node_type != FfsType::Section {
            return Err(HiiError::NotASetupItem);
        }
        if visible && let Some(scope) = ifr::find_suppress_if_scopes(&node.body).into_iter().next()
        {
            let mut body = node.body.clone();
            ifr::unsuppress(&mut body, &scope);
            node.body = body;
            changed = true;
        }
    }
    if changed {
        ops::mark_rebuild_to_root_by_path(&mut image.root, &path);
    }
    tracing::debug!(changed, "set_item_visibility done");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Action, FfsNode, FfsType, Image, ImageMode, ParsingData};
    use std::str::FromStr;

    #[test]
    fn unsuppress_makes_block_empty() {
        let mut ifr = vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02];
        let scopes = ifr::find_suppress_if_scopes(&ifr);
        assert_eq!(scopes.len(), 1);
        ifr::unsuppress(&mut ifr, &scopes[0]);
        assert_eq!(&ifr[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(ifr.len(), 7);
    }

    #[test]
    fn set_item_visibility_true_unsuppresses_and_cascades() {
        let mut image = sample_image_with_ifr();
        set_item_visibility(&mut image, "0/0/0", true).unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(image.root.children[0].action, Action::Rebuild);
        assert_eq!(image.root.children[0].children[0].action, Action::Rebuild);
        assert_eq!(section.action, Action::Rebuild);
    }

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

    fn sample_image_with_ifr() -> Image {
        let section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        let file = mk_node(FfsType::File, vec![], vec![section]);
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    const FILE_GUID_STR: &str = "5C60F367-A505-419A-859E-2A4FF6CA6FE5";

    fn sample_image_with_ifr_guid() -> Image {
        let mut section = mk_node(
            FfsType::Section,
            vec![0x0A, 0x82, 0x12, 0x03, 0x40, 0x29, 0x02],
            vec![],
        );
        section.subtype = 0x19;
        let mut file = mk_node(FfsType::File, vec![], vec![section]);
        file.guid = Some(Guid::from_str(FILE_GUID_STR).unwrap());
        let volume = mk_node(FfsType::Volume, vec![], vec![file]);
        let root = mk_node(FfsType::Image, vec![], vec![volume]);
        Image {
            image_id: "img".into(),
            session_id: "s".into(),
            root,
            mode: ImageMode::Write,
        }
    }

    #[test]
    fn set_item_visibility_guid_section_target_unsuppresses() {
        let mut image = sample_image_with_ifr_guid();
        set_item_visibility(
            &mut image,
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:0",
            true,
        )
        .unwrap();
        let section = &image.root.children[0].children[0].children[0];
        assert_eq!(&section.body[0..4], &[0x0A, 0x82, 0x29, 0x02]);
        assert_eq!(section.action, Action::Rebuild);
        assert_eq!(image.root.action, Action::Rebuild);
    }

    #[test]
    fn set_item_visibility_guid_target_rejects_non_section() {
        let mut image = sample_image_with_ifr_guid();
        let err = set_item_visibility(&mut image, FILE_GUID_STR, true).unwrap_err();
        assert!(matches!(err, HiiError::NotASetupItem));
    }

    #[test]
    fn set_item_visibility_unknown_guid_is_not_found() {
        let mut image = sample_image_with_ifr_guid();
        let err = set_item_visibility(
            &mut image,
            "00000000-0000-0000-0000-000000000001:0x19:0",
            true,
        )
        .unwrap_err();
        assert!(matches!(err, HiiError::NotFound));
    }
}
