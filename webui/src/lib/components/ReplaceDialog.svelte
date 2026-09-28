<script lang="ts">
    import Modal from './Modal.svelte';
    import { replaceNode, uploadArtifact } from '$lib/api';

    let { imageId, target, ondone }: { imageId: string; target: string; ondone: () => void } = $props();
    let file = $state<File | null>(null);
    let bodyOnly = $state(false);
    let error = $state('');
    let busy = $state(false);

    async function submit() {
        busy = true;
        error = '';
        try {
            const artifactId = (await uploadArtifact(file!)).artifactId;
            await replaceNode(imageId, target, { artifactId }, bodyOnly);
            ondone();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<Modal title="Replace {target}" onclose={ondone}>
    <label for="rep-file">body file</label>
    <input id="rep-file" type="file" onchange={(e) => (file = e.currentTarget.files?.[0] ?? null)} />
    <label><input type="checkbox" bind:checked={bodyOnly} /> body only</label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button onclick={submit} disabled={!file || busy}>Replace</button>
</Modal>
