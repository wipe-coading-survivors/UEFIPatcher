import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import SchemaDialog from './SchemaDialog.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const SCHEMA = JSON.stringify({ questions: [] });
const file = () => new File([SCHEMA], 'schema.json');

describe('SchemaDialog', () => {
    it('question op sends target + schema, renders question ids', async () => {
        let captured: { method: string; body: unknown } | null = null;
        server.use(
            http.post('*/api/v1/rpc/HiiQuestionAdd', async ({ request }) => {
                captured = { method: request.url, body: await request.json() };
                return HttpResponse.json({
                    questions: [{ questionId: 600, stringIds: { prompt: 772 }, spfRecordOffset: 512 }],
                    refs: [],
                });
            }),
        );
        const user = userEvent.setup();
        render(SchemaDialog, {
            props: {
                imageId: 'i-1',
                op: 'question',
                target: 'G:0x10:0#10009',
                onclose: () => {},
                ondone: () => {},
            },
        });
        expect((screen.getByLabelText('target') as HTMLInputElement).value).toBe('G:0x10:0#10009');
        await user.upload(screen.getByLabelText('schema file'), file());
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() => expect(screen.getByText(/questions added: 600/)).toBeInTheDocument());
        expect((captured!.body as { target: string }).target).toBe('G:0x10:0#10009');
        expect((captured!.body as { schemaJson: string }).schemaJson).toBe(SCHEMA);
    });

    it('formset op shows ffs guid field, not target', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiFormSetAdd', () =>
                HttpResponse.json({ newFfsId: 'ffs-1', insertedFormIds: [10101], stringIds: { title: 3 } }),
            ),
        );
        const user = userEvent.setup();
        render(SchemaDialog, {
            props: { imageId: 'i-1', op: 'formset', target: '', onclose: () => {}, ondone: () => {} },
        });
        expect(screen.queryByLabelText('target')).toBeNull();
        await user.upload(screen.getByLabelText('schema file'), file());
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() => expect(screen.getByText(/new ffs ffs-1/)).toBeInTheDocument());
    });

    it('engine error renders inline without crash', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiFormAdd', () =>
                HttpResponse.json(
                    { error: 'varstore id 2 duplicates stock', code: 'INVALID_ARGUMENT' },
                    { status: 400 },
                ),
            ),
        );
        const user = userEvent.setup();
        render(SchemaDialog, {
            props: { imageId: 'i-1', op: 'form', target: 'G:0x10:0', onclose: () => {}, ondone: () => {} },
        });
        await user.upload(screen.getByLabelText('schema file'), file());
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() =>
            expect(screen.getByRole('alert')).toHaveTextContent(/varstore id 2 duplicates stock/),
        );
    });
});
