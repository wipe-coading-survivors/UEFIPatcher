<script lang="ts">
    import type { FormInfo } from '../proto/engine';
    import type { FormRow } from '../forms';
    let {
        rows,
        selectedKey,
        onselect,
        ongates,
        onshow,
        ontoggle,
        oncontext,
    }: {
        rows: FormRow[];
        selectedKey: string | null;
        onselect: (form: FormInfo) => void;
        ongates: (form: FormInfo) => void;
        onshow: (form: FormInfo) => void;
        ontoggle: (key: string) => void;
        oncontext?: (form: FormInfo, e: MouseEvent) => void;
    } = $props();
</script>

<ul role="tree" data-testid="formstree">
    {#each rows as row (row.key)}
        {#if row.kind === 'formset'}
            <li role="treeitem" aria-selected={selectedKey === row.key} style="padding-left: {row.depth * 16}px">
                <button
                    type="button"
                    aria-label="expand {row.formsetGuid}"
                    aria-expanded={row.expanded}
                    onclick={() => ontoggle(row.formsetGuid)}
                >
                    {row.expanded ? '▾' : '▸'}
                </button>
                <span class="fslabel">{row.formsetGuid}</span>
            </li>
        {:else if row.form}
            <li
                role="treeitem"
                aria-selected={selectedKey === row.key}
                style="padding-left: {row.depth * 16}px"
            >
                {#if row.hasChildren}
                    <button aria-label="expand {row.key}" onclick={() => ontoggle(row.key)}>▸</button>
                {/if}
                <button
                    type="button"
                    class="label"
                    onclick={() => onselect(row.form!)}
                    oncontextmenu={(e) => {
                        e.preventDefault();
                        oncontext?.(row.form!, e);
                    }}
                >
                    {row.form.formIdIfr} {row.form.title}
                </button>
                {#if !row.form.visible}
                    <span class="dim" title="hidden">hidden</span>
                    <button type="button" class="act" aria-label="show {row.key}" onclick={() => onshow(row.form!)}>
                        Show
                    </button>
                {/if}
                <button type="button" class="act" aria-label="gates {row.key}" onclick={() => ongates(row.form!)}>
                    Gates
                </button>
            </li>
        {/if}
    {/each}
</ul>

<style>
    ul { list-style: none; padding: 0; margin: 0; }
    li { display: flex; align-items: center; gap: 4px; padding: 1px 0; }
    li[aria-selected='true'] .label { background: #2c5d8f; border-radius: 4px; }
    .fslabel { font-weight: 600; }
    .label { cursor: pointer; padding: 1px 6px; background: none; border: none; color: inherit; font: inherit; text-align: left; }
    .label:hover, .act:hover { text-decoration: underline; }
    .act { background: none; border: none; color: #7aa7d9; font: inherit; font-size: 0.9em; cursor: pointer; padding: 0 2px; }
    .dim { color: #6b7280; font-size: 0.85em; }
</style>
