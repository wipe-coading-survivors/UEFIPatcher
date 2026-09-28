<script lang="ts">
    import Modal from './Modal.svelte';
    import { gatesList, hiiUnlock } from '$lib/api';
    import type { GateInfo } from '$lib/proto/engine';

    let {
        imageId,
        itemId,
        title,
        onclose,
        ondone,
    }: {
        imageId: string;
        itemId: string;
        title: string;
        onclose: () => void;
        ondone: () => void;
    } = $props();

    let gates: GateInfo[] | null = $state(null);
    let error = $state('');
    let busy = $state(false);
    let applied: string[] | null = $state(null);

    $effect(() => {
        imageId;
        itemId;
        gates = null;
        error = '';
        applied = null;
        gatesList(imageId, itemId)
            .then((r) => {
                gates = r.gates ?? [];
            })
            .catch((e: unknown) => {
                error = e instanceof Error ? e.message : String(e);
            });
    });

    async function onUnlock() {
        error = '';
        busy = true;
        try {
            const r = await hiiUnlock(imageId, itemId);
            applied = r.appliedFlips ?? [];
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<Modal {title} {onclose}>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if gates === null && !error}
        <p>loading…</p>
    {:else if (gates ?? []).length === 0}
        <p>no gates</p>
    {:else}
        <table>
            <thead>
                <tr><th>kind</th><th>wraps</th><th>host form</th><th>expression</th><th>flippable</th></tr>
            </thead>
            <tbody>
                {#each gates ?? [] as g, i (i)}
                    <tr>
                        <td>{g.gateKind}</td>
                        <td>{g.wraps}</td>
                        <td>{g.hostFormId}</td>
                        <td><code>{g.expression}</code></td>
                        <td>{g.flippable ? (g.sourceTarget ? `donor ${g.sourceTarget}` : 'yes') : 'no'}</td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}
    {#if applied !== null}
        <p role="status">unlocked: {applied.length} flips applied</p>
        {#if applied.length > 0}
            <ul>
                {#each applied as f (f)}<li><code>{f}</code></li>{/each}
            </ul>
        {/if}
    {/if}
    <footer>
        <button onclick={onUnlock} disabled={busy || !(gates ?? []).some((g) => g.flippable)}>Unlock</button>
        {#if applied !== null}
            <button class="primary" onclick={() => { ondone(); onclose(); }}>Done</button>
        {:else}
            <button onclick={onclose}>Close</button>
        {/if}
    </footer>
</Modal>

<style>
    table { border-collapse: collapse; margin: 8px 0; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    footer { display: flex; gap: 8px; justify-content: flex-end; margin-top: 8px; }
    .error { color: #e06c75; }
</style>
