# Rust workbench service contracts (`apps/api-rust`, issue #47)

- No financial calculation and no Python. The service invokes the native `portfolio-model-ledger`
  process and the independent `portfolio-verification-native` crate. Never depend on the Risk,
  Ledger, Decision or model-core crates (a test enforces this).
- Shared tables keep the SQLAlchemy definitions in `apps/api/app/repository.py`; change them only
  together, and keep `tests/boundary.rs` passing. Rust-owned tables change only through a new entry
  in `migrate::MIGRATIONS`; never edit an applied migration (its SHA-256 is recorded).
- Every statement is scoped to the configured tenant/workspace. Mutations take the workspace row
  lock first. Native jobs use only `native_*` statuses.
- Authority (raw input, executable identity, original deadline) comes from `native_job_authority`,
  never from a job view. Retries never reset the deadline.
- Write and reread objects before any reference commits. Publication happens only in
  `Repo::publish`, under lease, epoch, authority, token and deadline checks.
- Regenerate `contract/openapi.json` and `../web/src/lib/native-api.generated.ts` with
  `workbench-native contract openapi|typescript` after changing `src/contract.rs`.
- Gate: `cargo fmt --check`, `cargo clippy --all-targets --features s3,fault-injection -- -D warnings`,
  `cargo test --features fault-injection`. Set `TEST_POSTGRES_URL`, `TEST_S3_ENDPOINT`/`TEST_S3_BUCKET`
  and `WORKBENCH_TEST_LEDGER_EXE` to repeat the scenarios on PostgreSQL, S3 and the live producer.
  `fault-injection` is test-only; never enable it in a deployment.
- Saved-book composition (`src/compose.rs`) is structural only: it binds the saved revision, checks
  references and explicit treatments, and assembles the request. Never compute amounts there, and
  never map `formal` treatment to the economic lane. Liquidity amounts shown in the workbench come
  from the verified native receipt (`steps`) or published tables.
- `workbench-native dev` runs a native workspace, the API and the worker in one process for local
  development and `apps/web/playwright.native.config.ts`; deployments run `api` and `worker` apart.
