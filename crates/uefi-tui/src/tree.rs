use crate::app::TreeNode;
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
                region: nd.region.clone(),
                action: nd.action as u8,
                expanded: depth == 0,
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

pub fn compute_scrolled_offset(
    cursor: usize,
    prev_off: usize,
    inner_h: usize,
    total: usize,
    pad: usize,
) -> usize {
    if inner_h == 0 || total <= inner_h {
        return 0;
    }
    let max_off = total - inner_h;
    let pad = pad.min(inner_h.saturating_sub(1) / 2);
    let first = prev_off;
    let last = prev_off + inner_h - 1;
    if cursor < first + pad {
        cursor.saturating_sub(pad).min(max_off)
    } else if cursor > last.saturating_sub(pad) {
        cursor
            .saturating_add(pad)
            .saturating_sub(inner_h - 1)
            .min(max_off)
    } else {
        prev_off.min(max_off)
    }
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
            action: 0,
            region: String::new(),
        }
    }

    #[test]
    fn build_tree_maps_action_from_proto() {
        let mut n = nd("0/0", 66, 0x07);
        n.action = 54;
        let tree = build_tree(&[nd("", 62, 0), n]);
        assert_eq!(tree[1].action, 54);
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
    fn build_tree_default_expanded_only_root() {
        let nodes = vec![
            nd("", 62, 0),
            nd("0", 65, 0),
            nd("0/0", 66, 0),
            nd("0/0/0", 67, 0),
        ];
        let tree = build_tree(&nodes);
        assert!(tree[0].expanded);
        assert!(!tree[1].expanded);
        assert!(!tree[2].expanded);
        assert!(!tree[3].expanded);
    }

    #[test]
    fn build_tree_transfers_region() {
        let mut me = nd("0", 63, 0);
        me.region = "ME".into();
        let tree = build_tree(&[nd("", 62, 0), me]);
        assert_eq!(tree[0].region, "");
        assert_eq!(tree[1].region, "ME");
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

    #[test]
    fn offset_fits_all_returns_zero() {
        assert_eq!(compute_scrolled_offset(5, 3, 10, 5, 2), 0);
        assert_eq!(compute_scrolled_offset(0, 0, 10, 5, 2), 0);
    }

    #[test]
    fn offset_keeps_stable_within_padding_zone() {
        let inner_h = 10;
        let total = 100;
        let pad = 2;
        let off = 5;
        for cur in (off + pad)..=(off + inner_h - 1 - pad) {
            assert_eq!(
                compute_scrolled_offset(cur, off, inner_h, total, pad),
                off,
                "cursor {cur} should not scroll"
            );
        }
    }

    #[test]
    fn offset_scrolls_down_at_bottom_padding_boundary() {
        let inner_h = 10;
        let total = 100;
        let pad = 2;
        let off = 5;
        let cur = off + inner_h - 1 - pad + 1;
        let new_off = compute_scrolled_offset(cur, off, inner_h, total, pad);
        assert!(new_off > off);
        assert_eq!(cur - new_off, inner_h - 1 - pad);
    }

    #[test]
    fn offset_scrolls_up_at_top_padding_boundary() {
        let inner_h = 10;
        let total = 100;
        let pad = 2;
        let off = 10;
        let cur = off + pad - 1;
        let new_off = compute_scrolled_offset(cur, off, inner_h, total, pad);
        assert!(new_off < off);
        assert_eq!(cur - new_off, pad);
    }

    #[test]
    fn offset_clamps_at_top_and_bottom() {
        let inner_h = 10;
        let total = 100;
        let pad = 2;
        assert_eq!(compute_scrolled_offset(0, 0, inner_h, total, pad), 0);
        assert_eq!(
            compute_scrolled_offset(99, 0, inner_h, total, pad),
            total - inner_h
        );
    }

    #[test]
    fn offset_caps_pad_for_tiny_views() {
        let inner_h = 3;
        let total = 100;
        let new_off = compute_scrolled_offset(50, 40, inner_h, total, 2);
        assert!(new_off <= 50);
        assert!(new_off + inner_h > 50);
    }
}
