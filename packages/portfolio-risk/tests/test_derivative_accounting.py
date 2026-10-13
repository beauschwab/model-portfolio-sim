"""Native production calculation followed by an independent raw-source verifier."""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path
import subprocess
import sys

import pytest

from portfolio_risk.analytics.model_contracts import account_derivative_contract
from portfolio_risk.analytics.derivative_verification import (
    DerivativeVerificationError, verify_derivative_accounting,
)

ROOT=Path(__file__).resolve().parents[3]


def identity(name): return {"id":name,"revision":"r1"}
def day(n): return dt.date(2024,1,n).toordinal()
def utc(ordinal): return (ordinal-dt.date(1970,1,1).toordinal())*86400+9*3600
def request(): return json.loads((ROOT/'examples/models/derivative-accounting-1.json').read_text())['request']
def csa(r): return r['derivative']['operation']['request']


def reset_node(n, ordinal):
    n['ordinal']=ordinal; n['valuation_cutoff_unix_seconds']=utc(ordinal)
    for t in n['trades']:
        if t['kind']!='swap': continue
        s=t['request']; s['asof_ordinal']=ordinal; s['valuation_cutoff_unix_seconds']=utc(ordinal); s['tail_end_ordinal']=max(ordinal,day(11))
        curve=s['discount_curve']; curve['asof_ordinal']=ordinal; curve['information_unix_seconds']=utc(ordinal)
        curve['points']=[{'ordinal':ordinal,'discount_factor':1.}]+([{'ordinal':day(11),'discount_factor':1.}] if ordinal<day(11) else [])
    return n


def mpor_request(rate=.08, settle=False):
    r=request();r['derivative']['model_version']='derivative-contracts-2'
    p=r['physical_index_paths'].pop();p['observations'][0]['rate']=rate
    c=csa(r);c['policy']['receive_threshold']=c['policy']['post_threshold']=10.
    dispositions=[{'position_id':'swap','event_id':e,'payment_ordinal':day(11),'disposition':{'kind':'settle_on_node','ordinal':day(11),'evidence':identity('modeled-settlement')} if settle else {'kind':'retain_to_closeout'}} for e in ('fixed:f1','floating:l1')]
    c['paths'][0]['default']={'default_ordinal':day(3),'closeout_ordinal':day(12),'legal_review':identity('default-legal'),'replacement_value_policy':identity('replacement'),'counterparty_recovery':.4,'posted_vm_return_recovery':1.,'posted_im_return_recovery':1.,'posted_collateral_setoff':True,'received_im_available_on_default':False,'mpor':{'identity':identity('mpor'),'claim_measure':'physical','carry_policy':'nominal_no_interest','exercise_rights_policy':'preserve_contractual_european_to_closeout','physical_index_paths':[p],'settlement_instructions':dispositions}}
    return r


def option_request(physical=True, short=False, delay=False):
    r=request();r['physical_index_paths']=[]
    c=csa(r);c['policy']['tail_return_ordinal']=day(14);c['policy']['receive_threshold']=c['policy']['post_threshold']=10.
    m=c['mapping'][0];m['position_id']='option';m['contract']=identity('option-contract')
    review=r['policy']['trades'][0];review['position_id']='option';review['delivery_review']=identity('delivery-accounting')
    option=json.loads((ROOT/'examples/models/derivative-option-1.json').read_text())['request']['operation']['request']
    option['scenario_id']='p';option['settlement']='physical_delivery' if physical else 'collateralized_cash_price';option['side']='short' if short else 'long';option['settlement_ordinal']=day(4) if delay else day(3)
    for s in option['states']:
        s['underlying']['scenario_id']='p'
        if delay:s['underlying']['discount_curve']['points'].insert(1,{'ordinal':day(4),'discount_factor':.98})
    terminal=deepcopy(option);terminal['asof_ordinal']=day(3);terminal['states']=terminal['states'][:1];terminal['states'][0]['probability']=1.;terminal['states'][0]['discount_to_expiry']=1.
    base=deepcopy(c['paths'][0]['nodes'][0]);nodes=[]
    for ordinal in range(day(1),day(14)+1):
        n=reset_node(deepcopy(base),ordinal)
        if ordinal<day(3):
            live=deepcopy(option);live['asof_ordinal']=ordinal;n['trades']=[{'kind':'european_swaption','request':live}]
        else:
            u=deepcopy(terminal['states'][0]['underlying']);u['position_id']='delivered-swap';u['asof_ordinal']=ordinal;u['valuation_cutoff_unix_seconds']=utc(ordinal);u['tail_end_ordinal']=max(ordinal,day(13))
            u['discount_curve']['asof_ordinal']=ordinal;u['discount_curve']['information_unix_seconds']=utc(ordinal);u['discount_curve']['points']=[{'ordinal':ordinal,'discount_factor':1.}]+([{'ordinal':day(13),'discount_factor':1.}] if ordinal<day(13) else [])
            if short:u['side']='receiver'
            curve=None
            if delay and ordinal<day(4):
                curve=deepcopy(u['discount_curve']);curve['points']=[{'ordinal':ordinal,'discount_factor':1.},{'ordinal':day(4),'discount_factor':.98}]
            n['trades']=[{'kind':'expired_european','terminal':deepcopy(terminal),'election':identity('election'),'delivered_underlying':u if physical else None,'settlement_discount':curve}]
        nodes.append(n)
    c['paths'][0]['nodes']=nodes
    value=-.2475 if short else .2475;r['opening']['trades']=[{'position_id':'option','clean_value':value}];r['opening']['equity_credit_balance']=100.+value
    if physical:
        obs=deepcopy(terminal['states'][0]['underlying']['floating_periods'][0]['coupon']['observations'])
        obs[0]['source']={'kind':'observed','fixing':identity('physical-reset'),'publication_unix_seconds':utc(day(3))-3600}
        r['physical_index_paths']=[{'position_id':'option','period_id':'l1','path':identity('physical-path'),'observations':obs}]
    return r


def accounting_cases():
    cases={"base":request(),"mpor":mpor_request(),"mpor-negative":mpor_request(-.02),"mpor-settled":mpor_request(settle=True),"physical-option":option_request(),"short-physical":option_request(short=True),"cash-option":option_request(False),"cash-delayed":option_request(False,delay=True)}
    r=request();r['policy']['trades'][0]['coupon_recognition']='contractual_income_at_due_with_mark_offset';cases['coupon-income']=r
    r=request();r['physical_index_paths'][0]['observations'][0]['rate']=-.02;cases['actual-negative']=r
    r=request();r['opening']['trades'][0]['clean_value']=-.5;r['opening']['equity_credit_balance']=99.5
    for n in csa(r)['paths'][0]['nodes']:n['trades'][0]['request']['side']='receiver'
    cases['receiver']=r
    r=request();r['opening']['posted_im_asset']=.2;r['opening']['received_im_custody']=.3;r['opening']['equity_credit_balance']+=.2
    c=csa(r);c['policy']['opening_posted_im']=.2;c['policy']['opening_received_im']=.3;c['policy']['receive_threshold']=c['policy']['post_threshold']=10.
    for n in c['paths'][0]['nodes']:n['collateral_rate']=-.036
    cases['signed-im-interest']=r
    r=request();r['physical_index_paths']=[];r['opening']['posted_im_asset']=.2;r['opening']['received_vm_liability']=.2;r['opening']['received_im_custody']=.1
    c=csa(r);c['policy']['opening_posted_im']=.2;c['policy']['opening_received_im']=.1;c['policy']['opening_vm']=.2;c['policy']['receive_threshold']=c['policy']['post_threshold']=10.
    c['paths'][0]['default']={'default_ordinal':day(1),'closeout_ordinal':day(3),'legal_review':identity('closeout-legal'),'replacement_value_policy':identity('replacement'),'counterparty_recovery':.5,'posted_vm_return_recovery':1.,'posted_im_return_recovery':.25,'posted_collateral_setoff':True,'received_im_available_on_default':True,'mpor':None};cases['default-im']=r
    r=request();c=csa(r);c['policy']['settlement_lag_calendar_days']=2;c['policy']['tail_return_ordinal']=day(14);base=deepcopy(c['paths'][0]['nodes'][0]);c['paths'][0]['nodes']=[reset_node(deepcopy(base),d) for d in range(day(1),day(14)+1)];c['paths'][0]['nodes'][2]['settlement_available']=False;cases['margin-lag']=r
    r=request();n=csa(r)['paths'][0]['nodes'][10];n['payment_timing']=n['trades'][0]['request']['payment_timing']='before_scheduled_payments';cases['before-cash']=r
    r=request();diagnostic=mpor_request()['derivative'];q=diagnostic['operation']['request'];q['measure']='pricing';p=q['paths'][0]['default']['mpor'];p['claim_measure']='pricing';p['physical_index_paths']=[];r['pricing_xva_reference']=diagnostic;cases['xva']=r
    return cases


def overnight_request(calculation, margin_treatment='simple', shift=False):
    r=request();c=csa(r)
    base=deepcopy(c['paths'][0]['nodes'][0]['trades'][0]['request']['floating_periods'][0]['coupon'])
    base.update(index={'index_id':'explicit-overnight','kind':'overnight','tenor_months':None,'average_calendar_days':None},calculation=calculation,margin_treatment=margin_treatment,margin=.01,reset_cutoff_unix_seconds=None,observation_shift=shift,index_bounds={'floor':-.01,'cap':.09},all_in_bounds={'floor':None,'cap':.095})
    observations=[]
    for start,end,rate in [(day(1),day(4),.10),(day(4),day(11),-.02)]:
        obs_start=start-2 if shift else start;obs_end=end-2 if shift else end
        observations.append({'interest_start_ordinal':start,'interest_end_ordinal':end,'observation_start_ordinal':obs_start,'observation_end_ordinal':obs_end,'fixing_ordinal':obs_start,'selection_cutoff_unix_seconds':utc(obs_start),'rate':rate,'source':{'kind':'conditional_projection','model':identity('overnight-model'),'path':identity('index-path'),'information_unix_seconds':utc(day(1)-3),'measure':'pricing'}})
    base['observations']=observations
    for n in c['paths'][0]['nodes']:n['trades'][0]['request']['floating_periods'][0]['coupon']=deepcopy(base)
    actual=deepcopy(observations)
    for o in actual:o['source']={'kind':'observed','fixing':identity('overnight-'+str(o['fixing_ordinal'])),'publication_unix_seconds':utc(o['fixing_ordinal'])-3600}
    r['physical_index_paths'][0]['observations']=actual
    # Independent hand calculation: index .10 -> .09 plus margin .01 -> .095;
    # second index -.02 -> -.01 plus margin -> zero. 3 and 7 actual days.
    if calculation=='daily_simple':factor=.095*3/360.
    elif margin_treatment=='compounded':factor=(1+.095*3/360.)*(1+0.*7/360.)-1.
    else:factor=(1+.085*3/360.)*(1-.01*7/360.)-1.+.01*10/360.
    clean=360*factor-.5;r['opening']['trades'][0]['clean_value']=clean;r['opening']['equity_credit_balance']=100.+clean
    return r


@pytest.mark.parametrize('calculation,margin,shift',[('daily_simple','simple',False),('daily_simple','simple',True),('daily_compounded','simple',False),('daily_compounded','simple',True),('daily_compounded','compounded',False),('daily_compounded','compounded',True)])
def test_all_admitted_overnight_weight_margin_cap_floor_conventions(calculation,margin,shift):
    r=overnight_request(calculation,margin,shift);out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified']
    assert out['daily'][-1]['cash']==pytest.approx(r['opening']['equity_credit_balance'],abs=1e-9)


@pytest.mark.parametrize('basis,denominator',[('act365_fixed',365.),('act_act_isda',366.)])
def test_actual_date_bases_are_independent_from_native_helpers(basis,denominator):
    r=request()
    for n in csa(r)['paths'][0]['nodes']:
        for leg in ('fixed_periods','floating_periods'):n['trades'][0]['request'][leg][0]['day_count']=basis
    value=360*.05*10/denominator;r['opening']['trades'][0]['clean_value']=value;r['opening']['equity_credit_balance']=100.+value
    assert verify_derivative_accounting(r,account_derivative_contract(r))['verified']


@pytest.mark.parametrize('netted',[True,False])
def test_two_raw_trades_gross_legal_buckets_and_reviewed_state(netted):
    r=request();c=csa(r);other=deepcopy(c['mapping'][0]);other['position_id']='receiver';other['contract']=identity('other-contract');other['enforceable_netting']=netted;c['mapping'][0]['enforceable_netting']=netted;c['mapping'].append(other)
    p=deepcopy(r['policy']['trades'][0]);p['position_id']='receiver';r['policy']['trades'].append(p)
    r['opening']['trades'].append({'position_id':'receiver','clean_value':-.5});r['opening']['equity_credit_balance']=100.
    actual=deepcopy(r['physical_index_paths'][0]);actual['position_id']='receiver';r['physical_index_paths'].append(actual)
    for n in c['paths'][0]['nodes']:
        t=deepcopy(n['trades'][0]);t['request']['position_id']='receiver';t['request']['contract']=identity('other-contract');t['request']['side']='receiver';n['trades'].append(t)
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert out['daily'][-1]['cash']==pytest.approx(100.,abs=1e-9)


@pytest.mark.parametrize('physical',[True,False])
def test_mpor_option_raw_election_and_delivered_claims_survive_to_closeout(physical):
    r=option_request(physical,delay=not physical);r['derivative']['model_version']='derivative-contracts-2';c=csa(r)
    observations=r['physical_index_paths'];r['physical_index_paths']=[]
    instructions=[{'position_id':'option','event_id':e,'payment_ordinal':day(13) if physical else day(4),'disposition':{'kind':'retain_to_closeout'}} for e in ('fixed:f1','floating:l1') if physical]
    if not physical:instructions=[{'position_id':'option','event_id':'option_cash:up','payment_ordinal':day(4),'disposition':{'kind':'retain_to_closeout'}}]
    c['paths'][0]['default']={'default_ordinal':day(1),'closeout_ordinal':day(14),'legal_review':identity('default-legal'),'replacement_value_policy':identity('replacement'),'counterparty_recovery':.4,'posted_vm_return_recovery':1.,'posted_im_return_recovery':1.,'posted_collateral_setoff':True,'received_im_available_on_default':False,'mpor':{'identity':identity('mpor'),'claim_measure':'physical','carry_policy':'nominal_no_interest','exercise_rights_policy':'preserve_contractual_european_to_closeout','physical_index_paths':observations,'settlement_instructions':instructions}}
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert len(out['native']['paths'][0]['mpor_exercises'])==1


@pytest.mark.parametrize('name',list(accounting_cases()))
def test_native_raw_contract_economics_journal_and_complete_daily_reference(name):
    r=accounting_cases()[name];out=account_derivative_contract(r);receipt=verify_derivative_accounting(r,out)
    assert receipt['verified'] and not receipt['persisted_journal_verified']
    assert receipt['checked_daily_rows']==len(csa(r)['paths'][0]['nodes'])
    if name=='mpor':assert out['daily'][-1]['cash']==pytest.approx(100.12,abs=1e-9)
    if name=='physical-option':assert out['daily'][-1]['cash']==pytest.approx(100.5,abs=1e-9)


@pytest.mark.parametrize('field', ['cash','mark','right','source','journal','date','duplicate','policy','mapping','margin','xva'])
def test_tampered_output_has_no_verification_receipt(field):
    r=accounting_cases()['xva'] if field=='xva' else request();out=account_derivative_contract(r)
    if field=='cash':out['daily'][-1]['cash']+=1.
    elif field=='mark':out['native']['paths'][0]['exposures'][0]['clean_trade_values'][0][1]+=.1
    elif field=='right':out['cash_rights'][1]['physical_amount']+=.2
    elif field=='source':out['cash_rights'][1]['sources'][0]['fixing']['revision']='tampered'
    elif field=='journal':out['intents'][0]['postings'][0]['debit_amount']+=.1;out['intents'][0]['postings'][1]['debit_amount']-=.1
    elif field=='date':out['intents'][0]['ordinal']+=1
    elif field=='duplicate':out['intents'].append(deepcopy(out['intents'][0]))
    elif field=='policy':out['policy']['trades'][0]['designation_review']['revision']='other'
    elif field=='mapping':out['native']['legal_mapping'][0]['legal_review']['revision']='other'
    elif field=='margin':out['native']['paths'][0]['calls'][0]['amount']+=.1
    elif field=='xva':out['pricing_xva_diagnostic']['cva']+=.1
    with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,out)


@pytest.mark.parametrize('field',['default','closeout','settlement','delivery','underlying','exercise'])
def test_default_and_option_tampering_rejected(field):
    r=option_request() if field in {'delivery','underlying','exercise'} else mpor_request(settle=field=='settlement');out=account_derivative_contract(r)
    if field=='default':out['native']['paths'][0]['closeouts'][0]['counterparty_loss']+=.1
    elif field=='closeout':out['native']['paths'][0]['closeouts'][0]['unpaid_obligation_claim']+=.1
    elif field=='settlement':out['cash_rights'][0]['settled_ordinal']+=1
    elif field=='delivery':out['daily'][3]['trades'][0]['contract_position_id']='duplicate-swap'
    elif field=='underlying':r['derivative']['operation']['request']['paths'][0]['nodes'][3]['trades'][0]['delivered_underlying']['fixed_rate']+=.01
    elif field=='exercise':r['derivative']['operation']['request']['paths'][0]['nodes'][3]['trades'][0]['election']['revision']='other'
    with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,out)


def test_fresh_process_native_calculation_forbids_python_financial_callbacks_then_verifies():
    script='''
import json
from pathlib import Path
from portfolio_risk.analytics.model_contracts import account_derivative_contract
from portfolio_risk.analytics import derivative_verification as independent
from portfolio_risk.core import conventions, pricing
from portfolio_risk.products import hedges
r=json.loads(Path('examples/models/derivative-accounting-1.json').read_text())['request']
def forbidden(*args,**kwargs):raise AssertionError('Python financial callback during native calculation')
for module,names in [(conventions,['year_fraction','gen_schedule']), (pricing,['solve_oas_from_A','pv_from_A']), (hedges,['run_hedge_risk','swap_mtm_and_carry','swaption_value'])]:
    for name in names:
        if hasattr(module,name):setattr(module,name,forbidden)
oracle=independent._derive
independent._derive=forbidden
out=account_derivative_contract(r)
independent._derive=oracle
assert independent.verify_derivative_accounting(r,out)['verified']
print('native calculation then independent verification')
'''
    done=subprocess.run([sys.executable,'-c',script],cwd=ROOT,capture_output=True,text=True,timeout=60)
    assert done.returncode==0,done.stderr


def test_unsupported_source_policy_calendar_and_large_work_fail_closed():
    out=account_derivative_contract(request())
    for field in ['hedge','unit','measure','calendar','future','work','override']:
        r=request()
        if field=='hedge':r['policy']['trades'][0]['designation']='cash_flow_hedge'
        elif field=='unit':r['monetary_unit']='millions'
        elif field=='measure':r['measure']='pricing'
        elif field=='calendar':csa(r)['paths'][0]['nodes'].pop(3)
        elif field=='future':r['physical_index_paths'][0]['observations'][0]['source']['publication_unix_seconds']=utc(day(12))
        elif field=='work':r['physical_index_paths'][0]['observations']*=250001
        elif field=='override':r['cash_override']=1000.
        with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,out)


@pytest.mark.parametrize('timing',['before_scheduled_payments','after_scheduled_payments'])
def test_default_on_payment_date_retains_or_settles_the_declared_right_once(timing):
    r=mpor_request();c=csa(r);d=c['paths'][0]['default'];d['default_ordinal']=day(11)
    n=c['paths'][0]['nodes'][10];n['payment_timing']=n['trades'][0]['request']['payment_timing']=timing
    if timing=='after_scheduled_payments':
        r['physical_index_paths']=d['mpor']['physical_index_paths'];d['mpor']['physical_index_paths']=[];d['mpor']['settlement_instructions']=[]
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified']
    assert out['daily'][-1]['cash']==pytest.approx(100.12 if timing=='before_scheduled_payments' else 100.3,abs=1e-9)


def test_delayed_unpaid_settlement_before_same_day_closeout_has_no_loss_or_duplicate_cash():
    r=mpor_request(settle=True)
    for i in csa(r)['paths'][0]['default']['mpor']['settlement_instructions']:i['disposition']['ordinal']=day(12)
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert out['daily'][-1]['cash']==pytest.approx(100.3,abs=1e-9)
    assert out['daily'][10]['unpaid_receivable']==pytest.approx(.8,abs=1e-9)


def test_raw_clean_curve_marks_and_zero_contract_are_not_read_from_producer_exposure():
    r=request()
    csa(r)['paths'][0]['nodes'][0]['trades'][0]['request']['discount_curve']['points'][1]['discount_factor']=.9
    r['opening']['trades'][0]['clean_value']=.45;r['opening']['equity_credit_balance']=100.45
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert out['daily'][-1]['cash']==pytest.approx(100.5,abs=1e-9)
    # Mutate raw source while keeping producer marks/journals internally balanced.
    csa(r)['paths'][0]['nodes'][0]['trades'][0]['request']['discount_curve']['points'][1]['discount_factor']=.8
    with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,out)
    r=request();r['opening']['trades'][0]['clean_value']=0.;r['opening']['equity_credit_balance']=100.
    for n in csa(r)['paths'][0]['nodes']:n['trades'][0]['request']['notional']=0.
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert out['daily'][-1]['cash']==100.


def test_multiple_periods_and_reference_paths_keep_contract_cash_and_discounts_separate():
    r=request();c=csa(r)
    for n in c['paths'][0]['nodes']:
        s=n['trades'][0]['request'];fixed=s['fixed_periods'][0];first=deepcopy(fixed);first.update(end_ordinal=day(4),payment_ordinal=day(4));second=deepcopy(fixed);second.update(period_id='f2',start_ordinal=day(4));s['fixed_periods']=[first,second]
        if n['ordinal']<day(4):s['discount_curve']['points'].insert(1,{'ordinal':day(4),'discount_factor':1.})
    out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert len(out['cash_rights'])==3
    r=accounting_cases()['xva'];q=r['pricing_xva_reference']['operation']['request'];second=deepcopy(q['paths'][0]);second['path_id']='q2';q['paths'][0]['probability']=.4;second['probability']=.6
    for n in second['nodes']:n['trades'][0]['request']['scenario_id']='q2'
    q['paths'].append(second);out=account_derivative_contract(r);assert verify_derivative_accounting(r,out)['verified'];assert len(out['pricing_xva_diagnostic']['paths'])==2


def test_verified_analysis_receipt_is_absent_after_publication_tampering():
    r=request();out=account_derivative_contract(r);published={}
    def gate_then_publish(payload):
        receipt=verify_derivative_accounting(r,payload)
        published['result']=payload;published['receipt']=receipt
    out['daily'][-1]['cash']+=.5
    with pytest.raises(DerivativeVerificationError):gate_then_publish(out)
    assert not published


@pytest.mark.parametrize('name',['base','mpor','physical-option','xva'])
def test_browser_json_whole_money_and_rate_numbers_are_verified_without_coercing_dates(name):
    def browser_number(value):
        if isinstance(value,dict):return {k:browser_number(v) for k,v in value.items()}
        if isinstance(value,list):return [browser_number(v) for v in value]
        if isinstance(value,float) and value.is_integer():return int(value)
        return value
    r=browser_number(accounting_cases()[name]);out=account_derivative_contract(r)
    assert verify_derivative_accounting(r,out)['verified']
    bad=deepcopy(out);bad['opening']['ordinal']=float(bad['opening']['ordinal'])
    with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,bad)
    bad=deepcopy(out);bad['cash_rights'][-1]['sources'][0]['publication_unix_seconds']+=1
    with pytest.raises(DerivativeVerificationError):verify_derivative_accounting(r,bad)
