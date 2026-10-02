# Rates Workbench monorepo
.PHONY: install dev-api dev-worker dev-web init-db test-py test build build-native

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

test-py:            ## engine and API regression suites
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
