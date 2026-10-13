"""Real native graph, LP, independent pricing/strategy replay, and rollback gates."""
from contextlib import contextmanager
import copy

import numba
import numpy as np
import pytest

from portfolio_risk.strategy import decision
from portfolio_risk.strategy.unitlib import build_unit_library, evaluate_strategy
from portfolio_risk.strategy.optimizer import optimize_balance_sheet
from portfolio_risk.analytics.whatif import apply_overrides
from portfolio_risk.analytics.incremental import price_books, SUPPORTED_BOOKS
from portfolio_risk.core.runtime import RunConfig, run_context


LIMITS = dict(lcr_min=.01, nsfr_min=.01, cet1_min=.001, eve_limit=1.,
              max_total_assets=1e7, cash_budget=1e7, commercial=[])


@pytest.fixture(scope='module', params=['python', 'rust'])
def inputs(request):
    from portfolio_risk.core.native import library_path
    if request.param == "rust" and not library_path().is_file():
        pytest.skip("build native product backend")
    from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history
    old = numba.get_num_threads()
    numba.set_num_threads(2)
    bs = model_balance_sheet(scale=.001, basis='amortized_cost', include_markets_bs=True)
    sr, vp = demo_market()
    yield dict(books={k: bs[k].head(2) for k in SUPPORTED_BOOKS}, asof=bs['asof'],
        swap_rates=sr, vol_pts=vp, config=RunConfig(16, 16, 6, compute_backend=request.param),
        mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history(), constraints=LIMITS,
        extras={'mm': bs['mm'], 'equity': bs['equity']},
        markets=[('base', sr, vp, 0.), ('up10bp', sr+.001, vp, .0002)])
    numba.set_num_threads(old)


@pytest.fixture
def session(inputs):
    try:
        decision.NativeDecision()
    except RuntimeError:
        pytest.skip('optional native decision library not built')
    s = decision.DecisionSession(**inputs)
    try:
        s.update(version=0)
        yield s
    finally:
        s.close()


def test_native_optimizer_matches_scipy_and_replays(session):
    r = session.result
    ref = optimize_balance_sheet(list(zip(session.libraries, session.bases)), **LIMITS)
    assert r['validated'] and ref['validated']
    np.testing.assert_allclose(r['worst_case_nii_$'], ref['worst_case_nii_$'], rtol=1e-9, atol=.01)
    manual = session.evaluate(r['allocation'], session.version)
    assert manual['replay'] == r['replay']


def test_constraint_only_never_calls_product_engines(session, monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('constraint-only edit invoked pricing')
    monkeypatch.setattr(decision, 'price_books', forbidden)
    monkeypatch.setattr(session, '_build_units', forbidden)
    out = session.update(version=session.version, constraints=LIMITS | {'max_total_assets': 5e6})
    assert out['solver']['model_reused']
    assert out['work']['positions_repriced'] == out['work']['templates_rebuilt'] == 0
    assert out['total_new_assets_$'] <= 5e6 + .01
    assert out['work']['ffi_request_bytes'] < 2000


def test_null_asset_cap_replays_with_commercial_bound(session):
    constraints = LIMITS | {'max_total_assets': None, 'commercial': [
        {'label':'commercial cap', 'template': 'ALL_ASSET', 'sense': '<=', 'rhs': 5e6}]}
    out = session.update(version=session.version, constraints=constraints)
    assert out['feasible'] and out['validated']
    assert out['total_new_assets_$'] <= 5e6 + .01
    assert constraints['commercial'] == [{'label':'commercial cap', 'template': 'ALL_ASSET', 'sense': '<=', 'rhs': 5e6}]


def test_five_book_deltas_match_full_repricing(session, inputs):
    fields = {'mbs': ('wac', .06), 'loans': ('coupon_or_spread', .06),
              'debt': ('coupon_or_spread', .04), 'deposits': ('rate_paid', .015), 'cds': ('rate', .035)}
    patches, edits = {}, {}
    for book, (field, value) in fields.items():
        ident = session.books[book]['cusip' if book == 'mbs' else 'id'][0]
        patches[book] = {ident: {field: value}}
        edits[f'{book}:{ident}'] = {field: value}
    r = session.update(version=session.version, edits=edits)
    assert r['work']['positions_repriced'] == 5
    for graph in r['work']['pricing_graph']:
        assert graph.get('cashflows:loans', {}).get('computed', 0) <= 3
    revised = apply_overrides(session.books, patches)
    for si, (_, sr, vp, spread) in enumerate(session.markets):
        full = price_books(session.books, **session.pricing, valuation_books=revised,
                           scenario_market=(sr, vp), spread_shift=spread, balance_sheet_extras=inputs['extras'])
        base = full['kpis'] | {'nii_total_$': full['nii']['total']}
        for key, value in decision._base(session.libraries[si], base).items():
            np.testing.assert_allclose(r['bases'][si][key], value, rtol=1e-9, atol=1e-5)
    reset = {key: {field: None for field in patch} for key, patch in edits.items()}
    restored = session.update(version=session.version, edits=reset)
    initial = decision.DecisionSession(**inputs)
    try:
        for actual, expected in zip(session.bases, initial.bases):
            np.testing.assert_allclose(actual['nii_total_$'], expected['nii_total_$'], rtol=1e-12)
        assert restored['work']['positions_repriced'] == 5
    finally:
        initial.close()


def test_mbs_prepay_multiplier_edit_matches_full_repricing(session, inputs):
    """A pool's speed multiplier is a decision edit like any term: Rust patches it,
    reprices only that pool, and matches a full repricing of the revised book."""
    ident = session.books['mbs']['cusip'][0]
    r = session.update(version=session.version, edits={f'mbs:{ident}': {'prepay_mult': 2.5}})
    assert r['work']['positions_repriced'] == 1
    revised = apply_overrides(session.books, {'mbs': {ident: {'prepay_mult': 2.5}}})
    for si, (_, sr, vp, spread) in enumerate(session.markets):
        full = price_books(session.books, **session.pricing, valuation_books=revised,
                           scenario_market=(sr, vp), spread_shift=spread, balance_sheet_extras=inputs['extras'])
        base = full['kpis'] | {'nii_total_$': full['nii']['total']}
        for key, value in decision._base(session.libraries[si], base).items():
            np.testing.assert_allclose(r['bases'][si][key], value, rtol=1e-9, atol=1e-5)
    restored = session.update(version=session.version, edits={f'mbs:{ident}': {'prepay_mult': None}})
    initial = decision.DecisionSession(**inputs)
    try:
        for actual, expected in zip(session.bases, initial.bases):
            np.testing.assert_allclose(actual['nii_total_$'], expected['nii_total_$'], rtol=1e-12)
        assert restored['work']['positions_repriced'] == 1
    finally:
        initial.close()


@pytest.mark.parametrize('template', ['agency_mbs', 'cml_fixed_5y', 'cd_2y', 'mmda_growth'])
def test_selective_template_rebuild_matches_full_library(session, template):
    out = session.update(version=session.version, templates={template: {'spread_bp': 275.}})
    assert out['changed_templates'] == [template] and out['work']['positions_repriced'] == 0
    with run_context(session.config):
        for si, (_, sr, vp, _) in enumerate(session.markets):
            full = session._build_units(sr, vp, template_overrides={template: {'spread_bp': 275.}})
            for field in ('nii', 'runoff', 'balance', 'dv01'):
                np.testing.assert_allclose(session.libraries[si][field], full[field], rtol=1e-9, atol=1e-10)


def test_failed_validation_and_publication_leave_session_unchanged(session, monkeypatch):
    version, original = session.version, copy.deepcopy(session.bases)
    real = decision.validate_replay
    monkeypatch.setattr(decision, 'validate_replay', lambda *a: (_ for _ in ()).throw(RuntimeError('forced replay failure')))
    with pytest.raises(RuntimeError, match='forced replay'):
        session.update(version=version, constraints=LIMITS | {'max_total_assets': 5e6})
    assert session.version == version and session.bases == original
    monkeypatch.setattr(decision, 'validate_replay', real)
    @contextmanager
    def deny():
        raise RuntimeError('snapshot changed')
        yield
    with pytest.raises(RuntimeError, match='snapshot changed'):
        session.update(version=version, publish_guard=deny)
    assert session.version == version
    assert session.update(version=version)['validated']


def test_invalid_edits_stale_versions_and_infeasibility(session):
    key = next(iter(session.index))
    for edits in ({'loans:missing': {'coupon_or_spread': .05}}, {key: {'burn_k': None}}, {key: {'wac': float('nan')}}):
        with pytest.raises((ValueError, TypeError)):
            session.update(version=session.version, edits=edits)
    with pytest.raises(ValueError, match='stale'):
        session.update(version=0)
    with pytest.raises(ValueError):
        session.evaluate([{'template': 'cml_fixed_5y', 'purchase_m': 0, 'notional': -1}], session.version)
    result = session.update(version=session.version, constraints=LIMITS | {
        'commercial': [{'label': 'impossible', 'template': 'ALL_ASSET', 'sense': '>=', 'rhs': 1e10}]})
    assert not result['feasible'] and not result['validated'] and not result['allocation']
    assert session.update(version=session.version, constraints=LIMITS)['validated']


def test_native_registry_sessions_are_isolated(session, inputs):
    other = decision.DecisionSession(**inputs)
    try:
        other.update(version=0)
        original = other.result['worst_case_nii_$']
        session.update(version=session.version, constraints=LIMITS | {'max_total_assets': 1.})
        assert other.version == 1 and other.result['worst_case_nii_$'] == original
    finally:
        other.close()


def test_owned_actor_has_no_python_pricing_or_delta_callbacks(inputs, monkeypatch):
    if inputs['config'].compute_backend != 'rust':
        pytest.skip('native ownership contract')
    def forbidden(*args, **kwargs):
        raise AssertionError('Python pricing/coordinator callback invoked')
    for name in ('price_books', 'apply_overrides', '_contributions', '_apply_delta', 'prepare_library'):
        monkeypatch.setattr(decision, name, forbidden)
    monkeypatch.setattr(decision.DecisionSession, '_build_units', forbidden)
    s=decision.DecisionSession(**inputs)
    try:
        key='loans:'+s.books['loans']['id'][0]
        out=s.update(version=0, edits={key:{'coupon_or_spread':.061}},
                     templates={'cml_fixed_5y':{'spread_bp':250.}})
        assert out['execution']['orchestration']=='rust' and out['validated']
        assert out['work']['positions_repriced']==1 and out['work']['templates_rebuilt']==1
        assert out['work']['ffi_request_bytes']<2000
        reset=s.update(version=s.version, edits={key:{'coupon_or_spread':None}},
                       templates={'cml_fixed_5y':{'spread_bp':None}})
        assert reset['validated']
        # Validation failure inside pricing aborts the staged patch, not the session.
        version=s.version
        with pytest.raises(ValueError,match='floor'):
            s.update(version=version, edits={key:{'cap':.01,'floor':.1}})
        assert s.version==version
        assert s.update(version=version)['validated']
    finally:
        s.close()


def test_owned_standalone_matches_actor_without_python_callbacks(inputs, monkeypatch):
    if inputs['config'].compute_backend != 'rust':
        pytest.skip('native ownership contract')
    import datetime
    import json
    import subprocess
    import sys
    captured=[]
    original=decision.NativeDecision.call
    def capture(self, **request):
        if request['op']=='create_owned': captured.append(copy.deepcopy(request))
        return original(self, **request)
    monkeypatch.setattr(decision.NativeDecision,'call',capture)
    s=decision.DecisionSession(**inputs)
    try:
        first=s.update(version=0)
        key='loans:'+s.books['loans']['id'][0]
        step={'edits':{key:{'coupon_or_spread':.061}},'templates':{'agency_mbs':{'spread_bp':200.}}}
        second=s.update(version=s.version,**step)
        request=captured[0] | {'op':'run_owned','steps':[{},step]}
        exe=decision._DECISION_DEFAULT.parent/('portfolio-strategy.exe' if sys.platform=='win32' else 'portfolio-strategy')
        def encode(value):
            assert isinstance(value,datetime.date)
            return value.toordinal()
        proc=subprocess.run([str(exe)],input=json.dumps(request,default=encode,allow_nan=False),
                            text=True,capture_output=True,timeout=90,check=True)
        result=json.loads(proc.stdout)['ok']
        assert result['execution']['orchestration']=='rust'
        for actual,expected in zip(result['results'],[first,second]):
            np.testing.assert_allclose(actual['worst_case_nii_$'],expected['worst_case_nii_$'],rtol=1e-10,atol=1e-5)
            assert actual['work']['positions_repriced']==expected['work']['positions_repriced']
        for actual,expected in zip(result['snapshot']['bases'],s.bases):
            np.testing.assert_allclose(actual['nii_total_$'],expected['nii_total_$'],rtol=1e-12)
    finally:
        s.close()
