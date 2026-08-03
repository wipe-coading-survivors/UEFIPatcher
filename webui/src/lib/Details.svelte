<script lang="ts">
    import { selectedStore } from './stores';
    let node = $derived($selectedStore);
</script>

<div class="details">
    {#if node}
        <h3>Details</h3>
        <table>
            <tbody>
                <tr><td>Path</td><td>{node.path}</td></tr>
                <tr><td>Type</td><td>{node.type}</td></tr>
                <tr><td>Subtype</td><td>0x{node.subtype.toString(16)}</td></tr>
                <tr><td>GUID</td><td>{node.guid || '(none)'}</td></tr>
                <tr><td>Offset</td><td>0x{node.offset.toString(16)}</td></tr>
                <tr><td>Size</td><td>0x{node.size.toString(16)}</td></tr>
                <tr><td>Name</td><td>{node.name}</td></tr>
            </tbody>
        </table>
    {:else}
        <p>Select a node from the tree</p>
    {/if}
</div>

<style>
    .details { padding: 12px; }
    table { border-collapse: collapse; }
    td { padding: 4px 8px; border: 1px solid #444; }
    td:first-child { color: #888; font-weight: 500; }
</style>
