import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import StringsPanel from './StringsPanel.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const strings = [
    { language: 'en-US', stringId: 772, text: 'Serial Port Configuration', source: 'res' },
    { language: 'en-US', stringId: 773, text: 'Above 4G Decoding', source: '899407D7-…/bare' },
    { language: 'ru-RU', stringId: 100, text: 'Другое', source: 'res' },
];

describe('StringsPanel', () => {
    it('loads strings and shows count', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiListStrings', () => HttpResponse.json({ strings })),
        );
        render(StringsPanel, { imageId: 'i-1' });
        await waitFor(() => expect(screen.getByText('Serial Port Configuration')).toBeInTheDocument());
        expect(screen.getByText('3 of 3')).toBeInTheDocument();
    });

    it('filter narrows by text and by id', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiListStrings', () => HttpResponse.json({ strings })),
        );
        render(StringsPanel, { imageId: 'i-1' });
        await waitFor(() => expect(screen.getByText('3 of 3')).toBeInTheDocument());
        const f = screen.getByLabelText('filter strings');
        await fireEvent.input(f, { target: { value: '4G' } });
        expect(screen.getByText('1 of 3')).toBeInTheDocument();
        expect(screen.queryByText('Другое')).not.toBeInTheDocument();
        await fireEvent.input(f, { target: { value: '773' } });
        expect(screen.getByText('1 of 3')).toBeInTheDocument();
    });

    it('load error inline', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiListStrings', () =>
                HttpResponse.json({ error: 'no image', code: 'NOT_FOUND' }, { status: 404 })),
        );
        render(StringsPanel, { imageId: 'nope' });
        await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('no image'));
    });
});
