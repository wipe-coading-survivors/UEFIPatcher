import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import QuestionTable from './QuestionTable.svelte';
import type { QuestionSummary } from '../proto/engine';

const q = (over: Partial<QuestionSummary> = {}): QuestionSummary => ({
    questionId: 0x3b,
    kind: 'one_of',
    prompt: 'Above 4G Decoding',
    varStoreId: 1,
    varOffset: 0x3a,
    width: 1,
    ...over,
});

describe('QuestionTable', () => {
    it('renders rows with hex qid, prompt, seed', () => {
        render(QuestionTable, {
            questions: [q(), q({ questionId: 0x3c, kind: 'numeric', prompt: 'Fan speed', seedValue: '60000' })],
            onsetvalue: () => {},
            ongates: () => {},
        });
        expect(screen.getByText('0x3B')).toBeInTheDocument();
        expect(screen.getByText('Above 4G Decoding')).toBeInTheDocument();
        expect(screen.getByText('60000')).toBeInTheDocument();
    });

    it('fires set/gates with the row question', () => {
        const onsetvalue = vi.fn();
        const ongates = vi.fn();
        render(QuestionTable, { questions: [q()], onsetvalue, ongates });
        fireEvent.click(screen.getByRole('button', { name: 'set 0x3B' }));
        expect(onsetvalue).toHaveBeenCalledWith(expect.objectContaining({ questionId: 0x3b }));
        fireEvent.click(screen.getByRole('button', { name: 'gates 0x3B' }));
        expect(ongates).toHaveBeenCalledWith(expect.objectContaining({ questionId: 0x3b }));
    });

    it('empty state', () => {
        render(QuestionTable, { questions: [], onsetvalue: () => {}, ongates: () => {} });
        expect(screen.getByText('no questions')).toBeInTheDocument();
    });
});
