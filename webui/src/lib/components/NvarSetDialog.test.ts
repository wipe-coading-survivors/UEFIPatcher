import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import NvarSetDialog from './NvarSetDialog.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const prefill = {
    name: 'Setup',
    guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9',
    offset: '0',
    size: 2,
    attributes: 7,
    depth: 0,
    data: '',
};

describe('NvarSetDialog', () => {
    it('prefills name and guid, sends decimal offset/value (0x accepted)', async () => {
        let captured: unknown = null;
        server.use(
            http.post('*/api/v1/rpc/NvarSet', async ({ request }) => {
                captured = await request.json();
                return HttpResponse.json({
                    applied: ['0/28', '1/3/2'],
                    stores: ['0/28', '1/3/2'],
                });
            }),
        );
        const user = userEvent.setup();
        render(NvarSetDialog, {
            props: {
                imageId: 'i-1',
                prefill,
                onclose: () => {},
                ondone: () => {},
            },
        });
        const name = screen.getByLabelText('name');
        const guid = screen.getByLabelText('guid');
        expect((name as HTMLInputElement).value).toBe('Setup');
        expect((guid as HTMLInputElement).value).toBe(prefill.guid);
        await user.clear(screen.getByLabelText('offset'));
        await user.type(screen.getByLabelText('offset'), '0x3A');
        await user.clear(screen.getByLabelText('value'));
        await user.type(screen.getByLabelText('value'), '1');
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() => expect(screen.getByText(/applied 2 of 2 stores/)).toBeInTheDocument());
        expect(captured).toEqual({
            imageId: 'i-1',
            name: 'Setup',
            guid: prefill.guid,
            offset: '58',
            value: '1',
            width: 1,
        });
    });

    it('rejects invalid offset inline without RPC', async () => {
        const calls: unknown[] = [];
        server.use(
            http.post('*/api/v1/rpc/NvarSet', async ({ request }) => {
                calls.push(await request.json());
                return HttpResponse.json({ applied: [], stores: [] });
            }),
        );
        const user = userEvent.setup();
        render(NvarSetDialog, {
            props: { imageId: 'i-1', prefill, onclose: () => {}, ondone: () => {} },
        });
        await user.clear(screen.getByLabelText('offset'));
        await user.type(screen.getByLabelText('offset'), 'zz');
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        expect(screen.getByRole('alert')).toHaveTextContent(/offset/);
        expect(calls).toHaveLength(0);
    });
});
