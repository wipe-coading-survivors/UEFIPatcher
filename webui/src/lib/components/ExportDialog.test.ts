import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import ExportDialog from './ExportDialog.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('ExportDialog', () => {
    it('loads schema, shows lossy warnings, offers download', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiFormExport', () =>
                HttpResponse.json({
                    schemaJson: '{"forms":[{}]}',
                    formsetGuid: '7B59104A-C00D-4158-87FF-F04D6396A915',
                    parentFormId: 10002,
                    lossy: ['suppressed-if dropped'],
                    parentEntries: [{ prompt: 'Advanced', help: '' }],
                }),
            ),
        );
        render(ExportDialog, { imageId: 'i-1', itemId: 'G:0x10:0#10029', onclose: () => {} });
        await waitFor(() =>
            expect((screen.getByLabelText('schema json') as HTMLTextAreaElement).value).toContain(
                '"forms"',
            ),
        );
        expect(screen.getByText('suppressed-if dropped')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Download schema' })).toBeInTheDocument();
    });
});
