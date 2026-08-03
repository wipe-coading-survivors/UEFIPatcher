import { imageStore, sessionStore, treeStore, type TreeNode } from './stores';

const API = '/api/v1';

async function req(path: string, opts: RequestInit = {}): Promise<any> {
    const resp = await fetch(`${API}${path}`, { ...opts, headers: { 'Content-Type': 'application/json', ...opts.headers } });
    if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: resp.statusText }));
        throw new Error(body.error || resp.statusText);
    }
    return resp.json();
}

export async function createSession() {
    const r = await req('/session', { method: 'POST', body: '{}' });
    sessionStore.set(r.session_id);
    return r;
}

export async function destroySession() {
    await req('/session', { method: 'DELETE' });
    sessionStore.set(null);
    imageStore.set(null);
    treeStore.set([]);
}

export async function openImage(path: string, mode: string = 'read') {
    const r = await req('/image/open', { method: 'POST', body: JSON.stringify({ path, mode }) });
    imageStore.set(r.image_id);
    return r;
}

export async function uploadImage(file: File): Promise<string> {
    const form = new FormData();
    form.append('file', file);
    const resp = await fetch(`${API}/image/upload`, { method: 'POST', body: form });
    if (!resp.ok) throw new Error('upload failed');
    const r = await resp.json();
    return r.path;
}

export async function dumpTree(imageId: string, format: string = 'text') {
    return req(`/image/${imageId}/dump?format=${format}`);
}

export async function listItems(imageId: string, filter: string = '') {
    return req(`/image/${imageId}/items?filter=${filter}`);
}

export async function findItem(imageId: string, target: string) {
    return req(`/image/${imageId}/find?target=${encodeURIComponent(target)}`);
}

export async function insert(imageId: string, target: string, ffsPath: string, mode: string = 'into') {
    return req(`/image/${imageId}/insert`, { method: 'POST', body: JSON.stringify({ target, ffs_path: ffsPath, mode }) });
}

export async function remove(imageId: string, target: string) {
    return req(`/image/${imageId}/remove`, { method: 'POST', body: JSON.stringify({ target }) });
}

export async function replace(imageId: string, target: string, dataPath: string, bodyOnly: boolean = false) {
    return req(`/image/${imageId}/replace`, { method: 'POST', body: JSON.stringify({ target, data_path: dataPath, body_only: bodyOnly }) });
}

export async function rebuild(imageId: string, target: string) {
    return req(`/image/${imageId}/rebuild`, { method: 'POST', body: JSON.stringify({ target }) });
}

export async function setVisibility(imageId: string, itemId: string, visible: boolean) {
    return req(`/image/${imageId}/set-visibility`, { method: 'POST', body: JSON.stringify({ item_id: itemId, visible }) });
}

export async function saveImage(imageId: string, outputPath: string) {
    return req(`/image/${imageId}/save`, { method: 'POST', body: JSON.stringify({ output_path: outputPath }) });
}

export async function downloadImage(imageId: string): Promise<Blob> {
    const resp = await fetch(`${API}/image/${imageId}/download`);
    if (!resp.ok) throw new Error('download failed');
    return resp.blob();
}

export async function addFormSet(imageId: string, schemaJson: string, targetFfsGuid: string = '') {
    return req(`/image/${imageId}/add-formset`, { method: 'POST', body: JSON.stringify({ schema_json: schemaJson, target_ffs_guid: targetFfsGuid }) });
}
