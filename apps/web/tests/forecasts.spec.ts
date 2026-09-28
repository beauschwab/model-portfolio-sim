import { test, expect } from "@playwright/test";
import { tableFromArrays, tableToIPC } from "apache-arrow";

const id = "f".repeat(64);
const snapshot = { id, dataset: "fed_stress", as_of: "2026-09-28", fetched_at: "2026-09-28T12:00:00Z",
  observation_count: 4, curve: null, warnings: [], observations: [],
  forecast: { scenarios: ["baseline", "adverse"], runnable: ["baseline", "adverse"], periods: ["2026-01-01", "2026-04-01"], variables: ["short_rate"], alignment: "relative_replay" } };
const preview = { snapshot_id: id, dataset: "fed_stress", book_as_of: "2026-06-10", source_as_of: "2026-09-28", revision: 7,
  start_period: "2026-01-01", horizon_months: 27, warnings: ["Conditional income only; no default losses."], unused_variables: ["unemployment"],
  coverage: { short_rate: { first_month: 1, last_month: 6, tail_months_in_report: 21 } }, drivers: [{ month: 1, short_rate: .04 }] };

function resultEnvelope() {
  const frames = [tableFromArrays({ month: [1,2], base_nii: [100,100], nii: [80,70], delta_nii: [-20,-30] }),
    tableFromArrays({ metric: ["nii_total_$"], value: [150] }), tableFromArrays({ month: [1,2], loans: [0,0] }),
    tableFromArrays({ month: [1,2], short_rate: [.04,.04] })];
  const skeleton = { monthly: { __arrow__: 0 }, summary: { __arrow__: 1 }, base_summary: { __arrow__: 1 },
    runoff: { __arrow__: 2 }, drivers: { __arrow__: 3 }, warnings: ["Research"], provenance: preview };
  const j = Buffer.from(JSON.stringify(skeleton));
  const buffers = frames.map(f => Buffer.from(tableToIPC(f)));
  const header = Buffer.alloc(8); header.write("ARW1"); header.writeUInt32LE(j.length, 4);
  const count = Buffer.alloc(4); count.writeUInt32LE(buffers.length);
  return Buffer.concat([header, j, count, ...buffers.map(b => { const n=Buffer.alloc(4); n.writeUInt32LE(b.length); return n; }), ...buffers]);
}

test("forecast preview gates execution, displays results, and invalidates after changes", async ({ page }) => {
  const errors: string[] = [];page.on("pageerror",e=>errors.push(e.message));
  const mutations: string[] = [];
  page.on("request",r=>{if(r.method()==="PUT")mutations.push(r.url());});
  await page.route("**/api/market",r=>r.fulfill({json:{swap_tenors:[1,2,3,4,5,7,10,15,20,30],swap_rates:Array(10).fill(.04),vol_pts:[[1,2,.25]],revision:7,source:"Synthetic"}}));
  await page.route("**/api/market-data/sources",r=>r.fulfill({json:[]}));
  await page.route("**/api/market-data/snapshots",r=>r.fulfill({json:[snapshot]}));
  await page.route(`**/api/market-data/snapshots/${id}?*`,r=>r.fulfill({json:snapshot}));
  await page.route("**/api/forecasts/preview",r=>r.fulfill({json:preview}));
  await page.route("**/api/forecasts/run",async r=>{
    expect(r.request().postDataJSON()).toEqual({snapshot_id:id,scenario:"baseline",start_period:"2026-01-01",horizon_months:27,alignment:"relative_replay",expected_revision:7});
    await r.fulfill({json:{id:"forecast",status:"queued"}});
  });
  await page.route("**/api/jobs/forecast",r=>r.fulfill({json:{id:"forecast",status:"done"}}));
  await page.route("**/api/jobs/forecast/result",r=>r.fulfill({body:resultEnvelope(),contentType:"application/octet-stream"}));
  await page.goto("/");await page.getByRole("button",{name:"Open Market & Scenarios",exact:true}).click();
  await page.getByLabel("Saved research snapshots").selectOption(id);
  const run=page.getByRole("button",{name:"Run conditional forecast",exact:true});
  await expect(run).toBeDisabled();
  await page.getByRole("button",{name:"Preview forecast",exact:true}).click();
  await expect(page.getByText("Months held flat in report")).toBeVisible();
  await expect(run).toBeEnabled();await run.click();
  await expect(page.getByText(/Cumulative NII change/)).toContainText("$-50");
  await expect(page.getByRole("button",{name:"Download forecast results"})).toBeVisible();
  await page.getByLabel("Forecast scenario").selectOption("adverse");
  await expect(run).toBeDisabled();
  await expect(page.getByText(/Preview again before running/)).toBeVisible();
  expect(mutations).toEqual([]);expect(errors).toEqual([]);
});
