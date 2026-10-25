"""Explicitly selected off-balance-sheet swap strategy units.

Native unit generation, prepared coefficients and the HiGHS solver are checked
against the independent Python reference. A swap carries net settlement NII,
DV01 and a supplied counterparty RWA weight per notional. It never contributes
balance, assets, liabilities or funding, and default template sets are unchanged.
"""
import copy

import numpy as np
import pytest

from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.strategy import optimizer, unitlib
from portfolio_risk.strategy.decision import NativeDecision
from test_native_products import equal
from test_native_remaining_lifecycle import unit_run
from test_review_regressions import synthetic_library

SWAPS = ['receive_fixed_swap_5y', 'pay_fixed_swap_5y']
BASE = dict(eve={'eve_$': 1e7, 'dv01_net_$': 450., 'mv_assets_$': 1e8},
            lcr={'hqla_$': 2e7, 'hqla_l1_$': 1.5e7, 'hqla_l2a_uncapped_$': 7e6, 'net_outflows_$': 1e7},
            nsfr={'asf_$': 7e7, 'rsf_$': 5e7}, capital={'cet1_path': [{'cet1_$': 8e6}], 'rwa_total_$': 5e7})


def native_decision():
    try:
        NativeDecision()
    except RuntimeError:
        pytest.skip('build the optional decision library')


def test_swap_templates_are_opt_in():
    assert not set(SWAPS) & set(unitlib.TEMPLATES)
    assert set(SWAPS) <= set(unitlib.KNOWN_TEMPLATES)
    assert set(unitlib.resolve_templates()) == set(unitlib.TEMPLATES)


@pytest.mark.parametrize('paths', [1, 4])
@pytest.mark.parametrize('names,grid', [(SWAPS, [0, 3, 12]), (SWAPS + ['agency_mbs', 'mmda_growth'], [0, 6])])
def test_native_swap_units_match_independent_reference(paths, names, grid):
    expected = unit_run('python', paths, names, grid)
    actual = unit_run('rust', paths, names, grid)
    equal(expected, actual, rtol=1e-7, atol=1e-8)
    assert [u for u in actual['units'] if u['kind'] == 'swap']


def test_swap_economics_are_symmetric_and_off_balance_sheet():
    lib = unit_run('python', 4, SWAPS, [0, 6])
    with run_context(RunConfig(compute_backend='python')):
        receive = unitlib.allocation_vectors(lib, 'receive_fixed_swap_5y', 0)
        pay = unitlib.allocation_vectors(lib, 'pay_fixed_swap_5y', 0)
        later = unitlib.allocation_vectors(lib, 'receive_fixed_swap_5y', 6)
    np.testing.assert_allclose(receive[0], -pay[0], rtol=0, atol=1e-15)
    np.testing.assert_allclose(receive[2], -pay[2], rtol=0, atol=1e-15)
    assert receive[2][0] > 0          # receive fixed gains when rates fall
    for x in (receive, pay, later):
        assert not x[1].any() and not x[8].any() and not x[9].any()
        assert not x[3:7].any()       # no liquidity or stable-funding weights
    np.testing.assert_allclose(receive[7], .02 * (receive[2] != 0), rtol=0, atol=1e-15)
    assert not later[:, :6].any()     # a forward-starting swap is inactive before purchase


@pytest.mark.parametrize('allocation', [
    [{'template': 'receive_fixed_swap_5y', 'purchase_m': 0, 'notional': 5e7}],
    [{'template': 'pay_fixed_swap_5y', 'purchase_m': 3, 'notional': 2e7},
     {'template': 'agency_mbs', 'purchase_m': 0, 'notional': 1e6}]])
def test_interactive_swap_evaluation_is_coefficient_only(monkeypatch, allocation):
    library = unit_run('rust', 3, SWAPS + ['agency_mbs'], [0, 3])
    with run_context(RunConfig(compute_backend='python')):
        expected = unitlib.evaluate_strategy(library, allocation, BASE)

    def forbidden(*a, **kw):
        raise AssertionError('Python interpolation or pricing called during interactive evaluation')
    monkeypatch.setattr(unitlib, 'allocation_vectors', forbidden)
    monkeypatch.setattr(unitlib, 'build_unit_library', forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        actual = unitlib.evaluate_strategy(library, allocation, BASE)
    equal(expected, actual, rtol=1e-12, atol=1e-8)
    swaps_only = [a for a in allocation if a['template'] in SWAPS]
    with run_context(RunConfig(compute_backend='rust')):
        alone = unitlib.evaluate_strategy(library, swaps_only, BASE)
    assert not np.any(alone['funding_gap']) and not np.any(alone['balance'])


def swap_scenarios():
    scenarios = []
    for shift in (0., .002):
        lib, base = synthetic_library()
        lib['units'] += [dict(template='funding', h=0, side=-1),
                         dict(template='rfs', h=0, side=1, kind='swap'),
                         dict(template='rfs', h=24, side=1, kind='swap')]
        lib['templates']['asset'].update(rwa=.5)
        lib['templates']['funding'] = dict(asf=.9, outflow30=.1)
        lib['templates']['rfs'] = dict(kind='swap', rwa=.02)
        lib['nii'] = np.vstack([np.full((2, 27), .05), np.full((1, 27), .01),
                                np.full((2, 27), .004 - shift)])
        lib['balance'] = np.ones((5, 27))
        lib['dv01'] = np.array([.0004, .0004, 0., .0004, .0004])
        base['capital']['rwa_total_$'] = 5000.
        base['eve']['eve_$'] = 1e4
        scenarios.append((lib, base))
    return scenarios


@pytest.mark.parametrize('constraints', [
    dict(max_total_assets=0., cash_budget=0.),
    dict(max_total_assets=200., cash_budget=0., commercial=[
        dict(label='asset cap', template='ALL_ASSET', sense='<=', rhs=150.)]),
])
def test_native_solver_treats_swaps_as_off_balance_sheet(constraints):
    native_decision()
    scenarios = swap_scenarios()
    with run_context(RunConfig(compute_backend='python')):
        reference = optimizer.optimize_balance_sheet(copy.deepcopy(scenarios), **constraints)
    with run_context(RunConfig(compute_backend='rust')):
        answer = optimizer.optimize_balance_sheet(copy.deepcopy(scenarios), **constraints)
    assert reference['validated'] and answer['validated']
    assert answer['worst_case_nii_$'] == pytest.approx(reference['worst_case_nii_$'], rel=1e-9, abs=1e-5)
    swap = sum(a['notional'] for a in answer['allocation'] if a['template'] == 'rfs')
    assets = sum(a['notional'] for a in answer['allocation'] if a['template'] == 'asset')
    assert swap > 0                                     # bounded by capital/EVE, not asset caps
    assert answer['total_new_assets_$'] == pytest.approx(assets, abs=1e-6)
    assert answer['total_new_assets_$'] <= constraints['max_total_assets'] + 1e-6
    for (lib, base), actual in zip(scenarios, answer['replay']):
        with run_context(RunConfig(compute_backend='python')):
            expected = unitlib.evaluate_strategy(lib, answer['allocation'], base)
        for key in ('nii_total_$', 'nii_incremental', 'funding_gap'):
            np.testing.assert_allclose(actual[key], expected[key], rtol=1e-10, atol=1e-8)


def test_swap_marker_must_match_template_kind():
    native_decision()
    scenarios = swap_scenarios()
    for lib, _ in scenarios:
        lib['units'][3].pop('kind')
    with run_context(RunConfig(compute_backend='rust')), pytest.raises(ValueError, match='swap unit and template kind'):
        optimizer.optimize_balance_sheet(scenarios, max_total_assets=0., cash_budget=0.)


@pytest.mark.parametrize('change', ['designation', 'kind'])
@pytest.mark.parametrize('backend', ['python', 'rust'])
def test_strategy_units_never_carry_accounting_eligibility(change, backend):
    if backend == 'rust': native_decision()
    scenarios = swap_scenarios()
    for lib, _ in scenarios:
        if change == 'designation': lib['units'][3]['designation'] = 'fair_value_hedge'
        else: lib['units'][3]['kind'] = 'fair_value_hedge'
    with run_context(RunConfig(compute_backend=backend)), pytest.raises(ValueError, match='not admitted|unknown field'):
        optimizer.optimize_balance_sheet(scenarios, max_total_assets=0., cash_budget=0.)
