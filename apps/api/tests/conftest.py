"""Existing API/engine contract tests use explicit isolated memory mode.

Durable integration tests select file-backed SQLite or TEST_POSTGRES_URL.
"""
import os

import pytest

# Existing contract tests cover the full surface; test_supported_scope.py covers the v1 default.
# Set at import, not per test: module- and session-scoped fixtures admit work before any test runs.
os.environ["WORKBENCH_EXPERIMENTAL_MODELS"] = "1"


@pytest.fixture(autouse=True)
def isolated_runtime(monkeypatch):
    monkeypatch.setenv("WORKBENCH_EXECUTION", "memory")
