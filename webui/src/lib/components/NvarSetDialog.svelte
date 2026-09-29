<script lang="ts">
    import Modal from './Modal.svelte';
    import { nvarSet } from '../api';
    import { parseNum, type VarRow } from '../nvar';

    let {
        imageId,
        prefill,
        onclose,
        ondone,
    }: {
        imageId: string;
        prefill: VarRow;
        onclose: () => void;
        ondone: () => void;
    } = $props();

    // svelte-ignore state_referenced_locally
    let name = $state(prefill.name);
    // svelte-ignore state_referenced_locally
    let guid = $state(prefill.guid);
    let offset = $state('');
    let value = $state('');
    let width = $state(1);
    let error = $state('');
    let result: { applied: string[]; stores: string[] } | null = $state(null);
    let busy = $state(false);

    async function apply() {
        error = '';
        result = null;
        if (!name.trim()) {
            error = 'name is required';
            return;
        }
        const off = parseNum(offset);
        if (off === null) {
            error = 'offset must be decimal or 0x-hex';
            return;
        }
        const val = parseNum(value);
        if (val === null) {
            error = 'value must be decimal or 0x-hex';
            return;
        }
        busy = true;
        try {
            result = await nvarSet(imageId, name.trim(), guid.trim(), off, val, width);
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

<Modal title="NVRAM set: {prefill.name}" onclose={onclose}>
    <form onsubmit={(e) => { e.preventDefault(); apply(); }}>
        <label>name <input id="nvar-name" bind:value={name} /></label>
        <label>guid <input id="nvar-guid" bind:value={guid} placeholder="required when name is ambiguous" /></label>
        <label>offset <input id="nvar-offset" bind:value={offset} placeholder="0x0" /></label>
        <label>value <input id="nvar-value" bind:value={value} placeholder="0" /></label>
        <label>width <input id="nvar-width" type="number" min="1" max="8" bind:value={width} /></label>
        {#if error}<p class="error" role="alert">{error}</p>{/if}
        {#if result}
            <p role="status">applied {result.applied.length} stores (of {result.stores.length}): {result.applied.join(' · ') || 'none'}</p>
        {/if}
        <button type="submit" disabled={busy}>Apply</button>
        {#if result}
            <button type="button" onclick={done}>Done</button>
        {:else}
            <button type="button" onclick={onclose}>Close</button>
        {/if}
    </form>
</Modal>

<style>
    label { display: block; margin: 4px 0; }
    input { margin-left: 8px; }
    .error { color: #e06c75; }
</style>
