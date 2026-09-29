import { test, expect } from '@playwright/test';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const IMAGE = resolve('../refs/fw/HNX99TF_200525_original_E5C88C6F.bin');

test.skip(!existsSync(IMAGE), 'живой образ отсутствует — E2E-гейт пропущен');
test.setTimeout(300_000);

async function upload(page: import('@playwright/test').Page) {
    await page.goto('/image');
    await expect(page.locator('#status-session')).toHaveText(/^session [0-9a-f]{8}$/, { timeout: 30_000 });
    await expect(page.getByRole('heading', { name: 'Image', exact: true })).toBeVisible();
    await page.locator('input#file').setInputFiles(IMAGE);
    await page.getByRole('button', { name: 'Upload' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 120_000 });
}

test('sidebar Image returns to open image view, not upload index', async ({ page }) => {
    await upload(page);
    await page.getByRole('link', { name: 'Forms' }).click();
    await expect(page.getByRole('heading', { name: 'Forms' })).toBeVisible({ timeout: 60_000 });
    await page.getByRole('link', { name: 'Image' }).click();
    await expect(page.locator('[role=treeitem]').first()).toBeVisible({ timeout: 60_000 });
    await expect(page.getByRole('button', { name: 'Save…' })).toBeVisible();
    await expect(page).toHaveURL(/\/image\/.+$/);
});

test('image context menu closes on Escape and outside click', async ({ page }) => {
    await upload(page);
    const label = page.locator('[role=treeitem] .label').first();
    await label.click({ button: 'right' });
    await expect(page.getByRole('menu')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByRole('menu')).toBeHidden();
    await label.click({ button: 'right' });
    await expect(page.getByRole('menu')).toBeVisible();
    await page.locator('main').click({ position: { x: 8, y: 8 } });
    await expect(page.getByRole('menu')).toBeHidden();
});

test('inspector shows no pending for untouched nodes', async ({ page }) => {
    await upload(page);
    await page.locator('[role=treeitem] .label').first().click();
    await expect(page.getByText(/pending:/)).toBeHidden();
});

test('forms context menu opens gates dialog', async ({ page }) => {
    await upload(page);
    await page.getByRole('link', { name: 'Forms' }).click();
    const formRow = page.locator('[data-testid=formstree] li[role=treeitem] .label').first();
    await formRow.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Gates…' }).click();
    await expect(page.getByText(/^Gates: form /).first()).toBeVisible({ timeout: 60_000 });
});

test('nvar context menu copy hex reports status', async ({ page }) => {
    await upload(page);
    await page.getByRole('link', { name: 'NVRAM' }).click();
    const storeRow = page.locator('section[aria-label="nvar stores"] tbody tr').first();
    await expect(storeRow).toBeVisible({ timeout: 60_000 });
    await storeRow.locator('button[aria-label^="store "]').click();
    const varRow = page.locator('section[aria-label="nvar vars"] tbody tr').first();
    await expect(varRow).toBeVisible({ timeout: 60_000 });
    await varRow.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Copy hex' }).click();
    await expect(page.getByRole('status')).toBeVisible();
});

test('tree rows render NF icons', async ({ page }) => {
    await upload(page);
    const icon = page.locator('span.nf').first();
    await expect(icon).toBeVisible();
    await expect(icon).toHaveCSS('font-family', /NF/);
});

test('image panes scroll independently of the page', async ({ page }) => {
    await upload(page);
    const tree = page.locator('.treepane');
    const aside = page.locator('.workbench aside');
    for (const pane of [tree, aside]) {
        await expect(pane).toHaveCSS('overflow-y', 'auto');
        const max = await pane.evaluate((el) => el.clientHeight);
        expect(max).toBeLessThan(page.viewportSize()!.height);
    }
});

test('expand triangles toggle with node state', async ({ page }) => {
    await upload(page);
    const first = page.locator('[role=treeitem] button[aria-label^="expand"]').first();
    await expect(first).toHaveAttribute('aria-expanded', 'true');
    await first.click();
    await expect(first).toHaveAttribute('aria-expanded', 'false');
    await expect(first).toHaveText('▸');
});

test('forms panes scroll independently of the page', async ({ page }) => {
    await upload(page);
    await page.getByRole('link', { name: 'Forms' }).click();
    const treePane = page.locator('section[aria-label="forms tree"]');
    const detailsPane = page.locator('section[aria-label="form details"]');
    for (const pane of [treePane, detailsPane]) {
        await expect(pane).toHaveCSS('overflow-y', 'auto', { timeout: 60_000 });
    }
});

test('form details header shows meta (Form ID, FormSet, Target)', async ({ page }) => {
    await upload(page);
    await page.getByRole('link', { name: 'Forms' }).click();
    const row = page.locator('[data-testid=formstree] li[role=treeitem] .label').first();
    await row.click();
    const meta = page.locator('dl.form-meta');
    await expect(meta).toBeVisible({ timeout: 60_000 });
    await expect(meta).toContainText('Form ID');
    await expect(meta).toContainText('FormSet');
    await expect(meta).toContainText('Target');
});
