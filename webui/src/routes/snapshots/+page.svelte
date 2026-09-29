<script lang="ts">
    import { appState } from '$lib/state.svelte';
</script>

<h1>Snapshots</h1>
{#if appState.imageId}
    <p><a href="/snapshots/{appState.imageId}">open snapshots for current image</a></p>
{:else}
    <p>no image open — <a href="/image">open one in Image</a></p>
{/if}
