"""Complete raw commercial closing/daily funding gates and financial adversaries."""
from copy import deepcopy
from pathlib import Path
import json
import math
import pytest
from portfolio_risk.core.commercial_treasury_native import run
from portfolio_risk.analytics.commercial_treasury_reference import verify

ROOT=Path(__file__).resolve().parents[3]
def request():return deepcopy(json.loads((ROOT/'examples/models/commercial-treasury-1.json').read_text()))


def test_complete_native_capital_and_daily_ftp_replay_retains_same_source():
    raw=request();actual,_=run(raw);receipt=verify(raw,actual)
    assert receipt['closing_capital_verified'] and receipt['daily_ftp_verified']
    assert actual['capital']['capital_bridge'][0]['assets']==1000.
    assert actual['capital']['capital_bridge'][0]['retained_earnings']==0.
    assert actual['ftp']['ftp_reconciliation'][0]['external_profit']==0.
    assert all(p['expected_loss']==0. for p in actual['ftp']['prepared_specification']['positions'])
    assert actual['economic_loss_deducted_again'] is False


@pytest.mark.parametrize('target',['money','boolean','canonical_coupon','funding_balance','funding_identity','capital_review','capital_ratio','capital_group'])
def test_native_treasury_rehashed_nested_scalar_tamper_is_independently_rejected(target):
    raw=request();actual,_=run(raw)
    if target=='money':actual['ftp']['ftp_positions'][0]['business_profit']+=1.
    elif target=='boolean':actual['economic_loss_deducted_again']=0
    elif target=='canonical_coupon':
        next(s for s in actual['source_receipt']['sources'][0]['carry']['carry_points'] if s.get('canonical_legal_coupons'))['canonical_legal_coupons'][0]['legal_remaining']+=1.
    elif target=='funding_balance':actual['funding_paths'][0]['preparation']['days'][0]['average_funding_balance']+=1.
    elif target=='funding_identity':actual['funding_paths'][0]['preparation']['raw_request']['states'][0]['origins'][0]['amount']=True
    elif target=='capital_review':actual['capital']['commercial_closing_review']['revision']='substituted'
    elif target=='capital_ratio':actual['capital']['capital_metrics'][0]['numerator']+=1.
    else:actual['capital']['exposure_groups'][0]['negative_accounting_exposure']=0
    with pytest.raises((ValueError,ArithmeticError)):verify(raw,actual)


def test_default_closing_retains_actual_claim_and_does_not_double_deduct_modeled_loss():
    raw=request();raw.pop('ftp');raw['raw_commercial']['spec']['horizon_days']=35
    for source in raw['raw_commercial']['commercial']:
        credit=source['credit'];credit['max_jumps_per_path']=1;terms=credit['intervals'][0];terms['hazards_per_year'][0][3]=100.;terms['loss_given_default']=[.25]*3;terms['complete_recovery_tails']=[[dict(lag_years=0.,weight=1.)] for _ in range(3)]
        credit['draw_stream']=dict(kind='supplied',stream_id='default-control',stream_revision='1',paths=[dict(path_key=17,pairs=[dict(waiting_uniform=math.exp(-100.*12.5/365),destination_uniform=.5)])])
    actual,_=run(raw);verify(raw,actual)
    claim=actual['source_receipt']['sources'][0]['carry']['credit']['result']['paths'][0]['default']
    assert claim['economic_principal_loss']==25.
    bridge=actual['capital']['capital_bridge'][0]
    assert bridge['retained_earnings']==0.
    assert bridge['assets']==pytest.approx(1000.,abs=1e-9)
    assert bridge['credit_rwa']>0. and actual['economic_loss_deducted_again'] is False


def test_native_performing_draw_owns_actual_funding_and_complete_daily_ftp():
    from test_commercial_reference import draw_request
    from datetime import date
    raw=request();template,_=draw_request();start=template['source']['current']['asof_ordinal'];cost=(20.+20.*.15*7/365)*math.exp(-math.log1p(.07)*7/365)
    raw['raw_commercial']['spec']['horizon_days']=30
    for source in raw['raw_commercial']['commercial']:
        draw=deepcopy(template['performing_draw_tranches'][0]);draw['source']['current']['scenario_id']=source['source']['current']['scenario_id']
        for role in ('original','current'):
            main=source['source'][role];main['reporting_end_ordinal']=main['tail_end_ordinal']=start+7;main['periods'][0].update(end_ordinal=start+5,payment_ordinal=start+7)
            tranche=draw['source'][role];tranche.update(asof_ordinal=start+3,reporting_end_ordinal=start+10,tail_end_ordinal=start+10);tranche['periods'][0].update(start_ordinal=start+3,end_ordinal=start+10,payment_ordinal=start+10)
        source['source']['acquisition']['cost']=100.+100.*.1*5/365;draw['source']['acquisition']['cost']=cost;draw['draw_ordinal']=start+3
        source['performing_draw_tranches']=[draw];source['credit']['opening_undrawn_commitment']=40.;source['credit']['max_jumps_per_path']=1;source['credit']['intervals'][0]['end_year']=10/365
        source['maturity_basis_policy']=dict(review=dict(id='complete-review',revision='1'),disposition='dispose_financial_basis_after_exact_legal_close-1')
    raw['capital']['treasury']['as_of']=date.fromordinal(start+30).isoformat()
    for policy in raw['capital']['policies']:policy['capital']['period']=raw['capital']['treasury']['as_of']
    actual,_=run(raw);verify(raw,actual)
    carry=actual['source_receipt']['sources'][0]['carry'];draw=next(p for p in carry['carry_points'] if p['kind']=='funded_draw')
    assert draw['principal']==120. and draw['undrawn_commitment']==20.
    assert draw['cash']==-cost and draw['draw_tranches'][0]['original_annual_effective_rate']==.07
    funding=actual['funding_paths'][0]['preparation'];draw_day=next(d for d in funding['days'] if d['ordinal']==start+3)
    assert draw_day['bank_cash']==-cost
    assert funding['closing_funding_balance']==pytest.approx(0.,abs=1e-12)


def test_native_large_complete_tail_preserves_preflight_work_refusal():
    from test_commercial_reference import draw_request
    from datetime import date
    raw=request();raw.pop('ftp');template,_=draw_request();start=template['source']['current']['asof_ordinal']
    for source in raw['raw_commercial']['commercial']:
        draw=deepcopy(template['performing_draw_tranches'][0]);draw['source']['current']['scenario_id']=source['source']['current']['scenario_id']
        source['performing_draw_tranches']=[draw];source['credit']['opening_undrawn_commitment']=40.;source['credit']['intervals'][0]['end_year']=50/365;source['credit']['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=1000/365,weight=1.)]for _ in range(3)]
    raw['capital']['treasury']['as_of']=date.fromordinal(start+1050).isoformat()
    for policy in raw['capital']['policies']:policy['capital']['period']=raw['capital']['treasury']['as_of']
    with pytest.raises(ValueError,match='upper exceeds|preflight exceeds|preflight exceeded|admission exceeds'):run(raw)


@pytest.mark.parametrize('target',['complete_date','missing_review','late_reset','zero_allocation_unclosed'])
def test_native_treasury_invalid_source_or_policy_refuses_before_publication(target):
    raw=request()
    if target=='complete_date':raw['capital']['treasury']['as_of']='2024-01-31'
    elif target=='missing_review':raw['ftp']['reviews'].pop()
    elif target=='late_reset':raw['ftp']['reviews'][0]['resets'][0]['ordinal']+=1
    else:
        raw['raw_commercial']['allocations'][0]['allocated_face']=0.
        for source in raw['raw_commercial']['commercial']:source['credit']['opening_undrawn_commitment']=10.
    with pytest.raises(ValueError):run(raw)


def test_reviewed_provision_chargeoff_waiver_and_recovery_enter_capital_once():
    from test_commercial_reference import reviewed_request
    from datetime import date
    raw=request();raw.pop('ftp');template=reviewed_request();start=template['source']['current']['asof_ordinal']
    for source in raw['raw_commercial']['commercial']:
        source['credit']=deepcopy(template['credit']);source['credit']['max_jumps_per_path']=1;source['reviewed_credit_actions']=deepcopy(template['reviewed_credit_actions'])
    raw['capital']['treasury']['as_of']=date.fromordinal(start+45).isoformat()
    for policy in raw['capital']['policies']:policy['capital']['period']=raw['capital']['treasury']['as_of']
    actual,_=run(raw);verify(raw,actual)
    bridge=actual['capital']['capital_bridge'][0]
    assert bridge['retained_earnings']==pytest.approx(-40.,abs=1e-12)
    assert actual['economic_loss_deducted_again']is False
    state=actual['source_receipt']['sources'][0]['accounting']['closing'][0]
    assert state['principal']==0. and state['reviewed_credit_state']['legal_principal']==30.


def test_reviewed_credit_legal_claim_tail_refuses_daily_ftp_instead_of_retiring_chargeoff():
    from test_commercial_reference import reviewed_request
    from datetime import date
    raw=request();template=reviewed_request();start=template['source']['current']['asof_ordinal']
    for source in raw['raw_commercial']['commercial']:
        source['credit']=deepcopy(template['credit']);source['credit']['max_jumps_per_path']=1;source['reviewed_credit_actions']=deepcopy(template['reviewed_credit_actions'])
    raw['capital']['treasury']['as_of']=date.fromordinal(start+45).isoformat()
    for policy in raw['capital']['policies']:policy['capital']['period']=raw['capital']['treasury']['as_of']
    with pytest.raises(ValueError,match='tail remains'):run(raw)
