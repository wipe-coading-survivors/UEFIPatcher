import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import Page from './+page.svelte';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

const S1 = '899407D7-99FE-43D8-9A21-79EC328CAC21';
const forms = [
    { formId: `${S1}:0x10:0`, formsetGuid: S1, formIdIfr: 10002, title: 'Advanced', visible: true },
    { formId: `${S1}:0x10:0`, formsetGuid: S1, formIdIfr: 10029, title: 'PCI Subsystem', visible: false },
];
const gates = [
    { gateKind: 'suppress', wraps: 'ref', formId: 10029, hostFormId: 10002, questionId: 0,
      expression: '1 == 1', flippable: true, flip: 'pkg+0x66: 01 -> 00', scopeOffset: 102, sourceTarget: '' },
];

function mockAll(over: Record<string, () => HttpResponse<any>> = {}) {
    server.use(
        http.post('*/api/v1/rpc/HiiListForms', () => HttpResponse.json({ forms })),
        http.post('*/api/v1/rpc/HiiFormTree', () => HttpResponse.json({ edges: [] })),
        http.post('*/api/v1/rpc/HiiListQuestions', ({ request }) =>
            request.json().then((b) =>
                HttpResponse.json({
                    questions: [
                        { questionId: 0x3b, kind: 'one_of', prompt: 'Above 4G Decoding',
                          varStoreId: 1, varOffset: 0x3a, width: 1 },
                    ].filter(() => (b as { formId: number }).formId === 10029),
                }))),
        http.post('*/api/v1/rpc/HiiSetFormVisibility', () => HttpResponse.json({})),
        http.post('*/api/v1/rpc/HiiGatesList', () => HttpResponse.json({ gates })),
        http.post('*/api/v1/rpc/HiiUnlock', () => HttpResponse.json({ gates, appliedFlips: ['pkg+0x66: 01 -> 00'] })),
        ...Object.entries(over).map(([k, v]) => http.post(`*/api/v1/rpc/${k}`, v)),
    );
}

vi.mock('$app/navigation', () => ({ goto: () => Promise.resolve() }));

describe('forms page', () => {
    it('renders formsets and forms, selects form, loads questions', async () => {
        mockAll();
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/Advanced/)).toBeInTheDocument());
        expect(screen.getByText(/PCI Subsystem/)).toBeInTheDocument();
        fireEvent.click(screen.getByText(/PCI Subsystem/));
        await waitFor(() => expect(screen.getByText('Above 4G Decoding')).toBeInTheDocument());
        expect(screen.getByRole('heading', { name: /form 10029/ })).toBeInTheDocument();
    });

    it('selected form shows meta details (Form ID, FormSet, Target)', async () => {
        mockAll();
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/Advanced/)).toBeInTheDocument());
        fireEvent.click(screen.getByText(/PCI Subsystem/));
        const meta = await waitFor(() => {
            const el = document.querySelector('dl.form-meta');
            expect(el).not.toBeNull();
            return el!;
        });
        expect(meta).toHaveTextContent('Form ID');
        expect(meta).toHaveTextContent('10029');
        expect(meta).toHaveTextContent('FormSet');
        expect(meta).toHaveTextContent(S1);
        expect(meta).toHaveTextContent('Target');
        expect(meta).toHaveTextContent(`${S1}:0x10:0#10029`);
    });

    it('late response does not overwrite newer selection', async () => {
        mockAll();
        server.use(
            http.post('*/api/v1/rpc/HiiListQuestions', ({ request }) =>
                request.json().then(async (b) => {
                    if ((b as { formId: number }).formId === 10002) {
                        await new Promise((r) => setTimeout(r, 150));
                        return HttpResponse.json({
                            questions: [
                                { questionId: 0x3b, kind: 'one_of', prompt: 'Above 4G Decoding',
                                  varStoreId: 1, varOffset: 0x3a, width: 1 },
                            ],
                        });
                    }
                    return HttpResponse.json({
                        questions: [
                            { questionId: 0x55, kind: 'one_of', prompt: 'Fast Question',
                              varStoreId: 1, varOffset: 0x55, width: 1 },
                        ],
                    });
                })),
        );
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/Advanced/)).toBeInTheDocument());
        fireEvent.click(screen.getByText(/Advanced/));
        fireEvent.click(screen.getByText(/PCI Subsystem/));
        await waitFor(() => expect(screen.getByRole('heading', { name: /form 10029/ })).toBeInTheDocument());
        await waitFor(() => expect(screen.getByText('Fast Question')).toBeInTheDocument());
        await new Promise((r) => setTimeout(r, 250));
        expect(screen.queryByText('Above 4G Decoding')).not.toBeInTheDocument();
        expect(screen.getByText('Fast Question')).toBeInTheDocument();
        expect(screen.getByRole('heading', { name: /form 10029/ })).toBeInTheDocument();
    });

    it('Show fires HiiSetFormVisibility and refreshes forms', async () => {
        const bodies: unknown[] = [];
        server.use(
            http.post('*/api/v1/rpc/HiiListForms', () => {
                bodies.push('list');
                return HttpResponse.json({ forms });
            }),
            http.post('*/api/v1/rpc/HiiFormTree', () => HttpResponse.json({ edges: [] })),
            http.post('*/api/v1/rpc/HiiSetFormVisibility', async ({ request }) => {
                bodies.push(await request.json());
                return HttpResponse.json({});
            }),
        );
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/PCI Subsystem/)).toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: `show ${S1}#10029` }));
        await waitFor(() => expect(bodies).toContainEqual({
            imageId: 'i-1', itemId: `${S1}:0x10:0#10029`, visible: true,
        }));
    });

    it('gates dialog from form row: unlock refreshes', async () => {
        mockAll();
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/PCI Subsystem/)).toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: `gates ${S1}#10029` }));
        await waitFor(() => expect(screen.getByText('1 == 1')).toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
        await waitFor(() => expect(screen.getByText(/unlocked: 1 flips applied/)).toBeInTheDocument());
    });

    it('form context menu opens gates dialog', async () => {
        mockAll();
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/Advanced/)).toBeInTheDocument());
        fireEvent.contextMenu(screen.getByText(/Advanced/));
        await fireEvent.click(screen.getByRole('menuitem', { name: 'Gates…' }));
        await waitFor(() => expect(screen.getByText(/Gates: form 10002/)).toBeInTheDocument());
    });

    it('question context menu offers Set value', async () => {
        mockAll({
            HiiQuestionInfo: () => HttpResponse.json({
                question: { formId: 10029, questionId: 0x3b, kind: 'one_of', varStoreId: 1,
                    varstore: undefined, varOffset: 0x3a, width: 1, min: '0', max: '1', step: '0',
                    options: [], defaults: [] },
            }),
        });
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/PCI Subsystem/)).toBeInTheDocument());
        fireEvent.click(screen.getByText(/PCI Subsystem/));
        await waitFor(() => expect(screen.getByText('Above 4G Decoding')).toBeInTheDocument());
        fireEvent.contextMenu(screen.getByText('Above 4G Decoding').closest('tr')!);
        await fireEvent.click(screen.getByRole('menuitem', { name: 'Set value…' }));
        await waitFor(() => expect(screen.getByText('Above 4G Decoding', { selector: 'h2, h3, label, .prompt, dialog *' })).toBeVisible());
    });

    it('strings tab renders panel', async () => {
        mockAll({
            HiiListStrings: () =>
                HttpResponse.json({ strings: [{ language: 'en-US', stringId: 1, text: 'Hello', source: 'res' }] }),
        });
        render(Page, { data: { imageId: 'i-1' } });
        await waitFor(() => expect(screen.getByText(/Advanced/)).toBeInTheDocument());
        fireEvent.click(screen.getByText(/Advanced/));
        fireEvent.click(screen.getByRole('tab', { name: 'Strings' }));
        await waitFor(() => expect(screen.getByText('Hello')).toBeInTheDocument());
    });

    it('Add question opens SchemaDialog with form target and applies', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiListForms', () =>
                HttpResponse.json({
                    forms: [
                        {
                            formId: `${S1}:0x10:0`,
                            formsetGuid: S1,
                            formIdIfr: 10009,
                            title: 'Serial Port Configuration',
                            visible: true,
                        },
                    ],
                }),
            ),
            http.post('*/api/v1/rpc/HiiFormTree', () => HttpResponse.json({ edges: [] })),
            http.post('*/api/v1/rpc/HiiListQuestions', () => HttpResponse.json({ questions: [] })),
            http.post('*/api/v1/rpc/HiiQuestionAdd', () =>
                HttpResponse.json({
                    questions: [{ questionId: 600, stringIds: {}, spfRecordOffset: 0 }],
                    refs: [],
                }),
            ),
        );
        const user = userEvent.setup();
        render(Page, { data: { imageId: 'i-1' } });
        const row = await screen.findByText(/10009/);
        await user.click(row);
        await waitFor(() => expect(screen.getByRole('heading', { name: /form 10009/ })).toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: 'add question' }));
        await waitFor(() =>
            expect((screen.getByLabelText('target') as HTMLInputElement).value).toBe(`${S1}:0x10:0#10009`),
        );
        await user.upload(
            screen.getByLabelText('schema file'),
            new File([JSON.stringify({ questions: [] })], 'schema.json'),
        );
        await user.click(screen.getByRole('button', { name: 'Apply' }));
        await waitFor(() => expect(screen.getByText(/questions added: 600/)).toBeInTheDocument());
    });
});
