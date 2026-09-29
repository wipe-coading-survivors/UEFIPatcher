import { test, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2E_DIR, E2E_SOCK } from '../playwright.config';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');
const CLI_BIN = resolve('../target/debug/uefi-cli');
const TARGET = '4/2';
const PARENT = '4';
const CLI_CWD = `${E2E_DIR}/artifacts-cli`;
const WEB_ART = `${E2E_DIR}/artifact-web.bin`;
const CLI_ART = `${E2E_DIR}/artifact-cli.bin`;

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

test('extract → artifacts download ≡ CLI extract + export', async ({ page }) => {
    await page.goto('/image');
    await expect(page.locator('#status-session')).toHaveText(/^session [0-9a-f]{8}$/, { timeout: 30_000 });
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 120_000 });

    await page.locator(`button[aria-label="expand ${PARENT}"]`).click();
    const item = page.locator('[role=treeitem]').filter({
        has: page.locator(`button[aria-label="expand ${TARGET}"]`),
    });
    await item.locator('.label').click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Extract' }).click();
    const notice = page.locator('p, [role=status]').filter({ hasText: /extracted artifact / });
    await expect(notice).toBeVisible({ timeout: 60_000 });
    const webArtifactId = (await notice.textContent())!.match(/extracted artifact (\S+)/)![1];

    await page.getByRole('link', { name: 'Artifacts' }).click();
    const row = page.locator('tbody tr').filter({ hasText: webArtifactId.slice(0, 8) });
    await expect(row).toBeVisible({ timeout: 30_000 });
    const [download] = await Promise.all([
        page.waitForEvent('download'),
        row.getByRole('button', { name: new RegExp(`^download ${webArtifactId}`) }).click(),
    ]);
    await download.saveAs(WEB_ART);

    cli(['image', 'open', IMAGE, '--mode', 'write']);
    const out = cli(['--format', 'json', 'node', 'extract', TARGET]);
    const cliArtifactId = (JSON.parse(out) as { artifact_id: string }).artifact_id;
    cli(['artifact', 'export', cliArtifactId, CLI_ART]);

    expect(sha256(WEB_ART)).toBe(sha256(CLI_ART));
});
