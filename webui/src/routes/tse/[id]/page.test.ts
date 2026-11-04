import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import Page from './+page.svelte';

const spf = {
    pageCount: 214,
    formsets: [{ guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', rawU32: 2, pages: 161 }],
    vars: [{ guid: '899407D7-99FE-43D8-9A21-79EC328CAC21', name: 'Setup', attrs: 3, size: 148 }],
    stringControls: 4,
};
const report = {
    blocks: [{
        peOffset: 0x1b40,
        entries: [
            { formsetGuid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', formId: '1' },
            { formsetGuid: '7B59104A-C00D-4158-87FF-F04D6396A915', formId: '10000' },
        ],
    }],
    spf, peLen: 0x48860,
};

const server = setupServer(
    http.post('*/api/v1/rpc/TseReport', () => HttpResponse.json(report)),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

async function loaded() {
    await waitFor(() => expect(screen.getByRole('button', { name: /#1$/ })).toBeInTheDocument());
}

describe('/tse/[id] page', () => {
    it('renders stride blocks and $SPF card', async () => {
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        expect(screen.getAllByText('@1b40')).toHaveLength(2);
        expect(screen.getByRole('button', { name: /#1$/ })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: /#10000$/ })).toBeInTheDocument();
        expect(screen.getByText(/raw=2 pages=161/)).toBeInTheDocument();
        expect(screen.getByText(/Setup/)).toBeInTheDocument();
        expect(screen.queryByText('no $SPF')).not.toBeInTheDocument();
    });

    it('renders no $SPF note when spf is null', async () => {
        server.use(
            http.post('*/api/v1/rpc/TseReport', () => HttpResponse.json({ ...report, spf: null })),
        );
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await waitFor(() => expect(screen.getByText('no $SPF')).toBeInTheDocument());
    });

    it('prefills unhide fields on entry click', async () => {
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: /#1$/ }));
        expect((screen.getByLabelText('formset-guid') as HTMLInputElement).value)
            .toBe('EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9');
        expect((screen.getByLabelText('form-id') as HTMLInputElement).value).toBe('1');
    });

    it('unhide: first submit arms, second fires RPC and refreshes report', async () => {
        let reportCalls = 0;
        let unhideBody: Record<string, unknown> | null = null;
        server.use(
            http.post('*/api/v1/rpc/TseReport', () => {
                reportCalls++;
                return HttpResponse.json(report);
            }),
            http.post('*/api/v1/rpc/TseUnhide', async ({ request }) => {
                unhideBody = await request.json() as Record<string, unknown>;
                return HttpResponse.json({ peOffset: 0x1b40, entryPeOffset: 0x1b40 });
            }),
        );
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: /#1$/ }));
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        await user.click(screen.getByRole('button', { name: 'Unhide — confirm' }));
        await waitFor(() => expect(screen.getByRole('status'))
            .toHaveTextContent('unhidden: entry @ 0x1b40'));
        expect(reportCalls).toBeGreaterThanOrEqual(2);
        expect(unhideBody).toEqual({
            imageId: 'i-1',
            formsetGuid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9',
            formId: 1,
        });
    });

    it('unhide: engine error lands in alert', async () => {
        server.use(
            http.post('*/api/v1/rpc/TseUnhide', () =>
                HttpResponse.json(
                    { error: 'no AMITSE setup browser module', code: 'NOT_FOUND' },
                    { status: 400 },
                )),
        );
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: /#1$/ }));
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        await user.click(screen.getByRole('button', { name: 'Unhide — confirm' }));
        await waitFor(() => expect(screen.getByRole('alert'))
            .toHaveTextContent('no AMITSE setup browser module'));
    });

    it('expert block-offset is sent as blockPeOffset', async () => {
        let captured: Record<string, unknown> | null = null;
        server.use(
            http.post('*/api/v1/rpc/TseUnhide', async ({ request }) => {
                captured = await request.json() as Record<string, unknown>;
                return HttpResponse.json({ peOffset: 0x1b40, entryPeOffset: 0x1b41 });
            }),
        );
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: /#1$/ }));
        await user.click(screen.getByText('expert'));
        await user.type(screen.getByLabelText('block-offset'), '1b40');
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        await user.click(screen.getByRole('button', { name: 'Unhide — confirm' }));
        await waitFor(() => expect(captured).not.toBeNull());
        expect(captured!.blockPeOffset).toBe(0x1b40);
    });

    it('validates block offset before RPC', async () => {
        const calls: unknown[] = [];
        server.use(
            http.post('*/api/v1/rpc/TseUnhide', async ({ request }) => {
                calls.push(await request.json());
                return HttpResponse.json({ peOffset: 0, entryPeOffset: 0 });
            }),
        );
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: /#1$/ }));
        await user.click(screen.getByText('expert'));
        const off = screen.getByLabelText('block-offset');
        await user.type(off, '0x0x10');
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        expect(screen.getByRole('alert')).toHaveTextContent('block offset must be hex');
        expect(calls).toHaveLength(0);
        await user.clear(off);
        await user.type(off, '0x100000000');
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        expect(screen.getByRole('alert')).toHaveTextContent('block offset exceeds u32');
        expect(calls).toHaveLength(0);
    });

    it('validates guid and form id before RPC', async () => {
        const calls: unknown[] = [];
        server.use(
            http.post('*/api/v1/rpc/TseUnhide', async ({ request }) => {
                calls.push(await request.json());
                return HttpResponse.json({ peOffset: 0, entryPeOffset: 0 });
            }),
        );
        const user = userEvent.setup();
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await loaded();
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        expect(screen.getByRole('alert')).toHaveTextContent('formset guid required');
        expect(calls).toHaveLength(0);
        await user.type(screen.getByLabelText('formset-guid'), 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9');
        await user.type(screen.getByLabelText('form-id'), 'abc');
        await user.click(screen.getByRole('button', { name: 'Unhide' }));
        expect(screen.getByRole('alert')).toHaveTextContent('form id must be a number');
        expect(calls).toHaveLength(0);
    });
});
