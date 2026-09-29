<script lang="ts">
    import type { TreeNode } from '../tree';
    let { node }: { node: TreeNode | null } = $props();
    const ACTION_NO = 50;
    const actionNames: Record<number, string> = {
        51: 'create', 52: 'insert', 53: 'replace', 54: 'remove', 55: 'rebuild', 56: 'rebase',
    };
</script>

{#if node}
    <h2>Node</h2>
    <dl>
        <dt>name</dt><dd>{node.name || '—'}</dd>
        <dt>path</dt><dd><code>{node.path}</code></dd>
        <dt>type</dt><dd>{node.type}/{node.subtype}</dd>
        <dt>guid</dt><dd><code>{node.guid || '—'}</code></dd>
        <dt>offset</dt><dd>{node.offset}</dd>
        <dt>size</dt><dd>{node.size}</dd>
        {#if node.region}<dt>region</dt><dd>{node.region}</dd>{/if}
        {#if node.isNvar}<dt>nvar</dt><dd>yes</dd>{/if}
        {#if node.action && node.action !== ACTION_NO}<dt>status</dt><dd class="pending">pending: {actionNames[node.action] ?? node.action}</dd>{/if}
    </dl>
{:else}
    <p>select a node</p>
{/if}

<style>
    dl { display: grid; grid-template-columns: auto 1fr; gap: 2px 12px; font-size: 13px; }
    dt { color: #9aa0aa; }
    .pending { color: #e0b040; }
</style>
