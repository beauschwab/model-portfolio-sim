import { test, expect } from '@playwright/test';

// Actual same-origin browser JSON spelling, public worker and partition transport.
// Dedicated route has no saved-book/Strategy/UI adoption or regulatory permission.
test('joint deposit and derivative raw sources queues and reopens native mixed journal partitions',async({page})=>{
  test.setTimeout(150_000);
  const hydration:string[]=[];
  page.on('request',r=>{if(/\/jobs\/[^/]+\/result(?:\?|$)/.test(r.url()))hydration.push(r.url());});
  await page.goto('/');
  const queued=await page.evaluate(async()=>{
    const raw=await(await fetch('/api/derivatives/deposit-portfolio/example')).json();
    const state=await(await fetch('/api/state')).json();
    const response=await fetch('/api/derivatives/deposit-portfolio/runs',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({expected_revision:state.revision,specification:raw,timeout:120.})});
    if(!response.ok)throw new Error(await response.text());return response.json();
  });
  await expect.poll(async()=>{const r=await page.request.get(`/api/jobs/${queued.id}`);const v=await r.json();if(v.status==='error')throw new Error(v.detail);return v.status;},{timeout:120_000}).toBe('done');
  const table=await page.request.get(`/api/jobs/${queued.id}/table?path=${encodeURIComponent('/journal')}&offset=0&limit=10`);
  expect(table.ok()).toBeTruthy();expect((await table.json()).total).toBeGreaterThan(0);
  const part=await page.request.get(`/api/jobs/${queued.id}/parquet?path=${encodeURIComponent('/journal')}&partition=0`);
  expect(part.ok()).toBeTruthy();expect((await part.body()).subarray(0,4).toString()).toBe('PAR1');
  await page.reload();
  const reopen=await page.request.get(`/api/jobs/${queued.id}/table?path=${encodeURIComponent('/derivative_journal_lineage')}&offset=0&limit=10`);
  expect(reopen.ok()).toBeTruthy();expect((await reopen.json()).total).toBeGreaterThan(0);
  const deposit=await page.request.get(`/api/jobs/${queued.id}/table?path=${encodeURIComponent('/deposit_principal_lineage')}&offset=0&limit=10`);
  expect(deposit.ok()).toBeTruthy();expect((await deposit.json()).total).toBeGreaterThan(0);
  expect(hydration).toEqual([]);
});
