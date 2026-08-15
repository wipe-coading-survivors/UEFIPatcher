use super::ParserError;
use crate::types::*;
use std::str::FromStr;

pub fn parse_target(s: &str) -> Result<Target, ParserError> {
    if !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_digit())
        && s.chars().all(|c| c.is_ascii_digit() || c == '/')
    {
        let path: Result<Vec<usize>, _> = s.split('/').map(|p| p.parse::<usize>()).collect();
        return path
            .map(Target::Path)
            .map_err(|_| ParserError::InvalidHeader(format!("bad path: {s}")));
    }
    if let Some(colon_pos) = s.find(':')
        && colon_pos >= 36
    {
        let guid_str = &s[..colon_pos];
        let rest = &s[colon_pos + 1..];
        let guid = Guid::from_str(guid_str)
            .map_err(|e| ParserError::InvalidHeader(format!("bad guid: {e}")))?;
        let parts: Vec<&str> = rest.split(':').collect();
        let stype = u8::from_str_radix(parts[0].trim_start_matches("0x"), 16)
            .map_err(|_| ParserError::InvalidHeader(format!("bad section type: {}", parts[0])))?;
        let sidx = if parts.len() > 1 {
            Some(
                parts[1]
                    .parse::<usize>()
                    .map_err(|_| ParserError::InvalidHeader(format!("bad index: {}", parts[1])))?,
            )
        } else {
            None
        };
        return Ok(Target::GuidSection {
            guid,
            section_type: stype,
            section_index: sidx,
        });
    }
    if s.len() == 36 {
        let guid =
            Guid::from_str(s).map_err(|e| ParserError::InvalidHeader(format!("bad guid: {e}")))?;
        return Ok(Target::Guid(guid));
    }
    Err(ParserError::InvalidHeader(format!(
        "unrecognized target: {s}"
    )))
}

pub fn find_item<'a>(root: &'a FfsNode, target: &Target) -> Result<&'a FfsNode, ParserError> {
    match target {
        Target::Guid(g) => find_by_guid(root, g)
            .ok_or_else(|| ParserError::InvalidHeader(format!("guid {g} not found"))),
        Target::Path(indices) => {
            let mut node = root;
            for &i in indices {
                node = node.children.get(i).ok_or_else(|| {
                    ParserError::InvalidHeader(format!("path index {i} not found"))
                })?;
            }
            Ok(node)
        }
        Target::GuidSection {
            guid,
            section_type,
            section_index,
        } => {
            let file = find_by_guid(root, guid)
                .ok_or_else(|| ParserError::InvalidHeader(format!("guid {guid} not found")))?;
            let mut count = 0usize;
            find_guid_section_dfs(
                file,
                *section_type,
                *section_index,
                &mut count,
                &mut Vec::new(),
            )
            .map(|(node, _)| node)
            .ok_or_else(|| {
                ParserError::InvalidHeader(format!(
                    "section type {section_type:#x} not found in {guid}"
                ))
            })
        }
    }
}

pub fn find_item_path(root: &FfsNode, target: &Target) -> Option<Vec<usize>> {
    match target {
        Target::Path(indices) => {
            let mut node = root;
            for &i in indices {
                node = node.children.get(i)?;
            }
            Some(indices.clone())
        }
        Target::Guid(g) => find_path_by_guid(root, g, &mut Vec::new()),
        Target::GuidSection {
            guid,
            section_type,
            section_index,
        } => {
            let mut file_path = find_path_by_guid(root, guid, &mut Vec::new())?;
            let file = node_at_path(root, &file_path)?;
            let mut count = 0usize;
            let (_, rel) = find_guid_section_dfs(
                file,
                *section_type,
                *section_index,
                &mut count,
                &mut Vec::new(),
            )?;
            file_path.extend(rel);
            Some(file_path)
        }
    }
}

pub fn find_item_mut<'a>(
    root: &'a mut FfsNode,
    target: &Target,
) -> Result<&'a mut FfsNode, ParserError> {
    let path = find_item_path(root, target)
        .ok_or_else(|| ParserError::InvalidHeader(format!("target {target:?} not found")))?;
    let mut node = root;
    for &i in &path {
        node = node
            .children
            .get_mut(i)
            .ok_or_else(|| ParserError::InvalidHeader(format!("path index {i} not found")))?;
    }
    Ok(node)
}

fn node_at_path<'a>(root: &'a FfsNode, path: &[usize]) -> Option<&'a FfsNode> {
    let mut node = root;
    for &i in path {
        node = node.children.get(i)?;
    }
    Some(node)
}

fn find_guid_section_dfs<'a>(
    node: &'a FfsNode,
    section_type: u8,
    wanted: Option<usize>,
    count: &mut usize,
    path: &mut Vec<usize>,
) -> Option<(&'a FfsNode, Vec<usize>)> {
    for (i, child) in node.children.iter().enumerate() {
        if child.node_type != FfsType::Section {
            continue;
        }
        let hit = child.subtype == section_type && wanted.is_none_or(|w| w == *count);
        if child.subtype == section_type {
            *count += 1;
        }
        if hit {
            path.push(i);
            return Some((child, path.clone()));
        }
        if child.subtype == crate::ffs::EFI_SECTION_COMPRESSION
            || child.subtype == crate::ffs::EFI_SECTION_GUID_DEFINED
        {
            path.push(i);
            if let Some(found) = find_guid_section_dfs(child, section_type, wanted, count, path) {
                return Some(found);
            }
            path.pop();
        }
    }
    None
}

fn find_path_by_guid(node: &FfsNode, g: &Guid, path: &mut Vec<usize>) -> Option<Vec<usize>> {
    if node.guid == Some(*g) {
        return Some(path.clone());
    }
    if let ParsingData::Volume(vd) = &node.parsing_data
        && vd.extended_header_guid == Some(*g)
    {
        return Some(path.clone());
    }
    if let ParsingData::GuidedSection(gs) = &node.parsing_data
        && gs.guid == *g
    {
        return Some(path.clone());
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if let Some(p) = find_path_by_guid(child, g, path) {
            return Some(p);
        }
        path.pop();
    }
    None
}

fn find_by_guid<'a>(node: &'a FfsNode, g: &Guid) -> Option<&'a FfsNode> {
    if node.guid == Some(*g) {
        return Some(node);
    }
    if let ParsingData::Volume(vd) = &node.parsing_data
        && vd.extended_header_guid == Some(*g)
    {
        return Some(node);
    }
    if let ParsingData::GuidedSection(gs) = &node.parsing_data
        && gs.guid == *g
    {
        return Some(node);
    }
    for child in &node.children {
        if let Some(n) = find_by_guid(child, g) {
            return Some(n);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_guid_target() {
        let t = parse_target("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        assert!(matches!(t, Target::Guid(_)));
    }

    #[test]
    fn parse_path_target() {
        let t = parse_target("0/2/207").unwrap();
        assert_eq!(t, Target::Path(vec![0, 2, 207]));
    }

    #[test]
    fn parse_single_index_path_target() {
        assert_eq!(parse_target("0").unwrap(), Target::Path(vec![0]));
        assert_eq!(parse_target("7").unwrap(), Target::Path(vec![7]));
    }

    #[test]
    fn parse_guid_type_target() {
        let t = parse_target("899407D7-92A6-4174-968F-6F0B47F86A23:0x10").unwrap();
        match t {
            Target::GuidSection {
                section_type,
                section_index,
                ..
            } => {
                assert_eq!(section_type, 0x10);
                assert_eq!(section_index, None);
            }
            _ => panic!("expected GuidSection"),
        }
    }

    #[test]
    fn parse_guid_type_index_target() {
        let t = parse_target("899407D7-92A6-4174-968F-6F0B47F86A23:0x10:2").unwrap();
        match t {
            Target::GuidSection {
                section_type,
                section_index,
                ..
            } => {
                assert_eq!(section_type, 0x10);
                assert_eq!(section_index, Some(2));
            }
            _ => panic!("expected GuidSection"),
        }
    }

    #[test]
    fn parse_lowercase_guid_target() {
        let t = parse_target("5c60f367-a505-419a-859e-2a4ff6ca6fe5").unwrap();
        assert!(matches!(t, Target::Guid(_)));
    }

    #[test]
    fn parse_invalid_target() {
        assert!(parse_target("not-a-target").is_err());
        assert!(parse_target("").is_err());
    }

    #[test]
    fn find_item_by_path_navigates_children() {
        let tree = sample_tree();
        let file = find_item(&tree, &Target::Path(vec![0, 0])).unwrap();
        assert_eq!(file.node_type, FfsType::File);
        let section = find_item(&tree, &Target::Path(vec![0, 0, 0])).unwrap();
        assert_eq!(section.node_type, FfsType::Section);
        assert_eq!(section.subtype, 0x10);
    }

    #[test]
    fn find_item_by_guid_finds_file() {
        let tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let node = find_item(&tree, &Target::Guid(g)).unwrap();
        assert_eq!(node.node_type, FfsType::File);
    }

    #[test]
    fn find_item_guid_section_type() {
        let tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let t = Target::GuidSection {
            guid: g,
            section_type: 0x10,
            section_index: None,
        };
        let node = find_item(&tree, &t).unwrap();
        assert_eq!(node.subtype, 0x10);
    }

    #[test]
    fn find_item_path_resolves_guid_and_guid_section() {
        let tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let p = find_item_path(&tree, &Target::Guid(g)).unwrap();
        assert_eq!(p, vec![0, 0]);
        let p = find_item_path(
            &tree,
            &Target::GuidSection {
                guid: g,
                section_type: 0x10,
                section_index: None,
            },
        )
        .unwrap();
        assert_eq!(p, vec![0, 0, 0]);
    }

    #[test]
    fn find_item_path_returns_none_on_miss() {
        let tree = sample_tree();
        assert!(find_item_path(&tree, &Target::Path(vec![0, 9])).is_none());
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        assert!(
            find_item_path(
                &tree,
                &Target::GuidSection {
                    guid: g,
                    section_type: 0x02,
                    section_index: None,
                },
            )
            .is_none()
        );
        let other = Guid::from_str("00000000-0000-0000-0000-000000000000").unwrap();
        assert!(find_item_path(&tree, &Target::Guid(other)).is_none());
    }

    #[test]
    fn find_item_mut_guid_section_gives_mutable_access() {
        let mut tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let t = Target::GuidSection {
            guid: g,
            section_type: 0x10,
            section_index: None,
        };
        let node = find_item_mut(&mut tree, &t).unwrap();
        node.body.extend_from_slice(b"patched");
        assert!(
            tree.children[0].children[0].children[0]
                .body
                .ends_with(b"patched")
        );
    }

    #[test]
    fn find_item_mut_guid_gives_mutable_access() {
        let mut tree = sample_tree();
        let g = Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap();
        let node = find_item_mut(&mut tree, &Target::Guid(g)).unwrap();
        node.body.push(0xAA);
        assert_eq!(tree.children[0].children[0].body, vec![0xAA]);
    }

    #[test]
    fn find_item_mut_path_still_works() {
        let mut tree = sample_tree();
        let node = find_item_mut(&mut tree, &Target::Path(vec![0, 0, 0])).unwrap();
        node.body.push(1);
        assert_eq!(tree.children[0].children[0].children[0].body, vec![1]);
    }

    fn wrapped_tree() -> FfsNode {
        let inner = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x10,
            offset: 0,
            header: vec![],
            body: vec![0xAA],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let wrapper = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x02,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![inner],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let direct = FfsNode {
            guid: None,
            node_type: FfsType::Section,
            subtype: 0x10,
            offset: 0,
            header: vec![],
            body: vec![0xBB],
            tail: vec![],
            children: vec![],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let file = FfsNode {
            guid: Some(Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap()),
            node_type: FfsType::File,
            subtype: 0x07,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![wrapper, direct],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        let volume = FfsNode {
            guid: None,
            node_type: FfsType::Volume,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![file],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        };
        FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![volume],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }

    fn wrapped_target(index: Option<usize>) -> Target {
        Target::GuidSection {
            guid: Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap(),
            section_type: 0x10,
            section_index: index,
        }
    }

    #[test]
    fn find_item_resolves_sections_inside_guided_wrapper() {
        let tree = wrapped_tree();
        let inner = find_item(&tree, &wrapped_target(Some(0))).unwrap();
        assert_eq!(inner.body, vec![0xAA]);
        let direct = find_item(&tree, &wrapped_target(Some(1))).unwrap();
        assert_eq!(direct.body, vec![0xBB]);
        let first = find_item(&tree, &wrapped_target(None)).unwrap();
        assert_eq!(first.body, vec![0xAA]);
    }

    #[test]
    fn find_item_path_resolves_sections_inside_guided_wrapper() {
        let tree = wrapped_tree();
        assert_eq!(
            find_item_path(&tree, &wrapped_target(Some(0))).unwrap(),
            vec![0, 0, 0, 0]
        );
        assert_eq!(
            find_item_path(&tree, &wrapped_target(Some(1))).unwrap(),
            vec![0, 0, 1]
        );
        assert!(find_item_path(&tree, &wrapped_target(Some(2))).is_none());
    }

    #[test]
    fn find_item_mut_reaches_wrapped_section() {
        let mut tree = wrapped_tree();
        let node = find_item_mut(&mut tree, &wrapped_target(Some(0))).unwrap();
        node.body.push(1);
        assert_eq!(
            tree.children[0].children[0].children[0].children[0].body,
            vec![0xAA, 1]
        );
    }

    fn sample_tree() -> FfsNode {
        FfsNode {
            guid: None,
            node_type: FfsType::Image,
            subtype: 0,
            offset: 0,
            header: vec![],
            body: vec![],
            tail: vec![],
            children: vec![FfsNode {
                guid: None,
                node_type: FfsType::Volume,
                subtype: 0,
                offset: 0,
                header: vec![],
                body: vec![],
                tail: vec![],
                children: vec![FfsNode {
                    guid: Some(Guid::from_str("5C60F367-A505-419A-859E-2A4FF6CA6FE5").unwrap()),
                    node_type: FfsType::File,
                    subtype: 0x07,
                    offset: 0x100,
                    header: vec![],
                    body: vec![],
                    tail: vec![],
                    children: vec![FfsNode {
                        guid: None,
                        node_type: FfsType::Section,
                        subtype: 0x10,
                        offset: 0x118,
                        header: vec![],
                        body: vec![],
                        tail: vec![],
                        children: vec![],
                        action: Action::NoAction,
                        parsing_data: ParsingData::None,
                        fixed: false,
                        compressed: false,
                        alignment_bytes: vec![],
                    }],
                    action: Action::NoAction,
                    parsing_data: ParsingData::None,
                    fixed: false,
                    compressed: false,
                    alignment_bytes: vec![],
                }],
                action: Action::NoAction,
                parsing_data: ParsingData::None,
                fixed: false,
                compressed: false,
                alignment_bytes: vec![],
            }],
            action: Action::NoAction,
            parsing_data: ParsingData::None,
            fixed: false,
            compressed: false,
            alignment_bytes: vec![],
        }
    }
}
