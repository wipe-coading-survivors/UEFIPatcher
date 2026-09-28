import { test, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2E_DIR, E2E_SOCK } from '../playwright.config';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');
const CLI_BIN = resolve('../target/debug/uefi-cli');
const TARGET = '4/2';
const PARENT = '4';
const FIXTURE = `${E2E_DIR}/fixture.bin`;
const CLI_COPY = `${E2E_DIR}/parity-copy.bin`;
const CLI_OUT = `${E2E_DIR}/cli-out.bin`;
const WEB_OUT = `${E2E_DIR}/web-out.bin`;

const sha256 = (p: string) => createHash('sha256').update(readFileSync(p)).digest('hex');

function cli(args: string[], cwd = E2E_DIR) {
    const r = spawnSync(CLI_BIN, ['--sock', E2E_SOCK, ...args], { encoding: 'utf8', cwd });
    if (r.status !== 0) throw new Error(`cli ${args.join(' ')} failed: ${r.stderr}`);
    return r.stdout;
}

test.skip(!existsSync(IMAGE), 'живой образ отсутствует — E2E-гейт пропущен');

test.beforeAll(() => {
    cli(['session', 'init']);
    cli(['image', 'open', IMAGE, '--mode', 'write']);
    const out = cli(['--format', 'json', 'node', 'extract', TARGET]);
    const artifactId = (JSON.parse(out) as { artifact_id: string }).artifact_id;
    cli(['artifact', 'export', artifactId, FIXTURE]);
});

test('upload → replace → download ≡ CLI replace → save', async ({ page }) => {
    await page.goto('/image');
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();

    const treeitem = page.locator('[role=treeitem]').first();
    await expect(treeitem).toBeVisible({ timeout: 120_000 });

    await page.locator(`button[aria-label="expand ${PARENT}"]`).click();
    const item = page.locator('[role=treeitem]').filter({
        has: page.locator(`button[aria-label="expand ${TARGET}"]`),
    });
    await item.locator('.label').click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Replace…' }).click();
    await page.locator('input#rep-file').setInputFiles(FIXTURE);
    await page.getByRole('button', { name: 'Replace' }).click();

    const [download] = await Promise.all([
        page.waitForEvent('download'),
        page.getByRole('button', { name: 'Download' }).click(),
    ]);
    await download.saveAs(WEB_OUT);

    copyFileSync(IMAGE, CLI_COPY);
    cli(['image', 'open', CLI_COPY, '--mode', 'write']);
    cli(['node', 'replace', TARGET, '--file', FIXTURE]);
    cli(['image', 'save', CLI_OUT]);

    expect(sha256(WEB_OUT)).toBe(sha256(CLI_OUT));
});
