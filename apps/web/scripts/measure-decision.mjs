// Disposable test services only: node scripts/measure-decision.mjs
import { chromium, expect } from '@playwright/test';
import { writeFile } from 'node:fs/promises';
const browser=await chromium.launch({channel:process.platform==='win32'?'msedge':undefined});
const page=await browser.newPage({viewport:{width:1920,height:1700}});
page.setDefaultTimeout(60_000);
const base='http://127.0.0.1:5174';
const errors=[];page.on('pageerror',e=>errors.push(e.message));
const settings=await(await page.request.get(`${base}/api/settings`)).json();
const results=[];
page.on('response',async response=>{
  if(!/\/api\/jobs\/[^/]+\/result$/.test(response.url())||!response.ok())return;
  const b=await response.body();const n=b.readUInt32LE(4);const r=JSON.parse(b.subarray(8,8+n).toString());
  if(r.session_id)results.push(r);
});
try{
  await page.request.put(`${base}/api/settings`,{data:{...settings,n_paths:128,n_paths_base:128,n_threads:4,horizon_months:27}});
  await page.goto(base);
  await page.getByRole('button',{name:'Open Decision Lab',exact:true}).click();
  const panel=page.getByRole('region',{name:'Decision Lab',exact:true});
  async function action(button){
    await page.evaluate(()=>{
      const region=document.querySelector('[role="region"][aria-label="Decision Lab"]');
      const version=()=>[...region.querySelectorAll('span')].find(e=>/^Version \d+ ·/.test(e.textContent))?.textContent;
      const old=version();
      window.__decisionTiming=new Promise(resolve=>{
        const click=event=>{
          if(!event.target.closest('button')?.textContent.match(/Apply and optimize|Build decision session/))return;
          document.removeEventListener('click',click,true);const start=performance.now();
          const observer=new MutationObserver(()=>{if(version()&&version()!==old){observer.disconnect();requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(performance.now()-start)));}});
          observer.observe(region,{subtree:true,childList:true,characterData:true});
          setTimeout(()=>{observer.disconnect();resolve(null);},60_000);
        };document.addEventListener('click',click,true);
      });
    });
    await panel.getByRole('button',{name:button,exact:true}).click();
    const elapsed=await page.evaluate(()=>window.__decisionTiming);
    expect(elapsed).not.toBeNull();return elapsed;
  }
  const initialization=await action('Build decision session');
  const constraints=[],instruments=[],templates=[];
  for(let i=0;i<5;i++){
    await panel.getByLabel('New asset cap ($m)',{exact:true}).fill(String(29900-i*100));
    constraints.push(await action('Apply and optimize'));
  }
  await panel.getByLabel('Include instrument edit',{exact:true}).check();
  for(let i=0;i<5;i++){
    await panel.getByLabel('Decision assumption value',{exact:true}).fill(String(.065+i*.0001));
    instruments.push(await action('Apply and optimize'));
  }
  await panel.getByLabel('Include instrument edit',{exact:true}).uncheck();
  await panel.getByLabel('Include template edit',{exact:true}).check();
  for(let i=0;i<3;i++){
    await panel.getByLabel('Template spread (bp)',{exact:true}).fill(String(251+i));
    templates.push(await action('Apply and optimize'));
  }
  await panel.getByLabel('Allocation scale (%)',{exact:true}).fill('50');
  await panel.getByRole('button',{name:'Replay allocation',exact:true}).click();
  await expect(panel.getByText('Manual allocation replay',{exact:true})).toBeVisible();
  await panel.evaluate(e=>{e.scrollTop=0;});
  await page.screenshot({path:'../../docs/reviews/decision-desktop.png',fullPage:true});
  await panel.screenshot({path:'../../docs/reviews/decision-panel.png'});
  await panel.evaluate(e=>{e.scrollTop=e.scrollHeight;});
  await panel.screenshot({path:'../../docs/reviews/decision-results.png'});
  await panel.evaluate(e=>{e.scrollTop=0;});
  await page.setViewportSize({width:850,height:1000});
  await expect(panel.getByRole('button',{name:'Apply and optimize',exact:true})).toBeVisible();
  await page.screenshot({path:'../../docs/reviews/decision-narrow.png',fullPage:true});
  expect(errors).toEqual([]);
  const stats=a=>({samples_ms:a,median_ms:[...a].sort((x,y)=>x-y)[Math.floor(a.length/2)],max_ms:Math.max(...a)});
  const report={scope:'Browser clock click to version publication plus two animation frames; real API, Arrow envelope, 75ms polling and render scheduling. One base scenario, 375 positions, 128 paths, 4 threads, 27 months. No SLA claim.',initialization_ms:initialization,
    constraint_update:stats(constraints),instrument_update:stats(instruments),template_update:stats(templates),page_errors:errors,
    backend_results:results.map(r=>({version:r.version,validated:r.validated,work:r.work,timings_ms:r.timings_ms,solver:r.solver}))};
  await writeFile('../../docs/reviews/2026-09-28-decision-browser.json',JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify({...report,backend_results:`${results.length} captured job results`}));
}finally{
  for(const sid of new Set(results.map(r=>r.session_id)))await page.request.delete(`${base}/api/decision/sessions/${sid}`);
  await page.request.put(`${base}/api/settings`,{data:settings});await browser.close();
}
