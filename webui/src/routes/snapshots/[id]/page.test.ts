import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import Page from './+page.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('/snapshots/[id] page', () => {
    it('creates, lists and restores snapshots, syncing appState.snapshotCount', async () => {
        let created = false;
        let restored = false;
        server.use(
            http.post('*/api/v1/rpc/ImageSnapshotCreate', async () => {
                created = true;
                return HttpResponse.json({ snapshotId: 'snap-9', createdAt: '1760000000' });
            }),
            http.post('*/api/v1/rpc/ImageSnapshotsList', () =>
                HttpResponse.json({
                    snapshots: created
                        ? [
                              {
                                  snapshotId: 'snap-9',
                                  name: 'pre',
                                  createdAt: '1760000000',
                                  size: '16777216',
                              },
                          ]
                        : [],
                }),
            ),
            http.post('*/api/v1/rpc/ImageSnapshotRestore', async () => {
                restored = true;
                return HttpResponse.json({});
            }),
        );
        const user = userEvent.setup();
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText('no snapshots')).toBeInTheDocument());
        await user.type(screen.getByLabelText('snapshot name'), 'pre');
        await user.click(screen.getByRole('button', { name: 'Create' }));
        await waitFor(() => expect(screen.getByText('pre')).toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: 'restore snap-9' }));
        await user.click(screen.getByRole('button', { name: 'Confirm' }));
        await waitFor(() => expect(restored).toBe(true));
        const { appState } = await import('$lib/state.svelte');
        expect(appState.snapshotCount).toBe(1);
    });
});
