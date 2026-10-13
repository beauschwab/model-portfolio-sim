// Run with `node scripts/measure-whatif.mjs` against disposable ports 5174/8001.
import { chromium, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
const browser = await chromium.launch({ channel: process.platform === 'win32' ? 'msedge' : undefined });
const page = await browser.newPage({ viewport: { width: 1920, height: 1600 } });
page.setDefaultTimeout(30_000);
const base = 'http://127.0.0.1:5174';
const errors = []; page.on('pageerror', e => errors.push(e.message));
const settings = await (await page.request.get(`${base}/api/settings`)).json();
try {
  const configured = await page.request.put(`${base}/api/settings`, {data:{...settings,n_paths:128,n_paths_base:128,n_threads:4,horizon_months:27}});
  expect(configured.ok()).toBeTruthy();
  await page.goto(base);
  await page.getByRole('button',{name:'Open Instrument What-if',exact:true}).click();
  const panel=page.getByRole('region',{name:'Instrument What-if',exact:true});
  await expect(panel.getByRole('button',{name:'Compare',exact:true})).toBeEnabled();
  await panel.getByLabel('Full balance-sheet totals',{exact:true}).check();
  await panel.getByRole('button',{name:'Compare',exact:true}).click();
  await expect(panel.getByTestId('value-change')).toHaveText('$0',{timeout:60_000});
  const coupon=Number(await panel.getByLabel('Assumption value',{exact:true}).getAttribute('placeholder'));
  const samples=[];
  for(let i=1;i<=5;i++){
    await panel.getByLabel('Assumption value',{exact:true}).fill(String(coupon+.037*i+Number(process.env.WHATIF_EDIT_OFFSET ?? 0)));
    await page.evaluate(() => {
      window.__whatifTiming = new Promise(resolve => {
        const region = [...document.querySelectorAll('[role="region"]')].find(e => e.getAttribute('aria-label') === 'Instrument What-if');
        const click = event => {
          if (!event.target.closest('button')?.textContent.includes('Apply temporary change')) return;
          document.removeEventListener('click',click,true);
          const started=performance.now(); let hidden=false;
          const observer=new MutationObserver(() => {
            const value=region.querySelector('[data-testid="value-change"]');
            if(!value) hidden=true;
            if(hidden && value) {
              observer.disconnect();
              requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now()-started)));
            }
          });
          observer.observe(region,{childList:true,subtree:true});
          setTimeout(() => { observer.disconnect(); resolve(null); },60_000);
        };
        document.addEventListener('click',click,true);
      });
    });
    await panel.getByRole('button',{name:'Apply temporary change',exact:true}).click();
    await expect(panel.getByText('Updating comparison; previous results are hidden.')).toBeVisible();
    await expect(panel.getByTestId('value-change')).toBeVisible({timeout:60_000});
    const elapsed=await page.evaluate(() => window.__whatifTiming);
    expect(elapsed).not.toBeNull(); samples.push(elapsed);
  }
  await expect(panel.getByText('Horizon CET1 (%)',{exact:true})).toBeVisible();
  await panel.getByText('Instrument What-if',{exact:true}).scrollIntoViewIfNeeded();
  await page.screenshot({path:'../../docs/reviews/whatif-desktop.png',fullPage:true});
  expect(errors).toEqual([]);
  const sorted=[...samples].sort((a,b)=>a-b);
  const result={backend:'numpy',positions:375,paths:128,threads:4,analytics:true,includes_auxiliary:true,
    samples_ms:samples,median_ms:sorted[2],max_ms:sorted.at(-1),page_errors:errors,
    scope:'Browser-clock Apply click through result DOM publication plus two animation frames. Includes configured debounce/polling, HTTP/Arrow and render scheduling. MutationObserver measurement excludes Playwright assertion polling. Five samples, no competing jobs; not a production latency SLA.'};
  await writeFile('../../docs/reviews/2026-09-28-whatif-browser.json',JSON.stringify(result,null,2)+'\n');
  console.log(JSON.stringify(result));
} finally {
  await page.request.put(`${base}/api/settings`,{data:settings});
  await browser.close();
}
