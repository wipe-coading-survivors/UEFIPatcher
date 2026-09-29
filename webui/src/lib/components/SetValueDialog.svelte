<script lang="ts">
    import Modal from './Modal.svelte';
    import { questionInfo, setValue } from '$lib/api';
    import type { QuestionInfo } from '$lib/proto/engine';

    let {
        imageId,
        itemId,
        prompt,
        onclose,
        ondone,
    }: {
        imageId: string;
        itemId: string;
        prompt: string;
        onclose: () => void;
        ondone: () => void;
    } = $props();

    let info: QuestionInfo | null = $state(null);
    let error = $state('');
    let busy = $state(false);
    let value = $state('');
    let done: { flips: string[]; stores: string[] } | null = $state(null);

    $effect(() => {
        imageId;
        itemId;
        info = null;
        error = '';
        done = null;
        value = '';
        questionInfo(imageId, itemId)
            .then((r) => {
                info = r.question ?? null;
                if (info) value = info.options.length > 0 ? info.options[0].value : info.min;
            })
            .catch((e: unknown) => {
                error = e instanceof Error ? e.message : String(e);
            });
    });

    async function onApply() {
        error = '';
        if (!/^\d+$/.test(value)) {
            error = 'value must be decimal digits (0..18446744073709551615)';
            return;
        }
        busy = true;
        try {
            const r = await setValue(imageId, itemId, value);
            done = { flips: r.appliedFlips ?? [], stores: r.stores ?? [] };
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<Modal title="Set value" {onclose}>
    <h3>{prompt}</h3>
    <p class="meta">
        {#if info}
            kind {info.kind} · store {info.varStoreId} @0x{info.varOffset.toString(16)} · width {info.width}
        {/if}
    </p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if info === null && !error}
        <p>loading…</p>
    {:else if info}
        {#if info.options.length > 0}
            <select aria-label="value" bind:value>
                {#each info.options as o (o.value)}
                    <option value={o.value}>{o.text || o.value}</option>
                {/each}
            </select>
        {:else}
            <label>
                value
                <input id="value" aria-label="value" type="text" bind:value />
            </label>
            <p class="meta">min {info.min} · max {info.max} · step {info.step}</p>
        {/if}
    {/if}
    {#if done !== null}
        <p role="status">value set</p>
        {#if done.flips.length > 0}
            <p>flips:</p>
            <ul>{#each done.flips as f (f)}<li><code>{f}</code></li>{/each}</ul>
        {/if}
        {#if done.stores.length > 0}
            <p>stores: {done.stores.join(', ')}</p>
        {/if}
    {/if}
    <footer>
        {#if done !== null}
            <button class="primary" onclick={() => { ondone(); onclose(); }}>Done</button>
        {:else}
            <button onclick={onApply} disabled={busy || info === null}>Apply</button>
            <button onclick={onclose}>Close</button>
        {/if}
    </footer>
</Modal>

<style>
    h3 { margin: 4px 0; }
    .meta { color: #6b7280; font-size: 0.85em; }
    .error { color: #e06c75; }
    footer { display: flex; gap: 8px; justify-content: flex-end; margin-top: 8px; }
    select, input { background: #23262e; color: inherit; border: 1px solid #3a3f49; border-radius: 4px; padding: 4px; }
</style>
