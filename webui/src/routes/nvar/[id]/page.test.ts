import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import Page from './+page.svelte';

const server = setupServer(
    http.post('*/api/v1/rpc/NvarList', async ({ request }) => {
        const body = (await request.json()) as { path?: string; includeData?: boolean };
        if (body.path) {
            return HttpResponse.json({
                stores: [
                    {
                        path: body.path,
                        desc: 'StdDefaults',
                        records: 10,
                        freeTail: 256,
                        guidStoreSize: 0,
                        vars: [
                            {
                                name: 'Setup',
                                guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9',
                                offset: '58',
                                size: 2,
                                attributes: 7,
                                depth: 0,
                                data: 'AQID',
                            },
                        ],
                    },
                ],
            });
        }
        return HttpResponse.json({
            stores: [
                { path: '0/28', desc: 'StdDefaults FV0', records: 10, freeTail: 256, guidStoreSize: 0, vars: [] },
                { path: '1/3/2', desc: 'StdDefaults FV2', records: 10, freeTail: 128, guidStoreSize: 0, vars: [] },
            ],
        });
    }),
);

beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('/nvar/[id] page', () => {
    it('lists stores, loads vars with hex dump on select', async () => {
        const { userEvent } = await import('@testing-library/user-event');
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await waitFor(() => expect(screen.getByText('StdDefaults FV0')).toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: 'store 0/28' }));
        await waitFor(() => expect(screen.getByText('Setup')).toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: 'Setup' }));
        await waitFor(() => expect(screen.getByText(/00000000/)).toBeInTheDocument());
        expect(screen.getByText(/01 02 03/)).toBeInTheDocument();
    });

    it('var context menu Copy hex reports unavailable without clipboard', async () => {
        const { userEvent } = await import('@testing-library/user-event');
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await waitFor(() => expect(screen.getByRole('button', { name: 'store 0/28' })).toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: 'store 0/28' }));
        await waitFor(() => expect(screen.getByText('Setup')).toBeInTheDocument());
        await fireEvent.contextMenu(screen.getByText('Setup').closest('tr')!);
        await user.click(await screen.findByRole('menuitem', { name: 'Copy hex' }));
        await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent(/unavailable|copied/));
    });
});
