<script lang="ts">
    import Modal from './Modal.svelte';
    import { InsertMode } from '$lib/proto/engine';
    import { insertNode, uploadArtifact } from '$lib/api';

    let { imageId, target, ondone }: { imageId: string; target: string; ondone: () => void } = $props();
    let mode = $state(InsertMode.INTO);
    let file = $state<File | null>(null);
    let error = $state('');

    async function submit() {
        error = '';
        try {
            const artifactId = (await uploadArtifact(file!)).artifactId;
            await insertNode(imageId, target, mode, { artifactId });
            ondone();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
</script>

<Modal title="Insert into {target}" onclose={ondone}>
    <label for="ins-mode">mode</label>
    <select id="ins-mode" bind:value={mode}>
        <option value={InsertMode.INTO}>into</option>
        <option value={InsertMode.BEFORE}>before</option>
        <option value={InsertMode.AFTER}>after</option>
    </select>
    <label for="ins-file">FFS file</label>
    <input id="ins-file" type="file" onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)} />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button onclick={submit} disabled={!file}>Insert</button>
</Modal>
