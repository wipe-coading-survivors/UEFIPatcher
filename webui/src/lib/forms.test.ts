import { describe, expect, it } from 'vitest';
import { buildFormRows, buildFormsets, formItemId, questionItemId, type FormNode } from './forms';
import type { FormEdge, FormInfo } from './proto/engine';

const form = (formsetGuid: string, formIdIfr: number, title = '', visible = true): FormInfo => ({
    formId: `${formsetGuid}:0x10:0`,
    formsetGuid,
    formIdIfr,
    title,
    visible,
});

const edge = (formsetGuid: string, parentFormId: number, formId: number, targetFormsetGuid = ''): FormEdge =>
    ({ formsetGuid, parentFormId, formId, targetFormsetGuid });

describe('item ids', () => {
    it('formItemId: decimal form id after #', () => {
        expect(formItemId(form('AAA', 10029))).toBe('AAA:0x10:0#10029');
    });
    it('questionItemId: hex upper qid after colon', () => {
        expect(questionItemId(form('AAA', 10029), 0x3b)).toBe('AAA:0x10:0#10029:0x3B');
        expect(questionItemId(form('AAA', 1), 59)).toBe('AAA:0x10:0#1:0x3B');
    });
});

describe('buildFormsets', () => {
    it('groups by formset in first-seen order, forms keep order', () => {
        const sets = buildFormsets(
            [form('S1', 1, 'One'), form('S2', 2, 'Two'), form('S1', 3, 'Three')],
            [],
        );
        expect(sets.map((s) => s.guid)).toEqual(['S1', 'S2']);
        expect(sets[0].children.map((n) => n.form.formIdIfr)).toEqual([1, 3]);
        expect(sets[1].children.map((n) => n.form.formIdIfr)).toEqual([2]);
    });

    it('nests REF-targets under parent, roots exclude incoming', () => {
        const forms = [form('S', 1, 'Main'), form('S', 2, 'Child'), form('S', 3, 'GrandChild')];
        const sets = buildFormsets(forms, [edge('S', 1, 2), edge('S', 2, 3)]);
        expect(sets[0].children.map((n) => n.form.formIdIfr)).toEqual([1]);
        const child = sets[0].children[0].children[0];
        expect(child.form.formIdIfr).toBe(2);
        expect(child.children[0].form.formIdIfr).toBe(3);
    });

    it('cross-formset edge nests target form under source parent, target stays root in own set', () => {
        const forms = [form('S', 1), form('T', 5, 'Foreign')];
        const sets = buildFormsets(forms, [edge('S', 1, 5, 'T')]);
        const sRoot = sets[0].children[0];
        expect(sRoot.children).toHaveLength(1);
        expect(sRoot.children[0].form.formsetGuid).toBe('T');
        expect(sRoot.children[0].form.formIdIfr).toBe(5);
        expect(sets[1].children.map((n) => n.form.formIdIfr)).toEqual([5]);
    });

    it('dedups repeated edges', () => {
        const forms = [form('S', 1), form('S', 2)];
        const sets = buildFormsets(forms, [edge('S', 1, 2), edge('S', 1, 2)]);
        expect(sets[0].children[0].children).toHaveLength(1);
    });

    it('cycle does not hang: cycle members surface as roots', () => {
        const forms = [form('S', 1), form('S', 2)];
        const sets = buildFormsets(forms, [edge('S', 1, 2), edge('S', 2, 1)]);
        const flat: number[] = [];
        const walk = (nodes: FormNode[]) => {
            for (const n of nodes) { flat.push(n.form.formIdIfr); walk(n.children); }
        };
        walk(sets[0].children);
        expect(flat.sort()).toEqual([1, 2]);
    });

    it('dangling REF target is skipped', () => {
        const forms = [form('S', 1)];
        const sets = buildFormsets(forms, [edge('S', 1, 99)]);
        expect(sets[0].children[0].children).toHaveLength(0);
    });
});

describe('buildFormRows', () => {
    const sets = buildFormsets(
        [form('S', 1, 'Main'), form('S', 2, 'Child')],
        [edge('S', 1, 2)],
    );

    it('collapsed formset emits single row', () => {
        const rows = buildFormRows(sets, new Set());
        expect(rows).toHaveLength(1);
        expect(rows[0]).toMatchObject({ kind: 'formset', formsetGuid: 'S', depth: 0, expanded: false });
    });

    it('expanded formset walks nested forms with depth', () => {
        const rows = buildFormRows(sets, new Set(['S', 'S#1']));
        expect(rows.map((r) => [r.kind, r.depth, r.form?.formIdIfr ?? null])).toEqual([
            ['formset', 0, null],
            ['form', 1, 1],
            ['form', 2, 2],
        ]);
        expect(rows[1].hasChildren).toBe(true);
        expect(rows[2].hasChildren).toBe(false);
    });
});
