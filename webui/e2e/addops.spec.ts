import { test, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2E_DIR, E2E_SOCK } from '../playwright.config';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');
const CLI_BIN = resolve('../target/debug/uefi-cli');
const SCHEMA = resolve('../crates/uefi-engine/tests/data/serial/np_questions_10009.json');
const FORMSET = '899407D7-99FE-43D8-9A21-79EC328CAC21';
const FORMSET_IFR = '7B59104A-C00D-4158-87FF-F04D6396A915';
const FORM_ITEM = `${FORMSET}:0x10:0#10009`;
const CLI_CWD = `${E2E_DIR}/addops-cli`;
const CLI_COPY = `${E2E_DIR}/addops-cli-copy.bin`;
const CLI_OUT = `${E2E_DIR}/addops-cli-out.bin`;
const WEB_OUT = `${E2E_DIR}/addops-web-out.bin`;

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

test('question add on dead form 10009 ≡ CLI hii question add', async ({ page }) => {
    await page.goto('/image');
    await expect(page.locator('#status-session')).toHaveText(/^session [0-9a-f]{8}$/, { timeout: 30_000 });
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 120_000 });

    await page.getByRole('link', { name: 'Forms' }).click();
    const formRow = page.locator('[role=treeitem]').filter({ hasText: '10009' }).first();
    await expect(formRow).toBeVisible({ timeout: 60_000 });
    await formRow.locator('.label').click();
    await expect(page.getByRole('heading', { name: /form 10009/ })).toBeVisible({ timeout: 30_000 });

    await page.getByRole('button', { name: 'add question' }).click();
    await expect(page.locator('input#schema-target')).toHaveValue(FORM_ITEM);
    await page.locator('input#schema-file').setInputFiles(SCHEMA);
    await page.getByRole('button', { name: 'Apply' }).click();
    await expect(page.getByText(/questions added: 600, 601/)).toBeVisible({ timeout: 120_000 });
    await page.getByRole('button', { name: 'Done' }).click();

    const [download] = await Promise.all([
        page.waitForEvent('download'),
        page.getByRole('button', { name: 'Download' }).click(),
    ]);
    await download.saveAs(WEB_OUT);

    copyFileSync(IMAGE, CLI_COPY);
    cli(['image', 'open', CLI_COPY, '--mode', 'write']);
    cli(['hii', 'question', 'add', FORM_ITEM, '--file', SCHEMA]);
    cli(['image', 'save', CLI_OUT]);

    expect(sha256(WEB_OUT)).not.toBe(sha256(IMAGE));
    expect(sha256(WEB_OUT)).toBe(sha256(CLI_OUT));
});
