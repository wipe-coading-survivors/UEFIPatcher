<script lang="ts">
    import Modal from './Modal.svelte';

    let {
        title,
        message,
        onconfirm,
        oncancel,
        ondone,
    }: {
        title: string;
        message: string;
        onconfirm: () => Promise<void>;
        oncancel: () => void;
        ondone: () => void;
    } = $props();
    let error = $state('');
    let busy = $state(false);

    async function submit() {
        busy = true;
        error = '';
        try {
            await onconfirm();
            ondone();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<Modal {title} onclose={oncancel}>
    <p>{message}</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <button onclick={submit} disabled={busy}>Confirm</button>
    <button onclick={oncancel}>Cancel</button>
</Modal>
