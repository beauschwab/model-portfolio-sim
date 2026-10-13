import { test, expect, type Page } from '@playwright/test';

test('revised deposit expectations retain original yield in a durable verified journal', async ({page}) => {
  test.setTimeout(90_000);
  const errors:string[]=[], fullResults:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(/\/jobs\/[^/]+\/result$/.test(new URL(r.url()).pathname))fullResults.push(r.url());});
  const panel=await open(page);
  await panel.getByLabel('Cashflow timing',{exact:true}).selectOption('deposit_revised');
  await expect(panel.getByText(/Purchase-asof physical deposit histories retain/)).toBeVisible();
  await panel.getByRole('button',{name:'Run partitioned simulation',exact:true}).click();
  await expect(panel.getByText('Original yield revision and gross cash origins verified',{exact:true})).toBeVisible({timeout:60_000});
  await expect(panel.getByText('Saved journal verified',{exact:true})).toBeVisible();
  await expect(panel.getByText('Reviewed research policy; capital and FTP unavailable',{exact:true})).toBeVisible();
  await panel.getByLabel('Partitioned result table',{exact:true}).selectOption('/deposit_revision');
  await expect(panel.getByRole('table',{name:'Partitioned results',exact:true}).getByRole('columnheader',{name:'basis revision',exact:true})).toBeVisible();
  const id=await panel.getByLabel('Saved run ID',{exact:true}).inputValue();
  await page.reload();
  const restored=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
  await restored.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID',{exact:true})).toHaveValue(id);
  await expect(restored.getByText('Original yield revision and gross cash origins verified',{exact:true})).toBeVisible();
  expect(errors).toEqual([]);expect(fullResults).toEqual([]);
});

async function open(page: Page) {
  await page.goto('/');
  const button = page.getByRole('button', { name: 'Open Balance-sheet Stress', exact: true });
  await button.click();
  const panel = page.getByRole('region', { name: 'Balance-sheet Stress', exact: true });
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
  await page.getByRole('button',{name:'Open Balance-sheet Stress',exact:true}).click();
  const panel=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
  await panel.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(panel.getByLabel('Partitioned engine',{exact:true}).locator('option')).toHaveText('Rust — unavailable');
  await expect(panel.getByRole('button',{name:'Run partitioned simulation',exact:true})).toBeDisabled();
  await expect(panel.getByLabel('Partitioned engine',{exact:true})).toHaveValue('rust');
  await expect(panel.locator('option[value="python"]')).toHaveCount(0);
  await page.unrouteAll({behavior:'wait'});
});

test('actual-date Rust run retains source evidence when reopened', async ({ page }) => {
  const errors: string[] = [], fullResults: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('request', r => { if (/\/jobs\/[^/]+\/result$/.test(r.url())) fullResults.push(r.url()); });
  const panel = await open(page);
  await panel.getByLabel('Cashflow timing', { exact: true }).selectOption('dated');
  await panel.getByText('Edit partitioned specification (JSON)', { exact: true }).click();
  const editor = panel.getByLabel('Explicit balance-sheet specification');
  const raw = JSON.parse(await editor.inputValue());
  expect(raw.schema).toBe('dated-ledger-1');
  await editor.fill('');
  await expect(panel.getByRole('button', { name: 'Run partitioned simulation', exact: true })).toBeDisabled();
  await editor.fill(JSON.stringify(raw));
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Dated source reconciled', { exact: true })).toBeVisible({ timeout: 40_000 });
  await expect(panel.getByText('Saved journal verified', { exact: true })).toBeVisible();
  const id = await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/event_lineage');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('recognition');
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/dated_rights');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('source');
  await page.reload();
  const restored = page.getByRole('region', { name: 'Balance-sheet Stress', exact: true });
  await restored.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID', { exact: true })).toHaveValue(id);
  await expect(restored.getByText('Dated source reconciled', { exact: true })).toBeVisible();
  expect(fullResults).toEqual([]); expect(errors).toEqual([]);
});

test('daily credit run publishes states and complete claims through Rust', async ({ page }) => {
  const panel = await open(page);
  await panel.getByLabel('Cashflow timing', { exact: true }).selectOption('credit');
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Credit source reconciled', { exact: true })).toBeVisible({ timeout: 40_000 });
  await expect(panel.getByText('Saved journal verified', { exact: true })).toBeVisible();
  await expect(panel.getByText('Daily regulatory limits not evaluated', { exact: true })).toBeVisible();
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/credit_states');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('loan-1');
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/credit_claims');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('recovery tail after reporting');
  const id = await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  await page.reload();
  const restored = page.getByRole('region', { name: 'Balance-sheet Stress', exact: true });
  await restored.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID', { exact: true })).toHaveValue(id);
  await expect(restored.getByText('Credit source reconciled', { exact: true })).toBeVisible();
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
  const restored = page.getByRole('region', { name: 'Balance-sheet Stress', exact: true });
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


test('realized physical credit paths publish verified rights and reopen', async ({ page }) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  const panel = await open(page);
  await panel.getByLabel('Cashflow timing', { exact: true }).selectOption('credit_path');
  await panel.getByText('Edit partitioned specification (JSON)', { exact: true }).click();
  const editor=panel.getByLabel('Explicit balance-sheet specification', { exact: true });
  await expect(editor).toContainText('credit-path-ledger-1');
  const raw=JSON.parse(await editor.inputValue());
  await editor.fill(JSON.stringify(raw,null,2));
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Realized credit paths reconciled', { exact: true })).toBeVisible({ timeout: 40000 });
  await expect(panel.getByText('Saved journal verified', { exact: true })).toBeVisible();
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/credit_path_daily');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('loan');
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/credit_path_claims');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('claim');
  const id=await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  const download=page.waitForEvent('download');
  await panel.getByRole('button', {name:'Download partition',exact:true}).click();
  expect((await download).suggestedFilename()).toContain(`${id}-credit_path_claims-0.parquet`);
  await page.reload();
  const restored=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
  await restored.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID',{exact:true})).toHaveValue(id);
  await expect(restored.getByText('Realized credit paths reconciled',{exact:true})).toBeVisible();
  expect(errors).toEqual([]);
});


for(const timing of ['graph','driven','deposit_graph','deposit_income']) test(`native ${timing} sources reach verified daily replay`,async({page})=>{
  const panel=await open(page);
  await panel.getByLabel('Cashflow timing',{exact:true}).selectOption(timing);
  await panel.getByRole('button',{name:'Run partitioned simulation',exact:true}).click();
  const badge=timing==='graph'?'Consumer source economics verified':timing==='deposit_graph'?'Deposit source economics verified':timing==='deposit_income'?'Original deposit yield and basis verified':'Credit model and drivers verified';
  await expect(panel.getByText(badge,{exact:true})).toBeVisible({timeout:40000});
  await expect(panel.getByText('Saved journal verified',{exact:true})).toBeVisible();
  await panel.getByLabel('Partitioned result table',{exact:true}).selectOption(timing==='driven'?'/credit_path_lineage':'/event_lineage');
  await expect(panel.getByRole('table',{name:'Partitioned results',exact:true})).toContainText(timing==='graph'?'personal-1':timing.startsWith('deposit_')?'deposit':'loan');
  const id=await panel.getByLabel('Saved run ID',{exact:true}).inputValue();
  await page.reload();const restored=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
  await restored.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID',{exact:true})).toHaveValue(id);
  await expect(restored.getByText(badge,{exact:true})).toBeVisible();
});

test('mortgage expected accounting publishes one verified journal with its research scope', async ({ page }) => {
  test.setTimeout(90_000);
  const errors: string[] = [], fullResults: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (/\/jobs\/[^/]+\/result$/.test(new URL(request.url()).pathname)) fullResults.push(request.url());
  });
  const panel = await open(page);
  await panel.getByLabel('Cashflow timing', { exact: true }).selectOption('mortgage_mixed');
  await expect(panel.getByText(/Research policy only; no accounting approval, saved-book adoption, capital or FTP eligibility/)).toBeVisible();
  await panel.getByText('Edit partitioned specification (JSON)', { exact: true }).click();
  const editor = panel.getByLabel('Explicit balance-sheet specification', { exact: true });
  await expect(editor).toContainText('mortgage-mixed-accounting-1');
  const savedManifest = page.waitForResponse(response =>
    /\/jobs\/[^/]+\/manifest$/.test(new URL(response.url()).pathname) && response.status() === 200,
    { timeout: 60_000 });
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Mortgage source economics verified', { exact: true })).toBeVisible({ timeout: 60_000 });
  await expect(panel.getByText('Saved journal verified', { exact: true })).toBeVisible();
  await expect(panel.getByText('Pending rights and memo phase verified', { exact: true })).toBeVisible();
  await expect(panel.getByText('Research journal only; capital and FTP unavailable', { exact: true })).toBeVisible();
  const manifest = await (await savedManifest).json();
  expect(manifest.execution.model_version).toBe('mortgage-mixed-accounting-1');
  expect(manifest.execution.validation).toMatchObject({
    verified: true, native_mortgage_source_recomputed: true, single_daily_gl_verified: true,
    ordered_mixed_journal_recomputed: true, pending_memo_phase_verified: true,
    complete_unweighted_tail_verified: true, production_adoption: false,
    mixed_ledger_adoption: false, accounting_standard_approval: false,
    capital_ftp_eligible: false, closing_capital_eligible: false,
  });
  expect(manifest.execution.validation.journal_replayed).toBeUndefined();
  expect(manifest.execution.validation.dated_source_reconciled).toBeUndefined();
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/mortgage_daily');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('physical-sequential-path');
  await panel.getByLabel('Partitioned result table', { exact: true }).selectOption('/mortgage_lineage');
  await expect(panel.getByRole('table', { name: 'Partitioned results', exact: true })).toContainText('immutable-native-source-1');
  const id = await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  const download = page.waitForEvent('download');
  await panel.getByRole('button', { name: 'Download partition', exact: true }).click();
  expect((await download).suggestedFilename()).toContain(`${id}-mortgage_lineage-0.parquet`);
  await page.reload();
  const restored = page.getByRole('region', { name: 'Balance-sheet Stress', exact: true });
  await restored.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID', { exact: true })).toHaveValue(id);
  await expect(restored.getByText('Mortgage source economics verified', { exact: true })).toBeVisible();
  await expect(restored.getByText('Research journal only; capital and FTP unavailable', { exact: true })).toBeVisible();
  expect(fullResults).toEqual([]);
  expect(errors).toEqual([]);
});

// Installed/default-stack successor gate; no component-only compatibility claim.
test('deposit purchase prefix and revised future preserve verified gross rights on reopen',async({page})=>{
  test.setTimeout(90000);const errors:string[]=[],fullResults:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(/\/jobs\/[^/]+\/result$/.test(new URL(r.url()).pathname))fullResults.push(r.url());});
  const panel=await open(page);await panel.getByLabel('Cashflow timing',{exact:true}).selectOption('deposit_continuation');
  await expect(panel.getByText(/Complete supplied purchase, observed prefix and revised physical histories/)).toBeVisible();
  await panel.getByRole('button',{name:'Run partitioned simulation',exact:true}).click();
  await expect(panel.getByText('Original anchors, native principal and gross cash origins verified',{exact:true})).toBeVisible({timeout:60000});
  await expect(panel.getByText('Saved journal verified',{exact:true})).toBeVisible();
  await expect(panel.getByText('Reviewed research policy; capital and FTP unavailable',{exact:true})).toBeVisible();
  await panel.getByLabel('Partitioned result table',{exact:true}).selectOption('/deposit_principal_lineage');
  await expect(panel.getByRole('table',{name:'Partitioned results',exact:true})).toContainText('deposit');
  const id=await panel.getByLabel('Saved run ID',{exact:true}).inputValue();
  const download=page.waitForEvent('download');await panel.getByRole('button',{name:'Download partition',exact:true}).click();
  expect((await download).suggestedFilename()).toContain(`${id}-deposit_principal_lineage-0.parquet`);
  await page.reload();const restored=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
  await restored.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
  await expect(restored.getByLabel('Saved run ID',{exact:true})).toHaveValue(id);
  await expect(restored.getByText('Original anchors, native principal and gross cash origins verified',{exact:true})).toBeVisible();
  expect(errors).toEqual([]);expect(fullResults).toEqual([]);
});

test('multiple performing deposit sources retain pooled verified journal on durable reopen',async({page})=>{
 test.setTimeout(120000);const errors:string[]=[],full:string[]=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\/jobs\/[^/]+\/result$/.test(new URL(r.url()).pathname))full.push(r.url());});
 const panel=await open(page);await panel.getByLabel('Cashflow timing',{exact:true}).selectOption('deposit_portfolio');
 await expect(panel.getByText(/Explicit owners and complete baseline\/named physical paths share one account journal/)).toBeVisible();
 await panel.getByRole('button',{name:'Run partitioned simulation',exact:true}).click();
 await expect(panel.getByText('Complete owner sources and pooled gross-right journal verified',{exact:true})).toBeVisible({timeout:90000});
 await expect(panel.getByText('Saved journal verified',{exact:true})).toBeVisible();
 await panel.getByLabel('Partitioned result table',{exact:true}).selectOption('/deposit_revision');
 await expect(panel.getByRole('table',{name:'Partitioned results',exact:true})).toContainText('deposit-2');
 const id=await panel.getByLabel('Saved run ID',{exact:true}).inputValue();
 const download=page.waitForEvent('download');await panel.getByRole('button',{name:'Download partition',exact:true}).click();
 expect((await download).suggestedFilename()).toContain(`${id}-deposit_revision-0.parquet`);
 await page.reload();const restored=page.getByRole('region',{name:'Balance-sheet Stress',exact:true});
 await restored.getByLabel('Simulation workflow',{exact:true}).selectOption('partitioned');
 await expect(restored.getByLabel('Saved run ID',{exact:true})).toHaveValue(id);
 await expect(restored.getByText('Complete owner sources and pooled gross-right journal verified',{exact:true})).toBeVisible();
 await expect(restored.getByText('Reviewed research policy; capital and FTP unavailable',{exact:true})).toBeVisible();
 expect(errors).toEqual([]);expect(full).toEqual([]);
});
