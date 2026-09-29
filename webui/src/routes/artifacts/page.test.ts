import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import Page from './+page.svelte';
import { appState } from '$lib/state.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('/artifacts page', () => {
    it('lists artifacts, imports a file, downloads and exports via dialog', async () => {
        appState.sessionId = 's-1';
        appState.imageId = 'i-1';
        let imported = false;
        let exportedTo: string | null = null;
        let downloaded = 0;
        server.use(
            http.post('*/api/v1/rpc/ArtifactsList', () =>
                HttpResponse.json({
                    artifacts: imported
                        ? [{ artifactId: 'a-2', kind: 'body', size: '256', createdAt: '1760000000', source: 'upload' }]
                        : [
                              { artifactId: 'a-1', kind: 'body', size: '128', createdAt: '1759000000', source: '4/2' },
                          ],
                }),
            ),
            http.post('*/api/v1/artifact/upload', () => {
                imported = true;
                return HttpResponse.json({ artifact_id: 'a-2' });
            }),
            http.get('*/api/v1/artifact/:id/download', () => {
                downloaded += 1;
                return new HttpResponse(new Blob(['x']), {
                    status: 200,
                    headers: { 'Content-Type': 'application/octet-stream' },
                });
            }),
            http.post('*/api/v1/rpc/ArtifactExport', async ({ request }) => {
                exportedTo = ((await request.json()) as { outputPath: string }).outputPath;
                return HttpResponse.json({});
            }),
        );
        const user = userEvent.setup();
        render(Page);
        await waitFor(() => expect(screen.getByText('a-1')).toBeInTheDocument());

        await user.click(screen.getByRole('button', { name: 'download a-1' }));
        expect(downloaded).toBe(1);

        await user.click(screen.getByRole('button', { name: 'export a-1' }));
        await user.type(screen.getByLabelText('server output path'), '/tmp/out.bin');
        await user.click(screen.getByRole('button', { name: 'Export' }));
        await waitFor(() => expect(exportedTo).toBe('/tmp/out.bin'));

        const file = new File(['bytes'], 'x.bin');
        await user.upload(screen.getByLabelText('artifact file'), file);
        await waitFor(() => expect(screen.getByText('a-2')).toBeInTheDocument());
    });
});
