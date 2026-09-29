import type {
    Empty, ImageOpenResponse, ImagesListResponse, ImageNodesResponse,
    Node as EngineNode,
    FormInfo, FormEdge, GateInfo, QuestionInfo, QuestionSummary, StringInfo,
} from './proto/engine';

const API = '/api/v1';

export type { EngineNode };

export class ApiError extends Error {
    constructor(
        public code: string,
        message: string,
        public status: number,
    ) {
        super(message);
    }
}

async function req(path: string, opts: RequestInit = {}): Promise<Response> {
    const resp = await fetch(`${API}${path}`, opts);
    if (!resp.ok) {
        const body = await resp.json().catch(() => ({ error: resp.statusText }));
        throw new ApiError(body.code ?? 'UNKNOWN', body.error ?? resp.statusText, resp.status);
    }
    return resp;
}

export async function bridge<Req, Resp>(method: string, body: Req): Promise<Resp> {
    const resp = await req(`/rpc/${method}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    });
    return resp.json() as Promise<Resp>;
}

export async function createSession(): Promise<string> {
    const resp = await req('/session', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{}',
    });
    const r = (await resp.json()) as { session_id: string };
    return r.session_id;
}

export async function destroySession(): Promise<void> {
    await req('/session', { method: 'DELETE' });
}

export async function uploadImage(
    file: File,
    mode: 'read' | 'write' = 'read',
): Promise<{ imageId: string; rootGuid: string; name: string }> {
    const form = new FormData();
    form.append('file', file);
    form.append('mode', mode);
    const resp = await req('/image/upload', { method: 'POST', body: form });
    const r = (await resp.json()) as { image_id: string; root_guid: string; name: string };
    return { imageId: r.image_id, rootGuid: r.root_guid, name: r.name };
}

export async function uploadArtifact(file: File): Promise<{ artifactId: string }> {
    const form = new FormData();
    form.append('file', file);
    const resp = await req('/artifact/upload', { method: 'POST', body: form });
    const r = (await resp.json()) as { artifact_id: string };
    return { artifactId: r.artifact_id };
}

export async function downloadImage(imageId: string): Promise<Blob> {
    const resp = await req(`/image/${imageId}/download`);
    return resp.blob();
}

export const imagesList = (sessionId: string) =>
    bridge<{ sessionId: string }, ImagesListResponse>('ImagesList', { sessionId });

export const imageOpen = (
    sessionId: string,
    path: string,
    mode: 0 | 1,
    name = '',
) => bridge<{ sessionId: string; path: string; mode: number; name: string }, ImageOpenResponse>(
    'ImageOpen',
    { sessionId, path, mode, name },
);

export const imageClose = (imageId: string) =>
    bridge<{ imageId: string }, Empty>('ImageClose', { imageId });

export const listNodes = (imageId: string, filter = '') =>
    bridge<{ imageId: string; filter: string }, ImageNodesResponse>('ImageNodesList', {
        imageId,
        filter,
    });

export const searchNodes = (
    imageId: string,
    query: string,
    modes: number[] = [0],
    limit = 100,
) =>
    bridge<
        { imageId: string; query: string; modes: number[]; limit: number },
        ImageNodesResponse
    >('ImageNodesSearch', { imageId, query, modes, limit });

export const insertNode = (
    imageId: string,
    target: string,
    mode: number,
    source: { artifactId?: string; ffsPath?: string },
) =>
    bridge<
        {
            imageId: string;
            target: string;
            mode: number;
            artifactId: string;
            ffsPath: string;
        },
        { itemId: string }
    >('ImageNodeInsert', {
        imageId,
        target,
        mode,
        artifactId: source.artifactId ?? '',
        ffsPath: source.ffsPath ?? '',
    });

export const removeNode = (imageId: string, target: string) =>
    bridge<{ imageId: string; target: string }, Empty>('ImageNodeRemove', { imageId, target });

export const replaceNode = (
    imageId: string,
    target: string,
    source: { artifactId?: string; ffsPath?: string },
    bodyOnly: boolean,
) =>
    bridge<
        {
            imageId: string;
            target: string;
            artifactId: string;
            ffsPath: string;
            bodyOnly: boolean;
        },
        { itemId: string }
    >('ImageNodeReplace', {
        imageId,
        target,
        artifactId: source.artifactId ?? '',
        ffsPath: source.ffsPath ?? '',
        bodyOnly,
    });

export const rebuildNode = (imageId: string, target: string) =>
    bridge<{ imageId: string; target: string }, Empty>('ImageNodeRebuild', { imageId, target });

export const extractNode = (imageId: string, target: string, bodyOnly: boolean) =>
    bridge<
        { imageId: string; target: string; bodyOnly: boolean },
        { artifactId: string }
    >('ImageNodeExtract', { imageId, target, bodyOnly });

export const saveImage = (imageId: string, outputPath: string) =>
    bridge<{ imageId: string; outputPath: string }, Empty>('ImageSave', { imageId, outputPath });

export const listForms = (imageId: string) =>
    bridge<{ imageId: string }, { forms: FormInfo[] }>('HiiListForms', { imageId });

export const formTree = (imageId: string) =>
    bridge<{ imageId: string }, { edges: FormEdge[] }>('HiiFormTree', { imageId });

export const listQuestions = (imageId: string, target: string, formId: number) =>
    bridge<{ imageId: string; target: string; formId: number }, { questions: QuestionSummary[] }>(
        'HiiListQuestions',
        { imageId, target, formId },
    );

export const questionInfo = (imageId: string, itemId: string) =>
    bridge<{ imageId: string; itemId: string }, { question?: QuestionInfo }>('HiiQuestionInfo', {
        imageId,
        itemId,
    });

export const listStrings = (imageId: string) =>
    bridge<{ imageId: string }, { strings: StringInfo[] }>('HiiListStrings', { imageId });

export const setFormVisibility = (imageId: string, itemId: string, visible: boolean) =>
    bridge<{ imageId: string; itemId: string; visible: boolean }, Empty>(
        'HiiSetFormVisibility',
        { imageId, itemId, visible },
    );

export const gatesList = (imageId: string, itemId: string) =>
    bridge<{ imageId: string; itemId: string }, { gates: GateInfo[] }>('HiiGatesList', {
        imageId,
        itemId,
    });

export const hiiUnlock = (imageId: string, itemId: string) =>
    bridge<{ imageId: string; itemId: string }, { gates: GateInfo[]; appliedFlips: string[] }>(
        'HiiUnlock',
        { imageId, itemId },
    );

export const setValue = (imageId: string, itemId: string, value: string) =>
    bridge<
        { imageId: string; itemId: string; value: string },
        { question?: QuestionInfo; appliedFlips: string[]; stores: string[] }
    >('HiiSetValue', { imageId, itemId, value });
