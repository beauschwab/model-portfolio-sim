import { defineConfig } from "@playwright/test";

// Keep browser tests off the user's running workbench and mutable input state.
const apiPort = Number(process.env.WORKBENCH_TEST_API_PORT ?? 8001);
const webPort = Number(process.env.WORKBENCH_TEST_WEB_PORT ?? 5174);
const reuse = process.env.WORKBENCH_REUSE_TEST_SERVERS === '1';

export default defineConfig({
  testDir: "./tests", workers: 1, timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${webPort}`, viewport: { width: 1680, height: 1100 },
    channel: process.platform === "win32" ? "msedge" : undefined,
    trace: "retain-on-failure",
  },
  webServer: [
    { command: `uv run --project ../api uvicorn app.main:app --app-dir ../api --port ${apiPort}`,
      url: `http://127.0.0.1:${apiPort}/health`, reuseExistingServer: reuse,
      env: { NUMBA_NUM_THREADS: "4" }, timeout: 120_000 },
    { command: `bun run dev -- --host 127.0.0.1 --port ${webPort} --strictPort`, url: `http://127.0.0.1:${webPort}`,
      env: { WORKBENCH_API_URL: `http://127.0.0.1:${apiPort}` }, reuseExistingServer: reuse },
  ],
});
