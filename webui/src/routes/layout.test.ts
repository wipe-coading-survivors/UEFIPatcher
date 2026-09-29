import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import Layout from './+layout.svelte';

describe('shell layout', () => {
    it('renders all five nav sections as links', () => {
        const { getByText } = render(Layout, {
            children: createRawSnippet(() => ({
                render: () => '<main data-testid="children"></main>',
            })),
        });
        for (const label of ['Image', 'Forms', 'NVRAM', 'Snapshots', 'Artifacts']) {
            expect(getByText(label).closest('a,span')).toBeInstanceOf(HTMLAnchorElement);
        }
    });

    it('snapshot indicator hidden without snapshots', () => {
        const { queryByText } = render(Layout, {
            children: createRawSnippet(() => ({
                render: () => '<main data-testid="children"></main>',
            })),
        });
        expect(queryByText(/▣/)).toBeNull();
    });
});
