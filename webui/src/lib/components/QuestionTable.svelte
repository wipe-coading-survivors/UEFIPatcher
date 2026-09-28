<script lang="ts">
    import type { QuestionSummary } from '../proto/engine';
    let {
        questions,
        onsetvalue,
        ongates,
    }: {
        questions: QuestionSummary[];
        onsetvalue: (q: QuestionSummary) => void;
        ongates: (q: QuestionSummary) => void;
    } = $props();

    const hex = (qid: number) => `0x${qid.toString(16).toUpperCase()}`;
</script>

{#if questions.length === 0}
    <p>no questions</p>
{:else}
    <table>
        <thead>
            <tr><th>qid</th><th>kind</th><th>prompt</th><th>store</th><th>width</th><th>seed</th><th></th></tr>
        </thead>
        <tbody>
            {#each questions as q (q.questionId)}
                <tr>
                    <td><code>{hex(q.questionId)}</code></td>
                    <td>{q.kind}</td>
                    <td>{q.prompt}</td>
                    <td>{q.varStoreId}@0x{q.varOffset.toString(16).toUpperCase()}</td>
                    <td>{q.width}</td>
                    <td>{q.seedValue ?? q.ifrDefault ?? ''}</td>
                    <td>
                        <button type="button" class="act" aria-label="set {hex(q.questionId)}" onclick={() => onsetvalue(q)}>
                            Set…
                        </button>
                        <button type="button" class="act" aria-label="gates {hex(q.questionId)}" onclick={() => ongates(q)}>
                            Gates
                        </button>
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>
{/if}

<style>
    table { border-collapse: collapse; width: 100%; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    .act { background: none; border: none; color: #7aa7d9; font: inherit; font-size: 0.9em; cursor: pointer; }
    .act:hover { text-decoration: underline; }
</style>
