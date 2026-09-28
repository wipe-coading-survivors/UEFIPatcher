import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { render, screen, waitFor } from '@testing-library/svelte';
import Page from './+page.svelte';

const server = setupServer(
    http.post('*/api/v1/rpc/ImageNodesList', () =>
        HttpResponse.json({
            nodes: [
                { path: '', type: 0, name: 'image' },
                { path: '0', type: 0, name: 'ME' },
            ],
        })),
);
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('image page', () => {
    it('loads tree and renders nodes', async () => {
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText('ME')).toBeInTheDocument());
    });

    it('shows search box and save/download actions', async () => {
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText('ME')).toBeInTheDocument());
        expect(screen.getByPlaceholderText('search nodes')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Download' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Save…' })).toBeInTheDocument();
    });
});
