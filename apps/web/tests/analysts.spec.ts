import { test, expect } from '@playwright/test';

const status = { available: true, reason: null, access: 'read-only', provider: 'fixture', model: 'local-test',
  roles: { ops: 'Operations Analyst', strategy: 'Portfolio Analyst' },
  workflows: { 'morning-review': 'Morning review', 'explain-change': 'Explain a change', 'job-recovery': 'Recover job visibility' } };

test('analyst setup state is honest and cannot submit', async ({ page }) => {
  await page.route('**/api/analysts/status', route => route.fulfill({ json: { ...status, available: false, reason: 'Embedded analysts are disabled' } }));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Analysts', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Analysts', exact: true });
  await expect(panel.getByText('Analysts are not connected')).toBeVisible();
  await expect(panel.getByRole('button', { name: 'Ask analyst', exact: true })).toBeDisabled();
});

test('analyst conversation reopens, evidence loads on demand, stop affects only the answer', async ({ page }, testInfo) => {
  let state = 'running'; let stopped = false; let evidenceReads = 0;
  const summary = { id: 'conversation', role: 'ops', title: 'Morning review' };
  await page.route('**/api/analysts/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/status')) return route.fulfill({ json: { ...status, available: false, history_available: true, reason: 'Provider disconnected' } });
    if (path.endsWith('/conversations')) return route.fulfill({ json: [summary] });
    if (path.endsWith('/stop')) { stopped = true; state = 'cancelled'; return route.fulfill({ json: { stopped: true, financial_jobs_cancelled: false } }); }
    if (path.endsWith('/evidence/E123')) { evidenceReads++; return route.fulfill({ json: { id: 'E123', tool: 'get_jobs', arguments: {}, result: { original_job: 'job-123' }, sha256: 'receipt-hash' } }); }
    return route.fulfill({ json: { ...summary, turns: [{ id: 'turn', revision: 12, workflow: 'morning-review', prompt: 'Review completed runs',
      answer: 'The original job is still running. [E123]', status: state, detail: state === 'cancelled' ? 'Answer stopped. Financial jobs continue unchanged.' : null,
      evidence: [{ id: 'E123', tool: 'get_jobs', sha256: 'receipt-hash' }], receipt: {} }] } });
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Analysts', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Analysts', exact: true });
  await panel.getByRole('button', { name: 'Morning review Operations', exact: true }).click();
  await expect(panel.getByText('The original job is still running. [E123]')).toBeVisible();
  expect(evidenceReads).toBe(0);
  await panel.getByText('Evidence (1)', { exact: true }).click();
  await panel.getByText('[E123] get_jobs', { exact: true }).click();
  await expect(panel.getByText(/job-123/)).toBeVisible();
  await panel.screenshot({ path: testInfo.outputPath('analyst-evidence.png') });
  await panel.getByRole('button', { name: 'Stop answer', exact: true }).click();
  await expect(panel.getByText('Answer stopped. Financial jobs continue unchanged.')).toBeVisible();
  expect(stopped).toBe(true);
  await page.reload();
  await panel.getByRole('button', { name: 'Morning review Operations', exact: true }).click();
  await expect(panel.getByText('Answer stopped. Financial jobs continue unchanged.')).toBeVisible();
});

test('uncertain turn submission retries the exact request without creating another conversation', async ({ page }) => {
  const requests: unknown[] = []; let creates = 0;
  const conversation = { id: 'retry', role: 'ops', title: 'Morning review', turns: [] };
  await page.route('**/api/analysts/**', async route => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith('/status')) return route.fulfill({ json: status });
    if (path.endsWith('/conversations')) {
      if (route.request().method() === 'POST') { creates++; return route.fulfill({ json: conversation }); }
      return route.fulfill({ json: [] });
    }
    if (path.endsWith('/turns')) {
      requests.push(route.request().postDataJSON());
      return requests.length === 1 ? route.fulfill({ status: 503, json: { detail: 'Connection interrupted' } }) : route.fulfill({ json: { id: 'same-turn' } });
    }
    return route.fulfill({ json: conversation });
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Analysts', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Analysts', exact: true });
  await panel.getByRole('button', { name: 'Ask analyst', exact: true }).click();
  await expect(panel.getByRole('alert')).toContainText('Connection interrupted');
  await panel.getByRole('button', { name: 'Ask analyst', exact: true }).click();
  await expect(panel.getByRole('button', { name: 'Ask analyst', exact: true })).toBeEnabled();
  expect(requests).toHaveLength(2);
  expect(requests[1]).toEqual(requests[0]);
  expect(creates).toBe(1);
});
