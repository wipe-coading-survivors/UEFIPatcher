<script lang="ts">
    let { title, onclose, children }: { title: string; onclose: () => void; children?: import('svelte').Snippet } = $props();
</script>

<div class="backdrop" onclick={onclose} role="presentation">
    <div class="modal" role="dialog" aria-label={title} tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
        <header><h2>{title}</h2><button aria-label="close" onclick={onclose}>×</button></header>
        {@render children?.()}
    </div>
</div>

<style>
    .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; }
    .modal { background: #1d2027; border: 1px solid #3a3f49; border-radius: 8px; min-width: 420px; padding: 16px; }
    header { display: flex; justify-content: space-between; align-items: center; }
</style>
