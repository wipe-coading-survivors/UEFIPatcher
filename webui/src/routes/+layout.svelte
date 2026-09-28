<script lang="ts">
    import { appState } from '$lib/state.svelte';
    import { createSession } from '$lib/api';
    import { onMount } from 'svelte';
    let { children } = $props();
    onMount(async () => {
        if (!appState.sessionId) {
            try {
                appState.sessionId = await createSession();
            } catch {
                appState.sessionId = null;
            }
        }
    });
</script>

{@render children?.()}
