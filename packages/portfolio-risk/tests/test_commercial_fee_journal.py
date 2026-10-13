"""Native credit exits, independent fee money and the complete shared journal."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.core.commercial_fee_native import run
from portfolio_risk.analytics.commercial_fee_reference import verify

ROOT=Path(__file__).resolve().parents[3]

def raw():
    return json.loads((ROOT/'examples/models/commercial-fee-journal-1.json').read_text(encoding='utf-8'))

def exited(destination=3,day=12.5):
    from test_commercial_reference import default_request
    import math
    r=raw();source=default_request(destination)
    source['credit']['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform']=math.exp(-10.*day/365.)
    for i,scenario in enumerate(('baseline','stress')):
        s=deepcopy(source);s['source']['current']['scenario_id']=scenario;r['loan']['commercial'][i]=s
    return r

def execute(r):
    result,_=run(r);assert verify(r,result)['verified'];return result

def short_contract(r):
    # A genuine complete six-day instrument, not a truncated report or relaxed cap.
    r['loan']['spec']['horizon_days']=30
    for s,p in zip(r['loan']['commercial'],r['fees'],strict=True):
        origin=s['source']['current']['asof_ordinal']
        for role in('original','current'):
            c=s['source'][role];c.update(reporting_end_ordinal=origin+5,tail_end_ordinal=origin+6)
            c['periods'][0].update(end_ordinal=origin+6,payment_ordinal=origin+6)
        a=s['source']['acquisition'];a['cost']=(100.+100.*.1*6./365.)*(1.+a['original_annual_effective_rate'])**(-6./365.)
        s['credit']['intervals'][0]['end_year']=6./365.;s['credit']['max_jumps_per_path']=1
        p['complete_end_ordinal']=origin+12;p['events'][0]['ordinal']=origin+2;p['events'][1]['ordinal']=origin+10
    return r

def test_fee_complete_cash_tail_preserves_loan_carry_and_credit_draws(tmp_path):
    from portfolio_risk.analytics.model_ledger import run_model_ledger
    r=raw();result=execute(r)
    # A fee route must reuse the identical single credit sample and original EIR.
    manifest=run_model_ledger(r['loan'],tmp_path/'loan-control')
    controls=json.loads((manifest.parent/'prepared-source.json').read_text())['source_receipt']['sources']
    assert result['loan_receipt']['sources']==controls
    assert result['reporting_horizon_days']==35 and result['completion']['protocol']==7
    assert all(f['source']['reporting'][0]['state']['fee_receivable']==5. for paths in result['fee_sources']for f in paths)
    assert all(f['source']['closing'][0]['state']['fee_receivable']==0. for paths in result['fee_sources']for f in paths)
    assert any(row['day']==40 and row['event']=='commercial_nonintegral_fee'for row in result['tables']['journal'])

@pytest.mark.parametrize('sign',[1.,-1.])
def test_fee_fractional_default_uses_separate_waiver_and_recovery(sign):
    r=exited()
    for p in r['fees']:
        p['fees'][0]['entitlement']*=sign
        for e in p['events']:e['amount']*=sign
    result=execute(r)
    for paths in result['fee_sources']:
        f=paths[0];assert f['exit']['reason']=='default'
        assert f['exit']['native_year']==pytest.approx(12.5/365.,abs=1e-15)
        s=f['source']['closing'][0]['state']
        assert s['earned']==5.*sign and s['waived_earned']==5.*sign and s['recovered_waived']==2.5*sign
        assert s['fee_receivable']==s['fee_payable']==0.
        assert f['native_years'][-1]==pytest.approx(17.5/365.,abs=1e-15)
    assert all(row['day']!=40 for row in result['tables']['commercial_fee_lineage'])

def test_fee_recovery_cannot_borrow_loan_lgd_or_coupon_recovery_fraction():
    first=exited();second=deepcopy(first)
    for s in second['loan']['commercial']:
        s['credit']['intervals'][0]['loss_given_default']=[.9]*3
        s['credit']['intervals'][0]['interest_recovery_fraction']=[.1]*3
    a=execute(first);b=execute(second)
    assert a['fee_sources']==b['fee_sources']

def test_fee_joined_admission_charges_longer_loan_recovery_calendar():
    r=short_contract(exited(day=3.5))
    for s in r['loan']['commercial']:
        s['credit']['max_jumps_per_path']=1
        s['credit']['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=35./365.,weight=1.)]for _ in range(3)]
    v=execute(r)
    assert v['loan_receipt']['resolved_specification']['horizon_days']>=41
    assert v['reporting_horizon_days']==30
    assert all(p['day']>=41 for p in v['tables']['commercial_fee_closing'])

@pytest.mark.parametrize('destination,reason',[(4,'other_reviewed'),(None,'maturity')])
def test_fee_prepaid_refund_binds_real_native_exit(destination,reason):
    r=exited(destination)if destination else raw()
    if destination is None:
        for s in r['loan']['commercial']:
            s['maturity_basis_policy']=dict(review=dict(id='basis-disposal',revision='1'),disposition='dispose_financial_basis_after_exact_legal_close-1')
    for p in r['fees']:
        p['events'][0]['action']='cash';p['events'][0]['reason']='settlement'
        p['events'][1]['action']='earn';p['events'][1]['reason']='service'
        policy=p['prepayment'if destination else'maturity'];policy.update(cancel_future_service=True,cancel_future_cash=True,cancel_unearned_entitlement=True,refund_prepaid_fraction=1.)
    result=execute(r)
    for paths in result['fee_sources']:
        f=paths[0];assert f['exit']['reason']==reason
        s=f['source']['closing'][0]['state'];assert s['cancelled_unearned']==5. and s['net_service_cash']==0. and s['earned']==0.
        assert any(g['event']['action']=='refund_prepaid'for g in f['source']['groups'])

def test_fee_exit_precedes_coincident_service_and_cash():
    r=raw()
    for s in r['loan']['commercial']:
        s['maturity_basis_policy']=dict(review=dict(id='basis-disposal',revision='1'),disposition='dispose_financial_basis_after_exact_legal_close-1')
    for p in r['fees']:
        p['events'][0]['ordinal']=r['loan']['commercial'][0]['source']['current']['asof_ordinal']+35
        p['maturity']=deepcopy(p['default'])
    result=execute(r)
    for paths in result['fee_sources']:
        f=paths[0];s=f['source']['closing'][0]['state']
        assert s['cancelled_unearned']==5. and s['earned']==s['net_service_cash']==0.

def test_credit_clock_just_after_integer_cannot_be_posted_before_its_native_time():
    r=exited(day=10.);v=execute(r)
    assert all(row['native_credit_year']<=(row['day']-row['source_origin_day'])/365. for row in v['tables']['commercial_source_lineage'])

@pytest.mark.parametrize('face',[0.,50.])
def test_fee_zero_and_partial_participation_keep_unweighted_policy(face):
    r=exited();r['loan']['allocations'][0]['allocated_face']=face;result=execute(r)
    assert all(f['source']['closing'][0]['state']['recovered_waived']==2.5 for paths in result['fee_sources']for f in paths)
    assert all(v['allocation_factor']==face/100. for v in result['tables']['commercial_fee_daily'])

def test_fee_opposite_sign_rights_remain_gross():
    r=raw()
    for p in r['fees']:
        f=deepcopy(p['fees'][0]);f.update(fee_id='negative-fee',entitlement=-5.);p['fees'].append(f)
        for e in deepcopy(p['events']):
            e.update(event_id='negative-'+e['event_id'],fee_id=f['fee_id'],amount=-e['amount'],sequence=e['sequence']+1);p['events'].append(e)
        p['events'].sort(key=lambda e:(e['ordinal'],e['sequence']))
    result=execute(r)
    rows=result['tables']['trial_balance']
    assert all(v['balance']==0. for v in rows if v['gl_account']in('commercial_fee_receivable','commercial_fee_payable'))
    daily=[v['native_fee_snapshot'] for v in result['tables']['commercial_fee_daily']if v['day']==10]
    assert any(v['fee_receivable']==5. for v in daily)and any(v['fee_payable']==-5. for v in daily)

@pytest.fixture(scope='module')
def accepted():
    r=exited();return r,execute(r)

@pytest.mark.parametrize('target',['amount','exit','clock','review','posting','order','gross','closing','draw','flag','protocol','work','extra_table'])
def test_independent_fee_source_and_shared_reports_reject_tampering(accepted,target):
    r,v=deepcopy(accepted)
    if target=='amount':v['fee_sources'][0][0]['source']['groups'][0]['event']['amount']+=1.
    elif target=='exit':v['fee_sources'][0][0]['exit']['native_event_id']='foreign'
    elif target=='clock':v['tables']['commercial_fee_lineage'][0]['native_year']+=.001
    elif target=='review':v['fee_sources'][0][0]['source']['raw_request']['policy']['revision']='foreign'
    elif target=='posting':v['tables']['journal'][-1]['value']+=1.
    elif target=='order':v['tables']['journal'].reverse()
    elif target=='gross':v['tables']['commercial_fee_daily'][0]['native_fee_snapshot']['fee_receivable']+=1.
    elif target=='closing':v['tables']['trial_balance'][0]['balance']+=1.
    elif target=='draw':v['loan_receipt']['sources'][0]['carry']['credit']['result']['paths'][0]['random_pairs_used']+=1
    elif target=='flag':v['loan_carrying_value_changed']=True
    elif target=='protocol':v['completion']['protocol']=6
    elif target=='work':v['admitted_work_upper']+=1
    else:v['tables']['unowned']=[]
    with pytest.raises((ArithmeticError,ValueError,KeyError)):verify(r,v)

@pytest.mark.parametrize('target',['tranche','position','manual_default','duplicate','unknown','refund','clock','review','amount','oversize','bool_timeout'])
def test_fee_invalid_original_source_rejects(target):
    r=raw();p=r['fees'][0]
    if target=='tranche':p['fees'][0]['tranche_id']='secondary'
    elif target=='position':p['fees'][0]['position_id']='other'
    elif target=='manual_default':p['events'][0]['action']='waive_unpaid_earned';p['events'][0]['reason']='default'
    elif target=='duplicate':p['fees'].append(deepcopy(p['fees'][0]))
    elif target=='unknown':p['events'][0]['fee_id']='missing'
    elif target=='refund':p['fees'][0]['prepaid_refunds_allowed']=False;p['default']['refund_prepaid_fraction']=1.
    elif target=='clock':p['events'][0]['ordinal']=True
    elif target=='review':p['default']['review']['revision']=''
    elif target=='amount':p['events'][0]['amount']=6.
    elif target=='oversize':p['fees']*=65
    else:
        with pytest.raises(ValueError):run(r,timeout=True)
        return
    with pytest.raises(ValueError):run(r)

def test_fee_cancellation_never_returns_a_result():
    with pytest.raises(InterruptedError):run(raw(),cancelled=lambda:True)

def test_fee_multiple_paths_follow_source_binding_order_and_weight_once():
    r=short_contract(exited(day=3.5))
    r['loan']['participations']=[]
    for s in r['loan']['commercial']:
        scenario=s['source']['current']['scenario_id'];s['credit']['max_jumps_per_path']=1
        s['credit']['paths']=[];streams=[]
        pair=deepcopy(s['credit']['draw_stream']['paths'][0]['pairs'])
        for key,weight in((33,.25),(8,.75)):
            s['credit']['paths'].append(dict(path_id=f'path-{key}',path_key=key,initial_live_state='performing'))
            streams.append(dict(path_key=key,pairs=deepcopy(pair)))
            r['loan']['participations'].append(dict(scenario=scenario,position_id='commercial',path_key=key,source_weight=weight))
        s['credit']['draw_stream']['paths']=streams
    v=execute(r)
    rows=[p for p in v['tables']['commercial_fee_daily']if p['scenario']=='baseline'and p['day']==2]
    assert [p['path_key']for p in rows]==[33,8]
    assert [p['source_weight']for p in rows]==[.25,.75]

def test_fee_fresh_native_execution_forbids_python_financial_callbacks():
    import subprocess,sys
    from portfolio_risk.core import product_strategy_native
    script='''import json,sys
from pathlib import Path
from portfolio_risk.core import runtime,product_strategy_native
from portfolio_risk.analytics import model_contracts,commercial_credit_reference,commercial_accounting_reference,commercial_fee_reference,nonintegral_fee_reference
assert runtime._CURRENT.get()is None and runtime.RunConfig().compute_backend=='rust'
def forbidden(*a,**k):raise AssertionError('Python financial callback reached native execution')
commercial_credit_reference.replay=forbidden;commercial_accounting_reference.groups=forbidden
commercial_fee_reference.verify=forbidden;nonintegral_fee_reference.replay=forbidden
product_strategy_native.binary_path=lambda:Path(sys.argv[2])
r=json.load(open(sys.argv[1]));v=model_contracts.analyze_commercial_fees(r)
assert v['shared_daily_sim_adoption']is True and v['completion']==dict(complete=True,protocol=7)
print('fresh native fee journal ownership passed')
'''
    p=subprocess.run([sys.executable,'-c',script,str(ROOT/'examples/models/commercial-fee-journal-1.json'),str(product_strategy_native.binary_path())],capture_output=True,text=True,timeout=60)
    assert p.returncode==0,p.stdout+p.stderr
