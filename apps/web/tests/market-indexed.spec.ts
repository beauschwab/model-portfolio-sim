import {test,expect} from '@playwright/test';
test('supplied conditional pricing source saves and reopens with explicit limited scope',async({page})=>{
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption('market-indexed-adapter-1');
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText('market-indexed-adapter-1');
  await expect(panel.getByRole('button',{name:'Build dated strategy library',exact:true})).toHaveCount(0);
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  await expect(panel.getByText('Supplied collateral and named index curves were checked',{exact:false})).toBeVisible();
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  await expect(reopened.getByText('Rates are pricing assumptions',{exact:false})).toBeVisible();
});

test('fully-bound indexed-book source saves and reopens with no accounting eligibility',async({page})=>{
  await page.goto('/');await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const panel=page.getByRole('region',{name:'Product Models',exact:true});
  await panel.getByLabel('Product model',{exact:true}).selectOption('market-indexed-books-1');
  await expect(panel.getByLabel('Product terms, path assumptions and source policies')).toContainText('market-indexed-books-1');
  await panel.getByRole('button',{name:'Run product analysis',exact:true}).click();
  await expect(panel.getByRole('status')).toContainText('Analysis saved:',{timeout:40000});
  await expect(panel.getByText('Every loan was explicitly bound',{exact:false})).toBeVisible();
  const id=await panel.getByLabel('Stored analysis job').inputValue();
  await expect(panel.getByRole('link',{name:'Download Parquet',exact:true})).toBeVisible();
  await page.reload();await page.getByRole('button',{name:'Open Product Models',exact:true}).click();
  const reopened=page.getByRole('region',{name:'Product Models',exact:true});
  await reopened.getByLabel('Stored analysis job').fill(id);await reopened.getByRole('button',{name:'Open stored analysis',exact:true}).click();
  await expect(reopened.getByRole('status')).toContainText(id);
  await expect(reopened.getByText('does not supply Physical income',{exact:false})).toBeVisible();
});
