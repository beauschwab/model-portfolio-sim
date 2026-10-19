import {test,expect} from '@playwright/test';

// Requires durable API/worker/web servers (WORKBENCH_REUSE_TEST_SERVERS=1); memory mode reports
// that durable storage is required, as for every model analysis.
test('hedge accounting designates, replays, pages and reopens a stored run',async({page})=>{
  const errors:string[]=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto('/');await page.getByRole('button',{name:'Open Hedge Accounting',exact:true}).click();
  const panel=page.getByRole('region',{name:'Hedge Accounting',exact:true});
  const lower=panel.getByLabel('Assessment lower bound',{exact:true});
  await expect(lower).toHaveValue('0.8');
  await lower.fill('0.79');
  await expect(panel.getByLabel('Instrument fvh-1',{exact:true})).toHaveValue('swap-fvh');
  await expect(panel.getByLabel('Inception cfh-1',{exact:true})).toHaveValue('2024-02-01');
  await panel.getByRole('button',{name:'Run hedge accounting',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Hedge accounting saved:',{timeout:60000});
  await expect(panel.getByText('Independently replayed',{exact:true})).toBeVisible();
  await expect(panel.getByText('Not accounting-standard approval',{exact:true})).toBeVisible();
  await panel.getByRole('tab',{name:'Assessments',exact:true}).click();
  await expect(panel.getByRole('tabpanel')).toContainText('cumulative dollar offset');
  await panel.getByRole('tab',{name:'Basis and OCI',exact:true}).click();
  await expect(panel.getByRole('tabpanel')).toContainText('cfh-1');
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  const id=await panel.getByLabel('Stored hedge job',{exact:true}).inputValue();
  await page.reload();await page.getByRole('button',{name:'Open Hedge Accounting',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Hedge Accounting',exact:true});
  await reopened.getByLabel('Stored hedge job',{exact:true}).fill(id);
  await reopened.getByRole('button',{name:'Open stored hedge job',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  await reopened.getByRole('tab',{name:'History',exact:true}).click();
  await expect(reopened.getByRole('tabpanel')).toContainText('inception');
  expect(errors).toEqual([]);
});
