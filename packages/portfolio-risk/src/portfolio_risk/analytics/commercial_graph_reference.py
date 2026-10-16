"""Independent raw commercial graph/coefficient publication verification.

Only validation calls this module. Raw source factors, CTMC draws, coupon rights,
original quotes, accounting and full cash tails are independently recomputed;
native monetary results are compared and never feed reference calculations.
"""
from copy import deepcopy
import math
import struct
from scipy.optimize import brentq
from .commercial_credit_reference import replay, typed
from .commercial_accounting_reference import groups
from .commercial_source_reference import compare, verify_source
from .commercial_ledger_reference import identity, verify_compiler,verify_draw_sources
from .commercial_reference_math import _cash, _df, _price

ROLES=('physical','pricing','pricing_down','pricing_up')
METRICS=['nii','balance','dv01','hqla_l2a','outflow30','asf','rsf','rwa','assets','liabilities']
from .commercial_dto import graph_typed as _typed, graph_identity as _raw


def _economic(raw,actual,check,stops=(),measure='pricing',original_acquisition=None):
    check()
    verify_draw_sources(raw,actual,check,measure=measure,original_acquisition=original_acquisition)
    from .commercial_secondary_reference import verify_loan_transfer
    verify_loan_transfer(raw,actual,check)
    if actual.get('physical_journal_eligible') is not False:raise ArithmeticError('Pricing economics cannot be a physical journal source')
    _raw(list(stops),actual.get('native_stops',[]))
    for role in ('original','current'):
        source=actual[role+'_source'];identity(raw['source'][role],source['raw']);compare(raw['factor_policy'],source['policy'],check=check);verify_source(raw['source'][role],source,check)
    expected=replay(raw,check,stops=stops)
    expected['prepared']['raw_credit_path']['measure']=measure;expected['credit']['result']['measure']=measure
    carry=actual['carry'];identity(expected['prepared'],carry['raw_request'])
    if carry.get('schema')!='credit-path-income-proof-1':raise ArithmeticError('Pricing carry schema differs')
    for key in ('credit','carry_points','earned_origin_contributions','complete_recovery_receipts'):
        compare(expected[key],carry[key],f'$.pricing_carry.{key}',check)
    for key in ('native_source_cash','native_coupon_recoveries'):compare(expected[key],carry.get(key,[]),f'$.pricing_carry.{key}',check)
    for key in ('original_anchor_refitted','corporate_source_binding','mixed_ledger_adoption','accounting_approval','complete_coupon_settlement_ownership'):
        if carry.get(key) is not False:raise ArithmeticError('Pricing carry eligibility differs')
    for key in ('native_factor_provider_selected','default_interest_stopped'):
        if carry.get(key) is not True:raise ArithmeticError('Pricing carry source ownership differs')
    return expected


def verify_graph(raw,result,check=lambda:False):
    check();_raw(raw,result['raw_source'])
    if raw.get('schema')!='commercial-product-graph-1' or result.get('schema')!=raw['schema']:raise ArithmeticError('commercial graph explicit model identity differs')
    if any(p.get('generated_policy') for p in raw['positions']):
        from .commercial_generated_graph_reference import verify
        return verify(raw,result,check)
    from .commercial_exercise_reference import compile_policy,stops_for
    from . import commercial_fee_adoption_reference as fee_reference
    from . import commercial_fee_exercise_reference as joint_reference
    sources=[];metrics=[];anchors=[];policies=[];native=result['native_sources'];at=0
    original_sources=result.get('original_physical_sources',[]);original_at=0;proof_at=0
    for p in raw['positions']:
        fees=p.get('fees')
        if fees:
            fee_reference.validate(fees,raw,p,check)
        check();original=[(h['weight'],_cash(replay(h['source'],check)['credit'])) for h in p['original_pricing']]
        exercise=p.get('exercise')
        def compile_current(hs,scenario,role,original,oas,bump):
            if fees:
                policies=[fee_reference._history(fees,scenario['scenario_id'],role,h['path_id'],original)['policy']for h in hs]
                return joint_reference.compile_policy(exercise,hs,policies,scenario['collateral_curve'],oas,fees['discount_spread'],bump,check)
            return compile_policy(exercise,hs,scenario['collateral_curve'],oas,bump,check)
        def stops_current(compiled,h,scenario,role,original):
            if fees:return joint_reference.stops_for(compiled,h['source'],fee_reference._history(fees,scenario['scenario_id'],role,h['path_id'],original)['policy'],check)
            return stops_for(compiled,h['source'],check)
        def original_price(oas):
            if exercise:
                result=compile_current(p['original_pricing'],raw['original_scenario'],'pricing',True,oas,0.)['result']
                return result['joint_fee_values']['loan_value']if fees else result['value']
            return math.fsum(w*_price(c,raw['original_scenario']['collateral_curve'],raw['as_of_ordinal'],raw['as_of_ordinal'],oas,0.) for w,c in original)
        target=p['original_signed_cash_quote'];oas=p['oas_target'].get('base_oas')
        if oas is None:oas=brentq(lambda v:original_price(v)-target,-5.,5.,xtol=2e-15)
        actual=result['original_oas'][len(anchors)];compare(dict(position_id=p['position_id'],base_oas=float(oas),original_signed_cash_quote=float(target)),actual,check=check)
        if p['oas_target']['kind']=='locked' and struct.pack('>d',float(oas))!=struct.pack('>d',actual['base_oas']):raise ArithmeticError('original locked OAS changed bits')
        compare(float(target),float(original_price(oas)),check=check);anchors.append(actual)
        if exercise:
            compiled=compile_current(p['original_pricing'],raw['original_scenario'],'pricing',True,oas,0.)
            policies.append(dict(position_id=p['position_id'],scenario_id=raw['original_scenario']['scenario_id'],role='pricing',policy=compiled['result']))
        if fees:
            original_fee=0.
            for history in p['original_pricing']:
                stops=stops_current(compiled,history,raw['original_scenario'],'pricing',True)if exercise else[]
                projection=replay(history['source'],check,stops=stops)
                fee=fee_reference.project(fees,raw['original_scenario']['scenario_id'],'pricing',history,projection,None,check,original=True,stops=stops)
                original_fee+=history['weight']*fee_reference.price(fee,raw['original_scenario']['collateral_curve'],raw['as_of_ordinal'],raw['as_of_ordinal'],original=True)
            compare(float(fees['original_signed_fee_quote']),float(original_fee),check=check)
        acquisition=None
        if p.get('original_physical'):
            from .commercial_ledger_reference import compile_original_acquisition
            original_stops=[stops_current(compiled,h,raw['original_scenario'],'physical',True) for h in p['original_physical']]
            acquisition=compile_original_acquisition(p['original_physical'],original_stops,check)
            proof=result['original_acquisition_proofs'][proof_at];proof_at+=1
            if set(proof)!={'original_stopped_cash_quote','acquisition','weights','stopped_physical','conditional_tranche_proofs','acquired_cash_partitions'}:raise ArithmeticError('original Physical acquisition proof shape differs')
            for name in ('original_stopped_cash_quote','acquisition','weights','conditional_tranche_proofs','acquired_cash_partitions'):compare(acquisition[name],proof[name],check=check)
            if len(proof['stopped_physical'])!=len(p['original_physical']):raise ArithmeticError('original stopped Physical proof coverage differs')
            for h,stops,actual_economic in zip(p['original_physical'],original_stops,proof['stopped_physical'],strict=True):
                _economic(h['source'],actual_economic,check,stops,measure='physical',original_acquisition=acquisition)
                actual=original_sources[original_at];original_at+=1
                wanted=dict(position_id=p['position_id'],scenario_id=raw['original_scenario']['scenario_id'],run_id=raw['original_scenario']['run_id'],path_id=h['path_id'],path_key=h['source']['credit']['paths'][0]['path_key'],weight=float(h['weight']),role='physical')
                compare(wanted,{k:actual[k] for k in wanted},check=check);identity(h['source'],actual['source_request'])
                replayed=verify_compiler(h['source'],actual['projection'],check,stops=stops,original_acquisition=acquisition)
                compare(_cash(replayed['replay']['credit']),actual['cash'],check=check)
                if fees:fee_reference.project(fees,raw['original_scenario']['scenario_id'],'physical',h,replayed['replay'],actual.get('fee_projection'),check,original=True,stops=stops)
        for scenario in raw['scenarios']:
            hs=next(h for h in p['histories'] if h['scenario_id']==scenario['scenario_id']);bundle=[]
            current_policies={}
            if exercise:
                for role in ROLES[1:]:
                    bump=-raw['bump_bp']*1e-4 if role=='pricing_down' else raw['bump_bp']*1e-4 if role=='pricing_up' else 0.
                    compiled=compile_current(hs[role],scenario,role,False,oas,bump);current_policies[role]=compiled
                    policies.append(dict(position_id=p['position_id'],scenario_id=scenario['scenario_id'],role=role,policy=compiled['result']))
            for role in ROLES:
                for history in hs[role]:
                    check();actual=native[at];at+=1
                    wanted=dict(position_id=p['position_id'],scenario_id=scenario['scenario_id'],run_id=scenario['run_id'],path_id=history['path_id'],path_key=history['source']['credit']['paths'][0]['path_key'],weight=float(history['weight']),role=role)
                    compare(wanted,{k:actual[k] for k in wanted},check=check);identity(history['source'],actual['source_request'])
                    stops=stops_current(current_policies['pricing'if role=='physical'else role],history,scenario,role,False) if exercise else []
                    if role=='physical':verified=verify_compiler(history['source'],actual['projection'],check,stops=stops,original_acquisition=acquisition);ref=verified['replay'];gs=verified['groups']
                    else:ref=_economic(history['source'],actual['projection'],check,stops=stops);gs=None
                    cash=_cash(ref['credit']);compare(cash,actual['cash'],check=check)
                    fee_projection=None
                    if fees:
                        if 'fee_projection'not in actual:raise ArithmeticError('fee native source ownership absent')
                        fee_projection=fee_reference.project(fees,scenario['scenario_id'],role,history,ref,actual['fee_projection'],check,stops=stops)
                    elif 'fee_projection'in actual:raise ArithmeticError('unselected fee source ownership')
                    value=dict(**wanted,source_request=deepcopy(history['source']),cash=cash,reference=ref,groups=gs,fee_projection=fee_projection);bundle.append(value);sources.append(value)
            for mark in raw['reporting_ordinals']:
                prices={}
                for role in ROLES[1:]:
                    bump=-raw['bump_bp']*1e-4 if role=='pricing_down' else raw['bump_bp']*1e-4 if role=='pricing_up' else 0.
                    prices[role]=math.fsum(s['weight']*(_price(s['cash'],scenario['collateral_curve'],raw['as_of_ordinal'],mark,oas,bump)+(fee_reference.price(s['fee_projection'],scenario['collateral_curve'],raw['as_of_ordinal'],mark,bump)if s['fee_projection']else 0.)) for s in bundle if s['role']==role)
                metrics.append(dict(position_id=p['position_id'],scenario_id=scenario['scenario_id'],ordinal=mark,pv=prices['pricing'],dv01=(prices['pricing_down']-prices['pricing_up'])/(2.*raw['bump_bp'])))
    if at!=len(native) or len(anchors)!=len(result['original_oas']):raise ArithmeticError('commercial native source/anchor coverage differs')
    if original_at!=len(original_sources) or proof_at!=len(result.get('original_acquisition_proofs',[])):raise ArithmeticError('original Physical acquisition coverage differs')
    compare(metrics,result['metrics'],check=check)
    compare(policies,result.get('exercise_policies',[]),check=check)
    for name in ('shared_draws_retained','physical_accounting_source_retained'):
        if result.get(name) is not True:raise ArithmeticError('commercial graph ownership receipt differs')
    if result.get('empirical_validation') is not False:raise ArithmeticError('commercial graph empirical eligibility differs')
    if type(result.get('admitted_work_upper')) is not int or not 0<result['admitted_work_upper']<=32_000_000:raise ArithmeticError('commercial graph admitted work invalid')
    return dict(verified=True,sources=sources,metrics=metrics,original_oas=anchors,physical_accounting_recomputed=True,pricing_cash_recomputed=True,shared_draws_retained=True,empirical_validation=False)


def _date(asof,year):
    elapsed=year*365.;integer=round(elapsed)
    day=integer if integer/365.==year else math.ceil(elapsed)
    return asof+day+(1 if year>day/365. else 0)


def verify_units(raw,result,check=lambda:False):
    from .commercial_fee_adoption_reference import income_period
    check();_raw(raw,result['raw_source'])
    if raw.get('schema')!='commercial-units-1' or result.get('schema')!=raw['schema']:raise ArithmeticError('commercial unit explicit model identity differs')
    if any(p.get('exercise') and not p.get('original_physical') for u in raw['units'] for p in u['graph']['positions']):raise ArithmeticError('commercial exercise unit original stopped Physical acquisition anchor is unavailable')
    if type(result.get('admitted_work_upper')) is not int or not 0<result['admitted_work_upper']<=(4_000_000_000 if any(p.get('generated_policy') for u in raw['units'] for p in u['graph']['positions']) else 32_000_000):raise ArithmeticError('commercial unit admitted work invalid')
    n=len(raw['units']);scenarios=[s['scenario_id'] for s in raw['units'][0]['graph']['scenarios']];s=len(scenarios);h=len(raw['reporting_ordinals'])-1
    compare([s,n,10,h],result['shape'],check=check);compare(scenarios,result['scenario_ids'],check=check);compare(METRICS,result['metrics'],check=check)
    if len(result['native_graphs'])!=n:raise ArithmeticError('commercial coefficient graph coverage differs')
    values=[0.]*(s*n*10*h);fee_values=[0.]*(s*n*h)if any(p.get('fees')for u in raw['units']for p in u['graph']['positions'])else[];cash=[];unit_ids=[]
    for ui,u in enumerate(raw['units']):
        graph=verify_graph(u['graph'],result['native_graphs'][ui],check);purchase=u['graph']['as_of_ordinal'];face=u['unit_face'];loan_quote=graph['original_oas'][0]['original_signed_cash_quote']/face
        selection=u['graph']['positions'][0].get('fees');quote=selection['total_purchase_cost']/face if selection else loan_quote
        unit_ids.append(dict(unit_id=u['unit_id'],template_id=u['template_id'],purchase_ordinal=purchase,purchase_period=raw['reporting_ordinals'][:-1].index(purchase),side=1.,accounting_category=u['accounting_category'],capital_policy_revision=u['capital_policy_revision'],quote_price_per_face_unit=quote))
        for source in (v for v in graph['sources'] if v['role']=='physical'):
            si=scenarios.index(source['scenario_id']);w=source['weight'];gs=source['groups']
            compare(float(loan_quote),float(source['source_request']['source']['acquisition']['cost']/face),check=check)
            for pi,(left,right) in enumerate(zip(raw['reporting_ordinals'],raw['reporting_ordinals'][1:])):
                if left<purchase:continue
                t0,t1=(left-purchase)/365.,(right-purchase)/365.;state=next(g for g in reversed(gs) if g['year']<=t0)['next'];balance=state['principal']/face
                nii=-math.fsum(p['posting']['amount'] for g in gs if t0<g['year']<=t1 for p in g['postings'] if p['posting']['account']=='interest_income')/face
                if source.get('fee_projection'):fee_values[(si*n+ui)*h+pi]+=w*income_period(source['fee_projection'],t0,t1)/face
                weights=u['weights'];row=[nii,balance,0.,weights['hqla_l2a']*balance,weights['outflow30']*balance,weights['asf']*balance,weights['rsf']*balance,weights['rwa']*balance,balance,0.]
                for mi,x in enumerate(row):values[((si*n+ui)*10+mi)*h+pi]+=w*x
            for c in source['cash']+(source['fee_projection']['cash']if source.get('fee_projection')else[]):
                principal=w*c['principal']/face;interest=w*c['interest']/face;fees=w*c['fees']/face
                cash.append(dict(scenario_id=source['scenario_id'],unit_index=ui,path_id=source['path_id'],source_event_id=c['event_id'],year=c['year'],ordinal=_date(purchase,c['year']),source_weight=w,purchase_cash=0.,principal_cash=principal,interest_cash=interest,fee_cash=fees,bank_cash=principal+interest+fees))
        for si,id_ in enumerate(scenarios):
            cash.append(dict(scenario_id=id_,unit_index=ui,path_id='purchase',source_event_id='purchase:'+u['unit_id'],year=0.,ordinal=purchase,source_weight=1.,purchase_cash=-quote,principal_cash=0.,interest_cash=0.,fee_cash=0.,bank_cash=-quote))
            for m in graph['metrics']:
                if m['scenario_id']==id_ and m['ordinal'] in raw['reporting_ordinals'][:-1]:values[((si*n+ui)*10+2)*h+raw['reporting_ordinals'].index(m['ordinal'])]=m['dv01']/face
    compare(unit_ids,result['units'],check=check);compare(values,result['values'],check=check)
    if fee_values:compare(fee_values,result.get('fee_income_values'),check=check)
    elif 'fee_income_values'in result:raise ArithmeticError('unselected fee income coefficients')
    cash.sort(key=lambda e:e['ordinal']);dates=sorted(set(raw['reporting_ordinals'])|{e['ordinal'] for e in cash});f=len(dates);demand=[0.]*(s*n*f);balances=[0.]*(s*n);cursor=0
    for di,day in enumerate(dates):
        check()
        while cursor<len(cash) and cash[cursor]['ordinal']<=day:
            e=cash[cursor];balances[scenarios.index(e['scenario_id'])*n+e['unit_index']]+=e['bank_cash'];cursor+=1
        for ix,v in enumerate(balances):demand[ix*f+di]=-v
    funding=result['funding'];compare(dict(scenario_ids=scenarios,ordinals=dates,shape=[s,n,f],demand=demand,cash_events=cash),{k:funding[k] for k in ('scenario_ids','ordinals','shape','demand','cash_events')},check=check)
    if type(funding.get('policy')) is not str or not funding['policy']:raise ArithmeticError('commercial funding policy missing')
    return dict(verified=True,commercial_raw_sources_recomputed=True,original_yield_postings_recomputed=True,coefficients_recomputed=True,full_cash_funding_recomputed=True,empirical_validation=False)
