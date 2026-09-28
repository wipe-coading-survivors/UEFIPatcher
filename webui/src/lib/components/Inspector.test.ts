import { describe, expect, it } from 'vitest';
import { render } from '@testing-library/svelte';
import Inspector from './Inspector.svelte';
import type { TreeNode } from '../tree';

const node = {
    path: '1/28', type: 3, subtype: 17, guid: 'ABC', offset: '64', size: '128',
    name: 'TerminalDxe', action: 52, region: 'bios', isNvar: false, children: [],
} as TreeNode;

describe('Inspector', () => {
    it('shows node fields', () => {
        const { getByText } = render(Inspector, { node });
        expect(getByText('TerminalDxe')).toBeInTheDocument();
        expect(getByText('1/28')).toBeInTheDocument();
        expect(getByText('ABC')).toBeInTheDocument();
        expect(getByText('bios')).toBeInTheDocument();
    });

    it('shows pending action when set', () => {
        const { getByText } = render(Inspector, { node });
        expect(getByText(/pending/i)).toBeInTheDocument();
    });
});
