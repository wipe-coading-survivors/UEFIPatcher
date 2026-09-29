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

    it('replaces open menu when contextmenu fires on another node', () => {
        const oncontext = vi.fn();
        const rows = buildRows(roots, new Set(['', '0']));
        render(Tree, { rows, selectedPath: null, onselect: () => {}, oncontext, ontoggle: () => {} });
        fireEvent.contextMenu(screen.getByText('ME'));
        fireEvent.contextMenu(screen.getByText('FV1'));
        fireEvent.click(screen.getByRole('menuitem', { name: 'Remove…' }));
        expect(oncontext).toHaveBeenCalledTimes(1);
        expect(oncontext).toHaveBeenCalledWith(expect.objectContaining({ path: '0/1' }), 'remove');
    });

    it('closes menu on Escape', async () => {
        render(Tree, { rows: buildRows(roots, new Set(['', '0'])), selectedPath: null, onselect: () => {}, oncontext: () => {}, ontoggle: () => {} });
        fireEvent.contextMenu(screen.getByText('ME'));
        expect(screen.getByRole('menu')).toBeInTheDocument();
        await fireEvent.keyDown(window, { key: 'Escape' });
        await vi.waitFor(() => expect(screen.queryByRole('menu')).not.toBeInTheDocument());
    });

    it('expand triangle reflects collapsed state', () => {
        render(Tree, { rows: buildRows(roots, new Set([''])), selectedPath: null, onselect: () => {}, oncontext: () => {}, ontoggle: () => {} });
        const btn = screen.getByLabelText('expand 0');
        expect(btn).toHaveTextContent('▸');
        expect(btn).toHaveAttribute('aria-expanded', 'false');
    });

    it('expand triangle reflects expanded state', () => {
        render(Tree, { rows: buildRows(roots, new Set(['', '0'])), selectedPath: null, onselect: () => {}, oncontext: () => {}, ontoggle: () => {} });
        const [root, me] = screen.getAllByRole('button', { name: /expand/ });
        expect(root).toHaveTextContent('▾');
        expect(root).toHaveAttribute('aria-expanded', 'true');
        expect(me).toHaveTextContent('▾');
        expect(me).toHaveAttribute('aria-expanded', 'true');
    });

    it('renders aria-hidden store icon per row', () => {
        render(Tree, { rows: buildRows(roots, new Set(['', '0'])), selectedPath: null, onselect: () => {}, oncontext: () => {}, ontoggle: () => {} });
        const icons = document.querySelectorAll('span.nf');
        expect(icons.length).toBeGreaterThan(0);
        expect(icons[0]).toHaveAttribute('aria-hidden', 'true');
    });
});
