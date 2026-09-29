<script lang="ts">
    import './app.css';
    import { appState } from '$lib/state.svelte';
    import { createSession } from '$lib/api';
    import { onMount } from 'svelte';
    let { children } = $props();
    const sections = [
        { id: 'image', label: 'Image' },
        { id: 'forms', label: 'Forms' },
        { id: 'nvar', label: 'NVRAM' },
        { id: 'snapshots', label: 'Snapshots' },
        { id: 'artifacts', label: 'Artifacts' },
    ];
    const formsHref = $derived(appState.imageId ? `/forms/${appState.imageId}` : '/forms');
    const href = (id: string) =>
        id === 'image' || id === 'artifacts' || !appState.imageId
            ? `/${id}`
            : `/${id}/${appState.imageId}`;
    onMount(async () => {
        if (!appState.sessionId) {
            try {
                appState.sessionId = await createSession();
            } catch {
                appState.sessionId = null;
            }
        }
    });
    $effect(() => {
        appState.imageId;
        appState.snapshotCount = 0;
    });
</script>

<div class="shell">
    <nav aria-label="sections">
        {#each sections as s (s.id)}
            {#if s.id === 'forms'}
                <a href={formsHref}>{s.label}</a>
            {:else}
                <a href={href(s.id)}>{s.label}</a>
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
            {#if appState.snapshotCount > 0}
                <span id="status-snapshots" title="image snapshots">▣ {appState.snapshotCount}</span>
            {/if}
        </header>
        <svelte:boundary>
            <main>{@render children?.()}</main>
            {#snippet failed(message, reset)}
                <p class="error" role="alert">section crashed: {(message as Error).message}</p>
                <button onclick={reset}>Reset section</button>
            {/snippet}
        </svelte:boundary>
    </div>
</div>
