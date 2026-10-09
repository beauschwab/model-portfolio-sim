import { test, expect } from '@playwright/test';
import { tableFromArrays, tableToIPC } from 'apache-arrow';

function envelope(payload: unknown, frames: Uint8Array[] = []) {
  const json = Buffer.from(JSON.stringify(payload));
  const header = Buffer.alloc(8); header.write('ARW1'); header.writeUInt32LE(json.length, 4);
  const sizes = Buffer.alloc(4 + 4 * frames.length);
  sizes.writeUInt32LE(frames.length);
  frames.forEach((frame, i) => sizes.writeUInt32LE(frame.length, 4 + 4 * i));
  return Buffer.concat([header, json, sizes, ...frames.map(frame => Buffer.from(frame))]);
}

const KPIS = {
  eve: { 'eve_$': 1e9, duration_gap_y: 0.5, dur_assets_y: 2, dur_liab_y: 1.5, 'dv01_net_$': 1e5, irrbb_outlier: false, irrbb_worst_pct_eve: 5,
    sensitivity: [{ shock_bp: 200, d_eve_pct_eve: -5, 'd_eve_$': -5e7, method: 'mock' }] },
  lcr: { lcr_pct: 150, 'hqla_$': 1, 'net_outflows_$': 1 }, nsfr: { nsfr_pct: 120, 'asf_$': 1, 'rsf_$': 1 },
  capital: { 'rwa_total_$': 1, rwa_density_pct: 50, note: '', cet1_path: [{ quarter: 0, cet1_ratio_pct: 12, 'cet1_$': 1, drivers: '' }] },
};

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


test('the graph recomputes only the results whose inputs changed', async ({ page }) => {
  let revision = 1;
  const nodes: Record<string, string> = {
    market: 'm', settings: 's', 'assumptions:other': 'o', context: 'c', 'assumptions:deposits': 'd',
    'assumptions:cds': 'k', scenarios: 'sc', cohorts: 'co',
    ...Object.fromEntries(['mbs', 'loans', 'debt', 'deposits', 'cds', 'mm'].map(b => [`books:${b}`, b])),
  };
  const runs: { kind: string; books?: string[]; priority?: string }[] = [];
  const jobs = new Map<string, { kind: string; books?: string[]; revision: number }>();
  await page.route('**/api/state', route => route.fulfill({
    json: { revision, library_ready: false, library_horizon: null, inputs: { revision, nodes: { ...nodes } } } }));
  await page.route('**/api/run', async route => {
    const body = JSON.parse(route.request().postData() ?? '{}');
    runs.push({ kind: body.kind, books: body.books ?? undefined, priority: body.priority });
    const id = `g-${runs.length}`;
    jobs.set(id, { kind: body.kind, books: body.books ?? undefined, revision });
    await route.fulfill({ json: { id, revision, kind: body.kind, status: 'queued' } });
  });
  await page.route(/\/api\/jobs\/g-\d+$/, route => {
    const id = route.request().url().split('/').pop()!;
    const job = jobs.get(id)!;
    return route.fulfill({ json: { id, revision: job.revision, kind: job.kind, status: 'done' } });
  });
  await page.route(/\/api\/jobs\/g-\d+\/result$/, route => {
    const job = jobs.get(route.request().url().split('/').slice(-2)[0])!;
    const octet = 'application/octet-stream';
    if (job.kind === 'risk') {
      const frame = tableToIPC(tableFromArrays({ dv01: [1], krd01_1y: [1] }));
      // the server omits empty books: cds never comes back, and must still count as computed
      const books = (job.books ?? ['mbs', 'loans', 'debt', 'deposits', 'cds']).filter(b => b !== 'cds');
      const payload: Record<string, unknown> = Object.fromEntries(books.map(b => [b, { __arrow__: 0 }]));
      if (!job.books) payload.hedges = { __arrow__: 0 };
      return route.fulfill({ body: envelope(payload, [frame]), contentType: octet });
    }
    if (job.kind === 'nii') {
      const monthly = tableToIPC(tableFromArrays({ month: [1], nii: [1], interest_income: [1] }));
      const summary = tableToIPC(tableFromArrays({ metric: ['nii_annualized_$'], value: [12] }));
      return route.fulfill({ body: envelope({ monthly: { __arrow__: 0 }, summary: { __arrow__: 1 } }, [monthly, summary]), contentType: octet });
    }
    return route.fulfill({ body: envelope(KPIS), contentType: octet });
  });
  const changeInputs = (edit: () => void) => page.evaluate(() => window.dispatchEvent(new Event('engine:inputs-changed'))).then(() => edit);

  await page.goto('/');
  await expect.poll(() => runs.map(r => r.kind).sort().join(','), { timeout: 15_000 }).toBe('kpis,nii,risk');
  expect(runs.find(r => r.kind === 'risk')!.books).toBeUndefined();   // first risk run covers every book and the hedges
  // automatic refreshes queue behind anything a person asks for
  expect(runs.every(r => r.priority === 'background')).toBe(true);

  // a scenario edit feeds no base result: nothing recomputes
  revision = 2; nodes.scenarios = 'sc2';
  await changeInputs(() => {});
  await page.waitForTimeout(2000);
  expect(runs).toHaveLength(3);

  // a deposit-book edit recomputes KPIs, NII and deposit risk only
  revision = 3; nodes['books:deposits'] = 'deposits2';
  await changeInputs(() => {});
  await expect.poll(() => runs.length, { timeout: 15_000 }).toBe(6);
  await page.waitForTimeout(1500);
  expect(runs.slice(3).map(r => `${r.kind}:${(r.books ?? []).join('+')}`).sort()).toEqual(['kpis:', 'nii:', 'risk:deposits']);

  // the graph view says so
  await page.getByTitle('Open pipeline').click();
  const graph = page.getByRole('table', { name: 'Recalculation graph' });
  await expect(graph.getByRole('row', { name: /Risk · deposits/ })).toContainText('current');
  await expect(graph.getByRole('row', { name: /Risk · hedges/ })).toContainText('current');
});

test('background refreshes leave the run controls usable; a run a person asks for disables them', async ({ page }) => {
  // every job stays running, so the automatic refreshes queued on load never finish
  await page.route('**/api/state', route => route.fulfill({ json: { revision: 1, library_ready: false, library_horizon: null } }));
  await page.route('**/api/run', async route => {
    const body = JSON.parse(route.request().postData() ?? '{}');
    await route.fulfill({ json: { id: `bg-${body.kind}`, revision: 1, kind: body.kind, status: 'running' } });
  });
  await page.route(/\/api\/jobs\/bg-\w+$/, route => route.fulfill({ json: { id: 'bg', revision: 1, kind: 'kpis', status: 'running' } }));
  await page.goto('/');
  const runSheet = page.getByTitle('Run the KPI sheet (⌘K for more)');
  await expect(page.getByText('kpis', { exact: true }).first()).toBeVisible();   // a refresh is running
  await expect(runSheet).toBeEnabled();
  await runSheet.click();
  await expect(runSheet).toBeDisabled();
});
