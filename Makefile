# Rates Workbench monorepo
.PHONY: install dev-api dev-web test-py test build

install:            ## python (editable) + node deps
	pip install -e packages/mbs-risk
	pip install fastapi "uvicorn[standard]"
	npm install

dev-api:            ## FastAPI on :8000 (seeds the WFC-proportional demo book)
	cd apps/api && uvicorn app.main:app --reload --port 8000

dev-web:            ## Vite on :5173 (proxies /api -> :8000)
	npm run dev --workspace web

test-py:            ## engine test suite (the change gates)
	cd packages/mbs-risk && python -m pytest tests/ -q

build:              ## production web build
	npm run build --workspace web
