import {test,expect} from '@playwright/test';
import {readFileSync} from 'node:fs';
import {decodeEnvelope} from '../src/lib/api';

test('initial product example matches the selected model and can be published',async({page})=>{
  test.setTimeout(180000);
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  const selected=panel.getByLabel('Product model',{exact:true});
  const editor=panel.getByLabel('Product terms, path assumptions and source policies');
  await expect(panel.getByRole('button',{name:'Run product analysis',exact:true})).toBeEnabled();
  expect(JSON.parse(await editor.inputValue()).schema).toBe(await selected.inputValue());
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:120000});
});

for(const schema of ['commercial-product-graph-1','commercial-fee-journal-1','commercial-candidate-fee-treasury-1','commercial-candidate-holding-journal-1'])
test(`commercial workbench publishes and reopens ${schema}`,async({page})=>{
  test.setTimeout(180000);
  const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption(schema);
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText(schema);
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:120000});
  await expect(panel.getByText('Independent source verification',{exact:true})).toBeVisible();
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await expect(panel.getByLabel('Model result table')).toBeVisible();
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await expect(reopened.getByLabel('Product model',{exact:true}).locator('option')).not.toHaveCount(0);
  await reopened.getByLabel('Stored analysis job').fill(id);
  await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  await expect(reopened.getByText('Independent source verification',{exact:true})).toBeVisible();
  expect(errors).toEqual([]);
});

test('commercial fee library evaluates retained dated units with separate net fee earnings',async({page})=>{
  test.setTimeout(180000);
  const raw=JSON.parse(readFileSync(new URL('../../../examples/models/commercial-fee-units-1.json',import.meta.url),'utf8'));
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption('commercial-units-1');
  const editor=panel.getByLabel('Product terms, path assumptions and source policies');
  await expect(editor).toContainText('commercial-units-1');await editor.fill(JSON.stringify(raw));
  await panel.getByRole('button',{name:'Build dated strategy library',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Dated coefficients saved:',{timeout:120000});
  await page.getByRole('button',{name:'Open Decide',exact:true}).click();
  const strategy=page.getByRole('region',{name:'Decide',exact:true});
  await strategy.getByRole('tab',{name:'Sandbox',exact:true}).click();
  await expect(strategy.getByLabel('Purchase date')).toHaveCount(1);
  let notional=1000;
  await expect.poll(async()=>{
    const amount=++notional;
    const response=page.waitForResponse(r=>r.url().endsWith('/strategy/eval')&&r.request().method()==='POST'&&r.request().postDataJSON()?.[0]?.notional===amount);
    await strategy.getByRole('tabpanel',{name:'Sandbox',exact:true}).getByRole('textbox').fill(String(amount));
    const result=await response;
    if(result.status()===409)return false;
    expect(result.status()).toBe(200);
    const body=await result.body();
    const values=decodeEnvelope(body.buffer.slice(body.byteOffset,body.byteOffset+body.byteLength)) as Record<string,number>;
    expect(values.nonintegral_fee_income_total_$).toBeCloseTo(amount*.03,8);
    expect(values.nii_total_$).toBeCloseTo(0.,8);
    return true;
  },{timeout:60000}).toBe(true);
  await expect(strategy.getByText('Net service fee earnings',{exact:true})).toBeVisible();
  await expect(strategy.getByText('Interest and service fee earnings',{exact:true})).toBeVisible();
  await expect(strategy.getByText('Actual reporting intervals, $ — retained original effective yield',{exact:true})).toBeVisible();
});
