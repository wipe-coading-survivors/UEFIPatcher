import { test, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2E_DIR, E2E_SOCK } from '../playwright.config';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');
const CLI_BIN = resolve('../target/debug/uefi-cli');
const NVAR_GUID = 'EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9';
const CLI_CWD = `${E2E_DIR}/nvar-cli`;
const CLI_COPY = `${E2E_DIR}/nvar-cli-copy.bin`;
const CLI_OUT = `${E2E_DIR}/nvar-cli-out.bin`;
const WEB_OUT = `${E2E_DIR}/nvar-web-out.bin`;
const SNAP_OUT = `${E2E_DIR}/snap-out.bin`;

const sha256 = (p: string) => createHash('sha256').update(readFileSync(p)).digest('hex');

function cli(args: string[], cwd = CLI_CWD) {
    const r = spawnSync(CLI_BIN, ['--sock', E2E_SOCK, ...args], { encoding: 'utf8', cwd });
    if (r.status !== 0) throw new Error(`cli ${args.join(' ')} failed: ${r.stderr}`);
    return r.stdout;
}

test.skip(!existsSync(IMAGE), 'живой образ отсутствует — E2E-гейт пропущен');

test.beforeAll(() => {
    mkdirSync(CLI_CWD, { recursive: true });
    cli(['session', 'init']);
});

test.setTimeout(300_000);

async function uploadImage(page: import('@playwright/test').Page) {
    await page.goto('/image');
    await expect(page.locator('#status-session')).toHaveText(/^session [0-9a-f]{8}$/, { timeout: 30_000 });
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 120_000 });
}

async function nvarSetSetup(page: import('@playwright/test').Page) {
    await page.getByRole('link', { name: 'NVRAM' }).click();
    const storeRow = page.locator('section[aria-label="nvar stores"] tbody tr').first();
    await expect(storeRow).toBeVisible({ timeout: 60_000 });
    await storeRow.locator('button[aria-label^="store "]').click();
    const setupRow = page
        .locator('section[aria-label="nvar vars"] tbody tr')
        .filter({ hasText: 'Setup' })
        .first();
    await expect(setupRow).toBeVisible({ timeout: 60_000 });
    await setupRow.locator('button[aria-label^="set Setup#"]').click();
    await expect(page.locator('input#nvar-name')).toHaveValue('Setup');
    await expect(page.locator('input#nvar-guid')).toHaveValue(NVAR_GUID);
    await page.locator('input#nvar-offset').fill('0x3A');
    await page.locator('input#nvar-value').fill('1');
    await page.getByRole('button', { name: 'Apply' }).click();
    await expect(page.getByText(/applied [1-9]\d* of \d+ stores/)).toBeVisible({ timeout: 60_000 });
    await page.getByRole('button', { name: 'Done' }).click();
}

async function download(page: import('@playwright/test').Page, dest: string) {
    const [download] = await Promise.all([
        page.waitForEvent('download'),
        page.getByRole('button', { name: 'Download' }).click(),
    ]);
    await download.saveAs(dest);
}

test('nvar set ≡ CLI nvar set (sha256 parity)', async ({ page }) => {
    await uploadImage(page);
    await nvarSetSetup(page);
    await download(page, WEB_OUT);

    copyFileSync(IMAGE, CLI_COPY);
    cli(['image', 'open', CLI_COPY, '--mode', 'write']);
    cli(['nvar', 'set', 'Setup', '--guid', NVAR_GUID, '--offset', '0x3A', '--value', '1']);
    cli(['image', 'save', CLI_OUT]);

    expect(sha256(WEB_OUT)).not.toBe(sha256(IMAGE));
    expect(sha256(WEB_OUT)).toBe(sha256(CLI_OUT));
});

test('snapshot create → mutate → restore returns original bytes', async ({ page }) => {
    await uploadImage(page);
    await page.getByRole('link', { name: 'Snapshots' }).click();
    await page.locator('input#snap-name').fill('pre');
    await page.getByRole('button', { name: 'Create' }).click();
    await expect(page.locator('table tbody tr').filter({ hasText: 'pre' })).toBeVisible({
        timeout: 30_000,
    });
    await expect(page.locator('#status-snapshots')).toHaveText(/▣ [1-9]/);

    await nvarSetSetup(page);
    await download(page, SNAP_OUT);
    expect(sha256(SNAP_OUT)).not.toBe(sha256(IMAGE));

    await page.getByRole('link', { name: 'Snapshots' }).click();
    await page.getByRole('button', { name: /^restore / }).click();
    await page.getByRole('button', { name: 'Confirm' }).click();
    await expect(page.getByRole('button', { name: 'Confirm' })).toBeHidden({ timeout: 30_000 });
    await expect(page.locator('table tbody tr').filter({ hasText: 'pre' })).toBeVisible({
        timeout: 30_000,
    });

    await download(page, SNAP_OUT);
    expect(sha256(SNAP_OUT)).toBe(sha256(IMAGE));
});
