<script lang="ts">
    import type { VarRow } from '../nvar';
    let { imageId, prefill, onclose }: { imageId: string; prefill: VarRow; onclose: () => void; ondone?: () => void } = $props();
</script>
<div>set {prefill.name}: TODO Task 6</div>
<button onclick={onclose}>Close</button>
