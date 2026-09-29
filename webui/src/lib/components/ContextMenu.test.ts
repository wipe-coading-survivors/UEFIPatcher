import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import ContextMenu from './ContextMenu.svelte';
import src from './ContextMenu.svelte?raw';

const items = [
    { id: 'a', label: 'Action…' },
    { id: 'b', label: 'Other' },
    { id: 'c', label: 'Disabled', disabled: true },
];

describe('ContextMenu', () => {
    it('picks an item and closes', async () => {
        const onpick = vi.fn();
        const onclose = vi.fn();
        render(ContextMenu, { props: { x: 10, y: 10, items, onpick, onclose } });
        await fireEvent.click(screen.getByRole('menuitem', { name: 'Action…' }));
        expect(onpick).toHaveBeenCalledWith('a');
        expect(onclose).toHaveBeenCalled();
    });

    it('closes on click outside', async () => {
        const onclose = vi.fn();
        render(ContextMenu, { props: { x: 10, y: 10, items, onpick: () => {}, onclose } });
        await fireEvent.click(document.body);
        expect(onclose).toHaveBeenCalled();
    });

    it('closes on Escape', async () => {
        const onclose = vi.fn();
        render(ContextMenu, { props: { x: 10, y: 10, items, onpick: () => {}, onclose } });
        await fireEvent.keyDown(window, { key: 'Escape' });
        expect(onclose).toHaveBeenCalled();
    });

    it('disabled item does not pick', async () => {
        const onpick = vi.fn();
        render(ContextMenu, { props: { x: 10, y: 10, items, onpick, onclose: () => {} } });
        const user = userEvent.setup();
        await user.click(screen.getByRole('menuitem', { name: 'Disabled' }));
        expect(onpick).not.toHaveBeenCalled();
    });

    it('menu items are left-aligned (style contract)', () => {
        expect(src).toContain('text-align: left');
    });
});
