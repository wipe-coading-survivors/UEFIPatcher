import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import FormsTree from './FormsTree.svelte';
import { buildFormRows, buildFormsets, type FormRow } from '../forms';
import type { FormInfo } from '../proto/engine';

const form = (formsetGuid: string, formIdIfr: number, title = '', visible = true): FormInfo => ({
    formId: `${formsetGuid}:0x10:0`,
    formsetGuid,
    formIdIfr,
    title,
    visible,
});

const sets = buildFormsets(
    [form('S1', 1, 'Main'), form('S1', 2, 'Hidden', false), form('S2', 5, 'Other')],
    [],
);

describe('FormsTree', () => {
    it('renders collapsed formsets; expand shows forms', async () => {
        const rows = buildFormRows(sets, new Set());
        const { rerender } = render(FormsTree, {
            rows,
            selectedKey: null,
            onselect: () => {},
            ongates: () => {},
            onshow: () => {},
            ontoggle: () => {},
        });
        expect(screen.getByText('S1')).toBeInTheDocument();
        expect(screen.queryByText(/Main/)).not.toBeInTheDocument();
        const expanded = new Set(['S1']);
        const rows2 = buildFormRows(sets, expanded);
        await rerender({ rows: rows2, selectedKey: null, onselect: () => {}, ongates: () => {}, onshow: () => {}, ontoggle: () => {} });
        expect(screen.getByText(/Main/)).toBeInTheDocument();
        expect(screen.queryByText(/Other/)).not.toBeInTheDocument();
    });

    it('fires select, gates, show, toggle', async () => {
        const onselect = vi.fn();
        const ongates = vi.fn();
        const onshow = vi.fn();
        const ontoggle = vi.fn();
        const rows = buildFormRows(sets, new Set(['S1', 'S2']));
        render(FormsTree, { rows, selectedKey: null, onselect, ongates, onshow, ontoggle });
        fireEvent.click(screen.getByText(/Main/));
        expect(onselect).toHaveBeenCalledWith(expect.objectContaining({ formIdIfr: 1 }));
        fireEvent.click(screen.getByRole('button', { name: 'gates S1#2' }));
        expect(ongates).toHaveBeenCalledWith(expect.objectContaining({ formIdIfr: 2 }));
        fireEvent.click(screen.getByRole('button', { name: 'show S1#2' }));
        expect(onshow).toHaveBeenCalledWith(expect.objectContaining({ formIdIfr: 2 }));
        fireEvent.click(screen.getByRole('button', { name: 'expand S1' }));
        expect(ontoggle).toHaveBeenCalledWith('S1');
    });

    it('fires oncontext on form row right-click', () => {
        const oncontext = vi.fn();
        const rows = buildFormRows(sets, new Set(['S1']));
        render(FormsTree, {
            rows,
            selectedKey: null,
            onselect: () => {},
            ongates: () => {},
            onshow: () => {},
            ontoggle: () => {},
            oncontext,
        });
        fireEvent.contextMenu(screen.getByText(/Main/));
        expect(oncontext).toHaveBeenCalledWith(
            expect.objectContaining({ formIdIfr: 1 }),
            expect.any(MouseEvent),
        );
    });

    it('form row triangle reflects expanded state', () => {
        const parentSets = buildFormsets(
            [form('S1', 1, 'Root'), form('S1', 2, 'Child')],
            [{ formsetGuid: 'S1', parentFormId: 1, formId: 2, targetFormsetGuid: '' }],
        );
        const { rerender } = render(FormsTree, {
            rows: buildFormRows(parentSets, new Set(['S1'])),
            selectedKey: null,
            onselect: () => {},
            ongates: () => {},
            onshow: () => {},
            ontoggle: () => {},
        });
        const btn = screen.getByLabelText('expand S1#1');
        expect(btn).toHaveTextContent('▸');
        expect(btn).toHaveAttribute('aria-expanded', 'false');
        rerender({
            rows: buildFormRows(parentSets, new Set(['S1', 'S1#1'])),
            selectedKey: null,
            onselect: () => {},
            ongates: () => {},
            onshow: () => {},
            ontoggle: () => {},
        });
        expect(screen.getByLabelText('expand S1#1')).toHaveTextContent('▾');
        expect(screen.getByLabelText('expand S1#1')).toHaveAttribute('aria-expanded', 'true');
    });

    it('no show button for visible forms', () => {
        const rows = buildFormRows(sets, new Set(['S1']));
        render(FormsTree, { rows, selectedKey: null, onselect: () => {}, ongates: () => {}, onshow: () => {}, ontoggle: () => {} });
        expect(screen.queryByRole('button', { name: 'show S1#1' })).not.toBeInTheDocument();
    });
});
