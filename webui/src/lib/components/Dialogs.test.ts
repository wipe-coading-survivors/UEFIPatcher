import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import InsertDialog from './InsertDialog.svelte';
import ReplaceDialog from './ReplaceDialog.svelte';
import ConfirmDialog from './ConfirmDialog.svelte';

vi.mock('$lib/api', () => ({
    insertNode: vi.fn().mockResolvedValue({ itemId: 'n-1' }),
    replaceNode: vi.fn().mockResolvedValue({ itemId: 'n-1' }),
    removeNode: vi.fn().mockResolvedValue({}),
    rebuildNode: vi.fn().mockResolvedValue({}),
    extractNode: vi.fn().mockResolvedValue({ artifactId: 'a-1' }),
    uploadArtifact: vi.fn().mockResolvedValue({ artifactId: 'up-1' }),
}));
import * as api from '$lib/api';

describe('InsertDialog', () => {
    it('uploads file then inserts with returned artifact', async () => {
        const ondone = vi.fn();
        render(InsertDialog, { props: { imageId: 'i-1', target: '1', ondone } });
        const user = userEvent.setup();
        const input = screen.getByLabelText('FFS file') as HTMLInputElement;
        await user.upload(input, new File([new Uint8Array([9])], 'x.ffs'));
        await user.click(screen.getByRole('button', { name: 'Insert' }));
        await waitFor(() => expect(ondone).toHaveBeenCalled());
        expect(api.uploadArtifact).toHaveBeenCalled();
        expect(api.insertNode).toHaveBeenCalledWith('i-1', '1', 0, { artifactId: 'up-1' });
    });
});

describe('ReplaceDialog', () => {
    it('replaces from uploaded artifact with body-only option', async () => {
        const ondone = vi.fn();
        render(ReplaceDialog, { props: { imageId: 'i-1', target: '1/3', ondone } });
        const user = userEvent.setup();
        const input = screen.getByLabelText('body file') as HTMLInputElement;
        await user.upload(input, new File([new Uint8Array([9])], 'body.bin'));
        await user.click(screen.getByText('body only'));
        await user.click(screen.getByRole('button', { name: 'Replace' }));
        await waitFor(() => expect(ondone).toHaveBeenCalled());
        expect(api.replaceNode).toHaveBeenCalledWith('i-1', '1/3', { artifactId: 'up-1' }, true);
    });
});

describe('ConfirmDialog', () => {
    it('confirm fires onconfirm and ondone', async () => {
        const onconfirm = vi.fn().mockResolvedValue(undefined);
        const ondone = vi.fn();
        render(ConfirmDialog, { props: { title: 'Remove', message: 'remove 1/3?', onconfirm, oncancel: vi.fn(), ondone } });
        fireEvent.click(screen.getByRole('button', { name: 'Confirm' }));
        await waitFor(() => expect(ondone).toHaveBeenCalled());
        expect(onconfirm).toHaveBeenCalled();
    });
});
