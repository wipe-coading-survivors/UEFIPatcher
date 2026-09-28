<script lang="ts">
    import { appState } from '$lib/state.svelte';
</script>

<h1>Forms</h1>
{#if appState.imageId}
    <p><a href="/forms/{appState.imageId}">open forms for current image</a></p>
{:else}
    <p>no image open — <a href="/image">open one in Image</a></p>
{/if}
