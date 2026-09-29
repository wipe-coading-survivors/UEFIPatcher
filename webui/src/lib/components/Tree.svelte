<script lang="ts">
    import { nodeLabel, type TreeNode } from '../tree';
    import { storeIcon } from '../icons';
    import ContextMenu from './ContextMenu.svelte';
    let {
        rows,
        selectedPath,
        onselect,
        oncontext,
        ontoggle,
    }: {
        rows: { node: TreeNode; depth: number; hasChildren: boolean }[];
        selectedPath: string | null;
        onselect: (node: TreeNode) => void;
        oncontext: (node: TreeNode, action: string) => void;
        ontoggle: (path: string) => void;
    } = $props();

    let menuFor: TreeNode | null = $state(null);
    let menuX = $state(0);
    let menuY = $state(0);

    function openMenu(node: TreeNode, e: MouseEvent) {
        menuFor = node;
        menuX = Math.min(e.clientX, window.innerWidth - 140);
        menuY = Math.min(e.clientY, window.innerHeight - 180);
    }

    const menuActions = [
        { id: 'insert', label: 'Insert…' },
        { id: 'replace', label: 'Replace…' },
        { id: 'extract', label: 'Extract…' },
        { id: 'rebuild', label: 'Rebuild' },
        { id: 'remove', label: 'Remove…' },
    ];
</script>

<ul role="tree" data-testid="tree">
    {#each rows as row (row.node.path)}
        <li
            role="treeitem"
            aria-selected={selectedPath === row.node.path}
            style="padding-left: {row.depth * 16}px"
        >
            {#if row.hasChildren}
                <button aria-label="expand {row.node.path}" onclick={() => ontoggle(row.node.path)}>
                    ▸
                </button>
            {/if}
            <button
                type="button"
                class="label"
                onclick={() => onselect(row.node)}
                oncontextmenu={(e) => {
                    e.preventDefault();
                    openMenu(row.node, e);
                }}
            >
                <span class="nf" aria-hidden="true">{storeIcon(row.node.isNvar, row.node.type, row.node.subtype)}</span>
                {nodeLabel(row.node)}
            </button>
        </li>
    {/each}
</ul>

{#if menuFor}
    <ContextMenu
        x={menuX}
        y={menuY}
        items={menuActions}
        onpick={(id) => {
            if (menuFor) oncontext(menuFor, id);
        }}
        onclose={() => (menuFor = null)}
    />
{/if}

<style>
    ul { list-style: none; padding: 0; margin: 0; }
    li { display: flex; align-items: center; gap: 4px; padding: 1px 0; }
    li[aria-selected='true'] .label { background: #2c5d8f; border-radius: 4px; }
    .label { cursor: pointer; padding: 1px 6px; background: none; border: none; color: inherit; font: inherit; text-align: left; }
</style>
