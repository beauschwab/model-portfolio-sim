import {test,expect} from '@playwright/test';
import {readFileSync} from 'node:fs';

test('mixed Card2 gross rights publish and reopen with separate income and basis',async({page})=>{
  const raw=JSON.parse(readFileSync(new URL('../../../examples/models/product-mixed-card-credit-2.json',import.meta.url),'utf8'));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption('product-mixed-product-credit-1');
  const editor=panel.getByLabel('Product terms, path assumptions and source policies');
  await expect(editor).toContainText('product-mixed-product-credit-1');
  await editor.fill(JSON.stringify(raw));await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  await expect(panel.getByText('Gross card interest and its carrying adjustment share the dated book', {exact:false})).toBeVisible();
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  const tables=panel.getByLabel('Model result table');
  const selected=await tables.locator('option').evaluateAll(options=>options.map(o=>(o as HTMLOptionElement).value).find(v=>v.includes('product_rights_daily')));
  expect(selected).toBeTruthy();await tables.selectOption(selected!);
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);
  await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  await expect(reopened.getByText('Closing capital requires supplied reviewed policies and the complete-tail date',{exact:false})).toBeVisible();
});

test('verified Card2 gross rights supply reviewed full-tail closing capital',async({page,request})=>{
  const raw=JSON.parse(readFileSync(new URL('../../../examples/models/product-mixed-card-credit-2.json',import.meta.url),'utf8'));
  const example=await(await request.get('/api/models/examples/product-mixed-product-credit-1')).json();
  const source=await(await request.post('/api/models/runs',{data:{schema_name:example.schema_name,request:raw,expected_revision:example.revision}})).json();
  await expect.poll(async()=> (await(await request.get(`/api/jobs/${source.id}`)).json()).status,{timeout:40000}).toMatch(/done|error/);
  const finished=await(await request.get(`/api/jobs/${source.id}`)).json();expect(finished.status,finished.detail).toBe('done');
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('ledger');
  await panel.getByLabel('Treasury source job ID').fill(source.id);
  await panel.getByRole('button',{name:'Load source template',exact:true}).click();
  const editor=panel.getByLabel('Capital policy, scenario snapshots and FTP inputs');
  await expect(editor).toContainText('ledger');
  const capital=JSON.parse(await editor.inputValue());capital.amount_multiplier=1;
  const closingDate=capital.treasury.as_of;capital.treasury.policy_id='synthetic-card2-closing-policy';
  for(const policy of capital.policies){
    for(const [key,value] of Object.entries(policy)){if(value===null)policy[key]=key==='include_aoci'?true:0;}
    for(const [key,value] of Object.entries(policy.capital)){if(value===null&&key!=='advanced_rwa')policy.capital[key]=0;}
    policy.capital.period='closing';policy.gl_risk_weights={cash:0,asset_principal:1,book_adjustment:1,allowance:1,accrued_interest:1,
      dated_interest_receivable:1,dated_prepaid_expense:1,credit_default_principal:1,credit_default_book_adjustment:1,
      credit_default_interest_receivable:1,live_interest_basis_adjustment:1,default_interest_basis_adjustment:1};
  }
  await editor.fill(JSON.stringify(capital));await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:40000});
  await expect(panel.getByText('Ledger-to-capital reconciliation',{exact:true})).toBeVisible();
  await expect(editor).toContainText(closingDate);
});

test('native product analysis pages and reopens immutable results',async({page})=>{
  const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption('consumer-installment-1');
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText('consumer-installment-1');
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await expect(panel.getByLabel('Model result table')).toBeVisible();
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);
  await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  expect(errors).toEqual([]);
  await page.screenshot({path:'test-results/product-models-real.png',fullPage:true});
});

for(const librarySchema of ['aged-units-1','product-units-1']) test(`${librarySchema} native library supplies actual-date Decide coefficients`,async({page})=>{
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption(librarySchema);
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText(librarySchema);
  await panel.getByRole('button',{name:'Build dated strategy library',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Dated coefficients saved:',{timeout:40000});
  await page.getByRole('button',{name:'Open Decide',exact:true}).click();
  const strategy=page.getByRole('region',{name:'Decide',exact:true});
  await strategy.getByRole('tab',{name:'Sandbox',exact:true}).click();
  await expect(strategy.getByLabel('Purchase date')).toHaveCount(1);
  // A queued background refresh may start after the library publishes. The
  // worker explicitly refuses evaluation while busy; retry through an edit.
  let notional=1_000_000;let evaluationStatus=409;
  await expect.poll(async()=>{
    const amount=++notional;
    const evaluated=page.waitForResponse(r=>r.url().endsWith('/strategy/eval')&&r.request().method()==='POST'&&r.request().postDataJSON()?.[0]?.notional===amount);
    await strategy.getByRole('tabpanel',{name:'Sandbox',exact:true}).getByRole('textbox').fill(String(amount));
    evaluationStatus=(await evaluated).status();
    return evaluationStatus!==409;
  },{timeout:40_000}).toBe(true);
  expect(evaluationStatus).toBe(200);
  await expect(strategy.getByText('native actual-age repricing at fixed purchase OAS',{exact:true})).toBeVisible();
  await expect(strategy.getByText('Actual reporting intervals, $ — retained original effective yield',{exact:true})).toBeVisible();
  await page.screenshot({path:`test-results/${librarySchema}-strategy-real.png`,fullPage:true});
});

for(const creditRoute of ['credit','credit-path']) test(`verified ${creditRoute} journal becomes a source-linked capital report`,async({page,request})=>{
  const example=await(await request.get(`/api/balance-stress/${creditRoute}/example`)).json();
  const source=await(await request.post(`/api/balance-stress/${creditRoute}`,{data:{specification:example.specification,expected_revision:example.revision}})).json();
  await expect.poll(async()=> (await(await request.get(`/api/jobs/${source.id}`)).json()).status,{timeout:40000}).toMatch(/done|error/);
  const finished=await(await request.get(`/api/jobs/${source.id}`)).json();expect(finished.status,finished.detail).toBe('done');
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('credit_ledger');
  await panel.getByLabel('Treasury source job ID').fill(source.id);
  await panel.getByRole('button',{name:'Load source template',exact:true}).click();
  const editor=panel.getByLabel('Capital policy, scenario snapshots and FTP inputs');
  await expect(editor).toContainText('credit_ledger');
  const raw=JSON.parse(await editor.inputValue());raw.amount_multiplier=1;
  raw.treasury.as_of='2026-10-01';raw.treasury.policy_id='synthetic-browser-policy';
  for(const policy of raw.policies){
    for(const [key,value] of Object.entries(policy)){if(value===null)policy[key]=key==='include_aoci'?true:0;}
    for(const [key,value] of Object.entries(policy.capital)){if(value===null&&key!=='advanced_rwa')policy.capital[key]=0;}
    policy.capital.period='closing';policy.gl_risk_weights={cash:0,asset_principal:1,book_adjustment:1,allowance:1,accrued_interest:1,
      credit_default_principal:1,credit_default_book_adjustment:1,credit_default_accrued_interest:1};
  }
  await editor.fill(JSON.stringify(raw));await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:40000});
  await expect(panel.getByText('Ledger-to-capital reconciliation',{exact:true})).toBeVisible();
  await expect(panel.getByText('Mapped credit exposures',{exact:true})).toBeVisible();
  await page.screenshot({path:`test-results/${creditRoute}-capital-real.png`,fullPage:true});
});

for(const mixedSchema of ['product-mixed-dated-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'])
test(`verified ${mixedSchema} journal supplies closing capital with its actual date`,async({page,request})=>{
  const example=await(await request.get(`/api/models/examples/${mixedSchema}`)).json();
  const source=await(await request.post('/api/models/runs',{data:{schema_name:example.schema_name,request:example.request,expected_revision:example.revision}})).json();
  await expect.poll(async()=> (await(await request.get(`/api/jobs/${source.id}`)).json()).status,{timeout:40000}).toMatch(/done|error/);
  const finished=await(await request.get(`/api/jobs/${source.id}`)).json();expect(finished.status,finished.detail).toBe('done');
  await page.goto('/');await page.getByRole('button',{name:'Open Capital & FTP',exact:true}).click();
  const panel=page.getByRole('region',{name:'Capital & FTP',exact:true});
  await panel.getByLabel('Treasury input source').selectOption('ledger');
  await panel.getByLabel('Treasury source job ID').fill(source.id);
  await panel.getByRole('button',{name:'Load source template',exact:true}).click();
  const editor=panel.getByLabel('Capital policy, scenario snapshots and FTP inputs');
  await expect(editor).toContainText('ledger');
  const raw=JSON.parse(await editor.inputValue());raw.amount_multiplier=1;
  const closingDate=raw.treasury.as_of;expect(closingDate).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  raw.treasury.policy_id='synthetic-mixed-browser-policy';
  for(const policy of raw.policies){
    for(const [key,value] of Object.entries(policy)){if(value===null)policy[key]=key==='include_aoci'?true:0;}
    for(const [key,value] of Object.entries(policy.capital)){if(value===null&&key!=='advanced_rwa')policy.capital[key]=0;}
    policy.capital.period='closing';policy.gl_risk_weights={cash:0,asset_principal:1,book_adjustment:1,allowance:1,accrued_interest:1,
      dated_interest_receivable:1,dated_prepaid_expense:1,credit_default_principal:1,
      credit_default_book_adjustment:1,credit_default_interest_receivable:1};
  }
  await editor.fill(JSON.stringify(raw));await panel.getByRole('button',{name:'Run capital & FTP report',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Report saved:',{timeout:40000});
  await expect(panel.getByText('Ledger-to-capital reconciliation',{exact:true})).toBeVisible();
  await expect(editor).toContainText(closingDate);
});


for(const schema of ['derivative-accounting-1','hedge-accounting-1','credit-covariate-1','advanced-product-graph-1','card-product-graph-1','deposit-product-graph-1','mortgage-product-graph-1','funding-product-graph-1','graph-cohort-audit-1','term-product-graph-1','product-units-1','strategy-product-1','product-candidate-journal-1','product-mixed-dated-1','consumer-credit-accounting-1','card-credit-accounting-1','card-credit-accounting-2','mortgage-credit-accounting-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'])
test(`model analysis stores and restores ${schema}`,async({page})=>{
  const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption(schema);
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText(schema);
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  if(schema==='product-candidate-journal-1'){
    await expect(panel.getByText('Independent source verification',{exact:true})).toBeVisible();
    await expect(panel.getByText('Opening provenance is supplied; mixed-book and empirical acceptance remain open.',{exact:false})).toBeVisible();
  }
  if(schema==='hedge-accounting-1'){
    await expect(panel.getByText('Independent source verification',{exact:true})).toBeVisible();
    await expect(panel.getByText('Formal relationships, assessments, hedged-item basis and OCI mechanics were independently replayed',{exact:false})).toBeVisible();
  }
  if(schema==='product-mixed-dated-1'){
    await expect(panel.getByText('Independent source verification',{exact:true})).toBeVisible();
    await expect(panel.getByText('Credit, joint stochastic and empirical acceptance remain open.',{exact:false})).toBeVisible();
  }
  if(['consumer-credit-accounting-1','card-credit-accounting-1','card-credit-accounting-2','mortgage-credit-accounting-1','product-mixed-consumer-credit-1','product-mixed-product-credit-1'].includes(schema)){
    await expect(panel.getByText('Independent source verification',{exact:true})).toBeVisible();
  }
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await expect(panel.getByLabel('Model result table')).toBeVisible();
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);
  await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  expect(errors).toEqual([]);
});

// Authored actual default graph2/reprice2 browser acceptance; pending rebuilt tempstack.
for(const schema of ['term-product-graph-2','term-product-reprice-2']) test(`${schema} retains original OAS and full CD sequence sources after reopen`,async({page})=>{
  const example=JSON.parse(readFileSync(new URL(`../../../examples/models/${schema}.json`,import.meta.url),'utf8'));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption(schema);
  const editor=panel.getByLabel('Product terms, path assumptions and source policies');
  // Repricing transports base/revised graphs; the selected operation is outside the raw request.
  await expect.poll(async()=>{
    const value=await editor.inputValue();return value?JSON.parse(value):null;
  }).toEqual(example.request??example);
  await editor.fill(JSON.stringify(example.request??example));
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await panel.getByLabel('Model result table').selectOption('/model_result/metrics');
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
});
