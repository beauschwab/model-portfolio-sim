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

test("a saved layout with a retired panel falls back to the default dock", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("region", { name: "Morning Sheet", exact: true })).toBeVisible();
  // turn the saved default layout into one that still references a retired panel
  await page.evaluate(() => {
    const saved = localStorage.getItem("workspace.layout.v1");
    if (!saved || !saved.includes('"risk"')) throw new Error("no saved default layout");
    localStorage.setItem("workspace.layout.v1", saved.split('"risk"').join('"optimizer"'));
  });
  await page.reload();
  await expect(page.getByRole("region", { name: "Morning Sheet", exact: true })).toBeVisible();
  await expect(page.getByText(/Unknown panel/)).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => localStorage.getItem("workspace.layout.v1") ?? "")).toContain('"risk"');
  expect(await page.evaluate(() => localStorage.getItem("workspace.layout.v1"))).not.toContain('"optimizer"');
});

test("rate shocks run from the default Stress tab and chart the MBS P&L", async ({ page }) => {
  const job = { id: "rate-shocks", kind: "stress", status: "done", revision: 0 };
  await page.route("**/api/run", route => route.fulfill({ json: job }));
  await page.route("**/api/jobs/rate-shocks", route => route.fulfill({ json: job }));
  const agg = tableToIPC(tableFromArrays({ horizon_m: [3, 3], shock_bp: [-100, 100], "pnl_$": [2e6, -3e6] }));
  await page.route("**/api/jobs/rate-shocks/result", route => route.fulfill({
    body: envelope({ mbs: { agg: { __arrow__: 0 } } }, [agg]), contentType: "application/octet-stream",
  }));
  await page.goto("/");
  await page.getByRole("button", { name: "Open Stress", exact: true }).click();
  const panel = page.getByRole("region", { name: "Stress", exact: true });
  await expect(panel.getByRole("tab", { name: "Rate shocks", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(panel.getByRole("tabpanel", { name: "Rate shocks" })).toBeVisible();
  await panel.getByRole("button", { name: "Run 9Q stress", exact: true }).click();
  await expect(panel.locator(".recharts-line").first()).toBeVisible();

  // arrow keys move between tabs and reveal the linked panel
  await panel.getByRole("tab", { name: "Rate shocks", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(panel.getByRole("tab", { name: "Balance sheet", exact: true })).toBeFocused();
  await expect(panel.getByRole("tabpanel", { name: "Balance sheet" })).toBeVisible();
});
