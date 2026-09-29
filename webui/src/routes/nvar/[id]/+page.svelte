<script lang="ts">
    import NvarStores from '$lib/components/NvarStores.svelte';
    import NvarSetDialog from '$lib/components/NvarSetDialog.svelte';
    import NvarVars from '$lib/components/NvarVars.svelte';
    import { downloadImage, nvarList } from '$lib/api';
    import type { VarRow } from '$lib/nvar';

    let { data }: { data: { imageId: string } } = $props();
    const imageId = $derived(data.imageId);

    let stores: { path: string; desc: string; records: number; vars: number; freeTail: number }[] = $state([]);
    let storePath: string | null = $state(null);
    let vars: VarRow[] = $state([]);
    let selectedVar: VarRow | null = $state<VarRow | null>(null);
    let setVar: VarRow | null = $state<VarRow | null>(null);
    let error = $state('');
    let busy = $state(false);
    let loadSeq = 0;

    async function refreshStores() {
        const seq = ++loadSeq;
        error = '';
        try {
            const r = await nvarList(imageId);
            if (seq !== loadSeq) return;
            stores = (r.stores ?? []).map((s) => ({
                path: s.path,
                desc: s.desc,
                records: s.records,
                vars: (s.vars ?? []).length,
                freeTail: s.freeTail,
            }));
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }
    $effect(() => {
        imageId;
        refreshStores();
    });

    async function onselectStore(path: string) {
        const seq = ++loadSeq;
        error = '';
        storePath = path;
        selectedVar = null;
        try {
            const r = await nvarList(imageId, path, true);
            if (seq !== loadSeq) return;
            const seen = new Set<string>();
            vars = [];
            for (const s of r.stores ?? []) {
                for (const v of s.vars ?? []) {
                    const key = `${v.name}#${v.offset}`;
                    if (seen.has(key)) continue;
                    seen.add(key);
                    vars.push(v as unknown as VarRow);
                }
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        }
    }

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

<h1>NVRAM</h1>
{#if error}<p class="error" role="alert">{error}</p>{/if}
{#if setVar}
    <NvarSetDialog {imageId} prefill={setVar} onclose={() => (setVar = null)} ondone={() => onselectStore(storePath!)} />
{/if}

<div class="cols">
    <section aria-label="nvar stores">
        <NvarStores {stores} selected={storePath} onselect={onselectStore} />
    </section>
    <section aria-label="nvar vars">
        {#if storePath}
            <NvarVars {vars} selected={selectedVar} onselect={(v) => (selectedVar = v)} onset={(v) => (setVar = v)} />
        {:else}
            <p>select a store</p>
        {/if}
    </section>
</div>
<button onclick={onDownload} disabled={busy}>Download</button>

<style>
    .cols { display: grid; grid-template-columns: 1fr 2fr; gap: 16px; }
    .error { color: #e06c75; }
</style>
