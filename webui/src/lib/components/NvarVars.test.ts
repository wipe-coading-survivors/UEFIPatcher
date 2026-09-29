import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import NvarVars from './NvarVars.svelte';
import type { VarRow } from '../nvar';

const v = (over: Partial<VarRow> = {}): VarRow => ({
    name: 'Setup',
    guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9',
    offset: '58',
    size: 2,
    attributes: 7,
    depth: 0,
    data: 'AQID',
    ...over,
} as VarRow);

describe('NvarVars', () => {
    it('fires oncontext on var row right-click', () => {
        const oncontext = vi.fn();
        render(NvarVars, {
            vars: [v()],
            selected: null,
            onselect: () => {},
            onset: () => {},
            oncontext,
        });
        fireEvent.contextMenu(screen.getByText('Setup').closest('tr')!);
        expect(oncontext).toHaveBeenCalledWith(expect.objectContaining({ name: 'Setup' }), expect.any(MouseEvent));
    });
});
