import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { decodeEnvelope } from '../src/lib/api';

test('completed mortgage source separately publishes reviewed closing capital and preserves FTP refusal', async ({ page }) => {
  test.setTimeout(150_000);
  const api = '/api';
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Stress', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Stress', exact: true });
  await panel.getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  await panel.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await panel.getByLabel('Cashflow timing', { exact: true }).selectOption('mortgage_mixed');
  await panel.getByRole('button', { name: 'Run partitioned simulation', exact: true }).click();
  await expect(panel.getByText('Mortgage source economics verified', { exact: true })).toBeVisible({ timeout: 60_000 });
  const source = await panel.getByLabel('Saved run ID', { exact: true }).inputValue();
  const example = JSON.parse(readFileSync(resolve('../../examples/models/mortgage-closing-capital-1.json'), 'utf8'));
  const { raw_mixed: _raw, schema: _schema, closing_review, ...policies } = example;
  const state = await (await page.request.get(`${api}/state`)).json();
  const submitted = await page.request.post(`${api}/treasury/mortgage-closing-capital`, { data: {
    expected_revision: state.revision, source_job: source,
    specification: { kind: 'ledger', ...policies }, closing_review, timeout: 90.0,
  } });
  expect(submitted.ok()).toBeTruthy();
  const capital = await submitted.json();
  await expect.poll(async () => (await (await page.request.get(`${api}/jobs/${capital.id}`)).json()).status,
    { timeout: 100_000 }).toBe('done');
  const body = await (await page.request.get(`${api}/jobs/${capital.id}/result`)).body();
  const result = decodeEnvelope(Uint8Array.from(body).buffer) as {
    execution: { validation: Record<string, boolean>; source_binding: { id: string } }
  };
  expect(result.execution.validation).toMatchObject({
    closing_capital_eligible: true, capital_ftp_eligible: false,
    interim_capital_eligible: false, regulatory_credit_rwa_approval: false,
    accounting_standard_approval: false, empirical_validation: false,
  });
  expect(result.execution.source_binding.id).toBe(source);
  await page.reload();
  const reopened = page.getByRole('region', { name: 'Stress', exact: true });
  await reopened.getByRole('tab', { name: 'Balance sheet', exact: true }).click();
  await reopened.getByLabel('Simulation workflow', { exact: true }).selectOption('partitioned');
  await expect(reopened.getByLabel('Saved run ID', { exact: true })).toHaveValue(source);
  await expect(reopened.getByText('Research journal only; capital and FTP unavailable', { exact: true })).toBeVisible();
  const generic = await page.request.get(`${api}/treasury/sources/${source}/template?kind=cashflows`);
  expect(generic.status()).toBe(422);
});
