"""Explicit acceptance tier for the saved hedge journal: larger books, hash-bound input, same gates.

Without the tier every historical bound applies. With `acceptance_60000` the native process binds
the exact input envelope by SHA-256, streams the shared journal, and the independent preparation
and persisted replay still gate publication.
"""
import importlib.util
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics import saved_derivative_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import loads
from portfolio_risk.analytics.saved_derivative_mixed_reference import verify_preparation

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('scaled', ROOT.parent / 'scripts/make_scaled_hedge_book.py')
scaled = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scaled)


def test_tier_publishes_past_the_historical_cap_with_hash_bound_input(tmp_path):
    raw = scaled.scaled(600, tier=True)
    manifest = runtime.run(raw, tmp_path, timeout=600., partition_rows=65536)
    m = loads(manifest.read_bytes())
    assert m['validation']['verified'] is True
    source = json.loads((manifest.parent / m['modeled_source']['path']).read_bytes())
    # The header carries the input hash and no echo of the raw request.
    assert source['input_sha256'] == m['input']['sha256']
    assert source['source_receipt']['input_sha256'] == m['input']['sha256']
    assert 'prepared_request' not in source and 'original_request' not in source['source_receipt']


def test_historical_bounds_are_unchanged_without_the_tier(tmp_path):
    with pytest.raises(RuntimeError, match='at most 512'):
        runtime.run(scaled.scaled(513), tmp_path, timeout=120., partition_rows=65536)
    raw = scaled.scaled(2)
    raw['scale_tier'] = 'acceptance_120000'
    with pytest.raises((ValueError, RuntimeError)):
        runtime.run(raw, tmp_path, timeout=120., partition_rows=65536)
    assert not list(tmp_path.rglob('manifest.json'))


def test_tiered_preparation_requires_the_matching_input_hash():
    raw = scaled.scaled(3, tier=True)
    receipt = dict(schema=raw['schema'], scale_tier='acceptance_60000', input_sha256='a' * 64, native_sources=[],
                   native_hedges=[], joined_work_units=1, owner_capacity_bytes=1, empirical_validation=False,
                   accounting_standard_approval=False, capital_ftp_eligible=False)
    with pytest.raises((ValueError, ArithmeticError)):
        verify_preparation(raw, None, receipt, input_sha256='b' * 64)
    with pytest.raises((ValueError, ArithmeticError)):
        verify_preparation(raw, raw, receipt, input_sha256=None)
