import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import Tree from './Tree.svelte';
import { buildRows, buildTree, type TreeNode } from '../tree';
import type { Node as EngineNode } from '../proto/engine';

const n = (path: string, name = '') =>
    ({ path, type: 0, subtype: 0, guid: '', offset: '0', size: '0', name, action: 0, region: '', isNvar: false }) as EngineNode;
const roots = buildTree([n(''), n('0', 'ME'), n('0/0'), n('0/1', 'FV1')]);

describe('Tree', () => {
    it('renders rows and expands on toggle', async () => {
        const expanded = new Set(['']);
        const rows = buildRows(roots, expanded);
        render(Tree, {
            rows,
            selectedPath: null,
            onselect: () => {},
            oncontext: () => {},
            ontoggle: (p: string) => {
                if (expanded.has(p)) expanded.delete(p);
                else expanded.add(p);
            },
        });
        expect(screen.getByText('ME')).toBeInTheDocument();
        expect(screen.queryByText('FV1')).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: 'expand 0' }));
    });

    it('fires select and context events', () => {
        const onselect = vi.fn();
        const oncontext = vi.fn();
        const rows = buildRows(roots, new Set(['', '0']));
        render(Tree, { rows, selectedPath: null, onselect, oncontext, ontoggle: () => {} });
        fireEvent.click(screen.getByText('ME'));
        expect(onselect).toHaveBeenCalled();
        fireEvent.contextMenu(screen.getByText('ME'));
        fireEvent.click(screen.getByRole('menuitem', { name: 'Extract…' }));
        expect(oncontext).toHaveBeenCalledWith(expect.objectContaining({ path: '0' }), 'extract');
    });
});
