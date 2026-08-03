<script lang="ts">
    import { page } from '$app/stores';
    import { listItems, setVisibility, addFormSet } from '$lib/api';
    import { imageStore } from '$lib/stores';
    import type { TreeNode } from '$lib/stores';

    let setupItems = $state<TreeNode[]>([]);
    let schemaText = $state('');
    let status = $state('');

    async function loadSetup() {
        if (!$imageStore) return;
        const r = await listItems($imageStore);
        setupItems = r.items;
    }

    async function toggleVisibility(item: TreeNode, visible: boolean) {
        if (!$imageStore) return;
        await setVisibility($imageStore, item.path, visible);
        status = `visibility set for ${item.path}`;
    }

    async function doAddFormSet() {
        if (!$imageStore) return;
        await addFormSet($imageStore, schemaText, '');
        status = 'formset added';
    }

    $effect(() => { loadSetup(); });
</script>

<div class="setup-page">
    <h2>Setup</h2>
    <section>
        <h3>Visibility</h3>
        {#each setupItems as item (item.path)}
            <div class="setup-item">
                <span>{item.name || item.path}</span>
                <button onclick={() => toggleVisibility(item, true)}>Show</button>
                <button onclick={() => toggleVisibility(item, false)}>Hide</button>
            </div>
        {/each}
    </section>
    <section>
        <h3>Add FormSet (JSON schema)</h3>
        <textarea bind:value={schemaText} rows="15" cols="60" placeholder={'{"formset_guid":"...","title":"...","forms":[...]}'}></textarea>
        <br />
        <button onclick={doAddFormSet}>Add FormSet</button>
    </section>
    {#if status}<p class="status">{status}</p>{/if}
</div>

<style>
    .setup-page { padding: 12px; }
    .setup-item { display: flex; align-items: center; gap: 8px; padding: 4px 0; }
    textarea { font-family: monospace; }
    .status { color: green; }
</style>
