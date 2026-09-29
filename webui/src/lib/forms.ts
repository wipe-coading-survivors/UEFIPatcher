import type { FormEdge, FormInfo } from './proto/engine';

export interface FormNode {
    form: FormInfo;
    children: FormNode[];
}

export interface FormsetNode {
    guid: string;
    children: FormNode[];
}

export interface FormRow {
    key: string;
    kind: 'formset' | 'form';
    formsetGuid: string;
    form: FormInfo | null;
    depth: number;
    hasChildren: boolean;
    expanded: boolean;
}

export function formItemId(form: FormInfo): string {
    return `${form.formId}#${form.formIdIfr}`;
}

export function questionItemId(form: FormInfo, questionId: number): string {
    return `${formItemId(form)}:0x${questionId.toString(16).toUpperCase()}`;
}

interface ChildEntry {
    key: string;
    form: FormInfo;
}

export function buildFormsets(forms: FormInfo[], edges: FormEdge[]): FormsetNode[] {
    const bySet = new Map<string, FormInfo[]>();
    for (const f of forms) {
        const list = bySet.get(f.formsetGuid);
        if (list) list.push(f);
        else bySet.set(f.formsetGuid, [f]);
    }
    const global = new Map<string, FormInfo>();
    for (const f of forms) global.set(`${f.formsetGuid}#${f.formIdIfr}`, f);

    const out: FormsetNode[] = [];
    for (const [guid, list] of bySet) {
        const children = new Map<number, ChildEntry[]>();
        const incoming = new Set<string>();
        for (const e of edges) {
            if (e.formsetGuid !== guid) continue;
            const targetSet = e.targetFormsetGuid || guid;
            const tf = global.get(`${targetSet}#${e.formId}`);
            if (!tf) continue;
            const key = `${targetSet}#${e.formId}`;
            const kids = children.get(e.parentFormId) ?? [];
            if (kids.some((k) => k.key === key)) continue;
            kids.push({ key, form: tf });
            children.set(e.parentFormId, kids);
            if (targetSet === guid) incoming.add(key);
        }
        const seen = new Set<string>();
        const attach = (entries: ChildEntry[], onPath: Set<string>): FormNode[] => {
            const res: FormNode[] = [];
            for (const en of entries) {
                if (onPath.has(en.key)) continue;
                const childKey = `${en.form.formsetGuid}#${en.form.formIdIfr}`;
                seen.add(childKey);
                const next = new Set(onPath).add(en.key);
                const kids = en.form.formsetGuid === guid ? children.get(en.form.formIdIfr) ?? [] : [];
                res.push({ form: en.form, children: attach(kids, next) });
            }
            return res;
        };
        const roots: FormNode[] = [];
        for (const f of list) {
            const key = `${guid}#${f.formIdIfr}`;
            if (incoming.has(key)) continue;
            seen.add(key);
            roots.push({ form: f, children: attach(children.get(f.formIdIfr) ?? [], new Set([key])) });
        }
        for (const f of list) {
            const key = `${guid}#${f.formIdIfr}`;
            if (!seen.has(key)) roots.push({ form: f, children: [] });
        }
        out.push({ guid, children: roots });
    }
    return out;
}

export function buildFormRows(sets: FormsetNode[], expanded: Set<string>): FormRow[] {
    const rows: FormRow[] = [];
    const seen = new Set<string>();
    const walk = (nodes: FormNode[], guid: string, depth: number) => {
        for (const n of nodes) {
            const key = `${n.form.formsetGuid}#${n.form.formIdIfr}`;
            if (seen.has(key)) continue;
            seen.add(key);
            const hasChildren = n.children.length > 0;
            const isExpanded = expanded.has(key);
            rows.push({ key, kind: 'form', formsetGuid: guid, form: n.form, depth, hasChildren, expanded: isExpanded });
            if (hasChildren && isExpanded) walk(n.children, guid, depth + 1);
        }
    };
    for (const s of sets) {
        const isExpanded = expanded.has(s.guid);
        rows.push({
            key: `fs:${s.guid}`,
            kind: 'formset',
            formsetGuid: s.guid,
            form: null,
            depth: 0,
            hasChildren: true,
            expanded: isExpanded,
        });
        if (isExpanded) walk(s.children, s.guid, 1);
    }
    return rows;
}
