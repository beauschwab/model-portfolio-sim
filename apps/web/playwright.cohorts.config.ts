import { defineConfig } from '@playwright/test';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Dedicated ports, SQLite file and object root; never touch the user's workspace.
const folder=mkdtempSync(join(tmpdir(),'cohort-browser-'));
const env={WORKBENCH_EXECUTION:'external',WORKBENCH_ENV:'development',WORKBENCH_SEED_DEMO:'1',
  DATABASE_URL:`sqlite:///${join(folder,'state.db').replaceAll('\\','/')}`,ARTIFACT_URL:join(folder,'objects'),
  WORKBENCH_TENANT_ID:'tests',WORKBENCH_WORKSPACE_ID:'cohort-browser',
  WORKBENCH_WORKER_URL:'http://127.0.0.1:8026',WORKBENCH_WORKER_TOKEN:'',WORKBENCH_API_TOKEN:'',NUMBA_NUM_THREADS:'2'};
export default defineConfig({testDir:'./tests',testMatch:['cohorts.spec.ts','treasury.spec.ts','native-settings.spec.ts'],workers:1,timeout:90000,
  use:{baseURL:'http://127.0.0.1:5186',viewport:{width:1680,height:1100},channel:process.platform==='win32'?'msedge':undefined,trace:'retain-on-failure'},
  webServer:[
    {command:'uv run --no-sync --project ../api uvicorn app.main:app --app-dir ../api --host 127.0.0.1 --port 8025',url:'http://127.0.0.1:8025/health',env,timeout:120000},
    {command:'uv run --no-sync --project ../api uvicorn app.worker:app --app-dir ../api --host 127.0.0.1 --port 8026',url:'http://127.0.0.1:8026/state',env,timeout:120000},
    {command:'npm run dev -- --host 127.0.0.1 --port 5186 --strictPort',url:'http://127.0.0.1:5186',env:{WORKBENCH_API_URL:'http://127.0.0.1:8025'}}
  ]});
