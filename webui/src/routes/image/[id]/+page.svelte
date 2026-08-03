<script lang="ts">
    import { page } from '$app/stores';
    import { dumpTree, listItems, insert, remove, rebuild, saveImage, downloadImage } from '$lib/api';
    import { treeStore, selectedStore, type TreeNode } from '$lib/stores';
    import Tree from '$lib/Tree.svelte';
    import Details from '$lib/Details.svelte';

    let imageId = $derived($page.params.id);
    let items = $state<TreeNode[]>([]);
    let target = $state('');
    let ffsPath = $state('');
    let status = $state('');

    async function load() {
        const r = await listItems(imageId);
        items = r.items;
        treeStore.set(r.items);
    }

    async function doInsert() {
        await insert(imageId, target, ffsPath, 'into');
        status = 'inserted';
        await load();
    }
    async function doRemove() {
        await remove(imageId, target);
        status = 'removed';
        await load();
    }
    async function doRebuild() {
        await rebuild(imageId, target);
        status = 'rebuilt';
    }
    async function doSave() {
        const out = `/tmp/patched-${Date.now()}.bin`;
        await saveImage(imageId, out);
        status = `saved to ${out}`;
    }
    async function doDownload() {
        const blob = await downloadImage(imageId);
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url; a.download = 'patched.bin'; a.click();
        URL.revokeObjectURL(url);
    }

    $effect(() => { load(); });
</script>

<div class="image-page">
    <div class="tree-panel">
        <h2>Tree</h2>
        <Tree items={items} />
    </div>
    <div class="right-panel">
        <Details />
        <div class="ops">
            <h3>Operations</h3>
            <input bind:value={target} placeholder="target (GUID/path)" />
            <input bind:value={ffsPath} placeholder="ffs path (for insert)" />
            <button onclick={doInsert}>Insert</button>
            <button onclick={doRemove}>Remove</button>
            <button onclick={doRebuild}>Rebuild</button>
            <hr />
            <button onclick={doSave}>Save</button>
            <button onclick={doDownload}>Download</button>
            {#if status}<p class="status">{status}</p>{/if}
        </div>
    </div>
</div>

<style>
    .image-page { display: flex; gap: 12px; height: calc(100vh - 60px); }
    .tree-panel { width: 40%; overflow-y: auto; border-right: 1px solid #444; padding: 8px; }
    .right-panel { flex: 1; padding: 8px; }
    .ops { margin-top: 16px; }
    input { display: block; margin: 4px 0; width: 100%; }
    button { margin: 4px 4px 4px 0; padding: 4px 12px; }
    .status { color: green; }
</style>
