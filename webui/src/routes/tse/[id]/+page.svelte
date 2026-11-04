<script lang="ts">
    import { tseReport, tseUnhide, type TseReportResponse } from '$lib/api';

    let { data }: { data: { imageId: string } } = $props();
    const imageId = $derived(data.imageId);

    let report: TseReportResponse | null = $state(null);
    let error = $state('');
    let notice = $state('');
    let loadSeq = 0;

    let unhideGuid = $state('');
    let unhideFid = $state('');
    let unhideOffset = $state('');
    let armed = $state(false);
    let busy = $state(false);

    const pending = $derived(!report && !error);

    async function refresh() {
        const seq = ++loadSeq;
        error = '';
        try {
            const r = await tseReport(imageId);
            if (seq !== loadSeq) return;
            report = r;
        } catch (e) {
            if (seq !== loadSeq) return;
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        imageId;
        refresh();
    });

    function prefill(guid: string, fid: string) {
        unhideGuid = guid;
        unhideFid = fid;
        armed = false;
    }

    function disarm() {
        armed = false;
    }

    async function submit() {
        error = '';
        notice = '';
        const guid = unhideGuid.trim();
        if (!guid) {
            armed = false;
            error = 'formset guid required';
            return;
        }
        if (!/^\d+$/.test(unhideFid.trim())) {
            armed = false;
            error = 'form id must be a number';
            return;
        }
        let blockPeOffset: number | undefined;
        const offRaw = unhideOffset.trim();
        if (offRaw) {
            const hex = offRaw.startsWith('0x') ? offRaw.slice(2) : offRaw;
            if (!/^[0-9a-f]+$/i.test(hex)) {
                armed = false;
                error = 'block offset must be hex';
                return;
            }
            const off = parseInt(hex, 16);
            if (off > 0xffffffff) {
                armed = false;
                error = 'block offset exceeds u32';
                return;
            }
            blockPeOffset = off;
        }
        if (!armed) {
            armed = true;
            return;
        }
        busy = true;
        try {
            const r = await tseUnhide(imageId, guid, parseInt(unhideFid.trim(), 10), blockPeOffset);
            armed = false;
            notice = `unhidden: entry @ 0x${r.entryPeOffset.toString(16)}`;
            await refresh();
        } catch (e) {
            armed = false;
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<h1>TSE</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if notice}<p class="notice" role="status">{notice}</p>{/if}
{#if pending}
    <p>loading…</p>
{:else if report}
    <section aria-label="tse strides">
        <table>
            <thead><tr><th>block</th><th>entry</th></tr></thead>
            <tbody>
                {#each report.blocks as block (block.peOffset)}
                    {#each block.entries as entry, i (`${block.peOffset}:${i}`)}
                        <tr>
                            <td>@{block.peOffset.toString(16)}</td>
                            <td><button onclick={() => prefill(entry.formsetGuid, entry.formId)}>{entry.formsetGuid} #{entry.formId}</button></td>
                        </tr>
                    {/each}
                {/each}
            </tbody>
        </table>
    </section>
    {#if report.spf}
        <section aria-label="tse spf">
            <h2>$SPF</h2>
            <p>pages: {report.spf.pageCount} · string controls: {report.spf.stringControls} · peLen: 0x{report.peLen.toString(16)}</p>
            <ul>
                {#each report.spf.formsets as fs (fs.guid)}
                    <li>formset {fs.guid} raw={fs.rawU32} pages={fs.pages}</li>
                {/each}
            </ul>
            <ul>
                {#each report.spf.vars as v (`${v.guid}:${v.name}`)}
                    <li>var {v.guid} {v.name} attrs=#{v.attrs.toString(16)} size={v.size}</li>
                {/each}
            </ul>
        </section>
    {:else}
        <p>no $SPF</p>
    {/if}
{/if}

<section aria-label="tse unhide">
    <h2>Unhide</h2>
    <form onsubmit={(e) => { e.preventDefault(); submit(); }}>
        <label>formset-guid <input bind:value={unhideGuid} oninput={disarm} /></label>
        <label>form-id <input bind:value={unhideFid} oninput={disarm} /></label>
        <details>
            <summary>expert</summary>
            <label>block-offset <input placeholder="hex, e.g. 1b40" bind:value={unhideOffset} oninput={disarm} /></label>
        </details>
        <button type="submit" disabled={busy}>{armed ? 'Unhide — confirm' : 'Unhide'}</button>
    </form>
</section>

<style>
    .error { color: #e06c75; }
</style>
