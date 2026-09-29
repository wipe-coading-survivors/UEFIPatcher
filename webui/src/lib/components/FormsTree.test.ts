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

    it('no show button for visible forms', () => {
        const rows = buildFormRows(sets, new Set(['S1']));
        render(FormsTree, { rows, selectedKey: null, onselect: () => {}, ongates: () => {}, onshow: () => {}, ontoggle: () => {} });
        expect(screen.queryByRole('button', { name: 'show S1#1' })).not.toBeInTheDocument();
    });
});
