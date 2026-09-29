import type { Node as EngineNode } from './proto/engine';

export type TreeNode = EngineNode & { children: TreeNode[] };

export function buildTree(nodes: EngineNode[]): TreeNode[] {
    const byPath = new Map<string, TreeNode>();
    for (const n of nodes) byPath.set(n.path, { ...n, children: [] });
    const roots: TreeNode[] = [];
    for (const n of nodes) {
        const node = byPath.get(n.path)!;
        const parentPath = n.path.includes('/')
            ? n.path.slice(0, n.path.lastIndexOf('/'))
            : '';
        const parent = byPath.get(parentPath);
        if (parent && parent !== node) parent.children.push(node);
        else roots.push(node);
    }
    return roots;
}

export function buildRows(
    roots: TreeNode[],
    expanded: Set<string>,
): { node: TreeNode; depth: number; hasChildren: boolean; expanded: boolean }[] {
    const rows: { node: TreeNode; depth: number; hasChildren: boolean; expanded: boolean }[] = [];
    const walk = (nodes: TreeNode[], depth: number) => {
        for (const node of nodes) {
            const isExpanded = expanded.has(node.path);
            rows.push({ node, depth, hasChildren: node.children.length > 0, expanded: isExpanded });
            if (isExpanded) walk(node.children, depth + 1);
        }
    };
    walk(roots, 0);
    return rows;
}

export function nodeLabel(n: EngineNode): string {
    return n.name !== '' ? n.name : `type ${n.type}/${n.subtype}`;
}
