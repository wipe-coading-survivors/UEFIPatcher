import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest';
import { http, HttpResponse } from 'msw';
import { setupServer } from 'msw/node';
import {
    ApiError, bridge, createSession, listNodes, imageOpen as apiOpen,
    removeNode, uploadImage,
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

    it('api helpers send camelCase bodies', async () => {
        server.use(
            http.post('*/api/v1/rpc/ImageOpen', async ({ request }) => {
                const body = await request.json();
                expect(body).toEqual({ sessionId: 's-1', path: '/fw/x.bin', mode: 1, name: '' });
                return HttpResponse.json({ imageId: 'i-2', rootGuid: '', name: '' });
            }),
        );
        await apiOpen('s-1', '/fw/x.bin', 1);
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
});
