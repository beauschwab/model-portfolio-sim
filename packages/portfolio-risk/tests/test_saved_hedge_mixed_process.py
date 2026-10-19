"""saved-hedge-mixed-1: formal hedge journals in the shared daily Sim, gated by independent replay.

The native model-ledger binary is the only financial runtime. The independent hedge reference
replays each relationship from raw inputs; the mixed reference rebuilds the ordered shared GL,
daily balances and research path from the persisted partitions.
"""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import polars as pl
import pytest
from portfolio_risk.analytics import saved_derivative_runtime as runtime
from portfolio_risk.analytics.saved_derivative_identity import encoded, loads
from portfolio_risk.analytics.saved_derivative_mixed_reference import verify_preparation

FIXTURE = Path(__file__).parent / 'fixtures/saved_hedge_mixed/base.json'
DERIVATIVE_ONLY = Path(__file__).parent / 'fixtures/saved_derivative_mixed/base.json'


def raw():
    return loads(FIXTURE.read_bytes())


def forbid_callbacks(monkeypatch):
    from portfolio_risk.core import quant_native, native
    from portfolio_risk.analytics import ledger_native

    def forbidden(*a, **k):
        raise AssertionError('financial callback/DLL fallback forbidden')
    for module, attribute in [(quant_native, '_load'), (quant_native, 'call'), (quant_native, 'term_call'),
                              (native, 'discount_backend'), (ledger_native, 'validate_spec')]:
        monkeypatch.setattr(module, attribute, forbidden)


@pytest.fixture(scope='module')
def header():
    """The native preparation header (prepared request and source receipt) for the fixture."""
    body = encoded(dict(request=raw(), execution_timeout_ms=300000))
    out = subprocess.run([str(runtime.binary_path())], input=body, capture_output=True, check=False)
    assert out.returncode == 0, out.stderr[-2000:]
    first = json.loads(out.stdout.split(b'\n', 1)[0])
    return first['workflow']


def journal(manifest):
    root = manifest.parent
    m = loads(manifest.read_bytes())
    frames = [pl.read_parquet(root / part['path']) for part in m['tables']['journal']['parts']]
    return pl.concat(frames)


def test_formal_hedge_journal_posts_once_in_the_shared_daily_gl(tmp_path, monkeypatch):
    forbid_callbacks(monkeypatch)
    manifest = runtime.run(raw(), tmp_path, timeout=300., partition_rows=64)
    m = loads(manifest.read_bytes())
    receipt = m['validation']
    assert m['model_version'] == 'saved-hedge-mixed-1'
    assert receipt['schema'] == 'saved-hedge-mixed-verification-1'
    for flag in ('verified', 'formal_hedge_relationships_replayed', 'ordered_mixed_journal_recomputed',
                 'single_daily_gl_verified', 'complete_gross_rights_verified', 'complete_hedge_basis_oci_tail_verified'):
        assert receipt[flag] is True, flag
    for flag in ('empirical_validation', 'accounting_standard_approval', 'capital_ftp_eligible'):
        assert receipt[flag] is False, flag
    rows = journal(manifest)
    gls = set(rows['gl_account'].to_list())
    assert {'cash_flow_hedge_oci', 'hedged_item_basis_adjustment', 'pnl:hedged_item_basis',
            'pnl:hedged_forecast_reclassification'} <= gls
    # The undesignated mark is never posted for a re-owned cash-flow hedge mark: OCI owns it.
    hedge_rows = rows.filter(pl.col('instrument_id').str.starts_with('@hedge:'))
    assert hedge_rows.height > 0
    balances = rows.with_columns((pl.col('debit') - pl.col('credit')).alias('v')).group_by(
        'scenario', 'gl_account').agg(pl.col('v').sum())
    closing = {(r['scenario'], r['gl_account']): r['v'] for r in balances.iter_rows(named=True)}
    for scenario in ('baseline',):
        assert abs(closing.get((scenario, 'cash_flow_hedge_oci'), 0.)) < 1e-9
        assert abs(closing.get((scenario, 'hedged_item_basis_adjustment'), 0.)) < 1e-9


@pytest.mark.parametrize('target', ['native_hedge', 'approval', 'missing', 'overlay', 'raw'])
def test_independent_preparation_rejects_tampering(header, target):
    request, prepared, receipt = raw(), deepcopy(header['prepared_request']), deepcopy(header['source_receipt'])
    if target == 'native_hedge':
        intent = next(i for i in receipt['native_hedges'][0]['intents'] if i['event_id'].startswith('hedged_item_basis'))
        intent['postings'][0]['debit_amount'] += 1e-6
        intent['postings'][1]['debit_amount'] -= 1e-6
    elif target == 'approval':
        receipt['accounting_standard_approval'] = True
    elif target == 'missing':
        receipt.pop('native_hedges')
    elif target == 'overlay':
        for run in request['owners'][0]['runs']:
            run['hedge']['policy']['assessment']['lower_bound'] = 0.79
        prepared = deepcopy(request)
    else:
        request['owners'][0]['runs'][0]['hedge']['run']['revision'] = 'r2'
    with pytest.raises((ValueError, ArithmeticError)):
        verify_preparation(request, prepared, receipt)


def test_native_refusals_without_fallback(tmp_path):
    for change in (lambda r: r['owners'][0]['runs'][0]['hedge'].update(derivative={}),
                   lambda r: [run['hedge'].update(entity='other') for run in r['owners'][0]['runs']],
                   lambda r: r['owners'][0]['runs'].append(dict(deepcopy(r['owners'][0]['runs'][0]), ledger_scenario='zero'))):
        request = raw()
        change(request)
        with pytest.raises((RuntimeError, ValueError)):
            runtime.run(request, tmp_path, timeout=120.)
    assert not list(tmp_path.rglob('manifest.json'))


def test_cancel_and_binary_mismatch_leave_no_manifest(tmp_path):
    with pytest.raises(InterruptedError):
        runtime.run(raw(), tmp_path, cancelled=lambda: True, timeout=10.)
    with pytest.raises(RuntimeError):
        runtime.run(raw(), tmp_path, expected_binary='0' * 64, timeout=10.)
    assert not list(tmp_path.rglob('manifest.json'))


def test_derivative_only_schema_is_unchanged_by_the_hedge_accounts(tmp_path, monkeypatch):
    forbid_callbacks(monkeypatch)
    manifest = runtime.run(loads(DERIVATIVE_ONLY.read_bytes()), tmp_path, timeout=120., partition_rows=7)
    m = loads(manifest.read_bytes())
    assert m['model_version'] == 'saved-derivative-mixed-1'
    assert m['validation']['schema'] == 'saved-derivative-mixed-verification-1'
    assert not any(gl.startswith(('cash_flow_hedge', 'hedged_', 'pnl:hedged')) for gl in journal(manifest)['gl_account'].to_list())


def test_designation_changes_presentation_not_economics(tmp_path, monkeypatch):
    """#35 critical matrix: the same saved source under economic and FVH/CFH designation keeps
    identical daily cash, derivative carrying and collateral, and complete-tail equity; only the
    split between earnings, OCI and hedged-item basis moves under the reviewed rules."""
    forbid_callbacks(monkeypatch)
    designated = raw()
    economic = deepcopy(designated)
    economic['schema'] = 'saved-derivative-mixed-1'
    for owner in economic['owners']:
        for run in owner['runs']:
            run.pop('hedge')
    frames = {}
    for name, request in (('designated', designated), ('economic', economic)):
        frames[name] = journal(runtime.run(request, tmp_path / name, timeout=300., partition_rows=64)).with_columns(
            (pl.col('debit') - pl.col('credit')).alias('v'))
    economic_gls = ('cash', 'derivative_asset', 'derivative_liability', 'derivative_posted_vm_asset',
                    'derivative_received_vm_liability', 'asset_principal', 'dated_interest_receivable')

    def daily(frame):
        rows = frame.filter(pl.col('gl_account').is_in(economic_gls)).group_by('scenario', 'day', 'gl_account').agg(
            pl.col('v').sum())
        return {(r['scenario'], r['day'], r['gl_account']): r['v'] for r in rows.iter_rows(named=True)}
    a, b = daily(frames['designated']), daily(frames['economic'])
    assert a.keys() == b.keys()
    for key in a:
        assert a[key] == pytest.approx(b[key], rel=1e-12, abs=1e-9), key

    def equity(frame):
        rows = frame.filter(pl.col('gl_account').str.starts_with('pnl:') | pl.col('gl_account').is_in(
            ('opening_equity', 'cash_flow_hedge_oci', 'hedged_item_basis_adjustment')))
        return rows['v'].sum()
    assert equity(frames['designated']) == pytest.approx(equity(frames['economic']), rel=1e-12, abs=1e-9)
    oci = frames['designated'].filter(pl.col('gl_account') == 'cash_flow_hedge_oci')
    assert oci.height > 0 and oci['v'].abs().max() > 1e-9
    assert not frames['economic'].filter(pl.col('gl_account').str.contains('hedge')).height


def test_large_book_reconciliation_residual_scales_with_its_operands(tmp_path, monkeypatch):
    """A 200-loan backbook: the daily residual (assets - liabilities - equity) of large gross
    sums is checked at the operands' 1e-12 relative scale, so roundoff does not refuse a
    correct journal while every financial amount keeps the fixed tolerances."""
    import importlib.util
    spec = importlib.util.spec_from_file_location('scaled', Path(__file__).resolve().parents[3] / 'scripts/make_scaled_hedge_book.py')
    scaled = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(scaled)
    forbid_callbacks(monkeypatch)
    manifest = runtime.run(scaled.scaled(200), tmp_path, timeout=600., partition_rows=65536)
    m = loads(manifest.read_bytes())
    assert m['validation']['verified'] is True
    assert sum(p['rows'] for p in m['tables']['journal']['parts']) > 200 * 100
