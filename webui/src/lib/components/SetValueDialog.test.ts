import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import SetValueDialog from './SetValueDialog.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const oneof = (options: { value: string; text: string }[]) => ({
    question: {
        formId: 10029, questionId: 0x3b, kind: 'one_of', varStoreId: 1,
        varstore: { id: 1, guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', size: 0x72, name: 'Setup' },
        varOffset: 0x3a, width: 1, min: '0', max: '1', step: '0',
        options: options.map((o, i) => ({ stringId: i, value: o.value, flags: 0, text: o.text })),
        defaults: [],
    },
});

describe('SetValueDialog', () => {
    it('oneof renders select from options and applies decimal value', async () => {
        const bodies: unknown[] = [];
        server.use(
            http.post('*/api/v1/rpc/HiiQuestionInfo', () =>
                HttpResponse.json(oneof([{ value: '0', text: 'Disabled' }, { value: '1', text: 'Enabled' }]))),
            http.post('*/api/v1/rpc/HiiSetValue', async ({ request }) => {
                bodies.push(await request.json());
                return HttpResponse.json({ appliedFlips: [], stores: ['s1', 's2'] });
            }),
        );
        render(SetValueDialog, {
            imageId: 'i-1', itemId: 'X:0x10:0#10029:0x3B', prompt: 'Above 4G Decoding',
            onclose: () => {}, ondone: () => {},
        });
        const sel = await screen.findByRole('combobox', { name: 'value' });
        expect(sel).toBeInTheDocument();
        fireEvent.change(sel, { target: { value: '1' } });
        fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() => expect(screen.getByText('value set')).toBeInTheDocument());
        expect(bodies).toEqual([{ imageId: 'i-1', itemId: 'X:0x10:0#10029:0x3B', value: '1' }]);
    });

    it('numeric input rejects non-decimal input inline', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiQuestionInfo', () =>
                HttpResponse.json({
                    question: {
                        formId: 1, questionId: 5, kind: 'numeric', varStoreId: 1,
                        varstore: undefined, varOffset: 0, width: 2, min: '0', max: '60000', step: '100',
                        options: [], defaults: [],
                    },
                })),
        );
        render(SetValueDialog, {
            imageId: 'i-1', itemId: 'X#1:0x5', prompt: 'Speed',
            onclose: () => {}, ondone: () => {},
        });
        const input = await screen.findByLabelText('value');
        fireEvent.input(input, { target: { value: '0x10' } });
        fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
        expect(screen.getByRole('alert')).toHaveTextContent('decimal');
        expect(screen.queryByText('value set')).not.toBeInTheDocument();
    });

    it('done fires ondone after success', async () => {
        const ondone = vi.fn();
        server.use(
            http.post('*/api/v1/rpc/HiiQuestionInfo', () =>
                HttpResponse.json(oneof([{ value: '0', text: 'Off' }, { value: '1', text: 'On' }]))),
            http.post('*/api/v1/rpc/HiiSetValue', () =>
                HttpResponse.json({ appliedFlips: ['pkg+0x1: 01 -> 00'], stores: [] })),
        );
        render(SetValueDialog, {
            imageId: 'i-1', itemId: 'X#1:0x5', prompt: 'q',
            onclose: () => {}, ondone,
        });
        await screen.findByRole('combobox', { name: 'value' });
        fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
        const done = await screen.findByRole('button', { name: 'Done' });
        fireEvent.click(done);
        expect(ondone).toHaveBeenCalled();
    });
});
