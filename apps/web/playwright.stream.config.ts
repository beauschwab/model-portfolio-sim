import { defineConfig } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Dedicated durable stack; never point this configuration at a user's database.
const apiPort = Number(process.env.WORKBENCH_TEST_API_PORT ?? 8013);
const workerPort = Number(process.env.WORKBENCH_TEST_WORKER_PORT ?? 8014);
const webPort = Number(process.env.WORKBENCH_TEST_WEB_PORT ?? 5186);
const folder = join(tmpdir(), `wbs-${randomUUID().slice(0, 8)}`);
const env = {
  NUMBA_NUM_THREADS: '4', WORKBENCH_ENV: 'development', WORKBENCH_EXECUTION: 'external',
  DATABASE_URL: `sqlite:///${join(folder, 'test.db').replaceAll('\\', '/')}`,
  ARTIFACT_URL: join(folder, 'objects'), WORKBENCH_TENANT_ID: 'browser-tests',
  WORKBENCH_WORKSPACE_ID: randomUUID(), WORKBENCH_BALANCE_LARGE_BOOK: '0',
  WORKBENCH_API_TOKEN: '', WORKBENCH_WORKER_TOKEN: '', WORKBENCH_SEED_DEMO: '1',
  WORKBENCH_WORKER_URL: `http://127.0.0.1:${workerPort}`,
};
export default defineConfig({
  testDir: './tests', testMatch: ['streamed-balance.spec.ts', 'balance-stress.spec.ts'],
  workers: 1, timeout: 60_000,
  use: { baseURL: `http://127.0.0.1:${webPort}`, viewport: { width: 1680, height: 1100 },
    channel: process.platform === 'win32' ? 'msedge' : undefined, trace: 'retain-on-failure' },
  webServer: process.env.WORKBENCH_REUSE_TEST_SERVERS === '1' ? [] : [
    { command: `uv run --project ../api uvicorn app.main:app --app-dir ../api --port ${apiPort}`,
      url: `http://127.0.0.1:${apiPort}/health`, env, timeout: 120_000 },
    { command: `uv run --project ../api uvicorn app.worker:app --app-dir ../api --port ${workerPort}`,
      url: `http://127.0.0.1:${workerPort}/state`, env, timeout: 120_000 },
    { command: `bun run dev -- --host 127.0.0.1 --port ${webPort} --strictPort`,
      url: `http://127.0.0.1:${webPort}`, env: { WORKBENCH_API_URL: `http://127.0.0.1:${apiPort}` } },
  ],
});
