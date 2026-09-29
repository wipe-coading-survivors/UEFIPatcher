<script lang="ts">
    import Modal from './Modal.svelte';
    import { formExport } from '../api';
    import type { HiiFormExportResponse } from '../proto/engine';

    let {
        imageId,
        itemId,
        onclose,
    }: {
        imageId: string;
        itemId: string;
        onclose: () => void;
    } = $props();

    let resp: HiiFormExportResponse | null = $state<HiiFormExportResponse | null>(null);
    let error = $state('');

    load();

    async function load() {
        error = '';
        try {
            resp = await formExport(imageId, itemId);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    function download() {
        if (!resp) return;
        const blob = new Blob([resp.schemaJson], { type: 'application/json' });
        const a = document.createElement('a');
        a.href = URL.createObjectURL(blob);
        a.download = 'form-schema.json';
        a.click();
        URL.revokeObjectURL(a.href);
    }
</script>

<Modal title="Export form" onclose={onclose}>
    {#if error}
        <p class="error" role="alert">{error}</p>
    {:else if resp}
        <p>formset {resp.formsetGuid} · parent form {resp.parentFormId}</p>
        {#if (resp.lossy ?? []).length > 0}
            <div role="alert">
                <p>lossy exports (not round-trip safe):</p>
                <ul>
                    {#each resp.lossy as l (l)}
                        <li>{l}</li>
                    {/each}
                </ul>
            </div>
        {/if}
        <label>schema json <textarea id="export-schema" readonly rows="16" cols="80">{resp.schemaJson}</textarea></label>
        <button type="button" onclick={download}>Download schema</button>
    {:else}
        <p>loading…</p>
    {/if}
    <button type="button" onclick={onclose}>Close</button>
</Modal>

<style>
    .error { color: #e06c75; }
    textarea { display: block; font-family: monospace; }
</style>
