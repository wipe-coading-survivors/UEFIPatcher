<script lang="ts">
    import FormsTree from '$lib/components/FormsTree.svelte';
    import QuestionTable from '$lib/components/QuestionTable.svelte';
    import StringsPanel from '$lib/components/StringsPanel.svelte';
    import GatesDialog from '$lib/components/GatesDialog.svelte';
    import SetValueDialog from '$lib/components/SetValueDialog.svelte';
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
        | null = $state(null);
    let error = $state('');
    let busy = $state(false);
    let refreshSeq = 0;
    let questionsSeq = 0;

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
{/if}

<div class="cols">
    <section aria-label="forms tree">
        <FormsTree {rows} {selectedKey} {onselect} ongates={ongatesForm} onshow={onshow} {ontoggle} />
    </section>
    <section aria-label="form details">
        {#if selected}
            <h2>form {selected.formIdIfr} {selected.title}</h2>
            <div role="tablist">
                <button role="tab" aria-selected={tab === 'questions'} onclick={() => (tab = 'questions')}>Questions</button>
                <button role="tab" aria-selected={tab === 'strings'} onclick={() => (tab = 'strings')}>Strings</button>
                <button onclick={() => ongatesForm(selected!)}>Gates…</button>
                <button onclick={onDownload} disabled={busy}>Download</button>
            </div>
            {#if tab === 'questions'}
                <QuestionTable {questions} onsetvalue={onsetvalue} ongates={ongatesQ} />
            {:else}
                <StringsPanel {imageId} />
            {/if}
        {:else}
            <p>select a form</p>
        {/if}
    </section>
</div>

<style>
    .cols { display: grid; grid-template-columns: 1fr 2fr; gap: 16px; }
    .error { color: #e06c75; }
    [role='tablist'] { display: flex; gap: 8px; margin: 8px 0; }
    [role='tab'][aria-selected='true'] { border-bottom: 2px solid #7aa7d9; }
</style>
