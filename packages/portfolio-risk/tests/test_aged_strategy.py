"""Dated coefficient adoption against independent actual-date financial fixtures.

These are native ownership tests. Explicit Python formulas are test oracles;
neither preparation nor production evaluation may execute a Python pricer.
"""
import copy
import calendar
import datetime as dt
import json
import math
from pathlib import Path
import statistics
import subprocess
import sys
import time

import numpy as np
import pytest

from portfolio_risk.analytics.model_contracts import prepare_aged_units
from portfolio_risk.core import lifecycle_native, quant_native
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.strategy import aged, optimizer, unitlib


EXAMPLE = Path(__file__).resolve().parents[3] / 'examples/models/aged-units-1.json'


def request(coupon=0.):
    raw = json.loads(EXAMPLE.read_text())
    raw['templates'][0]['term']['coupon'] = coupon
    # Independent one-payment contract: actual Jan31 2024 -> Jan31 2025 = 366 days.
    raw['purchases'][0]['original_annual_effective_yield'] = (1. + coupon * 366. / 365.) ** (365. / 366.) - 1.
    return raw


def native_prepare(raw):
    with run_context(RunConfig(compute_backend='rust')):
        return prepare_aged_units(raw)


def base():
    return dict(nii=5., dv01=0., eve=1000., l1=1e6, l2=0., nco=1.,
                asf=1e6, rsf=1., cet1=1000., rwa=1., ni=.2365, mv_assets=1000.)


def forbidden(*args, **kwargs):
    raise AssertionError('Financial pricer/coefficient construction called during coefficient evaluation')


def independent_bullet_cash_events(raw, allocated):
    """Actual month clamp + ACT/365 fixture oracle, independent of native output."""
    assert len(allocated) == len(raw['purchases'])
    events=[]
    templates={t['template_id']:t for t in raw['templates']}
    for purchase,face in zip(raw['purchases'],allocated):
        template=templates[purchase['template_id']];terms=template['term']
        assert terms['amortization']=='bullet' and terms['bdc']=='NONE'
        assert terms['daycount']=='ACT/365F'
        start=dt.date.fromordinal(purchase['purchase_ordinal'])
        month=start.year*12+start.month-1+terms['term_months']
        year,month0=divmod(month,12)
        end=dt.date(year,month0+1,min(start.day,calendar.monthrange(year,month0+1)[1]))
        side=template['side']
        events.append((start.toordinal(),-side*face*purchase['quote_price_per_unit'],0.,0.))
        # These fixtures have one settlement, so scheduled ACT/365 coupon is hand-computable.
        assert terms['freq_months'] in (0,terms['term_months'])
        events.append((end.toordinal(),0.,side*face,side*face*terms['coupon']*(end-start).days/365.))
    return events


def assert_independent_allocated_cash_replay(raw,answer):
    events=independent_bullet_cash_events(raw,answer['native_allocated_face'])
    # Coverage comes from contractual input dates, not the native funding tensor.
    assert [row['ordinal'] for row in answer['cash_event_replay']] == sorted(
        set(raw['reporting_ordinals']) | {event[0] for event in events})
    for row in answer['cash_event_replay']:
        date=row['ordinal']
        cumulative=sum(sum(e[1:]) for e in events if e[0]<=date)
        assert row['cumulative_bank_cash']==pytest.approx(cumulative,abs=1e-8)
        assert row['funding_demand']==pytest.approx(-cumulative,abs=1e-8)
        for j,key in enumerate(('purchase_cash','principal_cash','interest_cash'),1):
            assert row[key]==pytest.approx(sum(e[j] for e in events if e[0]==date),abs=1e-8)


def test_actual_aged_risk_uses_remaining_days_and_preserves_base_oas():
    raw = request()
    raw['scenarios'].append(copy.deepcopy(raw['scenarios'][0]))
    raw['scenarios'][1]['scenario_id'] = 'up100'
    raw['scenarios'][1]['collateral_curve']['curve_id'] = 'up100-collateral'
    raw['scenarios'][1]['collateral_curve']['discount_factors'] = [math.exp(-.06*t) for t in [0, 1, 2, 5]]
    result = native_prepare(raw)
    tensor = np.asarray(result['values']).reshape(result['shape'])
    remaining = np.array([366., 337.]) / 365.
    expected = np.sinh(.0025 * remaining) / 25.
    np.testing.assert_allclose(tensor[0, 0, 2], expected, rtol=1e-11, atol=1e-13)
    np.testing.assert_allclose(tensor[1, 0, 2], np.exp(-.01 * remaining) * expected,
                               rtol=1e-11, atol=1e-13)
    np.testing.assert_array_equal(tensor[0, 0, 1], [1., 1.])
    assert tensor[0, 0, 2, 1] < tensor[0, 0, 2, 0]
    assert result['units'][0]['base_oas'] == pytest.approx(-.05, abs=1e-12)
    assert result['units'][0]['full_contract_end_ordinal'] == dt.date(2025, 1, 31).toordinal()
    assert result['units'][0]['opening_states'][-1]['principal'] == 0.


def test_original_yield_income_and_cash_retire_once_on_actual_dates():
    raw = request(.1)
    result = native_prepare(raw)
    tensor = np.asarray(result['values']).reshape(result['shape'])
    full_cash = 1. + .1 * 366. / 365.
    first_carry = full_cash ** (29. / 366.)
    np.testing.assert_allclose(tensor[0, 0, 0], [first_carry - 1., full_cash - first_carry],
                               rtol=1e-11, atol=1e-12)
    assert tensor[0, 0, 0].sum() == pytest.approx(.1 * 366. / 365., abs=1e-12)
    states = result['units'][0]['opening_states']
    assert states[1]['gross_carrying_value'] == pytest.approx(first_carry, abs=1e-12)
    assert states[-1]['gross_carrying_value'] == 0.
    assert states[-1]['cash_paid_to_date'] == pytest.approx(full_cash, abs=1e-12)
    # Distinct market risk and original book yield are both retained.
    assert result['units'][0]['base_oas'] == pytest.approx(math.log(full_cash)/(366./365.)-.05, abs=1e-12)


def test_interactive_evaluation_uses_native_prepared_coefficients_without_pricers(monkeypatch):
    raw = request(.1)
    with run_context(RunConfig(compute_backend='rust')):
        lib = aged.build_library(raw)
        monkeypatch.setattr(aged, 'term_call', forbidden)
        monkeypatch.setattr(quant_native, 'term_call', forbidden)
        monkeypatch.setattr(lifecycle_native, 'prepare_coefficients', forbidden)
        monkeypatch.setattr(unitlib, '_interp_unit', forbidden)
        monkeypatch.setattr(unitlib, 'allocation_vectors', forbidden)
        allocations = [dict(template='fixed-zero-coupon', purchase_m=0, notional=100.)]
        answer = unitlib.evaluate_strategy(lib, allocations)
        repeated = unitlib.evaluate_strategy(lib, allocations)
        scaled = unitlib.evaluate_strategy(lib, [dict(allocations[0], notional=250.)])
    full_cash = 1. + .1 * 366. / 365.
    first_carry = full_cash ** (29. / 366.)
    np.testing.assert_allclose(answer['nii_incremental'], 100. * np.array([first_carry-1., full_cash-first_carry]), atol=1e-10)
    np.testing.assert_array_equal(answer['nii_incremental'], repeated['nii_incremental'])
    np.testing.assert_allclose(scaled['fwd_dv01'], 2.5 * answer['fwd_dv01'], rtol=1e-12)
    assert answer['reporting_ordinals'] == raw['reporting_ordinals']
    assert 'actual-age' in answer['dv01_method']
    assert answer['nii_total_$'] == pytest.approx(100. * .1 * 366. / 365., abs=1e-9)
    assert answer['funding_ordinals'][-1] == dt.date(2025,1,31).toordinal()
    np.testing.assert_allclose(answer['funding_gap'],[100.,100.,-100.*.1*366./365.],atol=1e-8)
    np.testing.assert_array_equal(answer['principal_exposure_gap'],[100.,100.])


def test_missing_purchase_grid_and_coefficients_require_rebuild_without_pricing(monkeypatch):
    with run_context(RunConfig(compute_backend='rust')):
        lib = aged.build_library(request())
        complete_lib = copy.deepcopy(lib)
        monkeypatch.setattr(quant_native, 'term_call', forbidden)
        monkeypatch.setattr(lifecycle_native, 'prepare_coefficients', forbidden)
        missing = [dict(template='fixed-zero-coupon', purchase_m=1, notional=1.)]
        with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
            unitlib.evaluate_strategy(lib, missing)
        with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
            unitlib.allocation_vectors(lib, 'fixed-zero-coupon', 1)
        lib['vectors'] = {}
        with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
            unitlib.prepare_library(lib)
        for missing_coefficients in ({}, None):
            lib['vectors'] = missing_coefficients
            with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
                unitlib.evaluate_strategy(lib, [dict(template='fixed-zero-coupon', purchase_m=0, notional=1.)])
        lib.pop('vectors')
        with pytest.raises(ValueError, match='REBUILD_REQUIRED'):
            unitlib.evaluate_strategy(lib, [dict(template='fixed-zero-coupon', purchase_m=0, notional=1.)])
        for missing_funding in ({}, None):
            lib=copy.deepcopy(complete_lib)
            lib['funding_vectors']=missing_funding
            with pytest.raises(ValueError,match='REBUILD_REQUIRED'):
                unitlib.evaluate_strategy(lib,[dict(template='fixed-zero-coupon',purchase_m=0,notional=1.)])
        lib=copy.deepcopy(complete_lib)
        lib.pop('funding_vectors')
        with pytest.raises(ValueError,match='REBUILD_REQUIRED'):
            unitlib.evaluate_strategy(lib,[dict(template='fixed-zero-coupon',purchase_m=0,notional=1.)])


def test_optimize_aged_constructs_native_coefficients_solver_and_replay(monkeypatch):
    raw = request(.1)
    monkeypatch.setattr(optimizer, '_kpi_vectors', forbidden)
    monkeypatch.setattr(optimizer, 'linprog', forbidden)
    monkeypatch.setattr(unitlib, 'allocation_vectors', forbidden)
    monkeypatch.setattr(unitlib, 'evaluate_strategy', forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        answer = aged.optimize(raw, [dict(scenario_id='base', base=base())],
                               dict(max_total_assets=100., cash_budget=100.))
    assert answer['feasible'] and answer['validated'] and not answer['dynamic_validated']
    assert answer['total_new_assets_$'] == pytest.approx(100., abs=1e-7)
    assert answer['worst_case_nii_$'] == pytest.approx(5. + 100. * .1 * 366. / 365., abs=1e-8)
    assert answer['execution']['schema'] == 'strategy-aged-1'
    assert answer['execution']['product_preparation'] == 'rust'
    assert answer['execution']['solver'] == 'HiGHS (C++)'
    assert answer['aged_preparation']['shape'] == [1, 1, 10, 2]
    # Existing solver replay labels incremental candidate NII separately from
    # the base-inclusive worst-case objective; compare with that exact meaning.
    assert answer['replay'][0]['nii_total_$'] == pytest.approx(100. * .1 * 366. / 365., abs=1e-8)
    assert answer['replay'][0]['nii_total_$'] + base()['nii'] == pytest.approx(answer['worst_case_nii_$'], abs=1e-8)
    assert_independent_allocated_cash_replay(raw,answer)


def test_retained_capital_dependency_requires_exact_native_source_refresh():
    raw = request(.1)
    units = [dict(template='fixed-zero-coupon', h=0, side=1.)]
    capital = dict(label='supplied-capital-row', policy_id='capital-v1', metric='cet1',
                   scenario=0, month=1, units=units, numerator=100., denominator=10.,
                   required_ratio=.1, numerator_per_unit=[0.], denominator_per_unit=[1.])
    constraints = dict(max_total_assets=100., cash_budget=100., capital_limits=[capital])
    bases = [dict(scenario_id='base', base=base())]
    with run_context(RunConfig(compute_backend='rust')):
        with pytest.raises(ValueError, match='CAPITAL_REFRESH_REQUIRED'):
            aged.optimize(raw, bases, constraints)
        stale = copy.deepcopy(raw)
        stale['identity']['grid_revision'] = 'older-grid'
        with pytest.raises(ValueError, match='CAPITAL_REFRESH_REQUIRED'):
            aged.optimize(raw, bases, constraints, capital_dependency=stale)
        answer = aged.optimize(raw, bases, constraints, capital_dependency=copy.deepcopy(raw))
    assert answer['feasible'] and answer['validated'] and answer['capital_replay']


def test_missing_scenario_coverage_is_rejected():
    raw = request(.1)
    with run_context(RunConfig(compute_backend='rust')):
        with pytest.raises(ValueError, match='missing aged scenario base'):
            aged.optimize(raw, [dict(scenario_id='unknown', base=base())], dict(cash_budget=100.))


@pytest.mark.parametrize('quote',[.9,1.1])
def test_discount_premium_quotes_use_actual_purchase_cost_and_full_cash_replay(quote):
    raw=request();p=raw['purchases'][0]
    p['quote_price_per_unit']=quote
    p['original_annual_effective_yield']=(1./quote)**(365./366.)-1.
    constraints=dict(cash_budget=100.*quote,max_total_assets=100.,commercial=[
        dict(label='exact-origination',template='fixed-zero-coupon',sense='>=',rhs=100.)])
    with run_context(RunConfig(compute_backend='rust')):
        answer=aged.optimize(raw,[dict(scenario_id='base',base=base())],constraints,
                             max_purchase_cost_assets=100.*quote)
        lib=aged.library_from_result(raw,answer['aged_preparation'])
        evaluated=unitlib.evaluate_strategy(lib,answer['allocation'])
    assert answer['feasible'] and answer['validated']
    assert answer['total_new_assets_$']==pytest.approx(100.)
    assert answer['total_purchase_cost_assets_$']==pytest.approx(100.*quote)
    assert answer['worst_case_nii_$']==pytest.approx(5.+100.*(1.-quote),abs=1e-8)
    assert_independent_allocated_cash_replay(raw,answer)
    np.testing.assert_allclose(evaluated['funding_gap'],[100.*quote,100.*quote,100.*(quote-1.)],atol=1e-8)
    assert evaluated['nii_total_$']==pytest.approx(100.*(1.-quote),abs=1e-8)


def with_cd(raw,*,purchase_ordinal=None,months=12):
    r=copy.deepcopy(raw)
    t=copy.deepcopy(r['templates'][0]);t['template_id']='cd';t['side']=-1.;t['accounting_category']='liability'
    t['term'].update(product='cd',term_months=months,freq_months=0,coupon=0.)
    t['weights'].update(rwa=0.,rsf=0.,asf=1.)
    r['templates'].append(t)
    p=copy.deepcopy(r['purchases'][0]);p.update(template_id='cd',original_annual_effective_yield=0.)
    if purchase_ordinal is not None:p['purchase_ordinal']=purchase_ordinal
    r['purchases'].append(p)
    return r


def test_purchase_cost_cap_binds_separately_from_face_and_starting_cash():
    raw = request(.2)
    quote = 1.1
    cash = 1. + .2 * 366. / 365.
    raw['purchases'][0].update(
        quote_price_per_unit=quote,
        original_annual_effective_yield=(cash / quote) ** (365. / 366.) - 1.)
    with run_context(RunConfig(compute_backend='rust')):
        answer = aged.optimize(
            raw, [dict(scenario_id='base', base=base())],
            dict(cash_budget=110., max_total_assets=100.),
            max_purchase_cost_assets=55.)
    assert answer['feasible'] and answer['validated']
    assert answer['total_new_assets_$'] == pytest.approx(50., abs=1e-8)
    assert answer['total_purchase_cost_assets_$'] == pytest.approx(55., abs=1e-8)
    assert answer['worst_case_nii_$'] == pytest.approx(5. + 50. * (cash - quote), abs=1e-8)
    assert_independent_allocated_cash_replay(raw, answer)


def forced_asset_constraints():
    return dict(cash_budget=0.,max_total_assets=1.,commercial=[
        dict(label='mandatory-asset',template='fixed-zero-coupon',sense='>=',rhs=1.)])


def test_future_cd_receipt_cannot_fund_an_earlier_purchase():
    raw=with_cd(request(),purchase_ordinal=dt.date(2024,2,29).toordinal())
    with run_context(RunConfig(compute_backend='rust')):
        answer=aged.optimize(raw,[dict(scenario_id='base',base=base())],forced_asset_constraints())
    assert not answer['feasible'] and not answer.get('validated',False)
    assert answer['aged_preparation']['funding']['ordinals'][0]==dt.date(2024,1,31).toordinal()


def test_short_cd_long_asset_tail_is_checked_after_reporting_horizon():
    raw=with_cd(request(),months=6)
    raw['reporting_ordinals']=raw['reporting_ordinals'][:2]
    with run_context(RunConfig(compute_backend='rust')):
        answer=aged.optimize(raw,[dict(scenario_id='base',base=base())],forced_asset_constraints())
    assert not answer['feasible'] and not answer.get('validated',False)
    assert dt.date(2024,7,31).toordinal() in answer['aged_preparation']['funding']['ordinals']
    assert answer['aged_preparation']['horizon']==1


def test_matched_asset_cd_cash_is_netted_once_and_coupon_remains_cash_timed():
    raw=with_cd(request(.1))
    with run_context(RunConfig(compute_backend='rust')):
        answer=aged.optimize(raw,[dict(scenario_id='base',base=base())],forced_asset_constraints())
    assert answer['feasible'] and answer['validated']
    assert_independent_allocated_cash_replay(raw,answer)
    replay=answer['cash_event_replay']
    assert replay[1]['interest_cash']==0.
    assert replay[-1]['interest_cash']==pytest.approx(.1*366./365.,abs=1e-9)
    assert answer['replay'][0]['nii_total_$']==pytest.approx(.1*366./365.,abs=1e-9)


def test_fresh_process_default_native_coefficient_execution_has_no_financial_callback():
    # Runs outside conftest's explicit Python reference context.
    script = r'''
import json, sys
from portfolio_risk.core import quant_native, lifecycle_native
from portfolio_risk.strategy import aged, unitlib
assert quant_native.enabled()
lib=aged.build_library(json.load(open(sys.argv[1])))
def forbidden(*args,**kwargs):raise AssertionError("pricer entered interactive evaluation")
quant_native.term_call=forbidden
lifecycle_native.prepare_coefficients=forbidden
unitlib._interp_unit=forbidden
answer=unitlib.evaluate_strategy(lib,[dict(template="fixed-zero-coupon",purchase_m=0,notional=10.)])
print(json.dumps(dict(nii=answer["nii_total_$"],dv01=answer["fwd_dv01"].tolist(),dates=answer["reporting_ordinals"])))
'''
    proc = subprocess.run([sys.executable, '-c', script, str(EXAMPLE)], capture_output=True,
                          text=True, timeout=30, check=True)
    answer = json.loads(proc.stdout)
    assert answer['nii'] == 0.
    np.testing.assert_allclose(answer['dv01'], 10. * np.sinh(.0025 * np.array([366.,337.])/365.)/25., rtol=1e-11)


def test_interactive_timing_is_measured_on_one_unit_two_actual_periods(monkeypatch, record_property):
    with run_context(RunConfig(compute_backend='rust')):
        lib = aged.build_library(request())
        monkeypatch.setattr(quant_native, 'term_call', forbidden)
        monkeypatch.setattr(lifecycle_native, 'prepare_coefficients', forbidden)
        allocations = [dict(template='fixed-zero-coupon',purchase_m=0,notional=100.)]
        for _ in range(8): unitlib.evaluate_strategy(lib, allocations)
        elapsed = []
        for _ in range(128):
            start = time.perf_counter_ns()
            unitlib.evaluate_strategy(lib, allocations)
            elapsed.append(time.perf_counter_ns()-start)
    # Record specified-fixture evidence rather than make a portable machine-speed assertion.
    median = statistics.median(elapsed)
    record_property('aged_coefficient_eval_median_ns', median)
    record_property('aged_coefficient_eval_sub_ms', median < 1_000_000)
    print(f'aged coefficient evaluation: one unit/two dated periods/128 samples median={median:.0f} ns')
