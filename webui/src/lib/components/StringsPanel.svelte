<script lang="ts">
    import { onMount } from 'svelte';
    import { listStrings } from '$lib/api';
    import type { StringInfo } from '$lib/proto/engine';

    let { imageId }: { imageId: string } = $props();

    let strings: StringInfo[] = $state([]);
    let filter = $state('');
    let error = $state('');

    onMount(() => {
        listStrings(imageId)
            .then((r) => {
                strings = r.strings ?? [];
            })
            .catch((e: unknown) => {
                error = e instanceof Error ? e.message : String(e);
            });
    });

    const filtered = $derived.by(() => {
        const f = filter.trim().toLowerCase();
        if (!f) return strings;
        return strings.filter(
            (s) =>
                s.text.toLowerCase().includes(f) ||
                s.language.toLowerCase().includes(f) ||
                String(s.stringId).includes(f),
        );
    });
</script>

{#if error}<p class="error" role="alert">{error}</p>{/if}
<p>
    <input aria-label="filter strings" type="text" placeholder="filter: text / lang / id" bind:value={filter} />
    <span class="meta">{filtered.length} of {strings.length}</span>
</p>
<table>
    <thead><tr><th>lang</th><th>id</th><th>text</th><th>source</th></tr></thead>
    <tbody>
        {#each filtered as s (`${s.language}:${s.stringId}`)}
            <tr>
                <td>{s.language}</td>
                <td>{s.stringId}</td>
                <td>{s.text}</td>
                <td class="meta">{s.source}</td>
            </tr>
        {/each}
    </tbody>
</table>

<style>
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    input { background: #23262e; color: inherit; border: 1px solid #3a3f49; border-radius: 4px; padding: 4px; }
    .meta { color: #6b7280; font-size: 0.85em; }
    .error { color: #e06c75; }
</style>
