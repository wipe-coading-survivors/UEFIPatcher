import { test, expect } from '@playwright/test';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2E_DIR, E2E_SOCK } from '../playwright.config';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');
const CLI_BIN = resolve('../target/debug/uefi-cli');
const FORMSET = '899407D7-99FE-43D8-9A21-79EC328CAC21';
const FORMSET_IFR = '7B59104A-C00D-4158-87FF-F04D6396A915';
const FORM_ITEM = `${FORMSET}:0x10:0#10029`;
const Q_ITEM = `${FORM_ITEM}:0x3B`;
const FORM_KEY = `${FORMSET_IFR}#10029`;
const CLI_COPY = `${E2E_DIR}/forms-parity-copy.bin`;
const CLI_OUT = `${E2E_DIR}/forms-cli-out.bin`;
const WEB_OUT = `${E2E_DIR}/forms-web-out.bin`;
const CLI_CWD = `${E2E_DIR}/forms-cli`;

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

test('unlock + set-value + download ≡ CLI hii form unlock + question set-value + save', async ({ page }) => {
    await page.goto('/image');
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 120_000 });

    await page.getByRole('link', { name: 'Forms' }).click();
    await page.locator(`button[aria-label="expand ${FORMSET_IFR}#10000"]`).click();
    await page.locator(`button[aria-label="expand ${FORMSET_IFR}#10002"]`).click();
    const formRow = page.locator('[role=treeitem]').filter({ hasText: '10029' });
    await expect(formRow).toBeVisible({ timeout: 60_000 });

    await formRow.locator(`button[aria-label="gates ${FORM_KEY}"]`).click();
    await page.getByRole('button', { name: 'Unlock' }).click();
    await expect(page.getByText(/unlocked: \d+ flips applied/)).toBeVisible({ timeout: 30_000 });
    await page.getByRole('button', { name: 'Done' }).click();

    await formRow.locator('.label').click();
    await expect(page.getByRole('heading', { name: /form 10029/ })).toBeVisible({ timeout: 30_000 });
    await page.getByRole('button', { name: 'set 0x3B' }).click();
    const sel = page.locator('select[aria-label="value"]');
    const input = page.locator('input#value');
    await expect(sel.or(input)).toBeVisible({ timeout: 30_000 });
    if (await sel.count()) {
        await sel.selectOption('1');
    } else {
        await input.fill('1');
    }
    await page.getByRole('button', { name: 'Apply' }).click();
    await expect(page.getByText('value set')).toBeVisible({ timeout: 30_000 });
    await page.getByRole('button', { name: 'Done' }).click();

    const [download] = await Promise.all([
        page.waitForEvent('download'),
        page.getByRole('button', { name: 'Download' }).click(),
    ]);
    await download.saveAs(WEB_OUT);

    copyFileSync(IMAGE, CLI_COPY);
    cli(['image', 'open', CLI_COPY, '--mode', 'write']);
    cli(['hii', 'form', 'unlock', FORM_ITEM]);
    cli(['hii', 'question', 'set-value', Q_ITEM, '1']);
    cli(['image', 'save', CLI_OUT]);

    expect(sha256(WEB_OUT)).toBe(sha256(CLI_OUT));
});
