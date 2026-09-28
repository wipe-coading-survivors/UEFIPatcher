import { existsSync, readFileSync } from 'node:fs';
import { E2E_SOCK } from '../playwright.config';

export default async function setup() {
    const t0 = Date.now();
    while (!existsSync(E2E_SOCK)) {
        if (Date.now() - t0 > 30_000) throw new Error(`timeout waiting for ${E2E_SOCK}`);
        await new Promise((r) => setTimeout(r, 200));
    }
}

export async function teardown() {
    try {
        const pid = Number(readFileSync('/tmp/uefipatcher-e2e/engine.pid', 'utf8').trim());
        if (pid) process.kill(pid, 'SIGTERM');
    } catch {}
}
