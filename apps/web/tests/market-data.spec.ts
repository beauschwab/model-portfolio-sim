import { test, expect } from "@playwright/test";

const id = "a".repeat(64);
const market = { swap_tenors: [1, 2, 3, 4, 5, 7, 10, 15, 20, 30], swap_rates: Array(10).fill(.04),
  vol_pts: [[1, 2, .25]], source: "Synthetic research market", revision: 7,
  provenance: { curve: "assumed", volatility: "assumed" } };
const snap = { id, dataset: "eris_sofr", as_of: "2026-09-25", fetched_at: "2026-09-28T12:00:00Z",
  observation_count: 2, warnings: ["Volatility and behavioral histories are not refreshed."],
  curve: { swap_tenors: market.swap_tenors, swap_rates: Array(10).fill(.05), as_of: "2026-09-25", max_zero_error_bp_30y: 8, max_df_error_30y: .002 },
  observations: [{ date: "2026-09-25", maturity: "2026-09-26", series: "SOFR_DF", value: .9999, unit: "discount_factor", classification: "derived" }] };

test("research snapshot previews without mutation and applies with captured revision", async ({ page }) => {
  const errors: string[] = []; page.on("pageerror", e => errors.push(e.message));
  let applied = false;
  await page.route("**/api/market", route => route.fulfill({ json: market }));
  await page.route("**/api/market-data/sources", route => route.fulfill({ json: [
    { id: "eris_sofr", label: "SOFR discount curve", provider: "Eris", access: "public", configured: true, notes: "Derived research curve" },
    { id: "fred", label: "Macro series", provider: "FRED", access: "api_key", configured: false, notes: "Set FRED_API_KEY" },
  ] }));
  await page.route("**/api/market-data/snapshots", route => route.fulfill({ json: [snap] }));
  await page.route(`**/api/market-data/snapshots/${id}?*`, route => route.fulfill({ json: snap }));
  await page.route("**/api/market-data/active-curve", async route => {
    expect(route.request().postDataJSON()).toEqual({ snapshot_id: id, expected_revision: 7 });
    applied = true;
    await route.fulfill({ json: { ...market, swap_rates: snap.curve.swap_rates, revision: 8,
      provenance: { curve: "derived", volatility: "assumed", snapshot_id: id, curve_as_of: "2026-09-25" } } });
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Open Market & Scenarios", exact: true }).click();
  const panel = page.getByRole("region", { name: "Market & Scenarios", exact: true });
  await panel.getByLabel("Saved research snapshots").selectOption(id);
  await expect(panel.getByText(/Maximum zero-rate difference/)).toContainText("8.000 bp");
  await expect(panel.getByText("2026-09-26", { exact: true })).toBeVisible();
  expect(applied).toBe(false);
  await panel.getByRole("button", { name: "Apply research curve", exact: true }).click();
  await expect(panel.getByRole("status")).toContainText("Research curve applied");
  await expect(panel.getByText("Active curve: derived")).toBeVisible();
  await expect(panel.getByText("· Volatility: assumed")).toBeVisible();
  expect(applied).toBe(true);
  await panel.getByLabel("Research source").selectOption("fred");
  await expect(panel.getByRole("button", { name: "Fetch snapshot" })).toBeDisabled();
  expect(errors).toEqual([]);
});

test("failed source job surfaces error and keeps active market", async ({ page }) => {
  await page.route("**/api/market", route => route.fulfill({ json: market }));
  await page.route("**/api/market-data/sources", route => route.fulfill({ json: [
    { id: "eris_sofr", label: "SOFR discount curve", provider: "Eris", access: "public", configured: true, notes: "Research" },
  ] }));
  await page.route("**/api/market-data/snapshots", route => route.fulfill({ json: [] }));
  await page.route("**/api/market-data/fetch", route => route.fulfill({ json: { id: "bad-feed", status: "queued" } }));
  await page.route("**/api/jobs/bad-feed", route => route.fulfill({ json: { id: "bad-feed", status: "error", detail: "upstream HTTP 429" } }));
  await page.goto("/");
  await page.getByRole("button", { name: "Open Market & Scenarios", exact: true }).click();
  const panel = page.getByRole("region", { name: "Market & Scenarios", exact: true });
  await panel.getByRole("button", { name: "Fetch snapshot" }).click();
  await expect(panel.getByRole("alert")).toContainText("upstream HTTP 429");
  await expect(panel.getByText("Active curve: assumed")).toBeVisible();
  await expect(panel.getByRole("button", { name: "Fetch snapshot" })).toBeEnabled();
});
