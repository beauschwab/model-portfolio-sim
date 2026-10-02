"""Existing numerical tests explicitly exercise the independent Python reference.

Native ownership/parity tests override this context with compute_backend='rust'.
Production-default tests use a fresh subprocess, outside this reference harness.
"""
import pytest
from portfolio_risk.core.runtime import RunConfig, run_context


@pytest.fixture(autouse=True)
def python_reference():
    with run_context(RunConfig(compute_backend='python')):
        yield
