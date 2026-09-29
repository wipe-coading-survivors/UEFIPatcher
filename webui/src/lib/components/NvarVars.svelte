<script lang="ts">
    import { hexDump, decodeBase64, hexN, type VarRow } from '../nvar';
    let {
        vars,
        selected,
        onselect,
        onset,
        oncontext,
    }: {
        vars: VarRow[];
        selected: VarRow | null;
        onselect: (v: VarRow) => void;
        onset: (v: VarRow) => void;
        oncontext?: (v: VarRow, e: MouseEvent) => void;
    } = $props();
    const dump = $derived(selected ? hexDump(decodeBase64(selected.data)) : []);
</script>

{#if vars.length === 0}
    <p>no variables</p>
{:else}
    <table>
        <thead><tr><th>name</th><th>guid</th><th>offset</th><th>size</th><th>attrs</th><th>depth</th><th></th></tr></thead>
        <tbody>
            {#each vars as v (`${v.name}#${v.offset}`)}
                <tr class:selected={selected === v} oncontextmenu={(e) => { e.preventDefault(); oncontext?.(v, e); }}>
                    <td><button type="button" class="label" onclick={() => onselect(v)}>{v.name}</button></td>
                    <td><code>{v.guid}</code></td>
                    <td>{hexN(v.offset)}</td>
                    <td>{v.size}</td>
                    <td>{v.attributes}</td>
                    <td>{v.depth}</td>
                    <td>
                        <button type="button" class="act" aria-label="set {v.name}#{v.offset}" onclick={() => onset(v)}>
                            Set…
                        </button>
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>
{/if}
{#if selected}
    <h3>{selected.name} data</h3>
    <pre aria-label="var hex dump">{dump.join('\n')}</pre>
{/if}

<style>
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    tr.selected td { background: #262b33; }
    .label { background: none; border: none; color: #7aa7d9; font: inherit; cursor: pointer; }
    .label:hover { text-decoration: underline; }
    .act { background: none; border: none; color: #7aa7d9; font: inherit; font-size: 0.9em; cursor: pointer; }
    .act:hover { text-decoration: underline; }
    pre { background: #1b1f26; padding: 8px; overflow-x: auto; }
</style>
