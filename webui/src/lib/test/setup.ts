import { expect } from 'vitest';
import * as matchers from '@testing-library/jest-dom/matchers';
import { File as NodeFile } from 'node:buffer';

expect.extend(matchers);

const win = (globalThis as unknown as {
    window: { FormData: typeof FormData; Blob: typeof Blob };
}).window;
const JsdomFormData = win.FormData;
const JsdomBlob = win.Blob;
const EnvRequest: typeof Request = Request;

const seed = await new Response(
    '--b\r\nContent-Disposition: form-data; name="x"\r\n\r\n1\r\n--b--\r\n',
    { headers: { 'content-type': 'multipart/form-data; boundary=b' } },
).formData();
interface NodeFormDataLike {
    append(name: string, value: string | NodeFile): void;
}
const NodeFormData = Object.getPrototypeOf(seed).constructor as new () => NodeFormDataLike;
const implSymbol = Object.getOwnPropertySymbols(
    Object.getOwnPropertyDescriptors(new JsdomBlob()),
)[0] as symbol;

class FixedRequest extends EnvRequest {
    constructor(input: RequestInfo | URL, init?: RequestInit) {
        const body = init?.body;
        if (body instanceof JsdomFormData) {
            const form = body as unknown as {
                forEach(cb: (value: string | File, key: string) => void): void;
            };
            const nodeForm = new NodeFormData();
            form.forEach((value, key) => {
                if (value instanceof JsdomBlob) {
                    const impl = (value as unknown as Record<symbol, { _bytes?: Uint8Array; _buffer?: Uint8Array }>)[implSymbol];
                    const bytes = impl._bytes ?? impl._buffer;
                    nodeForm.append(
                        key,
                        new NodeFile([bytes ?? new Uint8Array()], value.name || 'blob', { type: value.type }),
                    );
                } else {
                    nodeForm.append(key, value);
                }
            });
            super(input, { ...init, body: nodeForm as unknown as BodyInit });
        } else {
            super(input, init);
        }
    }
}
globalThis.Request = FixedRequest as unknown as typeof Request;
