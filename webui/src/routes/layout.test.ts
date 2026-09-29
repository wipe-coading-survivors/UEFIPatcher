import { describe, expect, it } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import Layout from './+layout.svelte';
import { appState } from '$lib/state.svelte';

const children = () =>
    createRawSnippet(() => ({
        render: () => '<main data-testid="children"></main>',
    }));

describe('shell layout', () => {
    it('renders all five nav sections as links', () => {
        const { getByText } = render(Layout, { children: children() });
        for (const label of ['Image', 'Forms', 'NVRAM', 'Snapshots', 'Artifacts']) {
            expect(getByText(label).closest('a,span')).toBeInstanceOf(HTMLAnchorElement);
        }
    });

    it('snapshot indicator hidden without snapshots', () => {
        const { queryByText } = render(Layout, { children: children() });
        expect(queryByText(/▣/)).toBeNull();
    });

    it('snapshot indicator reset when image changes', async () => {
        appState.imageId = 'i-1';
        appState.snapshotCount = 3;
        const { queryByText } = render(Layout, { children: children() });
        await waitFor(() => expect(queryByText(/▣/)).toBeNull());
        appState.imageId = null;
        appState.snapshotCount = 0;
    });
});
