import { describe, expect, it } from 'vitest';
import { buildRows, buildTree, nodeLabel } from './tree';
import type { Node as EngineNode } from './proto/engine';

const n = (path: string, name = '', type = 0, subtype = 0): EngineNode =>
    ({ path, type, subtype, guid: '', offset: '0', size: '0', name, action: 0, region: '', isNvar: false }) as EngineNode;

describe('buildTree', () => {
    it('nests by path, root "" is the tree root', () => {
        const roots = buildTree([n(''), n('0', 'ME'), n('0/0'), n('1', 'FV1'), n('1/2')]);
        expect(roots).toHaveLength(1);
        expect(roots[0].path).toBe('');
        expect(roots[0].children.map((c) => c.path)).toEqual(['0', '1']);
        expect(roots[0].children[0].children.map((c) => c.path)).toEqual(['0/0']);
    });

    it('orphan paths become roots', () => {
        const roots = buildTree([n('9'), n('9/1')]);
        expect(roots[0].path).toBe('9');
    });
});

describe('buildRows', () => {
    const roots = buildTree([n(''), n('0', 'ME'), n('0/0'), n('1', 'FV1')]);
    it('collapsed: only top level', () => {
        const rows = buildRows(roots, new Set());
        expect(rows.map((r) => r.node.path)).toEqual(['']);
        expect(rows[0].hasChildren).toBe(true);
    });
    it('expanded root shows children indented', () => {
        const rows = buildRows(roots, new Set(['', '0']));
        expect(rows.map((r) => r.node.path)).toEqual(['', '0', '0/0', '1']);
        expect(rows[1].depth).toBe(1);
        expect(rows[2].depth).toBe(2);
    });
});

describe('nodeLabel', () => {
    it('prefers name', () => {
        expect(nodeLabel(n('0/1', 'TerminalDxe'))).toBe('TerminalDxe');
    });
    it('falls back to type/subtype', () => {
        expect(nodeLabel(n('0/1', '', 3, 17))).toBe('type 3/17');
    });
});
