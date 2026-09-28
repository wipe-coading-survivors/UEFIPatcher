<script lang="ts">
    import { nodeLabel, type TreeNode } from '../tree';
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
                    menuFor = row.node;
                }}
            >
                {nodeLabel(row.node)}
            </button>
        </li>
    {/each}
</ul>

{#if menuFor}
    <div class="ctxmenu" role="menu">
        {#each menuActions as a (a.id)}
            <button role="menuitem" onclick={() => { oncontext(menuFor!, a.id); menuFor = null; }}>
                {a.label}
            </button>
        {/each}
    </div>
{/if}

<style>
    ul { list-style: none; padding: 0; margin: 0; }
    li { display: flex; align-items: center; gap: 4px; padding: 1px 0; }
    li[aria-selected='true'] .label { background: #2c5d8f; border-radius: 4px; }
    .label { cursor: pointer; padding: 1px 6px; background: none; border: none; color: inherit; font: inherit; text-align: left; }
    .ctxmenu { position: fixed; background: #23262e; border: 1px solid #3a3f49; border-radius: 6px; display: flex; flex-direction: column; padding: 4px; z-index: 10; }
</style>
