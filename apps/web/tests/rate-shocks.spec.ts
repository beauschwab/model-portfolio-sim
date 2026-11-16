import { test, expect } from '@playwright/test';

test('rate shock report runs the engine, shows limit status and exports', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Rate Shocks', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Rate Shocks', exact: true });
  const policy = panel.getByLabel('Rate shock policy', { exact: true });
  await expect(policy).toHaveValue(/"parallel_bp"/);
  // A limit on a scenario outside the set is refused by the engine and nothing is saved.
  const saved = JSON.parse(await policy.inputValue());
  await policy.fill(JSON.stringify({ ...saved, limits: [{ metric: 'eve', scenario: 'parallel_up_900', applicability: 'internal_limit', max_decline_pct: 5 }] }));
  await panel.getByRole('button', { name: 'Save policy', exact: true }).click();
  await expect(panel.getByRole('alert')).toContainText('outside the policy set');
  await policy.fill(JSON.stringify({ ...saved, parallel_bp: [200], limits: [
    { metric: 'eve', scenario: 'parallel_up_200', applicability: 'internal_limit', max_decline_pct: 0.01 },
    { metric: 'nii_12m', scenario: 'parallel_up_200', applicability: 'internal_limit', max_decline_pct: 99 }] }));
  await panel.getByRole('button', { name: 'Save policy', exact: true }).click();
  await expect(panel.getByText('Policy saved. Run the report again to apply it.', { exact: true })).toBeVisible();
  await panel.getByRole('button', { name: 'Run rate shocks', exact: true }).click();
  const table = panel.getByRole('table', { name: 'Rate shock report', exact: true });
  await expect(table).toBeVisible({ timeout: 55_000 });
  await expect(table.getByRole('row')).toHaveCount(10);   // two header rows and eight scenarios
  const up = table.getByRole('row').filter({ hasText: 'parallel up 200 bp' });
  await expect(up).toContainText('Breach −0.01%');
  await expect(up).toContainText('Within limit −99%');
  await expect(up).toContainText('No limit set');
  await expect(table.getByRole('row').filter({ hasText: 'ramp up 200 bp' })).toContainText('—');
  await expect(panel.getByText('Breach', { exact: true }).first()).toBeVisible();
  const download = page.waitForEvent('download');
  await panel.getByRole('button', { name: 'Export CSV', exact: true }).click();
  expect((await download).suggestedFilename()).toBe('rate-shocks.csv');
  await page.screenshot({ path: 'test-results/rate-shocks-desktop.png', fullPage: true });
  expect(errors).toEqual([]);
});
