import { test, expect, type Page } from '@playwright/test';

/** Live positions grid against the real API and native product engine.
 * Each test restores the books it changes, because the test API is shared. */

async function openBook(page: Page, book: string) {
  await page.addInitScript(() => localStorage.setItem('engine.autoRecalc', 'off'));
  await page.goto('/');
  await page.getByRole('button', { name: 'Open Positions', exact: true }).click();
  const panel = page.getByRole('region', { name: 'Positions', exact: true });
  await panel.locator('td', { hasText: book }).first().click();
  return panel;
}

test('a deposit account override saves to that row and clears back to the segment', async ({ page }) => {
  const panel = await openBook(page, 'deposits');
  const writes: Record<string, unknown>[][] = [];
  page.on('request', r => { if (r.method() === 'PUT' && r.url().endsWith('/api/books/deposits')) writes.push(JSON.parse(r.postData() ?? '[]')); });

  await panel.getByLabel('Assumptions for ').first().click();
  const field = page.getByLabel('Monthly base decay override for NIB0000', { exact: true });
  await expect(field).toHaveValue('');
  await field.fill('0.031');
  await page.getByRole('button', { name: 'Save position', exact: true }).click();
  await expect.poll(() => writes.length).toBe(1);
  const saved = writes[0].find(r => r.id === 'NIB0000')!;
  expect(saved.attrition_base).toBe(0.031);
  expect(writes[0].filter(r => r.attrition_base != null)).toHaveLength(1);

  // reopen after the grid reloads: the override is read back from the book
  await page.keyboard.press('Escape');
  await page.mouse.click(5, 5);
  await panel.getByLabel('Assumptions for ').first().click();
  await expect(page.getByLabel('Monthly base decay override for NIB0000', { exact: true })).toHaveValue('0.031');

  await page.getByRole('button', { name: 'Clear overrides', exact: true }).click();
  await expect.poll(() => writes.length).toBe(2);
  expect(writes[1].find(r => r.id === 'NIB0000')!.attrition_base).toBeNull();
});

test('a CD withdrawal multiplier saves to that position and adds the column at its default', async ({ page, request }) => {
  const panel = await openBook(page, 'cds');
  const writes: Record<string, unknown>[][] = [];
  page.on('request', r => { if (r.method() === 'PUT' && r.url().endsWith('/api/books/cds')) writes.push(JSON.parse(r.postData() ?? '[]')); });
  try {
    await panel.getByLabel('Assumptions for ').first().click();
    const field = page.getByLabel(/^Withdrawal multiplier for /);
    await expect(field).toHaveValue('1');
    const id = (await field.getAttribute('aria-label'))!.replace('Withdrawal multiplier for ', '');
    await field.fill('2.5');
    await page.getByRole('button', { name: 'Save position', exact: true }).click();
    await expect.poll(() => writes.length).toBe(1);
    expect(writes[0].find(r => r.id === id)!.ew_mult).toBe(2.5);
    expect(writes[0].filter(r => r.id !== id).every(r => r.ew_mult === 1)).toBe(true);
    await expect(page.getByRole('alert')).toHaveCount(0);
  } finally {
    if (writes.length) {
      const rows = writes[0].map(({ ew_mult: _drop, ...r }) => r);
      expect((await request.put('/api/books/cds', { data: rows })).ok()).toBeTruthy();
    }
  }
});

test('an MBS pool prepay speed saves to that pool and adds the column at its default', async ({ page, request }) => {
  const panel = await openBook(page, 'mbs');
  const writes: Record<string, unknown>[][] = [];
  page.on('request', r => { if (r.method() === 'PUT' && r.url().endsWith('/api/books/mbs')) writes.push(JSON.parse(r.postData() ?? '[]')); });
  try {
    await panel.getByLabel('Assumptions for ').first().click();
    const field = page.getByLabel(/^Prepay speed × for /);
    await expect(field).toHaveValue('1');
    const cusip = (await field.getAttribute('aria-label'))!.replace('Prepay speed × for ', '');
    await field.fill('1.8');
    await page.getByRole('button', { name: 'Save position', exact: true }).click();
    await expect.poll(() => writes.length).toBe(1);
    expect(writes[0].find(r => r.cusip === cusip)!.prepay_mult).toBe(1.8);
    expect(writes[0].filter(r => r.cusip !== cusip).every(r => r.prepay_mult === 1)).toBe(true);
    await expect(page.getByRole('alert')).toHaveCount(0);
  } finally {
    if (writes.length) {
      const rows = writes[0].map(({ prepay_mult: _drop, ...r }) => r);
      expect((await request.put('/api/books/mbs', { data: rows })).ok()).toBeTruthy();
    }
  }
});

test('a balance edit is funded by a saved money-market plug, and undoing it removes the plug', async ({ page }) => {
  const panel = await openBook(page, 'mbs');
  const mmWrites: Record<string, unknown>[][] = [];
  page.on('request', r => { if (r.method() === 'PUT' && r.url().endsWith('/api/books/mm')) mmWrites.push(JSON.parse(r.postData() ?? '[]')); });

  await panel.locator('span.cursor-pointer.underline').first().click();
  const slider = page.getByRole('slider');
  await slider.fill('1.1');
  await expect.poll(() => mmWrites.length, { timeout: 10_000 }).toBe(1);
  const plug = mmWrites[0].find(r => r.id === 'PLUG_ST_FUNDING');
  expect(plug?.side).toBe('liability');
  expect(plug?.spread_bp).toBe(0);
  expect(Number(plug?.balance)).toBeGreaterThan(0);
  await expect(panel.getByText('ST funding raised', { exact: true })).toBeVisible();

  await slider.fill('1');
  await expect.poll(() => mmWrites.length, { timeout: 10_000 }).toBe(2);
  expect(mmWrites[1].some(r => String(r.id).startsWith('PLUG_'))).toBe(false);
  await expect(panel.getByText('balanced as booked', { exact: true })).toBeVisible();
});
