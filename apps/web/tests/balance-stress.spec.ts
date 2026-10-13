import { test, expect } from '@playwright/test';

test('balance stress runs real engine and explains scenarios and local breaches', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Stress', exact: true }).click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await expect(panel.getByRole('button', { name: 'Run balance-sheet stress', exact: true })).toBeEnabled();
  await panel.getByRole('button', { name: 'Run balance-sheet stress', exact: true }).click();
  await expect(panel.getByRole('table', { name: 'comparison', exact: true })).toBeVisible({ timeout: 40_000 });
  await expect(panel.getByRole('table', { name: 'comparison', exact: true }).getByRole('row')).toHaveCount(11);
  await panel.getByLabel('Stress scenario', { exact: true }).selectOption('confidence_outage');
  await expect(panel.getByRole('table', { name: 'attribution', exact: true })).toContainText('deposit_withdrawal');
  await panel.getByLabel('Stress detail view', { exact: true }).selectOption('reverse_grid');
  await expect(panel.getByRole('table', { name: 'reverse_grid', exact: true }).getByRole('row')).toHaveCount(6);
  const download = page.waitForEvent('download');
  await panel.getByRole('button', { name: 'Export reverse_grid', exact: true }).click();
  expect((await download).suggestedFilename()).toBe('balance-stress-reverse_grid.csv');
  await panel.getByLabel('Stress account', { exact: true }).selectOption('dealer_usd');
  await panel.getByLabel('Stress detail view', { exact: true }).selectOption('actions');
  await expect(panel.getByRole('table', { name: 'actions', exact: true })).toContainText('dealer_support');
  await expect(panel.getByText('Model boundaries', { exact: true })).toBeVisible();
  await panel.getByLabel('Stress detail view', { exact: true }).selectOption('journal');
  await expect(panel.getByRole('table', { name: 'journal', exact: true })).toContainText('opening_equity');
  await panel.getByLabel('Stress detail view', { exact: true }).selectOption('trial_balance');
  await expect(panel.getByRole('table', { name: 'trial_balance', exact: true })).toContainText('cash');
  await page.screenshot({ path: 'test-results/balance-stress-desktop.png', fullPage: true });
  expect(errors).toEqual([]);
});

test('saved-book mapping lists required fields without guessing legal classifications', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Stress', exact: true }).click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await panel.getByRole('button', { name: 'Load saved-book mapping', exact: true }).click();
  await expect(panel.getByText(/Complete every null mapping/)).toBeVisible();
  await panel.getByText('Edit stress specification (JSON)', { exact: true }).click();
  const request = JSON.parse(await panel.getByLabel('Accounts, cohorts, policies, scenarios and reverse-stress severities').inputValue());
  expect(request.specification.accounts).toEqual([]);
  expect(request.allocation).toEqual([]);
  expect(Object.keys(request.position_mapping).length).toBeGreaterThan(0);
});

test('editing a completed specification hides obsolete results', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Stress', exact: true }).click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await panel.getByRole('button', { name: 'Run balance-sheet stress', exact: true }).click();
  await expect(panel.getByRole('table', { name: 'comparison', exact: true })).toBeVisible({ timeout: 40_000 });
  await panel.getByText('Edit stress specification (JSON)', { exact: true }).click();
  const editor = panel.getByLabel('Accounts, cohorts, policies, scenarios and reverse-stress severities');
  const spec = JSON.parse(await editor.inputValue()); spec.horizon_days = 60;
  await editor.fill(JSON.stringify(spec));
  await expect(panel.getByRole('table', { name: 'comparison', exact: true })).toHaveCount(0);
  await expect(panel.getByText('Inputs changed; run again to see matching results.')).toBeVisible();
});

test('invalid specification is rejected and narrow editor stays usable', async ({ page }) => {
  await page.setViewportSize({ width: 850, height: 950 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Stress', exact: true }).click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await panel.getByText('Edit stress specification (JSON)', { exact: true }).click();
  const editor = panel.getByLabel('Accounts, cohorts, policies, scenarios and reverse-stress severities');
  await expect(editor).not.toHaveValue('');
  const value = JSON.parse(await editor.inputValue());
  value.accounts[0].equity += 1;
  await editor.fill(JSON.stringify(value, null, 2));
  await panel.getByRole('button', { name: 'Run balance-sheet stress', exact: true }).click();
  await expect(panel.getByRole('alert')).toContainText('does not reconcile');
  await expect(panel.getByRole('table', { name: 'comparison', exact: true })).toHaveCount(0);
  await page.screenshot({ path: 'test-results/balance-stress-narrow.png', fullPage: true });
});
