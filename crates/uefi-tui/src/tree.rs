use crate::app::TreeNode;
use crate::theme::ACTION_NO;
use uefi_proto::Node;

pub fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

pub fn build_tree(nodes: &[Node]) -> Vec<TreeNode> {
    let n = nodes.len();
    nodes
        .iter()
        .enumerate()
        .map(|(i, nd)| {
            let segs = segments(&nd.path);
            let depth = segs.len();
            let has_children = i + 1 < n && {
                let next_segs = segments(&nodes[i + 1].path);
                next_segs.len() > depth && next_segs[..depth] == segs[..]
            };
            TreeNode {
                path: nd.path.clone(),
                depth,
                node_type: nd.r#type as u8,
                subtype: nd.subtype as u8,
                guid: if nd.guid.is_empty() {
                    None
                } else {
                    Some(nd.guid.clone())
                },
                name: nd.name.clone(),
                action: ACTION_NO,
                expanded: depth <= 1,
                has_children,
            }
        })
        .collect()
}

pub fn visible_rows(tree: &[TreeNode]) -> Vec<usize> {
    let mut visible = Vec::new();
    let mut ancestor_expanded: Vec<bool> = Vec::new();
    for (i, node) in tree.iter().enumerate() {
        ancestor_expanded.truncate(node.depth);
        if ancestor_expanded.iter().all(|&e| e) {
            visible.push(i);
        }
        ancestor_expanded.push(node.expanded);
    }
    visible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nd(path: &str, type_: u32, subtype: u32) -> Node {
        Node {
            path: path.into(),
            r#type: type_,
            subtype,
            guid: String::new(),
            offset: 0,
            size: 0,
            name: String::new(),
        }
    }

    #[test]
    fn segments_root_and_children() {
        assert_eq!(segments(""), Vec::<&str>::new());
        assert_eq!(segments("0"), vec!["0"]);
        assert_eq!(segments("0/3"), vec!["0", "3"]);
        assert_eq!(segments("0/3/1"), vec!["0", "3", "1"]);
    }

    #[test]
    fn build_tree_depth_and_children() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0x07),
            nd("0/0/0", 67, 0x15),
        ];
        let tree = build_tree(&nodes);
        assert_eq!(tree[0].depth, 0);
        assert_eq!(tree[1].depth, 1);
        assert_eq!(tree[2].depth, 2);
        assert_eq!(tree[3].depth, 3);
        assert!(tree[0].has_children);
        assert!(tree[1].has_children);
        assert!(tree[2].has_children);
        assert!(!tree[3].has_children);
    }

    #[test]
    fn build_tree_default_expanded_only_top_two_levels() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0),
            nd("0/0/0", 67, 0),
        ];
        let tree = build_tree(&nodes);
        assert!(tree[0].expanded);
        assert!(tree[1].expanded);
        assert!(!tree[2].expanded);
        assert!(!tree[3].expanded);
    }

    #[test]
    fn visible_rows_all_expanded_shows_everything() {
        let mut nodes = vec![nd("", 62, 0), nd("0", 65, 0), nd("0/0", 66, 0)];
        nodes[0].r#type = 62;
        let mut tree = build_tree(&nodes);
        for n in &mut tree {
            n.expanded = true;
        }
        let v = visible_rows(&tree);
        assert_eq!(v, vec![0, 1, 2]);
    }

    #[test]
    fn visible_rows_hides_collapsed_subtree() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0),
            nd("0/0/0", 67, 0),
            nd("1", 65, 0),
        ];
        let mut tree = build_tree(&nodes);
        tree[1].expanded = false;
        let v = visible_rows(&tree);
        assert_eq!(v, vec![0, 1, 4]);
    }
}
