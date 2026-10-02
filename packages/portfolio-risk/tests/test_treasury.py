"""Hand-computed capital/FTP cases and public C++ HiGHS capital constraints."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics.treasury import evaluate, example, replay_capital_limits
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.strategy.optimizer import optimize_balance_sheet
from test_review_regressions import synthetic_library


def test_capital_components_denominators_buffers_and_stress():
    result=evaluate(example())
    rows={(r['scenario'],r['metric']):r for r in result['capital_metrics'].to_dicts()}
    assert len(rows)==26
    for metric,n,d in [('cet1',100e6,900e6),('tier1',120e6,900e6),('total_capital',135e6,900e6),
                       ('advanced_cet1',100e6,950e6),('tier1_leverage',120e6,1500e6),
                       ('slr',120e6,2000e6),('tlac_rwa',220e6,900e6),('ltd_leverage',80e6,2000e6)]:
        r=rows['base',metric]
        assert r['ratio']==pytest.approx(n/d)
        assert r['headroom']==pytest.approx(n-d*r['required_ratio'])
    assert rows['base','cet1']['required_ratio']==pytest.approx(.07)
    assert rows['base','cet1']['management_target']==pytest.approx(.075)
    assert rows['adverse','cet1']['numerator']==60e6
    assert rows['adverse','cet1']['status']=='breach'


def test_ftp_matched_tenors_signed_credit_and_elimination():
    result=evaluate(example())
    loan,deposit=result['ftp_positions'].to_dicts()
    # 5y repricing reference=3.7%; 7y funding liquidity=0.58%.
    expected=(.037+.0058+.001)*1e6/12+10
    assert loan['ftp_charge']==pytest.approx(expected)
    assert loan['business_profit']==pytest.approx(5000+100-150-75-expected)
    assert loan['capital_cost']==pytest.approx(1000.)
    assert loan['raroc']==pytest.approx(loan['business_profit']*12/100000)
    # Deposit's repricing tenor=.25y, liquidity tenor=3y. Credit is negative.
    assert deposit['ftp_charge']==pytest.approx(-(.03975+.0035)*1e6/12+20)
    totals=result['ftp_reconciliation'].row(0,named=True)
    assert totals['consolidated_profit']==pytest.approx(3225.)
    assert totals['business_profit']+totals['treasury_profit']==pytest.approx(3225.)
    assert abs(totals['elimination_error'])<1e-9
    assert loan['loan_id']=='loan-1' and loan['cohort_id']=='mortgage-prime'


def test_ftp_changes_redistribute_profit_without_changing_external_profit():
    spec=example();base=evaluate(spec)
    spec['curves'][0]['liquidity_spreads']=[v+.02 for v in spec['curves'][0]['liquidity_spreads']]
    result=evaluate(spec)
    assert result['ftp_positions']['business_profit'][0]!=base['ftp_positions']['business_profit'][0]
    assert result['ftp_reconciliation']['external_profit'].to_list()==base['ftp_reconciliation']['external_profit'].to_list()
    assert result['input_sha256']!=base['input_sha256']


def test_unavailable_unconfigured_negative_capital_zero_ftp_capital_and_empty_tables():
    spec=example();c=spec['capital'][0]
    c.update(advanced_rwa=None,total_leverage_exposure=0.,retained_earnings=-200e6,requirements={})
    spec['positions'][0]['allocated_capital']=0.
    result=evaluate(spec)
    rows={r['metric']:r for r in result['capital_metrics'].to_dicts() if r['scenario']=='base'}
    assert rows['slr']['ratio'] is None and rows['slr']['status']=='unavailable'
    assert rows['advanced_cet1']['status']=='unavailable'
    assert rows['cet1']['ratio']<0 and rows['cet1']['status']=='unconfigured'
    assert result['ftp_positions']['raroc'][0] is None
    spec.update(capital=[],positions=[],curves=[])
    result=evaluate(spec)
    assert result['capital_metrics'].height==0 and result['capital_metrics'].width>0
    assert result['ftp_positions'].height==0 and result['ftp_positions'].width>0


@pytest.mark.parametrize('mutation',[
    lambda s:s['curves'][0].update(tenors=[0,0,5,10]),
    lambda s:s['curves'][0].update(as_of='2020-01-01'),
    lambda s:s['positions'][0].update(currency='EUR'),
    lambda s:s['positions'][0].update(side='equity'),
    lambda s:s['positions'][0].update(accrual_fraction=0.),
    lambda s:s['capital'][0].update(credit_rwa=-1.),
    lambda s:s['capital'][0]['requirements'].update(made_up=dict(minimum=.1,buffer=0.,management=0.)),
    lambda s:s['capital'][0]['requirements']['cet1'].update(minimum=4.5),
    lambda s:s['positions'].append(deepcopy(s['positions'][0])),
    lambda s:s['capital'].append(deepcopy(s['capital'][0])),
])
def test_invalid_inputs_fail_closed(mutation):
    spec=example();mutation(spec)
    with pytest.raises(ValueError):evaluate(spec)


def limit(lib,**overrides):
    return dict(label='USD-bank-slr-month0',policy_id='test-v1',metric='slr',scenario=0,month=0,
        units=deepcopy(lib['units']),numerator=12.,denominator=100.,required_ratio=.10,
        numerator_per_unit=[0.]*len(lib['units']),denominator_per_unit=[1.]*len(lib['units']))|overrides


@pytest.mark.parametrize('backend',['python','rust'])
def test_capital_limit_binds_and_replays(backend):
    lib,base=synthetic_library();cap=limit(lib)
    with run_context(RunConfig(compute_backend=backend)):
        out=optimize_balance_sheet([(lib,base)],cash_budget=100.,max_total_assets=100.,capital_limits=[cap])
    assert out['feasible']
    assert out['total_new_assets_$']==pytest.approx(20.)
    assert out['capital_replay'][0]['ratio']==pytest.approx(.10)
    assert any(r['constraint']=='capital:USD-bank-slr-month0' for r in out['binding_constraints'])
    assert replay_capital_limits([cap],lib['units'],out['allocation'])[0]['headroom']==pytest.approx(0.,abs=1e-8)
    with pytest.raises(RuntimeError,match='capital limit'):
        replay_capital_limits([cap],lib['units'],[dict(template='asset',purchase_m=0,notional=100.)])


@pytest.mark.parametrize('backend',['python','rust'])
def test_capital_infeasible_and_stale_grid(backend):
    lib,base=synthetic_library()
    with run_context(RunConfig(compute_backend=backend)):
        out=optimize_balance_sheet([(lib,base)],cash_budget=100.,capital_limits=[limit(lib,numerator=1.)])
        assert not out['feasible']
        cap=limit(lib);cap['units'].reverse()
        with pytest.raises(ValueError,match='stale unit grid'):
            optimize_balance_sheet([(lib,base)],capital_limits=[cap])


def test_native_report_prepares_policy_aware_solver_limits():
    lib,base=synthetic_library();spec=example()
    spec['capital'][0].update(common_equity=12.,retained_earnings=0.,eligible_aoci=0.,
        cet1_deductions=0.,additional_tier1=0.,total_leverage_exposure=100.)
    spec['capital'][0]['requirements']['slr']=dict(minimum=.08,buffer=.01,management=.01)
    spec['strategy_units']=lib['units']
    spec['capital_deltas']=[dict(snapshot=0,metric='slr',scenario=0,month=0,
        numerator_per_unit=[0.,0.],denominator_per_unit=[1.,1.],include_management_buffer=True)]
    limits=evaluate(spec)['capital_limits']
    with run_context(RunConfig(compute_backend='rust')):
        out=optimize_balance_sheet([(lib,base)],cash_budget=100.,max_total_assets=100.,capital_limits=limits)
    assert out['total_new_assets_$']==pytest.approx(20.)
    assert out['capital_replay'][0]['policy_id']==spec['policy_id']
    spec['capital'][0]['total_leverage_exposure']=0.
    with pytest.raises(ValueError,match='positive observed denominator'):evaluate(spec)
