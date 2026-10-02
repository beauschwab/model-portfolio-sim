"""Public optimizer ownership, native standalone execution and independent parity."""
import copy
import json
from pathlib import Path
import subprocess

import numpy as np
import pytest

from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.strategy import optimizer, unitlib
from portfolio_risk.strategy.decision import NativeDecision
from test_review_regressions import synthetic_library


@pytest.fixture(scope='module', autouse=True)
def native_present():
    try:
        NativeDecision()
    except RuntimeError:
        pytest.skip('build the optional decision library')


def native(scenarios, **constraints):
    with run_context(RunConfig(compute_backend='rust')):
        return optimizer.optimize_balance_sheet(scenarios, **constraints)


def test_public_solve_never_calls_python_financial_functions(monkeypatch):
    lib, base = synthetic_library()
    # A stale Python coefficient cache must not influence native construction.
    lib['vectors'] = {('asset', 0): np.full((10, 27), 1e30)}
    def forbidden(*a, **kw):
        raise AssertionError('Python financial execution inside the native solve')
    monkeypatch.setattr(optimizer, '_kpi_vectors', forbidden)
    monkeypatch.setattr(optimizer, 'linprog', forbidden)
    monkeypatch.setattr(unitlib, 'allocation_vectors', forbidden)
    monkeypatch.setattr(unitlib, 'evaluate_strategy', forbidden)
    result = native([(lib, base)], max_total_assets=100., cash_budget=100.)
    assert result['worst_case_nii_$'] == pytest.approx(2800.)
    assert result['validated'] and not result['dynamic_validated']
    assert result['execution']['coefficient_construction'] == 'rust'
    assert result['execution']['allocation_replay'] == 'rust'


@pytest.mark.parametrize('seed', range(10))
def test_multiscenario_native_solve_and_replay_match_independent_reference(seed):
    rng = np.random.default_rng(seed)
    scenarios = []
    for si in range(3):
        lib, base = synthetic_library()
        lib['units'] += [dict(template='funding', h=0, side=-1), dict(template='funding', h=24, side=-1)]
        lib['templates']['asset'].update(hqla_l2a=.2, rsf=.5, rwa=.8)
        lib['templates']['funding'] = dict(asf=.9, outflow30=.15)
        lib['nii'] = np.vstack([rng.uniform(.04, .10, (2, 27)), rng.uniform(.005, .025, (2, 27))])
        lib['balance'] = np.exp(-rng.uniform(.005, .05, (4, 1)) * np.arange(27))
        lib['dv01'] = rng.uniform(0., .0005, 4)
        base['nii_total_$'] = 100. - si * 10.
        scenarios.append((lib, base))
    constraints = dict(max_total_assets=500., cash_budget=0., commercial=[
        dict(label='origination', template='asset', sense='>=', rhs=30.),
        dict(label='funding cap', template='ALL_LIAB', sense='<=', rhs=700.)])
    reference = optimizer.optimize_balance_sheet(scenarios, **constraints)
    answer = native(scenarios, **constraints)
    assert reference['validated'] and answer['validated']
    assert answer['worst_case_nii_$'] == pytest.approx(reference['worst_case_nii_$'], rel=1e-9, abs=1e-5)
    for (lib, base), actual in zip(scenarios, answer['replay']):
        expected = unitlib.evaluate_strategy(lib, answer['allocation'], base)
        for key in ('nii_total_$', 'nii_incremental', 'funding_gap'):
            np.testing.assert_allclose(actual[key], expected[key], rtol=1e-10, atol=1e-8)
        for key in expected['kpi_path']:
            np.testing.assert_allclose(actual['kpi_path'][key], expected['kpi_path'][key], rtol=1e-10, atol=1e-8)
        assert actual['kpis']['cet1_horizon_pct'] == pytest.approx(expected['kpis']['cet1_horizon_pct'], rel=1e-10)


def test_no_implicit_asset_cap_and_infeasibility():
    lib, base = synthetic_library()
    answer = native([(lib, base)], cash_budget=2e8)
    assert answer['total_new_assets_$'] == pytest.approx(2e8)
    failed = native([(lib, base)], cash_budget=0., commercial=[
        dict(label='unfunded', template='asset', sense='>=', rhs=1.)])
    assert not failed['feasible'] and not failed.get('validated', False)
    assert 's0:m0:funding' in failed['labels']


@pytest.mark.parametrize('change', ['shape', 'grid', 'side', 'weights', 'horizon', 'eve', 'capital'])
def test_malformed_libraries_are_rejected_before_solving(change):
    lib, base = synthetic_library()
    if change == 'shape': lib['nii'] = lib['nii'][:, :3]
    if change == 'grid': lib['units'][1]['h'] = 0
    if change == 'side': lib['units'][1]['side'] = -1
    if change == 'weights': lib['templates']['asset']['rsf'] = -1
    if change == 'horizon': lib['horizon'] = 0
    if change == 'eve': base['eve']['eve_$'] = 0
    if change == 'capital': base['capital']['cet1_path'] = []
    with pytest.raises(ValueError):
        native([(lib, base)], max_total_assets=100.)


def test_mismatched_scenarios_rejected():
    lib, base = synthetic_library()
    other = copy.deepcopy(lib)
    other['horizon'] = 26
    with pytest.raises(ValueError, match='grids and horizons'):
        native([(lib, base), (other, base)])


def test_standalone_executable_consumes_same_raw_request(monkeypatch):
    lib, base = synthetic_library()
    recorded = []
    call = NativeDecision.call
    def capture(self, **request):
        recorded.append(request)
        return call(self, **request)
    monkeypatch.setattr(NativeDecision, 'call', capture)
    expected = native([(lib, base)], max_total_assets=100., cash_budget=100.)
    root = Path(__file__).resolve().parents[2]
    exe = root / 'portfolio-decision-native/target/release/portfolio-strategy'
    if __import__('os').name == 'nt': exe = exe.with_suffix('.exe')
    assert exe.is_file(), 'build the standalone native strategy executable'
    process = subprocess.run([str(exe)], input=json.dumps(recorded[0]), text=True,
                             capture_output=True, timeout=30, check=True)
    assert json.loads(process.stdout)['ok'] == expected
    failed = subprocess.run([str(exe)], input='{}', text=True, capture_output=True, timeout=30)
    assert failed.returncode == 2 and 'error' in json.loads(failed.stdout)
