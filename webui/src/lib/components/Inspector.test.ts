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

    it('NoAction (50) shows no pending row', () => {
        const plain = { ...node, action: 50 } as TreeNode;
        const { queryByText } = render(Inspector, { node: plain });
        expect(queryByText(/pending/i)).not.toBeInTheDocument();
    });

    it('action=50 default (proto3 omission) shows no pending row', () => {
        const plain = { ...node, action: 0 } as TreeNode;
        const { queryByText } = render(Inspector, { node: plain });
        expect(queryByText(/pending/i)).not.toBeInTheDocument();
    });

    it.each([
        [51, 'create'],
        [52, 'insert'],
        [53, 'replace'],
        [54, 'remove'],
        [55, 'rebuild'],
        [56, 'rebase'],
    ])('action %i renders %s', (action, name) => {
        const { getByText } = render(Inspector, { node: { ...node, action } as TreeNode });
        expect(getByText(`pending: ${name}`)).toBeInTheDocument();
    });

    it('unknown action renders raw number', () => {
        const { getByText } = render(Inspector, { node: { ...node, action: 99 } as TreeNode });
        expect(getByText('pending: 99')).toBeInTheDocument();
    });
});
