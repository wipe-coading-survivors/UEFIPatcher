import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import {
    ApiError, bridge, createSession, listNodes,
    removeNode, uploadImage,
    formTree, gatesList, hiiUnlock, listForms, listQuestions, listStrings,
    questionInfo, setFormVisibility, setValue,
    artifactExport, artifactsList, downloadArtifact, formAdd, formExport, formHijack,
    formSetAdd, nvarList, nvarSet, pageAdd, questionAdd,
    snapshotCreate, snapshotRestore, snapshotsList,
} from './api';

const server = setupServer();
beforeAll(() => server.listen());
afterEach(() => server.resetHandlers());
afterAll(() => server.close());

describe('api client', () => {
    it('bridge posts proto3-JSON to /api/v1/rpc/{Method}', async () => {
        server.use(
            http.post('*/api/v1/rpc/ImageNodesList', async ({ request }) => {
                const body = await request.json();
                expect(body).toEqual({ imageId: 'i-1', filter: '' });
                return HttpResponse.json({ nodes: [{ path: '0', type: 1, name: 'ME' }] });
            }),
        );
        const r = await bridge<{ imageId: string; filter: string }, { nodes: unknown[] }>('ImageNodesList', { imageId: 'i-1', filter: '' });
        expect(r.nodes).toHaveLength(1);
    });

    it('bridge maps error body to ApiError', async () => {
        server.use(
            http.post('*/api/v1/rpc/ImagesList', () =>
                HttpResponse.json({ error: 'boom', code: 'INVALID_ARGUMENT' }, { status: 400 })),
        );
        await expect(bridge('ImagesList', {})).rejects.toMatchObject({ code: 'INVALID_ARGUMENT', message: 'boom' });
    });

    it('createSession returns session_id', async () => {
        server.use(http.post('*/api/v1/session', () => HttpResponse.json({ session_id: 's-1' })));
        expect(await createSession()).toBe('s-1');
    });

    it('uploadImage sends multipart with mode', async () => {
        server.use(
            http.post('*/api/v1/image/upload', async ({ request }) => {
                const fd = await request.formData();
                expect(fd.get('mode')).toBe('write');
                const f = fd.get('file') as File;
                expect(f.name).toBe('bios.bin');
                return HttpResponse.json({ image_id: 'i-9', root_guid: '', name: 'bios.bin' });
            }),
        );
        const r = await uploadImage(new File([new Uint8Array([1, 2])], 'bios.bin'), 'write');
        expect(r.imageId).toBe('i-9');
    });

    it('removeNode sends imageId+target', async () => {
        server.use(
            http.post('*/api/v1/rpc/ImageNodeRemove', async ({ request }) => {
                expect(await request.json()).toEqual({ imageId: 'i-1', target: '1/3' });
                return HttpResponse.json({});
            }),
        );
        await removeNode('i-1', '1/3');
    });

    it('ApiError is instanceof Error', () => {
        const e = new ApiError('X', 'msg', 500);
        expect(e).toBeInstanceOf(Error);
        expect(e.message).toBe('msg');
    });

    it('hii wrappers send camelCase bodies to bridge methods', async () => {
        const calls: { method: string; body: unknown }[] = [];
        const handler = (method: string) =>
            http.post(`*/api/v1/rpc/${method}`, async ({ request }) => {
                calls.push({ method, body: await request.json() });
                return HttpResponse.json({});
            });
        server.use(
            handler('HiiListForms'), handler('HiiFormTree'), handler('HiiListQuestions'),
            handler('HiiQuestionInfo'), handler('HiiListStrings'), handler('HiiSetFormVisibility'),
            handler('HiiGatesList'), handler('HiiUnlock'), handler('HiiSetValue'),
        );
        await listForms('i-1');
        await formTree('i-1');
        await listQuestions('i-1', 'G:0x10:0', 10029);
        await questionInfo('i-1', 'G:0x10:0#10029:0x3B');
        await listStrings('i-1');
        await setFormVisibility('i-1', 'G:0x10:0#10029', true);
        await gatesList('i-1', 'G:0x10:0#10029');
        await hiiUnlock('i-1', 'G:0x10:0#10029');
        await setValue('i-1', 'G:0x10:0#10029:0x3B', '1');
        expect(calls).toEqual([
            { method: 'HiiListForms', body: { imageId: 'i-1' } },
            { method: 'HiiFormTree', body: { imageId: 'i-1' } },
            { method: 'HiiListQuestions', body: { imageId: 'i-1', target: 'G:0x10:0', formId: 10029 } },
            { method: 'HiiQuestionInfo', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029:0x3B' } },
            { method: 'HiiListStrings', body: { imageId: 'i-1' } },
            { method: 'HiiSetFormVisibility', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029', visible: true } },
            { method: 'HiiGatesList', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029' } },
            { method: 'HiiUnlock', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029' } },
            { method: 'HiiSetValue', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029:0x3B', value: '1' } },
        ]);
    });

    it('setValue value is a decimal string (proto3-JSON uint64)', async () => {
        server.use(
            http.post('*/api/v1/rpc/HiiSetValue', async ({ request }) => {
                expect(await request.json()).toMatchObject({ value: '18446744073709551615' });
                return HttpResponse.json({ appliedFlips: [], stores: ['a'] });
            }),
        );
        const r = await setValue('i-1', 'x', '18446744073709551615');
        expect(r.stores).toEqual(['a']);
    });

    it('w3 wrappers send camelCase bodies to bridge methods', async () => {
        const calls: { method: string; body: unknown }[] = [];
        const handler = (method: string) =>
            http.post(`*/api/v1/rpc/${method}`, async ({ request }) => {
                calls.push({ method, body: await request.json() });
                return HttpResponse.json({});
            });
        server.use(
            handler('NvarList'), handler('NvarSet'),
            handler('ImageSnapshotCreate'), handler('ImageSnapshotsList'), handler('ImageSnapshotRestore'),
            handler('ArtifactsList'), handler('ArtifactExport'),
            handler('HiiFormSetAdd'), handler('HiiFormAdd'), handler('HiiQuestionAdd'),
            handler('HiiPageAdd'), handler('HiiFormHijack'), handler('HiiFormExport'),
        );
        await nvarList('i-1');
        await nvarList('i-1', '0/2', true);
        await nvarSet('i-1', 'Setup', 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', '58', '1', 1);
        await nvarSet('i-1', 'Setup', '', '0', '0', 1);
        await snapshotCreate('i-1', 'pre');
        await snapshotsList('i-1');
        await snapshotRestore('i-1', 'snap-1');
        await artifactsList('s-1');
        await artifactExport('a-1', '/tmp/a.bin');
        await formSetAdd('i-1', '{}');
        await formSetAdd('i-1', '{}', '899407D7-99FE-43D8-9A21-79EC328CAC21');
        await formAdd('i-1', 'G:0x10:0', '{}');
        await questionAdd('i-1', 'G:0x10:0#10009', '{}');
        await pageAdd('i-1', 'G:0x10:0', '{}');
        await formHijack('i-1', 'G:0x10:0', '{}');
        await formHijack('i-1', 'G:0x10:0', '{}', 'SETUP-DATA-GUID');
        await formExport('i-1', 'G:0x10:0#10029');
        expect(calls).toEqual([
            { method: 'NvarList', body: { imageId: 'i-1', includeData: false } },
            { method: 'NvarList', body: { imageId: 'i-1', path: '0/2', includeData: true } },
            { method: 'NvarSet', body: { imageId: 'i-1', name: 'Setup', guid: 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9', offset: '58', value: '1', width: 1 } },
            { method: 'NvarSet', body: { imageId: 'i-1', name: 'Setup', offset: '0', value: '0', width: 1 } },
            { method: 'ImageSnapshotCreate', body: { imageId: 'i-1', name: 'pre' } },
            { method: 'ImageSnapshotsList', body: { imageId: 'i-1' } },
            { method: 'ImageSnapshotRestore', body: { imageId: 'i-1', snapshotId: 'snap-1' } },
            { method: 'ArtifactsList', body: { sessionId: 's-1' } },
            { method: 'ArtifactExport', body: { artifactId: 'a-1', outputPath: '/tmp/a.bin' } },
            { method: 'HiiFormSetAdd', body: { imageId: 'i-1', schemaJson: '{}', targetFfsGuid: '' } },
            { method: 'HiiFormSetAdd', body: { imageId: 'i-1', schemaJson: '{}', targetFfsGuid: '899407D7-99FE-43D8-9A21-79EC328CAC21' } },
            { method: 'HiiFormAdd', body: { imageId: 'i-1', target: 'G:0x10:0', schemaJson: '{}' } },
            { method: 'HiiQuestionAdd', body: { imageId: 'i-1', target: 'G:0x10:0#10009', schemaJson: '{}' } },
            { method: 'HiiPageAdd', body: { imageId: 'i-1', target: 'G:0x10:0', schemaJson: '{}' } },
            { method: 'HiiFormHijack', body: { imageId: 'i-1', target: 'G:0x10:0', schemaJson: '{}', setupdataGuid: '' } },
            { method: 'HiiFormHijack', body: { imageId: 'i-1', target: 'G:0x10:0', schemaJson: '{}', setupdataGuid: 'SETUP-DATA-GUID' } },
            { method: 'HiiFormExport', body: { imageId: 'i-1', itemId: 'G:0x10:0#10029' } },
        ]);
    });

    it('downloadArtifact fetches binary blob from REST route', async () => {
        server.use(
            http.get('*/api/v1/artifact/:id/download', () =>
                new HttpResponse(new Blob(['x']), {
                    status: 200,
                    headers: { 'Content-Type': 'application/octet-stream' },
                }),
            ),
        );
        const blob = await downloadArtifact('a-1');
        expect(blob.size).toBeGreaterThanOrEqual(0);
    });
});
