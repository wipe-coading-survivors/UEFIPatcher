<script lang="ts">
    import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
    import { appState } from '$lib/state.svelte';
    import { downloadImage, snapshotCreate, snapshotRestore, snapshotsList } from '$lib/api';
    import type { ImageSnapshotInfo } from '$lib/proto/engine';

    let { data }: { data: { imageId: string } } = $props();
    const imageId = $derived(data.imageId);

    let snapshots: ImageSnapshotInfo[] = $state([]);
    let name = $state('');
    let restoreId: string | null = $state(null);
    let error = $state('');
    let busy = $state(false);
    let loadSeq = 0;

    async function refresh() {
        const seq = ++loadSeq;
        error = '';
        try {
            const r = await snapshotsList(imageId);
            if (seq !== loadSeq) return;
            const seen = new Set<string>();
            snapshots = [];
            for (const s of r.snapshots ?? []) {
                if (seen.has(s.snapshotId)) continue;
                seen.add(s.snapshotId);
                snapshots.push(s);
            }
            appState.snapshotCount = snapshots.length;
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        imageId;
        refresh();
    });

    async function create() {
        error = '';
        busy = true;
        try {
            await snapshotCreate(imageId, name.trim());
            name = '';
            await refresh();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }

    async function doRestore() {
        if (!restoreId) return;
        await snapshotRestore(imageId, restoreId);
    }

    const fmtDate = (s: string) => new Date(Number(s) * 1000).toLocaleString();
    const fmtSize = (s: string) => `${(Number(s) / 1024 / 1024).toFixed(1)} MB`;

    async function onDownload() {
        busy = true;
        try {
            const blob = await downloadImage(imageId);
            const a = document.createElement('a');
            a.href = URL.createObjectURL(blob);
            a.download = 'patched.bin';
            a.click();
            URL.revokeObjectURL(a.href);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }
</script>

<h1>Snapshots</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if restoreId}
    <ConfirmDialog
        title="Restore snapshot"
        message="Restore the image to snapshot {restoreId.slice(0, 8)}? Unsaved changes after the snapshot are lost."
        onconfirm={doRestore}
        oncancel={() => (restoreId = null)}
        ondone={() => {
            restoreId = null;
            refresh();
        }}
    />
{/if}

<form onsubmit={(e) => { e.preventDefault(); create(); }}>
    <label>snapshot name <input id="snap-name" bind:value={name} /></label>
    <button type="submit" disabled={busy || !name.trim()}>Create</button>
</form>

{#if snapshots.length === 0}
    <p>no snapshots</p>
{:else}
    <table>
        <thead><tr><th>id</th><th>name</th><th>created</th><th>size</th><th></th></tr></thead>
        <tbody>
            {#each snapshots as s (s.snapshotId)}
                <tr>
                    <td><code title={s.snapshotId}>{s.snapshotId.slice(0, 8)}</code></td>
                    <td>{s.name}</td>
                    <td>{fmtDate(s.createdAt)}</td>
                    <td>{fmtSize(s.size)}</td>
                    <td>
                        <button type="button" class="act" aria-label="restore {s.snapshotId}" onclick={() => (restoreId = s.snapshotId)}>
                            Restore…
                        </button>
                    </td>
                </tr>
            {/each}
        </tbody>
    </table>
{/if}
<p>Create a snapshot before risky edits — Restore rolls the image bytes back.</p>
<button onclick={onDownload} disabled={busy}>Download</button>

<style>
    .error { color: #e06c75; }
    table { border-collapse: collapse; margin-top: 8px; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    .act { background: none; border: none; color: #7aa7d9; font: inherit; font-size: 0.9em; cursor: pointer; }
    .act:hover { text-decoration: underline; }
</style>
