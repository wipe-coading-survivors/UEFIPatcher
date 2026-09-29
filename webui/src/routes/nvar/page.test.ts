import { afterEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { appState } from '$lib/state.svelte';
import Page from './+page.svelte';

afterEach(() => {
    appState.imageId = null;
    appState.snapshotCount = 0;
});

describe('/nvar index page', () => {
    it('shows graceful message when no image open', () => {
        appState.imageId = null;
        render(Page);
        expect(screen.getByText(/no image open/)).toBeInTheDocument();
    });

    it('links to current image NVRAM when image open', () => {
        appState.imageId = 'i-1';
        render(Page);
        expect(screen.getByRole('link', { name: /current image/ })).toHaveAttribute(
            'href',
            '/nvar/i-1',
        );
    });
});
