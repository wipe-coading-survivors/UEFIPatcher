import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Page from './+page.svelte';
import { appState } from '$lib/state.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
import { goto } from '$app/navigation';

const server = setupServer(
    http.post('*/api/v1/rpc/ImagesList', () =>
        HttpResponse.json({ images: [{ imageId: 'i-1', name: 'live.bin' }] })),
);
beforeAll(() => server.listen());
afterEach(() => {
    server.resetHandlers();
    vi.clearAllMocks();
});
afterAll(() => server.close());

describe('image landing', () => {
    beforeEach(() => {
        appState.sessionId = 's-1';
        appState.imageId = null;
    });
    afterEach(() => {
        appState.sessionId = null;
        appState.imageId = null;
    });

    it('lists open images', async () => {
        render(Page);
        await waitFor(() => expect(screen.getByText('live.bin')).toBeInTheDocument());
    });

    it('no server-path open section (W4-5)', async () => {
        render(Page);
        await waitFor(() => expect(screen.getByText('live.bin')).toBeInTheDocument());
        expect(screen.queryByRole('heading', { name: 'Open by server path' })).not.toBeInTheDocument();
        expect(screen.queryByPlaceholderText('/path/to/image.bin')).not.toBeInTheDocument();
    });

    it('upload navigates to image page', async () => {
        server.use(
            http.post('*/api/v1/image/upload', () =>
                HttpResponse.json({ image_id: 'i-9', root_guid: '', name: 'bios.bin' })),
        );
        render(Page);
        const user = userEvent.setup();
        const input = screen.getByLabelText('BIOS image') as HTMLInputElement;
        await user.upload(input, new File([new Uint8Array([1, 2, 3])], 'bios.bin'));
        await user.click(screen.getByRole('button', { name: 'Upload' }));
        await waitFor(() => expect(goto).toHaveBeenCalledWith('/image/i-9'));
    });
});
