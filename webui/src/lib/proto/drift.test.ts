// Дрифт-трипваер: webui/src/lib/proto/engine.ts (коммитится генерат) не должен
// отставать от единого контракта crates/uefi-proto/proto/engine.proto (common).
// Каноничная регенерация — protoc: `npm run proto` (package.json). Здесь парсим
// контракт protobufjs'ом, чтобы тест не требовал protoc-бинарь в каждом
// окружении; полносеантическая проверка полей (buf, regenerate-and-diff) —
// отложена в TODO.md. Решение 2026-11-05, follow-up tse-followups.
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import protobuf from 'protobufjs';
import { describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const protoPath = join(here, '..', '..', '..', '..', 'crates', 'uefi-proto', 'proto', 'engine.proto');

const generated = readFileSync(join(here, 'engine.ts'), 'utf8');
const parsed = protobuf.parse(readFileSync(protoPath, 'utf8'), { keepCase: true });
const pkg = parsed.root.lookup('engine');
if (!(pkg instanceof protobuf.Namespace)) {
    throw new Error('engine.proto: package "engine" not found');
}

const messages: string[] = [];
const enums: string[] = [];
for (const [name, def] of Object.entries(pkg.nested ?? {})) {
    if (def instanceof protobuf.Type) messages.push(name);
    else if (def instanceof protobuf.Enum) enums.push(name);
}
messages.sort();
enums.sort();

describe('engine.ts drift against uefi-proto contract', () => {
    it('covers every message and enum of engine.proto', () => {
        expect(messages.length).toBeGreaterThan(50);
        const missing = [
            ...messages.filter((m) => !generated.includes(`export interface ${m} `)),
            ...enums.filter((e) => !generated.includes(`export enum ${e} `)),
        ];
        expect(
            missing,
            `engine.ts отстал от crates/uefi-proto/proto/engine.proto (нет: ${missing.join(', ')}). ` +
                'Запусти каноничную регенерацию: cd webui && npm run proto — и закоммить engine.ts.',
        ).toEqual([]);
    });
});
