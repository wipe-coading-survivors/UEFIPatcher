<script lang="ts">
    import FormsTree from '$lib/components/FormsTree.svelte';
    import QuestionTable from '$lib/components/QuestionTable.svelte';
    import StringsPanel from '$lib/components/StringsPanel.svelte';
    import GatesDialog from '$lib/components/GatesDialog.svelte';
    import SetValueDialog from '$lib/components/SetValueDialog.svelte';
    import SchemaDialog from '$lib/components/SchemaDialog.svelte';
    import ExportDialog from '$lib/components/ExportDialog.svelte';
    import ContextMenu from '$lib/components/ContextMenu.svelte';
    import { downloadImage, formTree, listForms, listQuestions, setFormVisibility } from '$lib/api';
    import { buildFormRows, buildFormsets, formItemId, questionItemId, type FormRow } from '$lib/forms';
    import type { FormInfo } from '$lib/proto/engine';

    let { data }: { data: { imageId: string } } = $props();
    const imageId = $derived(data.imageId);

    let rows: FormRow[] = $state([]);
    let sets: import('$lib/forms').FormsetNode[] = $state([]);
    let expanded = $state(new Set<string>());
    let expandedInit = false;
    let selected = $state<FormInfo | null>(null);
    let questions = $state<import('$lib/proto/engine').QuestionSummary[]>([]);
    let tab: 'questions' | 'strings' = $state('questions');
    let dialog:
        | { kind: 'gates'; itemId: string; title: string }
        | { kind: 'setvalue'; itemId: string; prompt: string }
        | { kind: 'schema'; op: 'formset' | 'form' | 'question' | 'page' | 'hijack'; target: string }
        | { kind: 'export'; itemId: string }
        | null = $state(null);
    let error = $state('');
    let busy = $state(false);
    let refreshSeq = 0;
    let questionsSeq = 0;
    let ctx: {
        x: number;
        y: number;
        kind: 'form' | 'question';
        form: FormInfo | null;
        question: import('$lib/proto/engine').QuestionSummary | null;
    } | null = $state(null);
    const formMenuItems = [
        { id: 'gates', label: 'Gates…' },
        { id: 'export', label: 'Export form…' },
        { id: 'addquestion', label: 'Add question…' },
        { id: 'addpage', label: 'Add page…' },
        { id: 'hijack', label: 'Hijack…' },
    ];
    const questionMenuItems = [
        { id: 'setvalue', label: 'Set value…' },
        { id: 'gates', label: 'Gates…' },
    ];

    function openFormMenu(form: FormInfo, e: MouseEvent) {
        ctx = { x: e.clientX, y: e.clientY, kind: 'form', form, question: null };
    }

    function openQuestionMenu(q: import('$lib/proto/engine').QuestionSummary, e: MouseEvent) {
        if (!selected) return;
        ctx = { x: e.clientX, y: e.clientY, kind: 'question', form: null, question: q };
    }

    function onmenupick(id: string) {
        const c = ctx;
        ctx = null;
        if (!c) return;
        if (c.kind === 'form' && c.form) {
            if (id === 'gates') ongatesForm(c.form);
            else if (id === 'export') dialog = { kind: 'export', itemId: formItemId(c.form) };
            else if (id === 'addquestion') dialog = { kind: 'schema', op: 'question', target: formItemId(c.form) };
            else if (id === 'addpage') dialog = { kind: 'schema', op: 'page', target: c.form.formId };
            else if (id === 'hijack') dialog = { kind: 'schema', op: 'hijack', target: c.form.formId };
        } else if (c.kind === 'question' && c.question) {
            if (id === 'setvalue') onsetvalue(c.question);
            else if (id === 'gates') ongatesQ(c.question);
        }
    }

    const selectedKey = $derived(selected ? `${selected.formsetGuid}#${selected.formIdIfr}` : null);

    async function refresh() {
        const seq = ++refreshSeq;
        error = '';
        try {
            const [fr, tr] = await Promise.all([listForms(imageId), formTree(imageId)]);
            if (seq !== refreshSeq) return;
            sets = buildFormsets(fr.forms ?? [], tr.edges ?? []);
            if (!expandedInit) {
                expanded = new Set(sets.map((s) => s.guid));
                expandedInit = true;
            }
            rows = buildFormRows(sets, expanded);
            if (selected) {
                const still = (fr.forms ?? []).find(
                    (f) => f.formsetGuid === selected!.formsetGuid && f.formIdIfr === selected!.formIdIfr,
                );
                if (!still) selected = null;
                else selected = still;
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        imageId;
        refresh();
    });

    async function loadQuestions(form: FormInfo) {
        const seq = ++questionsSeq;
        error = '';
        questions = [];
        try {
            const r = await listQuestions(imageId, form.formId, form.formIdIfr);
            if (seq !== questionsSeq) return;
            questions = r.questions ?? [];
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    function onselect(form: FormInfo) {
        selected = form;
        loadQuestions(form);
    }

    function ontoggle(key: string) {
        const next = new Set(expanded);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        expanded = next;
        rows = buildFormRows(sets, next);
    }

    async function onshow(form: FormInfo) {
        error = '';
        try {
            await setFormVisibility(imageId, formItemId(form), true);
            await refresh();
            if (selected?.formIdIfr === form.formIdIfr && selected?.formsetGuid === form.formsetGuid) {
                await loadQuestions(selected);
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    function ongatesForm(form: FormInfo) {
        dialog = { kind: 'gates', itemId: formItemId(form), title: `Gates: form ${form.formIdIfr}` };
    }

    function ongatesQ(q: import('$lib/proto/engine').QuestionSummary) {
        if (!selected) return;
        dialog = {
            kind: 'gates',
            itemId: questionItemId(selected, q.questionId),
            title: `Gates: 0x${q.questionId.toString(16).toUpperCase()}`,
        };
    }

    function onsetvalue(q: import('$lib/proto/engine').QuestionSummary) {
        if (!selected) return;
        dialog = {
            kind: 'setvalue',
            itemId: questionItemId(selected, q.questionId),
            prompt: q.prompt || `0x${q.questionId.toString(16).toUpperCase()}`,
        };
    }

    function onschema(op: 'formset' | 'form' | 'question' | 'page' | 'hijack') {
        dialog = { kind: 'schema', op, target: selected ? selected.formId : '' };
    }

    function onaddquestion() {
        if (!selected) return;
        dialog = { kind: 'schema', op: 'question', target: formItemId(selected) };
    }

    function onexport() {
        if (!selected) return;
        dialog = { kind: 'export', itemId: formItemId(selected) };
    }

    async function ondialogdone() {
        await refresh();
        if (selected) await loadQuestions(selected);
    }

    async function onDownload() {
        busy = true;
        try {
            const blob = await downloadImage(imageId);
            const a = document.createElement('a');
            a.href = URL.createObjectURL(blob);
            a.download = 'patched.bin';
            a.click();
            URL.revokeObjectURL(a.href);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<h1>Forms</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if dialog?.kind === 'gates'}
    <GatesDialog {imageId} itemId={dialog.itemId} title={dialog.title} onclose={() => (dialog = null)} ondone={ondialogdone} />
{:else if dialog?.kind === 'setvalue'}
    <SetValueDialog {imageId} itemId={dialog.itemId} prompt={dialog.prompt} onclose={() => (dialog = null)} ondone={ondialogdone} />
{:else if dialog?.kind === 'schema'}
    <SchemaDialog {imageId} op={dialog.op} target={dialog.target} onclose={() => (dialog = null)} ondone={ondialogdone} />
{:else if dialog?.kind === 'export'}
    <ExportDialog {imageId} itemId={dialog.itemId} onclose={() => (dialog = null)} />
{/if}

<div class="cols">
    <section aria-label="forms tree">
        <FormsTree {rows} {selectedKey} {onselect} ongates={ongatesForm} onshow={onshow} {ontoggle} oncontext={openFormMenu} />
    </section>
    <section aria-label="form details">
        <div role="toolbar" aria-label="hii add operations">
            <button onclick={() => onschema('formset')}>Add formset…</button>
            <button onclick={() => onschema('form')} disabled={!selected}>Add form…</button>
            <button onclick={() => onschema('page')} disabled={!selected}>Add page…</button>
            <button onclick={() => onschema('hijack')} disabled={!selected}>Hijack…</button>
            <button onclick={onexport} disabled={!selected}>Export form…</button>
        </div>
        {#if selected}
            <h2>form {selected.formIdIfr} {selected.title}</h2>
            <dl class="form-meta">
                <dt>Form</dt><dd>{selected.title}</dd>
                <dt>Form ID</dt><dd>{selected.formIdIfr}</dd>
                <dt>FormSet</dt><dd>{selected.formsetGuid}</dd>
                <dt>Target</dt><dd>{formItemId(selected)}</dd>
            </dl>
            <div role="tablist">
                <button role="tab" aria-selected={tab === 'questions'} onclick={() => (tab = 'questions')}>Questions</button>
                <button role="tab" aria-selected={tab === 'strings'} onclick={() => (tab = 'strings')}>Strings</button>
                <button onclick={() => ongatesForm(selected!)}>Gates…</button>
                <button onclick={onDownload} disabled={busy}>Download</button>
            </div>
            {#if tab === 'questions'}
                <QuestionTable {questions} onsetvalue={onsetvalue} ongates={ongatesQ} onadd={onaddquestion} oncontext={openQuestionMenu} />
            {:else}
                <StringsPanel {imageId} />
            {/if}
        {:else}
            <p>select a form</p>
        {/if}
    </section>
    {#if ctx}
        <ContextMenu
            x={ctx.x}
            y={ctx.y}
            items={ctx.kind === 'form' ? formMenuItems : questionMenuItems}
            onpick={onmenupick}
            onclose={() => (ctx = null)}
        />
    {/if}
</div>

<style>
    .cols { display: grid; grid-template-columns: 1fr 2fr; gap: 16px; align-items: start; }
    .cols > section { max-height: calc(100vh - 220px); overflow-y: auto; }
    .error { color: #e06c75; }
    [role='tablist'] { display: flex; gap: 8px; margin: 8px 0; }
    [role='toolbar'] { display: flex; gap: 8px; margin: 8px 0; }
    [role='tab'][aria-selected='true'] { border-bottom: 2px solid #7aa7d9; }
    .form-meta { display: grid; grid-template-columns: max-content 1fr; gap: 2px 12px; margin: 4px 0 8px; font-size: 13px; }
    .form-meta dt { color: #9aa0aa; }
    .form-meta dd { margin: 0; word-break: break-all; }
</style>
