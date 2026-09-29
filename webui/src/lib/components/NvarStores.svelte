<script lang="ts">
    import { hexN } from '../nvar';
    let {
        stores,
        selected,
        onselect,
    }: {
        stores: { path: string; desc: string; records: number; vars: number; freeTail: number }[];
        selected: string | null;
        onselect: (path: string) => void;
    } = $props();
</script>

{#if stores.length === 0}
    <p>no NVRAM stores</p>
{:else}
    <table>
        <thead><tr><th>path</th><th>desc</th><th>vars</th><th>records</th><th>free</th></tr></thead>
        <tbody>
            {#each stores as s (s.path)}
                <tr class:selected={s.path === selected}>
                    <td><button type="button" aria-label="store {s.path}" onclick={() => onselect(s.path)}>{s.path}</button></td>
                    <td>{s.desc}</td>
                    <td>{s.vars}</td>
                    <td>{s.records}</td>
                    <td>{hexN(s.freeTail)}B</td>
                </tr>
            {/each}
        </tbody>
    </table>
{/if}

<style>
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    tr.selected td { background: #262b33; }
    button { background: none; border: none; color: #7aa7d9; font: inherit; cursor: pointer; }
    button:hover { text-decoration: underline; }
</style>
