import { defineConfig } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/* Liquidity lifecycle browser flow against the Rust service only (issue #48). The only servers are
   `workbench-native dev` (Rust API and leased worker in one process, over a throwaway SQLite
   database and artifact folder) and Vite. No Python API, worker, verifier or build runs.
   Requires: `cargo build --manifest-path apps/api-rust/Cargo.toml` and WORKBENCH_TEST_LEDGER_EXE
   naming a built `portfolio-model-ledger`. */
const nativePort = Number(process.env.WORKBENCH_TEST_NATIVE_PORT ?? 8013);
const webPort = Number(process.env.WORKBENCH_TEST_NATIVE_WEB_PORT ?? 5175);
const scratch = mkdtempSync(join(tmpdir(), "wbn-browser-"));
const exe = resolve(dirname(fileURLToPath(import.meta.url)), "../api-rust/target/debug", process.platform === "win32" ? "workbench-native.exe" : "workbench-native");
const ledger = process.env.WORKBENCH_TEST_LEDGER_EXE ?? "";

export default defineConfig({
  testDir: "./tests", testMatch: "liquidity-lifecycle.spec.ts", workers: 1, timeout: 180_000,
  use: {
    baseURL: `http://127.0.0.1:${webPort}`, viewport: { width: 1680, height: 1100 },
    channel: process.platform === "win32" ? "msedge" : undefined,
    trace: "retain-on-failure",
  },
  webServer: [
    { command: `"${exe}" dev`, url: `http://127.0.0.1:${nativePort}/native/health`, reuseExistingServer: false, timeout: 60_000,
      env: {
        DATABASE_URL: `sqlite:///${join(scratch, "workbench.db").replaceAll("\\", "/")}`,
        ARTIFACT_URL: join(scratch, "artifacts"),
        WORKBENCH_TENANT_ID: "browser", WORKBENCH_WORKSPACE_ID: "lifecycle",
        WORKBENCH_NATIVE_BIND: `127.0.0.1:${nativePort}`,
        WORKBENCH_ALLOWED_ORIGIN: `http://127.0.0.1:${webPort}`,
        PORTFOLIO_MODEL_LEDGER_EXE: ledger,
        WORKBENCH_LEASE_SECONDS: "10",
      } },
    { command: `bun run dev -- --host 127.0.0.1 --port ${webPort} --strictPort`, url: `http://127.0.0.1:${webPort}`, reuseExistingServer: false,
      env: { WORKBENCH_NATIVE_URL: `http://127.0.0.1:${nativePort}`, WORKBENCH_API_URL: "http://127.0.0.1:9" } },
  ],
});
