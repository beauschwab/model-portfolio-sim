# Rates Workbench monorepo
.PHONY: install dev-api dev-web test-py test build

install:            ## python (editable) + node deps
	uv sync --project apps/api
	uv sync --project packages/mbs-risk --extra dev
	bun install

dev-api:            ## FastAPI on :8000 (seeds the WFC-proportional demo book)
	cd apps/api && uv run uvicorn app.main:app --reload --reload-dir app --port 8000

dev-web:            ## Vite on :5173 (proxies /api -> :8000)
	cd apps/web && bun run dev

test-py:            ## engine test suite (the change gates)
	cd packages/mbs-risk && uv run --extra dev python -m pytest tests/ -q

build:              ## production web build
	cd apps/web && bun run build
