<script lang="ts">
    import Tree from '$lib/components/Tree.svelte';
    import Inspector from '$lib/components/Inspector.svelte';
    import Modal from '$lib/components/Modal.svelte';
    import InsertDialog from '$lib/components/InsertDialog.svelte';
    import ReplaceDialog from '$lib/components/ReplaceDialog.svelte';
    import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
    import {
        downloadImage, extractNode, listNodes, rebuildNode, removeNode, saveImage, searchNodes,
    } from '$lib/api';
    import { appState } from '$lib/state.svelte';
    import { buildRows, buildTree, type TreeNode } from '$lib/tree';

    let { data }: { data: { imageId: string } } = $props();
    const imageId = $derived(data.imageId);

    let roots = $state<TreeNode[]>([]);
    let expanded = $state(new Set<string>(['']));
    let selected = $state<TreeNode | null>(null);
    let dialog:
        | { kind: 'insert'; target: string }
        | { kind: 'replace'; target: string }
        | { kind: 'remove'; target: string }
        | { kind: 'rebuild'; target: string }
        | null = $state(null);
    let query = $state('');
    let results = $state<{ path: string; name: string }[] | null>(null);
    let saveOpen = $state(false);
    let savePath = $state('');
    let notice = $state('');
    let error = $state('');
    let busy = $state(false);

    const rows = $derived(buildRows(roots, expanded));

    async function refresh() {
        error = '';
        try {
            const r = await listNodes(imageId);
            roots = buildTree(r.nodes ?? []);
            appState.pending = (r.nodes ?? []).some((n) => (n.action ?? 0) !== 0);
            if (selected && !r.nodes?.some((n) => n.path === selected?.path)) selected = null;
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        imageId;
        refresh();
    });

    function toggle(path: string) {
        const next = new Set(expanded);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        expanded = next;
    }

    async function onSearch() {
        if (!query) {
            results = null;
            return;
        }
        try {
            const r = await searchNodes(imageId, query, [0, 1, 2]);
            results = (r.nodes ?? []).map((n) => ({ path: n.path, name: n.name }));
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    function jumpTo(path: string) {
        const parts = path.split('/');
        const next = new Set(expanded);
        let acc = '';
        for (const p of parts) {
            next.add(acc);
            acc = acc === '' ? p : `${acc}/${p}`;
        }
        expanded = next;
        results = null;
        const hit = findNode(roots, path);
        if (hit) selected = hit;
    }

    function findNode(nodes: TreeNode[], path: string): TreeNode | null {
        for (const n of nodes) {
            if (n.path === path) return n;
            const child = findNode(n.children, path);
            if (child) return child;
        }
        return null;
    }

    async function onContext(node: TreeNode, action: string) {
        if (action === 'insert') dialog = { kind: 'insert', target: node.path };
        else if (action === 'replace') dialog = { kind: 'replace', target: node.path };
        else if (action === 'remove') dialog = { kind: 'remove', target: node.path };
        else if (action === 'rebuild') dialog = { kind: 'rebuild', target: node.path };
        else if (action === 'extract') {
            try {
                const r = await extractNode(imageId, node.path, false);
                notice = `extracted artifact ${r.artifactId}`;
            } catch (e) {
                error = e instanceof Error ? e.message : String(e);
            }
        }
    }

    function closeAndRefresh() {
        dialog = null;
        refresh();
    }

    async function onDownload() {
        busy = true;
        try {
            const blob = await downloadImage(imageId);
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = 'patched.bin';
            a.click();
            URL.revokeObjectURL(url);
        } finally {
            busy = false;
        }
    }

    async function onSave() {
        error = '';
        try {
            await saveImage(imageId, savePath);
            notice = `saved to ${savePath}`;
            saveOpen = false;
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
</script>

<h1>Image {imageId.slice(0, 8)}</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="notice" role="status">{notice}</p>{/if}

<div class="toolbar">
    <input placeholder="search nodes" bind:value={query} />
    <button onclick={() => onSearch()}>Search</button>
    <button onclick={onDownload} disabled={busy}>Download</button>
    <button onclick={() => (saveOpen = true)}>Save…</button>
</div>

{#if results}
    <ul data-testid="search-results">
        {#each results as r (r.path)}
            <li><button onclick={() => jumpTo(r.path)}>{r.path} {r.name}</button></li>
        {/each}
    </ul>
{/if}

<div class="workbench">
    <div class="treepane">
        <Tree {rows} selectedPath={selected?.path ?? null} onselect={(n) => (selected = n)} oncontext={onContext} ontoggle={toggle} />
    </div>
    <aside>
        <Inspector node={selected} />
    </aside>
</div>

{#if dialog?.kind === 'insert'}
    <InsertDialog {imageId} target={dialog.target} ondone={closeAndRefresh} />
{:else if dialog?.kind === 'replace'}
    <ReplaceDialog {imageId} target={dialog.target} ondone={closeAndRefresh} />
{:else if dialog?.kind === 'remove'}
    <ConfirmDialog
        title="Remove node"
        message="remove {dialog.target}?"
        onconfirm={async () => {
            await removeNode(imageId, dialog!.target);
        }}
        oncancel={() => (dialog = null)}
        ondone={closeAndRefresh}
    />
{:else if dialog?.kind === 'rebuild'}
    <ConfirmDialog
        title="Rebuild node"
        message="rebuild {dialog.target}?"
        onconfirm={async () => {
            await rebuildNode(imageId, dialog!.target);
        }}
        oncancel={() => (dialog = null)}
        ondone={closeAndRefresh}
    />
{/if}

{#if saveOpen}
    <Modal title="Save image" onclose={() => (saveOpen = false)}>
        <label for="save-path">server output path</label>
        <input id="save-path" type="text" bind:value={savePath} placeholder="/tmp/patched.bin" />
        <button onclick={onSave} disabled={!savePath}>Save</button>
    </Modal>
{/if}
