import { test, expect } from '@playwright/test';
import { tableFromArrays, tableToIPC } from 'apache-arrow';

function envelope(payload: unknown, frames: Uint8Array[] = []) {
  const json = Buffer.from(JSON.stringify(payload)); const h = Buffer.alloc(8);
  h.write('ARW1'); h.writeUInt32LE(json.length, 4);
  const sizes = Buffer.alloc(4 + 4*frames.length); sizes.writeUInt32LE(frames.length);
  frames.forEach((f,i) => sizes.writeUInt32LE(f.length, 4+4*i));
  return Buffer.concat([h,json,sizes,...frames.map(f => Buffer.from(f))]);
}

test('real instrument what-if preserves saved book and exposes risk and recalibration', async ({ page, request }) => {
  const errors: string[] = []; page.on('pageerror', e => errors.push(e.message));
  const original = await (await request.get('/api/books/loans')).body();
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Decide', exact: true }).click();
  await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'What-if', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Decide', exact: true });
  await expect(panel.getByRole('combobox', { name: 'What-if instrument' })).not.toBeEmpty();
  await panel.getByRole('button', { name: 'Compare', exact: true }).click();
  await expect(panel.getByTestId('value-change')).toHaveText('$0', { timeout: 50_000 });
  const current = await panel.getByLabel('Assumption value', { exact: true }).getAttribute('placeholder');
  await panel.getByLabel('Assumption value', { exact: true }).fill(String(Number(current)+1));
  await panel.getByRole('button', { name: 'Apply temporary change', exact: true }).click();
  await expect(panel.getByText('Updating comparison; previous results are hidden.')).toBeVisible();
  await expect(panel.getByTestId('value-change')).not.toHaveText('$0', { timeout: 50_000 });
  await expect(panel.getByText('Risk and earnings impact', { exact: true })).toBeVisible();
  await panel.getByText('Calibration and saved changes', { exact: true }).click();
  await panel.getByRole('button', { name: 'Recalibrate comparison', exact: true }).click();
  await expect(panel.getByTestId('value-change')).toHaveText('$0', { timeout: 50_000 });
  expect(await (await request.get('/api/books/loans')).body()).toEqual(original);
  await page.screenshot({ path: 'test-results/whatif-real-desktop.png', fullPage: true });
  expect(errors).toEqual([]);
});

test('obsolete what-if response is hidden and latest inputs determine results', async ({ page }) => {
  // counts only what-if requests; automatic downstream recalculation is covered by downstream.spec
  await page.addInitScript(() => localStorage.setItem('engine.autoRecalc', 'off'));
  let started!: () => void, release!: () => void;
  const first = new Promise<void>(r => { started=r; }); const gate = new Promise<void>(r => { release=r; });
  let count=0;
  await page.route('**/api/state', r => r.fulfill({ json: { revision: 77, library_ready: false, library_horizon: null } }));
  await page.route('**/api/run', r => { const id=String(++count); return r.fulfill({ json: { id,kind:'whatif',status:'done',revision:77 } }); });
  await page.route(/\/api\/jobs\/\d+$/, r => r.fulfill({ json: { id:r.request().url().split('/').pop(),kind:'whatif',status:'done',revision:77 } }));
  await page.route(/\/api\/jobs\/\d+\/result$/, async r => {
    const old = r.request().url().includes('/jobs/1/');
    if(old) { started(); await gate; }
    const amount=old?111e6:222e6;
    const frame=tableToIPC(tableFromArrays({id:['CML0000'],original_price:[100],model_price:[99],value_change:[-1],base_oas_bp:[10]}));
    const valuation={positions:{loans:{__arrow__:0}},scope_net_value:amount,graph:{}};
    await r.fulfill({body:envelope({baseline:valuation,revised:valuation,comparison:{loans:{__arrow__:0}},calibration_mode:'hold',revision:77,net_value_change:amount},[frame]),contentType:'application/octet-stream'});
  });
  await page.goto('/');
  await page.getByRole('button', {name:'Open Decide',exact:true}).click();
  await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'What-if', exact: true }).click();
  const panel=page.getByRole('region',{name:'Decide',exact:true});
  await panel.getByLabel('Include risk and earnings', {exact:true}).uncheck();
  await expect(panel.getByRole('button',{name:'Compare',exact:true})).toBeEnabled();
  await panel.getByRole('button',{name:'Compare',exact:true}).click();
  await first;
  await panel.getByLabel('Instrument spread shift',{exact:true}).fill('25');
  await panel.getByRole('button',{name:'Apply temporary change',exact:true}).click();
  release();
  await expect(panel.getByTestId('value-change')).toHaveText('$222.0M');
  await expect(panel.getByText('$111.0M',{exact:true})).toHaveCount(0);
  expect(count).toBe(2);
});

test('what-if controls remain usable in a narrow dock panel', async ({page}) => {
  await page.setViewportSize({width:850,height:900});
  await page.goto('/');
  await page.getByRole('button',{name:'Open Decide',exact:true}).click();
  await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'What-if', exact: true }).click();
  const panel=page.getByRole('region',{name:'Decide',exact:true});
  await expect(panel.getByLabel('Assumption value',{exact:true})).toBeVisible();
  await panel.getByLabel('Assumption value',{exact:true}).fill('1000');
  await expect(panel.getByRole('button',{name:'Apply temporary change',exact:true})).toBeDisabled();
  await expect(panel.getByRole('alert')).toHaveText('Enter a value within the supported assumption range.');
  await page.screenshot({path:'test-results/whatif-narrow.png',fullPage:true});
});
