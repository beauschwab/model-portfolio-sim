import { test, expect } from '@playwright/test';

test('raw modeled sources publish native FTP offsets and reopen bounded verified journal partitions',async({page})=>{
  test.setTimeout(150_000);
  const failures:string[]=[];const hydration:string[]=[];
  page.on('pageerror',e=>failures.push(e.message));
  page.on('request',r=>{if(/\/jobs\/[^/]+\/result(?:\?|$)/.test(r.url()))hydration.push(r.url());});
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('modeled_products');
  await panel.getByRole('button',{name:'Load modeled-product example',exact:true}).click();
  const input=panel.getByLabel('Capital policy, scenario snapshots and FTP inputs');
  await expect(input).toContainText('modeled-product-ftp-preparation-1');
  // Browser integer spelling must traverse the original Value/DTO boundary.
  await input.fill(JSON.stringify(JSON.parse(await input.inputValue())));
  await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:100_000});
  const id=await panel.getByLabel('Saved modeled FTP report ID').inputValue();expect(id).not.toBe('');
  await expect(panel.getByText('Verified modeled-source journal',{exact:true})).toBeVisible();
  await expect(panel.getByText('Retained native source completion',{exact:true})).toBeVisible();
  const journal=await page.request.get(`/api/jobs/${id}/table?path=${encodeURIComponent('/tables/source_journal')}&offset=0&limit=100`);
  expect(journal.ok()).toBeTruthy();const data=await journal.json();expect(data.total).toBeGreaterThan(0);
  const download=await page.request.get(`/api/jobs/${id}/parquet?path=${encodeURIComponent('/tables/source_journal')}`);
  expect(download.ok()).toBeTruthy();expect((await download.body()).subarray(0,4).toString()).toBe('PAR1');
  await page.reload();await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await reopened.getByLabel('Treasury input source').selectOption('modeled_products');
  await reopened.getByLabel('Saved modeled FTP report ID').fill(id);
  await reopened.getByRole('button',{name:'Reopen modeled FTP report',exact:true}).click();
  await expect(reopened.getByText('Verified modeled-source journal',{exact:true})).toBeVisible({timeout:30_000});
  expect(failures).toEqual([]);expect(hydration).toEqual([]);
});


for(const kind of ['card_v2','deposit_performing'] as const){
  test(`completed source FTP ${kind} worker source, explicit review, native publication and reopen`,async({page})=>{
    test.setTimeout(180_000);
    const {readFileSync}=await import('node:fs');const {resolve}=await import('node:path');
    const fixture=JSON.parse(readFileSync(resolve(process.cwd(),'../../packages/portfolio-risk/tests/fixtures/modeled_product_ftp',kind==='card_v2'?'card-reviewed-gross-funding.json':'deposit-continuation.json'),'utf8'));
    const selected=fixture.source.sources[0];const raw=selected.input.request;
    const state=await(await page.request.get('/api/state')).json();
    const submitted=await page.request.post(kind==='card_v2'?'/api/models/runs':'/api/balance-stress/modeled',{data:kind==='card_v2'?{schema_name:raw.schema,request:raw,expected_revision:state.revision,timeout:120.}:{specification:raw,expected_revision:state.revision,large_book:false}});
    expect(submitted.ok(),await submitted.text()).toBeTruthy();const sourceJob=(await submitted.json()).id;
    await expect.poll(async()=>{const r=await(await page.request.get(`/api/jobs/${sourceJob}`)).json();if(r.status==='failed')throw new Error(r.detail);return r.status;},{timeout:100_000}).toBe('done');
    const hydration:string[]=[];page.on('request',r=>{if(/\/jobs\/[^/]+\/(?:result|manifest)(?:\?|$)/.test(r.url()))hydration.push(r.url());});
    await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
    const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});await panel.getByLabel('Treasury input source').selectOption('completed_products');
    await panel.getByLabel('FTP completed source kind 1').selectOption(kind);
    await panel.getByLabel('FTP completed source job 1').fill('unknown-completed-job');
    await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();await expect(panel.getByRole('alert')).toContainText('unknown');
    await panel.getByLabel('FTP completed source job 1').fill(sourceJob);
    await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();
    await expect(panel.getByTestId('ftp-source-metadata-1')).toContainText('Completed source metadata checked');
    await panel.getByLabel('FTP source alias 1').fill(selected.source_id);
    await panel.getByLabel('FTP scope review ID 1').fill(selected.scope_review.id);
    await panel.getByLabel('FTP scope review revision 1').fill(selected.scope_review.revision);
    delete fixture.source.sources;
    await panel.getByLabel('Reviewed funding policy and indexed source reviews').fill(JSON.stringify(fixture));
    // Changing the ID invalidates previously checked metadata rather than using a stale selection.
    await panel.getByLabel('FTP completed source job 1').fill('unknown-after-check');
    await expect(panel.getByRole('button',{name:'Run capital & FTP report',exact:true})).toBeDisabled();
    await panel.getByLabel('FTP completed source job 1').fill(sourceJob);await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();
    await expect(panel.getByTestId('ftp-source-metadata-1')).toBeVisible();
    const ftpQueued=page.waitForResponse(r=>r.url().endsWith('/treasury/modeled-product-ftp/sources')&&r.request().method()==='POST');
    await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
    const ftpId=(await(await ftpQueued).json()).id;
    if(process.env.CONTAINED_LATE_FTP_FAILURE==='1'){
      await expect(panel.getByRole('alert')).toContainText('contained late storage publication refusal',{timeout:100_000});
      await expect(panel.getByRole('status')).toHaveCount(0);
      const refused=await(await page.request.get(`/api/jobs/${ftpId}`)).json();expect(refused.status).toBe('error');
      const noResult=await page.request.get(`/api/jobs/${ftpId}/table?path=${encodeURIComponent('/tables/source_journal')}`);expect(noResult.ok()).toBeFalsy();
      expect(hydration).toEqual([]);return;
    }
    await expect.poll(async()=>{if(await panel.getByRole('alert').count())throw new Error(await panel.getByRole('alert').innerText());return panel.getByRole('status').count();},{timeout:100_000}).toBe(1);
    await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:100_000});
    const id=await panel.getByLabel('Saved modeled FTP report ID').inputValue();expect(id).not.toBe('');
    const report=await(await page.request.get(`/api/jobs/${id}`)).json();expect(report.status).toBe('done');
    const journal=await page.request.get(`/api/jobs/${id}/table?path=${encodeURIComponent('/tables/source_journal')}&offset=0&limit=100`);
    expect(journal.ok()).toBeTruthy();expect((await journal.json()).total).toBeGreaterThan(0);
    const download=await page.request.get(`/api/jobs/${id}/parquet?path=${encodeURIComponent('/tables/source_journal')}`);expect(download.ok()).toBeTruthy();expect((await download.body()).subarray(0,4).toString()).toBe('PAR1');
    await page.reload();await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
    const reopened=page.getByRole('region',{name:'Capital & FTP',exact:true});await reopened.getByLabel('Treasury input source').selectOption('completed_products');
    await reopened.getByLabel('Saved modeled FTP report ID').fill(id);await reopened.getByRole('button',{name:'Reopen modeled FTP report',exact:true}).click();
    await expect(reopened.getByText('Verified modeled-source journal',{exact:true})).toBeVisible({timeout:30_000});expect(hydration).toEqual([]);
  });
}


test('two completed source families retain explicit indexed reviews and native FTP offsets',async({page})=>{
  test.skip(process.env.CONTAINED_LATE_FTP_FAILURE==='1','Separate happy-path pooling gate');test.setTimeout(200_000);
  const {readFileSync}=await import('node:fs');const {resolve}=await import('node:path');
  const policies=['card-reviewed-gross-funding.json','deposit-continuation.json'].map(f=>JSON.parse(readFileSync(resolve(process.cwd(),'../../packages/portfolio-risk/tests/fixtures/modeled_product_ftp',f),'utf8')));
  const kinds=['card_v2','deposit_performing']as const;const jobs:string[]=[];const selections=policies.map(p=>p.source.sources[0]);
  const state=await(await page.request.get('/api/state')).json();
  for(let i=0;i<2;i++){
    const raw=selections[i].input.request;const r=await page.request.post(i===0?'/api/models/runs':'/api/balance-stress/modeled',{data:i===0?{schema_name:raw.schema,request:raw,expected_revision:state.revision,timeout:120}:{specification:raw,expected_revision:state.revision,large_book:false}});
    expect(r.ok(),await r.text()).toBeTruthy();jobs.push((await r.json()).id);
    await expect.poll(async()=>{const j=await(await page.request.get(`/api/jobs/${jobs[i]}`)).json();if(j.status==='error')throw new Error(j.detail);return j.status;},{timeout:100_000}).toBe('done');
  }
  const hydration:string[]=[];page.on('request',r=>{if(/\/jobs\/[^/]+\/(?:result|manifest)(?:\?|$)/.test(r.url()))hydration.push(r.url());});
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('completed_products');await panel.getByRole('button',{name:'Add second completed source',exact:true}).click();
  for(let i=0;i<2;i++){
    await panel.getByLabel(`FTP completed source kind ${i+1}`).selectOption(kinds[i]);await panel.getByLabel(`FTP completed source job ${i+1}`).fill(jobs[i]);
    await panel.getByRole('button',{name:`Check completed source ${i+1}`,exact:true}).click();await expect(panel.getByTestId(`ftp-source-metadata-${i+1}`)).toBeVisible();
    await panel.getByLabel(`FTP source alias ${i+1}`).fill(selections[i].source_id);await panel.getByLabel(`FTP scope review ID ${i+1}`).fill(selections[i].scope_review.id);await panel.getByLabel(`FTP scope review revision ${i+1}`).fill(selections[i].scope_review.revision);
  }
  // Combine supplied reviews and charges; no rates, cash or financial outputs are calculated here.
  expect(policies[0].curves).toEqual(policies[1].curves);const policy=structuredClone(policies[0]);delete policy.source.sources;
  policy.source.reviews=[...policies[0].source.reviews,...policies[1].source.reviews];policy.charges=[...policies[0].charges,...policies[1].charges];
  await panel.getByLabel('Reviewed funding policy and indexed source reviews').fill(JSON.stringify(policy));await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect.poll(async()=>{if(await panel.getByRole('alert').count())throw new Error(await panel.getByRole('alert').innerText());return panel.getByRole('status').count();},{timeout:100_000}).toBe(1);
  const id=await panel.getByLabel('Saved modeled FTP report ID').inputValue();const response=await page.request.get(`/api/jobs/${id}/table?path=${encodeURIComponent('/tables/ftp_positions')}&offset=0&limit=100`);
  expect(response.ok()).toBeTruthy();const table=await response.json();expect(table.total).toBeGreaterThan(1);
  const wire=JSON.stringify(table);for(const s of selections)expect(wire).toContain(s.source_id);expect(hydration).toEqual([]);
});

for(const caseName of ['two40','one90'] as const){
  const kind='deposit_portfolio' as const;
  test(`completed multi-owner source2 FTP ${caseName} worker source, explicit review, native publication and reopen`,async({page})=>{
    test.setTimeout(240_000);
    const {readFileSync}=await import('node:fs');const {resolve}=await import('node:path');
    const fixture=JSON.parse(readFileSync(resolve(process.cwd(),'../../packages/portfolio-risk/tests/fixtures/modeled_product_ftp',`deposit-portfolio-${caseName}.json`),'utf8'));
    const selected=fixture.source.sources[0];const raw=selected.input.request;
    const state=await(await page.request.get('/api/state')).json();
    const submitted=await page.request.post('/api/balance-stress/modeled',{data:{specification:raw,expected_revision:state.revision,large_book:false}});
    expect(submitted.ok(),await submitted.text()).toBeTruthy();const sourceJob=(await submitted.json()).id;
    await expect.poll(async()=>{const r=await(await page.request.get(`/api/jobs/${sourceJob}`)).json();if(r.status==='failed'||r.status==='error')throw new Error(r.detail);return r.status;},{timeout:100_000}).toBe('done');
    const hydration:string[]=[];page.on('request',r=>{if(/\/jobs\/[^/]+\/(?:result|manifest)(?:\?|$)/.test(r.url()))hydration.push(r.url());});
    await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
    const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});await panel.getByLabel('Treasury input source').selectOption('completed_products');
    await panel.getByLabel('FTP completed source kind 1').selectOption(kind);
    await panel.getByLabel('FTP completed source job 1').fill('unknown-completed-job');
    await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();await expect(panel.getByRole('alert')).toContainText('unknown');
    await panel.getByLabel('FTP completed source job 1').fill(sourceJob);
    await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();
    await expect(panel.getByTestId('ftp-source-metadata-1')).toContainText('Completed source metadata checked');
    await expect(panel.getByTestId('ftp-source-metadata-1').getByRole('listitem')).toHaveCount(raw.positions.length*raw.runs.length);
    await panel.getByLabel('FTP source alias 1').fill(selected.source_id);
    await panel.getByLabel('FTP scope review ID 1').fill(selected.scope_review.id);
    await panel.getByLabel('FTP scope review revision 1').fill(selected.scope_review.revision);
    delete fixture.source.sources;
    await panel.getByLabel('Reviewed funding policy and indexed source reviews').fill(JSON.stringify(fixture));
    // Changing the ID invalidates previously checked metadata rather than using a stale selection.
    await panel.getByLabel('FTP completed source job 1').fill('unknown-after-check');
    await expect(panel.getByRole('button',{name:'Run capital & FTP report',exact:true})).toBeDisabled();
    await panel.getByLabel('FTP completed source job 1').fill(sourceJob);await panel.getByRole('button',{name:'Check completed source 1',exact:true}).click();
    await expect(panel.getByTestId('ftp-source-metadata-1')).toBeVisible();
    const ftpQueued=page.waitForResponse(r=>r.url().endsWith('/treasury/modeled-product-ftp/sources')&&r.request().method()==='POST');
    await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
    const ftpId=(await(await ftpQueued).json()).id;
    if(process.env.CONTAINED_LATE_FTP_FAILURE==='1'){
      await expect(panel.getByRole('alert')).toContainText('contained late storage publication refusal',{timeout:100_000});
      await expect(panel.getByRole('status')).toHaveCount(0);
      const refused=await(await page.request.get(`/api/jobs/${ftpId}`)).json();expect(refused.status).toBe('error');
      const noResult=await page.request.get(`/api/jobs/${ftpId}/table?path=${encodeURIComponent('/tables/source_journal')}`);expect(noResult.ok()).toBeFalsy();
      expect(hydration).toEqual([]);return;
    }
    await expect.poll(async()=>{if(await panel.getByRole('alert').count())throw new Error(await panel.getByRole('alert').innerText());return panel.getByRole('status').count();},{timeout:100_000}).toBe(1);
    await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:100_000});
    const id=await panel.getByLabel('Saved modeled FTP report ID').inputValue();expect(id).not.toBe('');
    const report=await(await page.request.get(`/api/jobs/${id}`)).json();expect(report.status).toBe('done');
    const journal=await page.request.get(`/api/jobs/${id}/table?path=${encodeURIComponent('/tables/source_journal')}&offset=0&limit=100`);
    expect(journal.ok()).toBeTruthy();expect((await journal.json()).total).toBeGreaterThan(0);
    const download=await page.request.get(`/api/jobs/${id}/parquet?path=${encodeURIComponent('/tables/source_journal')}`);expect(download.ok()).toBeTruthy();expect((await download.body()).subarray(0,4).toString()).toBe('PAR1');
    await page.reload();await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
    const reopened=page.getByRole('region',{name:'Capital & FTP',exact:true});await reopened.getByLabel('Treasury input source').selectOption('completed_products');
    await reopened.getByLabel('Saved modeled FTP report ID').fill(id);await reopened.getByRole('button',{name:'Reopen modeled FTP report',exact:true}).click();
    await expect(reopened.getByText('Verified modeled-source journal',{exact:true})).toBeVisible({timeout:30_000});expect(hydration).toEqual([]);
  });
}


