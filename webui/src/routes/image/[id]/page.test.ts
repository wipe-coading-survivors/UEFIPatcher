import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Page from './+page.svelte';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
import { goto } from '$app/navigation';

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

    it('switch button navigates to image index', async () => {
        render(Page, { props: { data: { imageId: 'i-1' } } });
        await fireEvent.click(screen.getByRole('button', { name: 'Switch/upload…' }));
        await waitFor(() => expect(goto).toHaveBeenCalledWith('/image'));
    });
});
