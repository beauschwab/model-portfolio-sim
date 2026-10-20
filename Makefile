# Rates Workbench monorepo
.PHONY: install dev-api dev-worker dev-web dev-mcp init-db test-py test-mcp test build build-native check-architecture check-native-service

install:            ## python (editable) + node deps
	uv sync --project apps/api
	uv sync --project packages/portfolio-risk --extra dev
	bun install

init-db:            ## SQLite + synthetic example workspace (preserves existing inputs)
	cd apps/api && uv run python -m app.storage_admin init-demo

dev-worker:         ## Dedicated pricing and interactive-session worker on :8002
	cd apps/api && uv run uvicorn app.worker:app --host 127.0.0.1 --port 8002 --workers 1

dev-api:            ## FastAPI on :8000; start dev-worker for calculations
	cd apps/api && uv run uvicorn app.main:app --reload --reload-dir app --port 8000

dev-web:            ## Vite on :5173 (proxies /api -> :8000)
	cd apps/web && bun run dev

dev-mcp:            ## Stdio MCP adapter; API and worker run separately
	uv run --project apps/mcp rates-workbench-mcp

test-mcp:           ## Protocol, permissions and bounded transport
	uv run --project apps/mcp --extra dev python -m pytest apps/mcp/tests -q
	cd apps/api && uv run --extra dev --with-editable ../mcp python -m pytest tests/test_agent_routes.py tests/test_agent_mcp_integration.py -q

check-architecture: ## static boundaries and reproducible HTTP/native contracts
	python scripts/check_architecture.py
	python -m unittest discover -s scripts/tests -v
	python scripts/generate_native_contracts.py --check
	uv run --project apps/api python scripts/generate_http_contracts.py --check
	uv run --project apps/api python scripts/generate_mcp_contracts.py --check

test-py: check-architecture ## engine and API regression suites
	cd packages/portfolio-risk && uv run --extra dev python -m pytest tests/ -q
	cd apps/api && uv run --extra dev python -m pytest tests/ -q

test: test-py      ## all regression gates (browser requires Edge on Windows or Playwright Chromium)
	cd apps/web && bun run test:e2e

build-native:       ## required production financial runtime (HiGHS stays C++)
	uv run --project apps/api python scripts/build_native.py
	uv run --project apps/api python scripts/build_ledger_native.py
	uv run --project apps/api --with libclang --with cmake python scripts/build_decision_native.py

build: build-native ## production native runtime and web build
	cd apps/web && bun run build

check-native-service: ## Rust service: fmt, clippy, process tests (no Python)
	cargo fmt --manifest-path apps/api-rust/Cargo.toml -- --check
	cargo clippy --locked --manifest-path apps/api-rust/Cargo.toml --all-targets --features s3,fault-injection -- -D warnings
	cargo test --locked --manifest-path apps/api-rust/Cargo.toml --features fault-injection
