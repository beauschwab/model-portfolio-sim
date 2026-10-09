import { test, expect, type Page } from '@playwright/test';

async function open(page: Page) {
  await page.goto('/');
  const button = page.getByRole('button', { name: 'Open Stress', exact: true });
  await button.click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await panel.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(panel.getByRole('button', { name: 'Run partitioned simulation', exact: true })).toBeEnabled();
  return panel;
}

async function run(page: Page) {
  const panel = await open(page);
  await expect(panel.getByLabel('Partitioned engine', { exact: true })).toHaveValue('rust');
  await expect(panel.getByLabel('Partitioned engine', { exact: true })).toBeDisabled();
  await panel.getByText('Edit partitioned specification (JSON)', { exact: true }).click();
  const editor = panel.getByLabel('Explicit balance-sheet specification');
  const spec = JSON.parse(await editor.inputValue());
  spec.horizon_days = 30; spec.reverse_severities = [];
  await editor.fill(JSON.stringify(spec));
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Saved journal verified', { exact: true })).toBeVisible({ timeout: 40_000 });
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toBeVisible();
  return panel;
}

test('missing Rust disables execution without a Python fallback', async ({page})=>{
  await page.route('**/balance-stress/capabilities', async route=>{
    const response=await route.fetch();
    await route.fulfill({response,json:{...(await response.json()),rust:false}});
  });
  await page.goto('/');
  await page.getByRole('button',{name:'Open Stress',exact:true}).click();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const panel=page.getByRole('region',{name:'Stress',exact:true});
  await panel.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(panel.getByRole('button',{name:'Run partitioned simulation',exact:true})).toBeDisabled();
  await expect(panel.getByLabel('Partitioned engine',{exact:true})).toHaveValue('rust');
  await expect(panel.locator('option[value="python"]')).toHaveCount(0);
});

test('real Rust run pages, downloads, reopens and never fetches full result', async ({ page }) => {
  const errors: string[] = [], fullResults: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('request', r => { if (/\/jobs\/[^/]+\/result$/.test(r.url())) fullResults.push(r.url()); });
  const panel = await run(page);
  await expect(panel.getByRole('checkbox', { name: /Large book/ })).toBeDisabled();
  const id = await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/journal');
  const table = panel.getByRole('table', { name: 'Partitioned results', exact: true });
  await expect(table.getByRole('row')).toHaveCount(101);
  const first = await table.innerText();
  await panel.getByRole('button', { name: 'Next page', exact: true }).click();
  await expect(panel.getByText(/Page 2 of/)).toBeVisible();
  await expect(table).not.toHaveText(first);
  await expect(table.getByRole('row')).toHaveCount(101);
  const download = page.waitForEvent('download');
  await panel.getByRole('button', { name: 'Download partition', exact: true }).click();
  expect((await download).suggestedFilename()).toContain(`${id}-journal-0.parquet`);
  await page.setViewportSize({ width: 850, height: 950 });
  await panel.getByText('Partitioned balance-sheet simulation', { exact: true }).scrollIntoViewIfNeeded();
  const panelBounds = await panel.boundingBox();
  const engineBounds = await panel.getByLabel('Partitioned engine', { exact: true }).boundingBox();
  expect(engineBounds!.x + engineBounds!.width).toBeLessThanOrEqual(panelBounds!.x + panelBounds!.width);
  await page.screenshot({ path: 'test-results/partitioned-stress-narrow.png', fullPage: true });
  await page.reload();
  await page.getByRole('region', { name: 'Stress', exact: true }).getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  const restored = page.getByRole('region', { name: 'Stress', exact: true });
  await restored.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID', { exact: true })).toHaveValue(id);
  await expect(restored.getByText('Saved journal verified', { exact: true })).toBeVisible();
  expect(fullResults).toEqual([]); expect(errors).toEqual([]);
});

test('Rust production, empty tables and failed page retry preserve selection', async ({ page }) => {
  const panel = await run(page);
  await expect(panel.getByText('rust engine', { exact: true })).toBeVisible();
  await panel.getByLabel('Partitioned result table').selectOption('/reverse_grid');
  await expect(panel.getByText('No rows in this table.')).toBeVisible();
  await expect(panel.getByRole('button', { name: 'Download partition', exact: true })).toBeDisabled();
  let fail = true;
  await page.route('**/jobs/*/table?**', async route => {
    if (fail && new URL(route.request().url()).searchParams.get('path') === '/journal') {
      fail = false; await route.fulfill({ status: 503, body: 'temporary query failure' });
    } else await route.continue();
  });
  await panel.getByLabel('Partitioned result table').selectOption('/journal');
  await expect(panel.getByRole('alert')).toContainText('temporary query failure');
  await panel.getByRole('button', { name: 'Retry table', exact: true }).click();
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true }).getByRole('row')).toHaveCount(101);
  await expect(panel.getByLabel('Partitioned result table')).toHaveValue('/journal');
});

test('late table responses cannot replace the newly selected table', async ({ page }) => {
  const panel = await run(page);
  let release!: () => void;
  const blocked = new Promise<void>(resolve => { release = resolve; });
  let intercepted!: () => void;
  const started = new Promise<void>(resolve => { intercepted = resolve; });
  await page.route('**/jobs/*/table?**', async route => {
    if (new URL(route.request().url()).searchParams.get('path') === '/journal') {
      const response = await route.fetch(); intercepted(); await blocked;
      await route.fulfill({ response }).catch(() => {});
    } else await route.continue();
  });
  await panel.getByLabel('Partitioned result table').selectOption('/journal');
  await started;
  await panel.getByLabel('Partitioned result table').selectOption('/reverse_grid');
  await expect(panel.getByText('No rows in this table.')).toBeVisible();
  release();
  await expect(panel.getByLabel('Partitioned result table')).toHaveValue('/reverse_grid');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true }).getByRole('row')).toHaveCount(1);
});

test('cancel queued run and retry missing run use explicit UI errors', async ({ page }) => {
  let cancelled = false;
  await page.route('**/balance-stress/stream', route => route.fulfill({ json: { id: 'fixture-run', kind: 'streamed_balance_stress', status: 'queued', revision: 0 } }));
  await page.route('**/jobs/fixture-run', route => {
    if (route.request().method() === 'DELETE') { cancelled = true; return route.fulfill({ json: { cancelled: true } }); }
    return route.fulfill({ json: { id: 'fixture-run', kind: 'streamed_balance_stress', revision: 0,
      status: cancelled ? 'error' : 'queued', detail: cancelled ? 'cancelled by user' : undefined } });
  });
  const panel = await open(page);
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await panel.getByRole('button', { name: 'Cancel run', exact: true }).click();
  await expect(panel.getByRole('alert')).toContainText('cancelled by user');
  await panel.getByLabel('Saved run ID').fill('missing-run');
  await panel.getByRole('button', { name: 'Open run', exact: true }).click();
  await expect(panel.getByRole('alert')).toContainText('404');
});
