import { test, expect } from '@playwright/test';

test('product backend setting persists after reload', async ({ page }) => {
  const errors: string[]=[]; page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Assumptions & Settings', exact: true }).click();
  const select = page.getByRole('combobox', { name: 'Product simulation backend' });
  await expect(select).toHaveValue('rust');
  await expect(select.locator('option[value="python"]')).toHaveCount(0);
  await select.selectOption('rust');
  const saved = page.waitForResponse(r => r.url().endsWith('/api/settings') && r.request().method() === 'PUT');
  await page.getByRole('region', { name: 'Assumptions & Settings', exact: true }).getByRole('button', { name: 'Save', exact: true }).click();
  expect((await saved).status()).toBe(200);
  await page.reload();
  await expect(select).toHaveValue('rust');
  expect(errors).toEqual([]);
  await expect(page.locator('vite-error-overlay')).toHaveCount(0);
  await page.screenshot({path:'../../.data/native-settings-ui.png'});
});
