import { test, expect } from '@playwright/test';

test('real decision build, selective edit, template update, optimization and replay', async ({ page, request }) => {
  test.setTimeout(120_000);
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  const original = await (await request.get('/api/settings')).json();
  const savedBook = await (await request.get('/api/books/loans')).body();
  await request.put('/api/settings', { data: { ...original, n_paths: 32, n_paths_base: 32, n_threads: 2, horizon_months: 6 } });
  try {
    await page.goto('/');
    await page.getByRole('button', { name: 'Open Decide', exact: true }).click();
    await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'Validate', exact: true }).click();
    const panel = page.getByRole('region', { name: 'Decide', exact: true });
    await panel.getByLabel('Minimum LCR (%)', { exact: true }).fill('1');
    await panel.getByLabel('Minimum NSFR (%)', { exact: true }).fill('1');
    await panel.getByLabel('Minimum CET1 (%)', { exact: true }).fill('0.1');
    await panel.getByLabel('Maximum EVE change (%)', { exact: true }).fill('100');
    await panel.getByLabel('New asset cap ($m)', { exact: true }).fill('10');
    await panel.getByLabel('Outside-book cash budget ($m)', { exact: true }).fill('10');
    await panel.getByRole('button', { name: 'Build decision session', exact: true }).click();
    await expect(panel.getByText('Version 1 · 375 instruments', { exact: true })).toBeVisible({ timeout: 60_000 });
    await expect(panel.getByText('Independent Python allocation replay passed', { exact: true })).toBeVisible();
    await panel.getByLabel('New asset cap ($m)', { exact: true }).fill('5');
    await panel.getByRole('button', { name: 'Apply and optimize', exact: true }).click();
    await expect(panel.getByText('Version 2 · 375 instruments', { exact: true })).toBeVisible();
    await panel.getByLabel('Include instrument edit', { exact: true }).check();
    await panel.getByLabel('Decision assumption value', { exact: true }).fill('0.065');
    await panel.getByRole('button', { name: 'Apply and optimize', exact: true }).click();
    await expect(panel.getByText('Version 3 · 375 instruments', { exact: true })).toBeVisible();
    await expect(panel.getByText(/Changed instruments: loans:/)).toBeVisible();
    await panel.getByLabel('Include instrument edit', { exact: true }).uncheck();
    await panel.getByLabel('Include template edit', { exact: true }).check();
    await panel.getByRole('button', { name: 'Apply and optimize', exact: true }).click();
    await expect(panel.getByText('Version 4 · 375 instruments', { exact: true })).toBeVisible();
    await panel.getByLabel('Allocation scale (%)', { exact: true }).fill('50');
    await panel.getByRole('button', { name: 'Replay allocation', exact: true }).click();
    await expect(panel.getByText('Manual allocation replay', { exact: true })).toBeVisible();
    expect(await (await request.get('/api/books/loans')).body()).toEqual(savedBook);
    await page.screenshot({ path: 'test-results/decision-real.png', fullPage: true });
    await request.put('/api/settings', { data: original });
    await panel.getByRole('button', { name: 'Apply and optimize', exact: true }).click();
    await expect(panel.getByRole('alert')).toContainText('Saved inputs changed');
    expect(errors).toEqual([]);
  } finally { await request.put('/api/settings', { data: original }); }
});

test('decision panel suppresses a build result invalidated by saved inputs', async ({ page }) => {
  let release!: () => void, entered!: () => void;
  const gate = new Promise<void>(r => release = r), started = new Promise<void>(r => entered = r);
  const payload = { session_id: 'obsolete', revision: 0, version: 1 };
  await page.route('**/api/decision/sessions', r => r.fulfill({ json: { id: 'obsolete', status: 'done' } }));
  await page.route('**/api/jobs/obsolete', r => r.fulfill({ json: { id: 'obsolete', status: 'done' } }));
  await page.route('**/api/jobs/obsolete/result', async r => {
    entered(); await gate;
    const json = Buffer.from(JSON.stringify(payload)), header = Buffer.alloc(8), frames = Buffer.alloc(4);
    header.write('ARW1'); header.writeUInt32LE(json.length, 4);
    await r.fulfill({ body: Buffer.concat([header, json, frames]), contentType: 'application/octet-stream' });
  });
  await page.route('**/api/decision/sessions/obsolete', r => r.fulfill({ json: { closed: true } }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Decide', exact: true }).click();
  await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'Validate', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Decide', exact: true });
  await panel.getByRole('button', { name: 'Build decision session', exact: true }).click();
  await started;
  await page.evaluate(() => window.dispatchEvent(new Event('engine:inputs-changed')));
  release();
  await expect(panel.getByRole('status')).toContainText('Saved inputs changed');
  await expect(panel.getByRole('button', { name: 'Apply and optimize', exact: true })).toBeDisabled();
  await expect(panel.getByText('Published allocation', { exact: true })).toHaveCount(0);
});
