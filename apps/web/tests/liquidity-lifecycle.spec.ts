import { test, expect } from '@playwright/test';

// Run with `bunx playwright test -c playwright.native.config.ts`: only the Rust service and Vite run.
test('a saved book is mapped, run, verified, published, explained and reopened through the Rust service', async ({ page }) => {
  const nativeCalls: string[] = [];
  page.on('request', r => { if (new URL(r.url()).pathname.startsWith('/native/')) nativeCalls.push(r.method() + ' ' + new URL(r.url()).pathname); });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Liquidity Lifecycle', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Liquidity Lifecycle', exact: true });

  // A saved book revision: lots, funding, pledges, margin and facilities (capacity, not cash).
  await panel.getByRole('button', { name: 'Save worked example as a saved book', exact: true }).click();
  await expect(panel.getByLabel('Saved book', { exact: true })).toHaveValue('worked-unwind-book');
  await expect(panel.getByText('Facilities (capacity, not cash)', { exact: true })).toBeVisible();
  await expect(panel.getByRole('region', { name: 'Funding and pledges' })).toContainText('bond-to-repo');
  await expect(panel.getByRole('region', { name: 'Hedge allocations' })).toContainText('economic link (undesignated)');

  // Full and partial exits are distinguished; planned obligations are listed before the run.
  const quantity = panel.getByLabel('Quantity of bond-lot', { exact: true });
  await quantity.fill('40');
  await expect(panel.getByRole('region', { name: 'Action unwind' })).toContainText('partial exit');
  await quantity.fill('100');
  await expect(panel.getByRole('region', { name: 'Action unwind' })).toContainText('full exit');
  await expect(panel.getByRole('region', { name: 'Planned obligations' })).toContainText('planned');

  // A missing financing treatment is an actionable mapping refusal, not a run.
  await panel.getByLabel('Financing treatment for repo-funds-bond', { exact: true }).selectOption('');
  await panel.getByRole('button', { name: 'Check mapping', exact: true }).click();
  const needs = panel.getByRole('region', { name: 'Mapping requirements' });
  await expect(needs).toContainText('funding allocation repo-funds-bond');
  await expect(needs).toContainText('set funding_instructions.repo-funds-bond to repay, retain or replace');
  await panel.getByLabel('Financing treatment for repo-funds-bond', { exact: true }).selectOption('repay');
  await panel.getByRole('button', { name: 'Check mapping', exact: true }).click();
  await expect(panel.getByText(/Mapping complete\. Composed request/)).toBeVisible();

  // Durable run -> native producer -> independent verification -> publication.
  await panel.getByRole('button', { name: 'Run', exact: true }).click();
  const status = panel.getByLabel('Job status');
  await expect(status).toContainText('verified and published', { timeout: 120_000 });
  await expect(panel.getByText('independent Rust verification passed', { exact: true })).toBeVisible();
  const funding = panel.getByLabel('Funding need');
  await expect(funding).toContainText('Facility draw draw adds 95.00');
  await expect(funding).toContainText('usable cash before it was 0.00');
  await expect(funding).toContainText('repay-repo pays 95.00 and depends on it');
  await expect(funding).toContainText('Posted VM returns on');
  await expect(funding).toContainText('Posted IM returns on');
  await expect(panel.getByRole('tabpanel')).toContainText('vm_return:swap-lot');
  await panel.getByRole('region', { name: 'Lifecycle results' }).screenshot({ path: test.info().outputPath('liquidity-timeline.png') });

  await panel.getByRole('tab', { name: 'Obligations', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('settled');
  await expect(panel.getByRole('tabpanel')).toContainText('4.00'); // termination payment pending until it settles
  await panel.getByRole('tab', { name: 'Collateral and facilities', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('Undrawn facility capacity (not cash)');
  await panel.getByRole('tab', { name: 'Accounting facts', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('none (economic links only)');
  await expect(panel.getByText('formal hedge accounting: not selected (economic links only)', { exact: true })).toBeVisible();

  // Paged published tables and a page-by-page download.
  await panel.getByRole('tab', { name: 'Lifecycle postings', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('sell-bond');
  const download = page.waitForEvent('download');
  await panel.getByRole('button', { name: 'Download', exact: true }).click();
  expect((await download).suggestedFilename()).toMatch(/^lifecycle_journal_lineage-[0-9a-f]{32}\.json$/);
  const jobText = await status.textContent();
  const jobId = /Job ([0-9a-f]{32})/.exec(jobText ?? '')?.[1];
  expect(jobId).toBeTruthy();

  // Formal treatment with equal economics is refused, never run as economic or shown as qualified.
  await panel.getByLabel('Accounting treatment of unwind', { exact: true }).selectOption('formal');
  await expect(panel.getByText(/Formal treatment needs an implemented ASC 815 owner/)).toBeVisible();
  await panel.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(status).toContainText('NATIVE_REFUSED: FORMAL_OWNER_REQUIRED', { timeout: 120_000 });
  await expect(status).not.toContainText('verified and published');

  // The same unwind under the reviewed funding policy: the bridge is drawn when the repo
  // repayment settles and repaid from the surplus, and the month keeps its minimum usable cash.
  await panel.getByLabel('Plan', { exact: true }).selectOption('worked-unwind-policy-plan');
  await expect(panel.getByLabel('Business plan mode', { exact: true })).toHaveValue('policy');
  await expect(panel.getByRole('region', { name: 'Business plan' })).toContainText('No perpetual funding');
  await panel.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(status).toContainText('verified and published', { timeout: 120_000 });
  await expect(panel.getByLabel('Funding need')).toContainText('The funding policy drew it for repay-repo, which needed 95.00 with usable cash at 0.00');
  await panel.getByRole('tab', { name: 'Funding decisions', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('repay-repo');
  await expect(panel.getByRole('tabpanel')).toContainText('surplus repayment');
  await panel.getByRole('tab', { name: 'Monthly', exact: true }).click();
  await expect(panel.getByRole('tabpanel')).toContainText('2024-01');
  await panel.getByRole('region', { name: 'Lifecycle results' }).screenshot({ path: test.info().outputPath('monthly.png') });

  // Reopen the published run after a reload, from storage.
  await page.reload();
  await page.getByRole('button', { name: 'Open Liquidity Lifecycle', exact: true }).click();
  const again = page.getByRole('region', { name: 'Liquidity Lifecycle', exact: true });
  await again.getByLabel('Job id', { exact: true }).fill(jobId!);
  await again.getByRole('button', { name: 'Reopen', exact: true }).click();
  await expect(again.getByLabel('Job status')).toContainText('verified and published');
  await expect(again.getByLabel('Funding need')).toContainText('Facility draw draw adds 95.00');

  // Every lifecycle call went to the Rust service.
  expect(nativeCalls.some(c => c.startsWith('POST /native/lifecycle/runs'))).toBe(true);
  expect(nativeCalls.some(c => c.startsWith('GET /native/jobs/') && c.endsWith('/receipt'))).toBe(true);
});
