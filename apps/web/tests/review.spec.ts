import { test, expect } from "@playwright/test";
import { tableFromArrays, tableToIPC } from "apache-arrow";

function envelope(payload: unknown, frames: Uint8Array[] = []) {
  const json = Buffer.from(JSON.stringify(payload));
  const header = Buffer.alloc(8); header.write("ARW1"); header.writeUInt32LE(json.length, 4);
  const sizes = Buffer.alloc(4 + 4 * frames.length);
  sizes.writeUInt32LE(frames.length);
  frames.forEach((frame, i) => sizes.writeUInt32LE(frame.length, 4 + 4 * i));
  return Buffer.concat([header, json, sizes, ...frames.map(frame => Buffer.from(frame))]);
}

test("optimizer retains input focus and workspace restores", async ({ page }) => {
  const errors: string[] = []; page.on("pageerror", e => errors.push(e.message));
  await page.goto("/");
  await page.getByRole("button", { name: "Open Decide", exact: true }).click();
  await page.getByRole('region', { name: 'Decide', exact: true }).getByRole('tab', { name: 'Optimize', exact: true }).click();
  const floor = page.getByRole("region", { name: "Decide", exact: true }).getByRole("spinbutton").first();
  await floor.fill("1");
  await floor.pressSequentially(".25", { delay: 100 });
  await expect(floor).toBeFocused();
  await expect(floor).toHaveValue("1.25");
  await page.reload();
  await expect(page.getByRole("region", { name: "Decide", exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("balance popover survives repeated slider changes", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Open Positions", exact: true }).click();
  const panel = page.getByRole("region", { name: "Positions", exact: true });
  await panel.getByRole("cell", { name: /▸mbs/ }).click();
  await panel.locator("tbody tr").filter({ has: page.locator("button span.num") }).first().locator("button").first().click();
  // popovers render in a portal, outside the scrolling panel
  const slider = page.getByRole("slider");
  await slider.focus();
  await slider.press("ArrowRight"); await slider.press("ArrowRight");
  await expect(slider).toHaveValue("1.1");
  await expect(slider).toBeFocused();
  await expect(page.getByText("110% of booked", { exact: true })).toBeVisible();
});

test("table bounds rows and unchanged JSON book saves", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Open Book Editor", exact: true }).click();
  const panel = page.getByRole("region", { name: "Book Editor", exact: true });
  await expect(panel.locator("tbody tr")).toHaveCount(100);
  await panel.getByRole("button", { name: "Next", exact: true }).click();
  await expect(panel.locator("tbody tr")).toHaveCount(20);
  await panel.getByRole("tab", { name: "loans", exact: true }).click();
  await expect(panel.getByText("loans — 90 positions", { exact: true })).toBeVisible();
  await panel.getByRole("button", { name: "Edit book (JSON)", exact: true }).click();
  const saved = page.waitForResponse(r => r.url().endsWith("/api/books/loans") && r.request().method() === "PUT");
  await panel.getByRole("button", { name: "Save", exact: true }).click();
  expect((await saved).status()).toBe(200);
  await expect(panel.getByRole("button", { name: "Edit book (JSON)", exact: true })).toBeVisible();
});

test("late strategy response cannot replace latest allocation", async ({ page }) => {
  let releaseOld!: () => void; let oldStarted!: () => void;
  const started = new Promise<void>(resolve => { oldStarted = resolve; });
  const holdOld = new Promise<void>(resolve => { releaseOld = resolve; });
  let calls = 0;
  await page.route("**/api/state", route => route.fulfill({ json: { revision: 99, library_ready: true, library_horizon: 27 } }));
  await page.route("**/api/strategy/eval", async route => {
    const call = ++calls;
    if (call === 1) { oldStarted(); await holdOld; }
    const amount = call === 1 ? 111e6 : 222e6;
    await route.fulfill({ body: envelope({ nii_total_$: amount, nii_incremental: [amount], balance: [1],
      fwd_dv01: [1], dv01_at_t0_$: 1, kpis: { "d_eve_pct_eve_+200": 1, duration_gap_y: 1,
        lcr_pct: 150, nsfr_pct: 150, cet1_q9_pct: 12 } }), contentType: "application/octet-stream" }).catch(() => {});
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Open Decide", exact: true }).click();
  await started;
  const panel = page.getByRole("region", { name: "Decide", exact: true });
  await panel.getByRole("textbox").first().fill("3000000000");
  await expect(panel.getByText("$222.0M", { exact: true })).toBeVisible();
  releaseOld();
  await expect(panel.getByText("$111.0M", { exact: true })).toHaveCount(0);
  await expect(panel.getByText("$222.0M", { exact: true })).toBeVisible();
});

test("top-bar run updates global telemetry and shared KPIs", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Open KPIs", exact: true }).click();
  const panel = page.getByRole("region", { name: "KPIs", exact: true });
  await page.getByRole("button", { name: "Run sheet", exact: true }).click();
  await expect(page.getByTitle("Run the KPI sheet (⌘K for more)")).toBeDisabled();
  await expect(panel.getByText("EVE", { exact: true }).first()).toBeVisible({ timeout: 50_000 });
  await page.getByRole("button", { name: "Open Morning Sheet", exact: true }).click();
  await expect(page.getByRole("region", { name: "Morning Sheet", exact: true }).getByText("Constraint ledger", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Run sheet", exact: true })).toBeEnabled();
});

test("equal asset and liability sensitivities cancel on Risk Desk", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('engine.autoRecalc', 'off'));
  await page.route("**/api/state", route => route.fulfill({ json: { revision: 0, library_ready: false, library_horizon: null } }));
  const job = { id: "signed-risk", kind: "risk", status: "done", revision: 0 };
  await page.route("**/api/run", route => route.fulfill({ json: job }));
  await page.route("**/api/jobs/signed-risk", route => route.fulfill({ json: job }));
  const frame = tableToIPC(tableFromArrays({ dv01: [100], krd01_1y: [100] }));
  await page.route("**/api/jobs/signed-risk/result", route => route.fulfill({
    body: envelope({ mbs: { __arrow__: 0 }, debt: { __arrow__: 1 } }, [frame, frame]),
    contentType: "application/octet-stream",
  }));
  await page.goto("/");
  await page.getByRole("button", { name: "Open Risk Desk", exact: true }).click();
  const panel = page.getByRole("region", { name: "Risk Desk", exact: true });
  await panel.getByRole("button", { name: "Refresh risk", exact: true }).click();
  await expect(panel.getByText("net dv01 (selected books) $0/bp", { exact: true })).toBeVisible();
});
