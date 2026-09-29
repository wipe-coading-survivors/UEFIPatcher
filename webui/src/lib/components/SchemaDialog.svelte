<script lang="ts">
    import Modal from './Modal.svelte';
    import { formAdd, formHijack, formSetAdd, pageAdd, questionAdd } from '../api';

    let {
        imageId,
        op,
        target,
        onclose,
        ondone,
    }: {
        imageId: string;
        op: 'formset' | 'form' | 'question' | 'page' | 'hijack';
        target: string;
        onclose: () => void;
        ondone: () => void;
    } = $props();

    const titles: Record<string, string> = {
        formset: 'Add formset',
        form: 'Add form',
        question: 'Add question',
        page: 'Add page',
        hijack: 'Hijack form',
    };

    // svelte-ignore state_referenced_locally
    let tgt = $state(target);
    let ffsGuid = $state('');
    let setupdataGuid = $state('');
    let schemaJson = $state('');
    let error = $state('');
    let result = $state('');
    let busy = $state(false);

    async function onfile(files: FileList | null) {
        error = '';
        if (!files?.length) return;
        try {
            schemaJson = await new Promise<string>((resolve, reject) => {
                const fr = new FileReader();
                fr.onload = () => resolve(fr.result as string);
                fr.onerror = () => reject(fr.error ?? new Error('schema file read failed'));
                fr.readAsText(files[0]);
            });
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    async function apply() {
        error = '';
        result = '';
        if (!schemaJson.trim()) {
            error = 'schema file is required';
            return;
        }
        if (op !== 'formset' && !tgt.trim()) {
            error = 'target is required';
            return;
        }
        busy = true;
        try {
            if (op === 'formset') {
                const r = await formSetAdd(imageId, schemaJson, ffsGuid.trim());
                result = `new ffs ${r.newFfsId} · forms [${(r.insertedFormIds ?? []).join(', ')}] · strings ${Object.keys(r.stringIds ?? {}).length}`;
            } else if (op === 'form') {
                const r = await formAdd(imageId, tgt.trim(), schemaJson);
                result = `forms [${(r.insertedFormIds ?? []).join(', ')}] · strings ${Object.keys(r.stringIds ?? {}).length}`;
            } else if (op === 'question') {
                const r = await questionAdd(imageId, tgt.trim(), schemaJson);
                result = `questions added: ${(r.questions ?? []).map((q) => q.questionId).join(', ') || 'none'} · refs ${(r.refs ?? []).length}`;
            } else if (op === 'page') {
                const r = await pageAdd(imageId, tgt.trim(), schemaJson);
                result = `form ${r.formId} · slot ${r.slot} · title string ${r.titleStringId}`;
            } else {
                const r = await formHijack(imageId, tgt.trim(), schemaJson, setupdataGuid.trim());
                result = `form IFR [${r.formIfrStart}..${r.formIfrEnd}] · flips ${(r.unlockFlips ?? []).length} · strings ${Object.keys(r.stringIds ?? {}).length}`;
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }

    function done() {
        ondone();
        onclose();
    }
</script>

<Modal title={titles[op]} onclose={onclose}>
    {#if op !== 'formset'}
        <label>target <input id="schema-target" bind:value={tgt} placeholder="899407D7-…:0x10:0" /></label>
    {/if}
    {#if op === 'formset'}
        <label>target ffs guid <input id="schema-ffs" bind:value={ffsGuid} placeholder="optional — append into existing FFS" /></label>
    {/if}
    {#if op === 'hijack'}
        <label>setupdata guid <input id="schema-setupdata" bind:value={setupdataGuid} placeholder="optional AMI SetupData GUID" /></label>
    {/if}
    <label>schema file <input id="schema-file" type="file" accept=".json,application/json" onchange={(e) => onfile(e.currentTarget.files)} /></label>
    {#if schemaJson}
        <p role="status">schema loaded: {schemaJson.length} bytes</p>
    {/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if result}<p role="status">{result}</p>{/if}
    <button type="button" onclick={apply} disabled={busy}>Apply</button>
    {#if result}
        <button type="button" onclick={done}>Done</button>
    {:else}
        <button type="button" onclick={onclose}>Close</button>
    {/if}
</Modal>

<style>
    label { display: block; margin: 4px 0; }
    input { margin-left: 8px; }
    .error { color: #e06c75; }
</style>
