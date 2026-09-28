<script lang="ts">
    import './app.css';
    import { appState } from '$lib/state.svelte';
    import { createSession } from '$lib/api';
    import { onMount } from 'svelte';
    let { children } = $props();
    const sections = [
        { id: 'image', label: 'Image', ready: true },
        { id: 'forms', label: 'Forms', ready: false },
        { id: 'nvar', label: 'NVRAM', ready: false },
        { id: 'snapshots', label: 'Snapshots', ready: false },
        { id: 'artifacts', label: 'Artifacts', ready: false },
    ];
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

<div class="shell">
    <nav aria-label="sections">
        {#each sections as s}
            {#if s.ready}
                <a href="/image">{s.label}</a>
            {:else}
                <span aria-disabled="true" title="not in W1">{s.label}</span>
            {/if}
        {/each}
    </nav>
    <div class="main">
        <header>
            <span id="status-session">
                {appState.sessionId ? `session ${appState.sessionId.slice(0, 8)}` : 'no session'}
            </span>
            <span id="status-image">{appState.imageId ?? 'no image'}</span>
            {#if appState.pending}<span id="status-pending" title="pending actions">●</span>{/if}
        </header>
        <main>{@render children?.()}</main>
    </div>
</div>
