import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
import Layout from './+layout.svelte';

describe('shell layout', () => {
    it('renders nav sections, future ones disabled', () => {
        const { getByText } = render(Layout, {
            children: createRawSnippet(() => ({
                render: () => '<main data-testid="children"></main>',
            })),
        });
        for (const label of ['Image', 'Forms', 'NVRAM', 'Snapshots', 'Artifacts']) {
            expect(getByText(label)).toBeInTheDocument();
        }
        expect(getByText('Forms').closest('a,span')).toHaveAttribute('aria-disabled', 'true');
        expect(getByText('NVRAM').closest('a,span')).toHaveAttribute('aria-disabled', 'true');
        expect(getByText('Snapshots').closest('a,span')).toHaveAttribute('aria-disabled', 'true');
        expect(getByText('Artifacts').closest('a,span')).toHaveAttribute('aria-disabled', 'true');
    });
});
