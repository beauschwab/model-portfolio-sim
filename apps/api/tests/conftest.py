"""Existing API/engine contract tests use explicit isolated memory mode.

Durable integration tests select file-backed SQLite or TEST_POSTGRES_URL.
"""
import pytest


@pytest.fixture(autouse=True)
def isolated_runtime(monkeypatch):
    monkeypatch.setenv("WORKBENCH_EXECUTION", "memory")
