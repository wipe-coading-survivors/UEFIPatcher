<script lang="ts">
    import type { TreeNode } from './stores';
    import { selectedStore } from './stores';

    let { items, level = 0 }: { items: TreeNode[]; level?: number } = $props();

    function select(node: TreeNode) {
        selectedStore.set(node);
    }
</script>

<ul class="tree">
    {#each items as node (node.path)}
        <li class="tree-node" style="padding-left: {level * 20}px" onclick={() => select(node)}>
            <span class="icon"></span>
            <span class="name">{node.name || node.path}</span>
            <span class="guid">{node.guid}</span>
        </li>
    {/each}
</ul>

<style>
    .tree { list-style: none; padding: 0; margin: 0; }
    .tree-node { cursor: pointer; padding: 2px 4px; }
    .tree-node:hover { background: #333; }
    .icon { margin-right: 4px; }
    .name { font-weight: 500; }
    .guid { color: #666; font-size: 0.85em; margin-left: 8px; }
</style>
