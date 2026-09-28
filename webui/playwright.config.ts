import { defineConfig } from '@playwright/test';

export const E2E_DIR = '/tmp/uefipatcher-e2e';
export const E2E_SOCK = `${E2E_DIR}/engine.sock`;
const GW_PORT = 8123;

export default defineConfig({
    testDir: './e2e',
    globalSetup: './e2e/global-setup.ts',
    use: { baseURL: `http://127.0.0.1:${GW_PORT}` },
    webServer: {
        command: 'bash e2e/start-stack.sh',
        url: `http://127.0.0.1:${GW_PORT}/api/v1/health`,
        reuseExistingServer: false,
        timeout: 60_000,
    },
});
