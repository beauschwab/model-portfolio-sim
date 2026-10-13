"""Complete native journals, cut openings, original credit streams and replay gates."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import pytest
from portfolio_risk.core.commercial_holding_native import run
from portfolio_risk.analytics.commercial_holding_reference import verify

ROOT=Path(__file__).resolve().parents[3]
def raw():return json.loads((ROOT/'examples/models/commercial-holding-journal-1.json').read_text())

def drawn_ledger(short=False):
    from test_commercial_reference import draw_request
    import math
    template=draw_request()[0];origin=template['source']['current']['asof_ordinal']
    if short:
        # A distinct complete12-day contract, not a truncated reporting horizon.
        # This performing control supplies zero transition hazards. Its explicit
        # one-jump guard needs no reservation for four unobserved credit jumps.
        template['credit']['max_jumps_per_path']=1
        for role in ('original','current'):
            src=template['source'][role];src.update(reporting_end_ordinal=origin+12,tail_end_ordinal=origin+12)
            src['periods'][0].update(end_ordinal=origin+10,payment_ordinal=origin+12)
        template['source']['acquisition']['cost']=100.+100.*.1*10./365.
        template['credit']['intervals'][0]['end_year']=12./365.
        draw=template['performing_draw_tranches'][0];draw['draw_ordinal']=origin+5
        for role in ('original','current'):
            src=draw['source'][role];src.update(asof_ordinal=origin+5,reporting_end_ordinal=origin+12,tail_end_ordinal=origin+12)
            src['periods'][0].update(start_ordinal=origin+5,end_ordinal=origin+12,payment_ordinal=origin+12)
        draw['source']['acquisition']['cost']=(20.+20.*.15*7./365.)*math.pow(1.07,-7./365.)
    return template

@pytest.fixture(scope='module')
def accepted():
    request=raw();result,_=run(request);return request,result

def test_commercial_holding_journal_raw_full_and_window_replay(accepted):
    request,result=accepted;assert verify(request,result)['verified']
    cut=result['cut_day'];journal=result['window_tables']['journal']
    assert all(r['day']>=cut for r in journal)
    assert all(r['event']not in ('reviewed_opening_snapshot','opening')for r in journal)
    assert all(r['signed_prior_pnl']<0. for r in result['window_tables']['retained_prior_pnl'])
    assert any(r['unweighted_book']['in_default']for r in result['window_tables']['commercial_daily'])
    assert result['prior_publication_verified']is False and result['original_credit_clock_restarted']is False

@pytest.mark.parametrize('target',['opening','prefix','postings','order','prior_pnl','closing','daily','evidence','yield'])
def test_commercial_holding_journal_independent_rehashed_money_and_order_tampering_reject(accepted,target):
    request,result=deepcopy(accepted);window=result['window_tables']
    if target=='opening':window['opening'][0]['balance']+=1.
    elif target=='prefix':result['prefix_proofs'][0]['openings'][0]['book']['principal']+=1.
    elif target=='postings':window['journal'][-1]['value']+=1.
    elif target=='order':window['journal'].reverse()
    elif target=='prior_pnl':window['retained_prior_pnl'][0]['signed_prior_pnl']+=1.
    elif target=='closing':window['trial_balance'][0]['balance']+=1.
    elif target=='daily':window['commercial_daily'][0]['unweighted_book']['gross_carry']+=1.
    elif target=='evidence':result['prior_publication_verified']=True
    else:result['source_receipt']['sources'][0]['raw_request']['source']['acquisition']['original_annual_effective_rate']+=.01
    with pytest.raises((ArithmeticError,ValueError,KeyError)):verify(request,result)

@pytest.mark.parametrize('face',[0.,50.])
def test_commercial_holding_journal_scaled_and_zero_allocation_replays(face):
    request=raw();request['origin_ledger']['allocations'][0]['allocated_face']=face
    result,_=run(request);assert verify(request,result)['verified']
    assert all(r['allocation_factor']==face/100. for r in result['window_tables']['commercial_daily'])

@pytest.mark.parametrize('kind',['nonaccrual','draw','reviewed','post_principal'])
def test_commercial_holding_journal_gross_and_reviewed_state_cuts(kind):
    from test_commercial_reference import request as base,draw_request,reviewed_request
    request=raw();old=request['origin_ledger'];template=drawn_ledger(True)if kind=='draw'else reviewed_request()if kind=='reviewed'else base()
    if kind=='nonaccrual':template['accounting']['recognition'][0]['by_origin']=['legal_only_income_stopped']*3
    if kind=='post_principal':
        for role in ('original','current'):template['source'][role]['periods'][0]['payment_ordinal']=template['source'][role]['asof_ordinal']+10
    cut=10 if kind in ('reviewed','draw')else 15
    old['commercial']=[]
    for scenario in ('baseline','stress'):
        source=deepcopy(template);source['source']['current']['scenario_id']=scenario
        for draw in source.get('performing_draw_tranches',[]):draw['source']['current']['scenario_id']=scenario
        old['commercial'].append(source)
    old['spec']['horizon_days']=35;request['revised_sources']=deepcopy(old['commercial']);request['cut_ordinal']=template['source']['current']['asof_ordinal']+cut
    result,_=run(request);assert verify(request,result)['verified']
    books=[p['openings'][0]['book']for p in result['prefix_proofs']]
    if kind=='draw':assert all(b['principal']==120. for b in books)
    elif kind=='reviewed':assert all(b['reviewed_credit_state']['legal_principal']==90. for b in books)
    elif kind=='post_principal':assert all(b['principal']==0. and b['legal_interest']<0. for b in books)
    else:assert all(b['legal_interest']>0. and b['recognized_interest']==0. for b in books)

def test_commercial_holding_journal_cancellation_has_no_publication():
    with pytest.raises(InterruptedError):run(raw(),cancelled=lambda:True)

def test_commercial_holding_journal_preserves_existing_large_draw_refusal():
    request=raw();template=drawn_ledger();request['origin_ledger']['commercial']=[]
    for scenario in ('baseline','stress'):
        src=deepcopy(template);src['source']['current']['scenario_id']=scenario
        for draw in src['performing_draw_tranches']:draw['source']['current']['scenario_id']=scenario
        request['origin_ledger']['commercial'].append(src)
    request['revised_sources']=deepcopy(request['origin_ledger']['commercial'])
    with pytest.raises(ValueError,match='all-run preflight exceeds'):run(request)

def test_commercial_holding_journal_fresh_default_forbids_python_finance():
    script='''import json,sys
from portfolio_risk.core import runtime
from portfolio_risk.analytics import model_contracts,commercial_credit_reference,commercial_accounting_reference,commercial_holding_reference
assert runtime._CURRENT.get()is None and runtime.RunConfig().compute_backend=="rust"
def forbidden(*a,**k):raise AssertionError("Python finance reached native journal")
commercial_credit_reference.replay=forbidden;commercial_accounting_reference.groups=forbidden;commercial_holding_reference.verify=forbidden
r=json.load(open(sys.argv[1]));v=model_contracts.journal_commercial_holding(r)
assert v["complete_origin_daily_sim_adoption"]is True and v["second_acquisition_posted"]is False
print("fresh native holding journal ownership passed")
'''
    process=subprocess.run([sys.executable,'-c',script,str(ROOT/'examples/models/commercial-holding-journal-1.json')],capture_output=True,text=True,timeout=60)
    assert process.returncode==0,process.stdout+process.stderr
