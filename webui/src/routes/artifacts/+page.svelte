<script lang="ts">
    import Modal from '$lib/components/Modal.svelte';
    import { appState } from '$lib/state.svelte';
    import {
        artifactExport,
        artifactsList,
        downloadArtifact,
        downloadImage,
        uploadArtifact,
    } from '$lib/api';
    import type { ArtifactInfo } from '$lib/proto/engine';

    let artifacts: ArtifactInfo[] = $state([]);
    let error = $state('');
    let notice = $state('');
    let busy = $state(false);
    let exportId: string | null = $state(null);
    let exportPath = $state('');
    let loadSeq = 0;

    async function refresh() {
        if (!appState.sessionId) return;
        const seq = ++loadSeq;
        error = '';
        try {
            const r = await artifactsList(appState.sessionId);
            if (seq !== loadSeq) return;
            const seen = new Set<string>();
            artifacts = [];
            for (const a of r.artifacts ?? []) {
                if (seen.has(a.artifactId)) continue;
                seen.add(a.artifactId);
                artifacts.push(a);
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        appState.sessionId;
        refresh();
    });

    async function onimport(files: FileList | null) {
        if (!files?.length) return;
        error = '';
        notice = '';
        busy = true;
        try {
            const r = await uploadArtifact(files[0]);
            notice = `imported artifact ${r.artifactId}`;
            await refresh();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            busy = false;
        }
    }

    async function ondownload(id: string) {
        error = '';
        try {
            const blob = await downloadArtifact(id);
            const a = document.createElement('a');
            a.href = URL.createObjectURL(blob);
            a.download = `artifact-${id}.bin`;
            a.click();
            URL.revokeObjectURL(a.href);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

    async function doExport() {
        if (!exportId || !exportPath.trim()) return;
        await artifactExport(exportId, exportPath.trim());
        notice = `exported ${exportId} to ${exportPath.trim()}`;
    }

    async function onDownloadImage() {
        if (!appState.imageId) return;
        busy = true;
        try {
            const blob = await downloadImage(appState.imageId);
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

    const fmtDate = (s: string) => new Date(Number(s) * 1000).toLocaleString();
</script>

<h1>Artifacts</h1>
{#if !appState.sessionId}
    <p>no session</p>
{:else}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    {#if notice}<p role="status">{notice}</p>{/if}
    {#if exportId}
        <Modal title="Export artifact" onclose={() => (exportId = null)}>
            <form onsubmit={(e) => { e.preventDefault(); doExport().then(() => { exportId = null; }); }}>
                <label>server output path <input id="art-export-path" bind:value={exportPath} placeholder="/tmp/artifact.bin" /></label>
                <button type="submit">Export</button>
                <button type="button" onclick={() => (exportId = null)}>Cancel</button>
            </form>
        </Modal>
    {/if}

    <form>
        <label>artifact file <input id="art-file" type="file" onchange={(e) => onimport(e.currentTarget.files)} /></label>
        <button type="button" disabled={busy}>Import…</button>
    </form>

    {#if artifacts.length === 0}
        <p>no artifacts — extract a node from the image section or import a file</p>
    {:else}
        <table>
            <thead><tr><th>id</th><th>kind</th><th>size</th><th>created</th><th>source</th><th></th></tr></thead>
            <tbody>
                {#each artifacts as a (a.artifactId)}
                    <tr>
                        <td><code title={a.artifactId}>{a.artifactId.slice(0, 8)}</code></td>
                        <td>{a.kind}</td>
                        <td>{a.size}</td>
                        <td>{fmtDate(a.createdAt)}</td>
                        <td>{a.source}</td>
                        <td>
                            <button type="button" class="act" aria-label="download {a.artifactId}" onclick={() => ondownload(a.artifactId)}>Download</button>
                            <button type="button" class="act" aria-label="export {a.artifactId}" onclick={() => { exportId = a.artifactId; exportPath = ''; }}>Export…</button>
                        </td>
                    </tr>
                {/each}
            </tbody>
        </table>
    {/if}
    {#if appState.imageId}
        <button onclick={onDownloadImage} disabled={busy}>Download</button>
    {/if}
{/if}

<style>
    .error { color: #e06c75; }
    table { border-collapse: collapse; margin-top: 8px; }
    th, td { border: 1px solid #3a3f49; padding: 2px 8px; text-align: left; }
    .act { background: none; border: none; color: #7aa7d9; font: inherit; font-size: 0.9em; cursor: pointer; }
    .act:hover { text-decoration: underline; }
</style>
