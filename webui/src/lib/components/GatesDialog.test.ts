import { afterEach, beforeAll, afterAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import GatesDialog from './GatesDialog.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const gate = (over: Partial<{ gateKind: string; wraps: string; hostFormId: number; expression: string; flippable: boolean }> = {}) => ({
    gateKind: 'suppress', wraps: 'ref', formId: 10029, hostFormId: 10002,
    questionId: 0, expression: '1 == 1', flippable: true,
    flip: 'pkg+0x66: 01 -> 00', scopeOffset: 102, sourceTarget: '',
});

describe('GatesDialog', () => {
    it('loads gates and renders table', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiGatesList', () =>
                HttpResponse.json({ gates: [gate()] })),
        );
        render(GatesDialog, {
            imageId: 'i-1', itemId: 'X:0x10:0#10029', title: 'Gates: 10029',
            onclose: () => {}, ondone: () => {},
        });
        await waitFor(() => expect(screen.getByText('suppress')).toBeInTheDocument());
        expect(screen.getByText('1 == 1')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Unlock' })).toBeEnabled();
    });

    it('unlock shows applied flips and fires ondone', async () => {
        const ondone = vi.fn();
        server.use(
            http.post('*/api/v1/rpc/HiiGatesList', () => HttpResponse.json({ gates: [gate()] })),
            http.post('*/api/v1/rpc/HiiUnlock', () =>
                HttpResponse.json({ gates: [gate()], appliedFlips: ['pkg+0x66: 01 -> 00'] })),
        );
        render(GatesDialog, {
            imageId: 'i-1', itemId: 'X:0x10:0#10029', title: 'Gates',
            onclose: () => {}, ondone,
        });
        await waitFor(() => fireEvent.click(screen.getByRole('button', { name: 'Unlock' })));
        await waitFor(() => expect(screen.getByText(/unlocked: 1 flips applied/)).toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: 'Done' }));
        expect(ondone).toHaveBeenCalled();
    });

    it('no flippable gates disables Unlock', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiGatesList', () =>
                HttpResponse.json({ gates: [{ ...gate(), flippable: false }] })),
        );
        render(GatesDialog, {
            imageId: 'i-1', itemId: 'X:0x10:0#10029', title: 'Gates',
            onclose: () => {}, ondone: () => {},
        });
        await waitFor(() => expect(screen.getByRole('button', { name: 'Unlock' })).toBeDisabled());
    });

    it('empty gates shows no gates', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiGatesList', () =>
                HttpResponse.json({ gates: [] })),
        );
        render(GatesDialog, {
            imageId: 'i-1', itemId: 'X:0x10:0#10029', title: 'Gates',
            onclose: () => {}, ondone: () => {},
        });
        await waitFor(() => expect(screen.getByText('no gates')).toBeInTheDocument());
    });

    it('engine error renders inline', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiGatesList', () =>
                HttpResponse.json({ error: 'not found', code: 'NOT_FOUND' }, { status: 404 })),
        );
        render(GatesDialog, {
            imageId: 'i-1', itemId: 'X', title: 'Gates',
            onclose: () => {}, ondone: () => {},
        });
        await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('not found'));
        expect(screen.queryByText('no gates')).not.toBeInTheDocument();
    });
});
