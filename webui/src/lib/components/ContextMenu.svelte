<script lang="ts">
    let {
        x,
        y,
        items,
        onpick,
        onclose,
    }: {
        x: number;
        y: number;
        items: { id: string; label: string; disabled?: boolean }[];
        onpick: (id: string) => void;
        onclose: () => void;
    } = $props();

    const left = $derived(Math.min(x, window.innerWidth - 170));
    const top = $derived(Math.min(y, window.innerHeight - items.length * 30 - 12));
</script>

<svelte:window
    onclick={() => onclose()}
    onkeydown={(e) => e.key === 'Escape' && onclose()}
/>

<div class="ctxmenu" role="menu" style="left: {left}px; top: {top}px">
    {#each items as a (a.id)}
        <button type="button" role="menuitem" disabled={a.disabled} onclick={() => onpick(a.id)}>
            {a.label}
        </button>
    {/each}
</div>

<style>
    .ctxmenu {
        position: fixed;
        background: #23262e;
        border: 1px solid #3a3f49;
        border-radius: 6px;
        display: flex;
        flex-direction: column;
        padding: 4px;
        z-index: 10;
        min-width: 160px;
    }
    .ctxmenu button {
        text-align: left;
        background: none;
        border: none;
        color: inherit;
        font: inherit;
        padding: 4px 10px;
        cursor: pointer;
        border-radius: 4px;
    }
    .ctxmenu button:hover:not([disabled]) { background: #2c5d8f; }
    .ctxmenu button[disabled] { color: #6b7280; cursor: default; }
</style>
