import { test, expect } from '@playwright/test';

test('capital report and FTP reconciliation use durable native execution', async ({page}) => {
  const errors:string[]=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.goto('/');
  await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await expect(panel.getByLabel('Capital policy, scenario snapshots and FTP inputs')).toContainText('synthetic-example');
  await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:30000});
  await expect(panel.getByText('Capital ratios and headroom',{exact:true})).toBeVisible();
  await expect(panel.getByRole('cell',{name:'slr',exact:true})).toHaveCount(2);
  await expect(panel.getByRole('cell',{name:'loan-1',exact:true})).toHaveCount(2);
  await expect(panel.getByText('Treasury offset and consolidated profit',{exact:true})).toBeVisible();
  expect(errors).toEqual([]);
  await page.screenshot({path:'test-results/treasury-real.png',fullPage:true});
});

test('captured native cohort cashflows become a source-linked FTP report',async({page,request})=>{
  const settings=await(await request.get('/api/settings')).json();
  expect((await request.put('/api/settings',{data:{...settings,compute_backend:'rust',n_paths:32,n_paths_base:32,n_threads:2,horizon_months:3}})).ok()).toBeTruthy();
  const wait=async(id:string)=>{
    await expect.poll(async()=> (await(await request.get(`/api/jobs/${id}`)).json()).status,{timeout:30000}).toMatch(/done|error/);
    const job=await(await request.get(`/api/jobs/${id}`)).json();expect(job.status,job.detail).toBe('done');return id;
  };
  const tape=await(await request.get('/api/cohorts/example')).json();
  const imported=await(await request.post('/api/cohorts/imports',{data:{name:'ftp-bridge.csv',format:'csv',content_base64:Buffer.from(tape.csv).toString('base64')}})).json();
  await wait(imported.id);
  const state=await(await request.get('/api/state')).json();
  expect((await request.put('/api/cohorts/tapes/ftp-bridge',{data:{job_id:imported.id,expected_revision:state.revision}})).ok()).toBeTruthy();
  const presets=await(await request.get('/api/cohorts/presets')).json();
  const built=await(await request.post('/api/cohorts/builds',{data:{tape_id:'ftp-bridge',config:presets.config,expected_revision:presets.revision}})).json();
  await wait(built.id);
  const source=await(await request.post(`/api/cohorts/builds/${built.id}/analytics`,{data:{products:['mortgage','deposit']}})).json();
  await wait(source.id);
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('cashflows');
  await panel.getByLabel('Treasury source job ID').fill(source.id);
  await panel.getByRole('button',{name:'Load source template',exact:true}).click();
  const input=panel.getByLabel('Capital policy, scenario snapshots and FTP inputs');
  await expect(input).toContainText('residual_tail_years');
  const spec=JSON.parse(await input.inputValue());
  const demo=await(await request.get('/api/treasury/example')).json();
  const curve={...demo.specification.curves[0],as_of:spec.treasury.as_of};
  spec.treasury.policy_id='browser-ftp-v1';spec.treasury.curves=[curve];
  for(const row of spec.mappings)Object.assign(row,{entity:curve.entity,curve_id:curve.id,residual_tail_years:3,repricing_years:row.book==='deposits'?.25:null});
  await input.fill(JSON.stringify(spec,null,2));
  await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:30000});
  await expect(panel.getByText('Cashflow balances and funding life',{exact:true})).toBeVisible();
  await page.screenshot({path:'test-results/treasury-source-real.png',fullPage:true});
  await panel.getByRole('button',{name:'Load source template',exact:true}).click();
  await expect(panel.getByRole('status')).toHaveCount(0);
  await expect(panel.getByText('Cashflow balances and funding life',{exact:true})).toHaveCount(0);
});
