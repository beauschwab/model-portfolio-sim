import { test, expect } from '@playwright/test';

/** Downstream results (KPIs, risk, NII) recompute when an input changes, with no
 * navigation. The engine is mocked so this runs without the native runtime: the
 * mocked jobs fail with a stated reason, which is enough to observe the requests. */
test('a settings save requests every downstream result again, once, without navigation', async ({ page }) => {
  let revision = 1;
  const runs: string[] = [];
  let jobs = 0;
  await page.route('**/api/state', route => route.fulfill({ json: { revision, library_ready: false, library_horizon: null } }));
  await page.route('**/api/run', async route => {
    const body = JSON.parse(route.request().postData() ?? '{}');
    runs.push(body.kind);
    jobs += 1;
    await route.fulfill({ json: { id: `mock-${jobs}`, revision, kind: body.kind, status: 'queued' } });
  });
  await page.route('**/api/jobs/mock-*', route => route.fulfill({
    json: { id: route.request().url().split('/').pop(), revision, kind: 'mock', status: 'error', detail: 'mocked engine' },
  }));
  await page.route('**/api/settings', route => {
    if (route.request().method() === 'PUT') revision += 1;
    return route.fallback();
  });

  await page.goto('/');
  await expect.poll(() => [...new Set(runs)].sort().join(','), { timeout: 15_000 }).toBe('kpis,nii,risk');
  await page.waitForTimeout(1500);
  const settled = runs.length;

  await page.getByRole('button', { name: /seed/ }).click();
  await page.getByRole('button', { name: 'Save settings', exact: true }).click();

  await expect.poll(() => runs.length, { timeout: 15_000 }).toBeGreaterThan(settled);
  const after = runs.slice(settled);
  expect(new Set(after)).toEqual(new Set(['kpis', 'nii', 'risk']));
  expect(after.length).toBe(3);

  // a failed result is not retried until the inputs change again
  await page.waitForTimeout(2500);
  expect(runs.length).toBe(settled + 3);
});

test('auto-recalculation can be switched off; results then wait for an explicit refresh', async ({ page }) => {
  let revision = 1;
  const runs: string[] = [];
  let jobs = 0;
  await page.route('**/api/state', route => route.fulfill({ json: { revision, library_ready: false, library_horizon: null } }));
  await page.route('**/api/run', async route => {
    const body = JSON.parse(route.request().postData() ?? '{}');
    runs.push(body.kind);
    jobs += 1;
    await route.fulfill({ json: { id: `mock-${jobs}`, revision, kind: body.kind, status: 'queued' } });
  });
  await page.route('**/api/jobs/mock-*', route => route.fulfill({
    json: { id: route.request().url().split('/').pop(), revision, kind: 'mock', status: 'error', detail: 'mocked engine' },
  }));
  await page.route('**/api/settings', route => {
    if (route.request().method() === 'PUT') revision += 1;
    return route.fallback();
  });

  await page.addInitScript(() => localStorage.setItem('engine.autoRecalc', 'off'));
  await page.goto('/');
  await page.waitForTimeout(2000);
  expect(runs.length).toBe(0);

  await page.getByRole('button', { name: /seed/ }).click();
  await page.getByRole('button', { name: 'Save settings', exact: true }).click();
  await page.waitForTimeout(2000);
  expect(runs.length).toBe(0);
});
