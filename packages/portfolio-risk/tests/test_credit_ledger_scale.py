"""credit-ledger-1 at book scale: reconciliation sums and the explicit acceptance tier.

The native daily reconciliation and the independent persisted check both compare account totals
that are sums of thousands of balances in different orders. Both now allow the floating summation
bound (terms * eps * summed magnitude) for those aggregates only, so a correct large book is no
longer refused by rounding. The acceptance tier is the only way past 62,000 run-position contracts.
"""
import importlib.util
from pathlib import Path

import pytest

from portfolio_risk.analytics.credit_ledger import run_credit_ledger
from portfolio_risk.analytics.saved_derivative_identity import loads

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('scaled_credit', ROOT / 'scripts/make_scaled_credit_book.py')
scaled = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scaled)


def test_a_1500_position_credit_book_reconciles_and_verifies(tmp_path):
    manifest = run_credit_ledger(scaled.scaled(1500), tmp_path, large_book=True, timeout=600.)
    m = loads(manifest.read_bytes())
    assert m['validation']['journal_replayed'] and m['validation']['credit_source_reconciled']
    assert m['validation']['credit_states'] == 1500 * 2 * 30


def test_the_acceptance_tier_is_explicit_and_bounded(tmp_path):
    raw = scaled.scaled(3, tier=True)
    manifest = run_credit_ledger(raw, tmp_path, large_book=True, timeout=120.)
    assert loads(manifest.read_bytes())['validation']['journal_replayed']
    raw = scaled.scaled(3)
    raw['scale_tier'] = 'acceptance_120000'
    with pytest.raises((ValueError, RuntimeError)):
        run_credit_ledger(raw, tmp_path / 'refused', large_book=True, timeout=120.)
    assert not list((tmp_path / 'refused').rglob('manifest.json'))
