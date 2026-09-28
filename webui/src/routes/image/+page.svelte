<script lang="ts">
    import { goto } from '$app/navigation';
    import { onMount } from 'svelte';
    import { appState } from '$lib/state.svelte';
    import { imageClose, imageOpen, imagesList, uploadImage } from '$lib/api';

    let file: File | null = null;
    let mode: 'read' | 'write' = 'write';
    let path = '';
    let openMode: 'read' | 'write' = 'write';
    let images: { imageId: string; name: string }[] = [];
    let error = '';

    async function refresh() {
        if (!appState.sessionId) return;
        try {
            const r = await imagesList(appState.sessionId);
            images = r.images ?? [];
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    onMount(refresh);

    async function onUpload() {
        if (!file) return;
        error = '';
        try {
            const r = await uploadImage(file, mode);
            appState.imageId = r.imageId;
            await goto(`/image/${r.imageId}`);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    async function onOpen() {
        if (!appState.sessionId || !path) return;
        error = '';
        try {
            const r = await imageOpen(appState.sessionId, path, openMode === 'write' ? 1 : 0);
            appState.imageId = r.imageId;
            await goto(`/image/${r.imageId}`);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    async function onSwitch(id: string) {
        appState.imageId = id;
        await goto(`/image/${id}`);
    }

    async function onClose(id: string) {
        try {
            await imageClose(id);
            if (appState.imageId === id) appState.imageId = null;
            await refresh();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
</script>

<h1>Image</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}

<section aria-label="upload">
    <h2>Upload</h2>
    <label for="file">BIOS image</label>
    <input id="file" type="file" onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)} />
    <select aria-label="mode" bind:value={mode}>
        <option value="write">write</option>
        <option value="read">read</option>
    </select>
    <button onclick={onUpload} disabled={!file}>Upload</button>
</section>

<section aria-label="open">
    <h2>Open by server path</h2>
    <input type="text" placeholder="/path/to/image.bin" bind:value={path} />
    <select aria-label="open mode" bind:value={openMode}>
        <option value="write">write</option>
        <option value="read">read</option>
    </select>
    <button onclick={onOpen} disabled={!path}>Open</button>
</section>

<section aria-label="open images">
    <h2>Open images</h2>
    {#if images.length === 0}
        <p>no open images</p>
    {:else}
        <table>
            <thead><tr><th>image</th><th>name</th><th></th></tr></thead>
            <tbody>
                {#each images as img (img.imageId)}
                    <tr>
                        <td><code>{img.imageId.slice(0, 8)}</code></td>
                        <td>{img.name}</td>
                        <td>
                            <button onclick={() => onSwitch(img.imageId)}>Open</button>
                            <button onclick={() => onClose(img.imageId)}>Close</button>
                        </td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}
</section>
