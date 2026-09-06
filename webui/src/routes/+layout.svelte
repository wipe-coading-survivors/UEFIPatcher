<script lang="ts">
    import { sessionStore } from '$lib/stores';
    let { children } = $props();
</script>

<header>
    <h1>UEFIPatcher</h1>
    {#if $sessionStore}
        <span>Session: {$sessionStore.slice(0, 8)}...</span>
    {/if}
</header>
<main>{@render children()}</main>
