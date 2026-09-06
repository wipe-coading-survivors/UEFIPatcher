<script lang="ts">
    import { createSession, openImage, uploadImage } from '$lib/api';
    import { sessionStore, imageStore } from '$lib/stores';
    import { goto } from '$app/navigation';

    let filePath = $state('');
    let mode = $state('read');
    let uploading = $state(false);

    async function handleUpload(e: Event) {
        const input = e.target as HTMLInputElement;
        if (!input.files?.[0]) return;
        uploading = true;
        try {
            const path = await uploadImage(input.files[0]);
            await openImage(path, 'write');
            goto(`/image/${$imageStore}`);
        } finally {
            uploading = false;
        }
    }
</script>

<div>
    {#if !$sessionStore}
        <button onclick={createSession}>Create Session</button>
    {:else}
        <h2>Open Image</h2>
        <input type="file" onchange={handleUpload} disabled={uploading} />
        {#if uploading}<p>Uploading...</p>{/if}
        <hr />
        <input bind:value={filePath} placeholder="/path/to/bios.bin" />
        <select bind:value={mode}><option value="read">read</option><option value="write">write</option></select>
        <button onclick={async () => { await openImage(filePath, mode); goto(`/image/${$imageStore}`); }}>Open</button>
    {/if}
</div>
