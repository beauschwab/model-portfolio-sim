"""Independent acquired nonintegral fee graph/cost/income replay.

This module is used only by validation. It derives exits from independently
recomputed loan paths and never consumes a native fee money checkpoint.
"""
from copy import deepcopy
import math
from .commercial_fee_reference import bind
from .commercial_dto import graph_typed,graph_identity
from .commercial_source_reference import compare
from .commercial_reference_math import _df
from .nonintegral_fee_reference import replay as replay_fee


def _history(selection, scenario, role, path_id, original=False):
    rows=selection.get('original_physical',[])if original and role=='physical'else selection['original_pricing']if original else next(h for h in selection['histories'] if h['scenario_id']==scenario)[role]
    matches=[h for h in rows if h['path_id']==path_id]
    if len(matches)!=1:raise ArithmeticError('complete exact fee path coverage required')
    return matches[0]


def validate(selection, graph, position, check):
    check()
    if selection.get('schema')!='commercial-fee-adoption-1' or position.get('generated_policy'):
        raise ArithmeticError('fee conditional exercise needs joint continuation ownership')
    spread=selection['discount_spread']
    if type(spread)not in(int,float) or not math.isfinite(spread) or not -5.<=spread<=5.:raise ArithmeticError('explicit fee spread required')
    allocations=selection['allocations'];ids=[a['fee_id']for a in allocations]
    if not 1<=len(ids)<=64 or len(ids)!=len(set(ids)):raise ArithmeticError('exact separate acquired fee allocation required')
    cost=math.fsum(a['allocated_cost']for a in allocations)
    compare(float(cost),float(selection['original_signed_fee_quote']),check=check)
    compare(float(position['original_signed_cash_quote']+cost),float(selection['total_purchase_cost']),check=check)
    if selection['total_purchase_cost']<=0.:raise ArithmeticError('positive combined purchase cost required')
    if {h['scenario_id']for h in selection['histories']}!={h['scenario_id']for h in position['histories']} or len(selection['histories'])!=len(position['histories']):raise ArithmeticError('fee scenario coverage differs')
    sets=[(graph['original_scenario']['scenario_id'],'pricing',position['original_pricing'],True)]
    if position.get('original_physical')or selection.get('original_physical'):sets.append((graph['original_scenario']['scenario_id'],'physical',position.get('original_physical',[]),True))
    sets.extend((h['scenario_id'],role,h[role],False)for h in position['histories']for role in ('physical','pricing','pricing_down','pricing_up'))
    contract=None
    for scenario,role,hs,original in sets:
        rows=selection.get('original_physical',[])if original and role=='physical'else selection['original_pricing']if original else next(h for h in selection['histories']if h['scenario_id']==scenario)[role]
        if len(rows)!=len(hs)or{h['path_id']for h in rows}!={h['path_id']for h in hs}:raise ArithmeticError('fee history path coverage differs')
        for loan in hs:
            h=_history(selection,scenario,role,loan['path_id'],original);policy=h['policy']
            if position.get('exercise')and policy['coincident_order']!='native_exit_follows_native_exercise_timing-1':raise ArithmeticError('joint fee exercise requires explicit native clock order')
            if {f['fee_id']for f in policy['fees']}!=set(ids)or len(policy['fees'])!=len(ids):raise ArithmeticError('allocated fee source ownership differs')
            current_contract=graph_typed(policy['fees'])
            if contract is None:contract=current_contract
            elif contract!=current_contract:raise ArithmeticError('fee contractual rights changed across measures/paths')
            for a in allocations:
                fee=next(f for f in policy['fees']if f['fee_id']==a['fee_id'])
                if a['allocated_cost']!=0. and math.copysign(1.,a['allocated_cost'])!=math.copysign(1.,fee['entitlement']):raise ArithmeticError('fee asset/liability allocation sign differs')
            releases=h.get('acquisition_releases',[])
            if role!='physical':
                if releases:raise ArithmeticError('Pricing cannot supply physical fee cost release')
                continue
            opening=replay_fee(dict(schema='nonintegral-fee-accounting-1',measure='physical',currency=graph['currency'],monetary_unit=loan['source']['source']['current']['monetary_unit'],policy=policy['policy'],asof_ordinal=loan['source']['source']['current']['asof_ordinal'],reporting_end_ordinal=loan['source']['source']['current']['reporting_end_ordinal'],complete_end_ordinal=policy['complete_end_ordinal'],fees=policy['fees'],events=[]),check,transfer=policy.get('transfer'))
            stock={r['fee_id']:r['state'].get('transferred_net_stock',0.)for r in opening['closing']}
            remaining={a['fee_id']:float(a['allocated_cost'])-stock[a['fee_id']]for a in allocations};previous=None
            for release in releases:
                clock=(release['ordinal'],release['sequence']);amount=release['amount'];fid=release['fee_id']
                if previous is not None and previous>=clock or not loan['source']['source']['current']['asof_ordinal']<=release['ordinal']<=policy['complete_end_ordinal']:raise ArithmeticError('fee basis chronology differs')
                if fid not in remaining or amount==0. or amount*remaining[fid]<=0. or abs(amount)>abs(remaining[fid]):raise ArithmeticError('fee basis release exceeds signed allocation')
                remaining[fid]-=amount;previous=clock
            for value in remaining.values():compare(0.,value,check=check)


def require_complete(bound,check=lambda:False):
    for closing in bound['source']['closing']:
        check();state=closing['state']
        for field in('unearned_entitlement','fee_receivable','fee_payable','deferred_fee_income','prepaid_fee_expense'):
            value=state[field]
            if abs(value)>1e-9+1e-12*abs(value):
                raise ArithmeticError('complete acquired fee source requires explicit disposition of remaining service/gross rights')


def project(selection, scenario, role, history, loan_reference, actual, check, original=False,stops=()):
    h=_history(selection,scenario,role,history['path_id'],original)
    paths=bind(h['policy'],history['source'],dict(replay=loan_reference),check,stops)
    if len(paths)!=1:raise ArithmeticError('one exact owned fee path required')
    bound=paths[0]
    require_complete(bound,check)
    if role=='physical':
        source=bound['source']['raw_request'];origin=source['asof_ordinal'];events=[(year,1,deepcopy(event))for year,event in zip(bound['native_years'][len(h['policy']['fees'])if h['policy'].get('transfer')else 0:],source['events'],strict=True)]
        fees={f['fee_id']:f for f in source['fees']}
        stock={r['fee_id']:r['state'].get('transferred_net_stock',0.)for r in bound['source']['closing']}
        for i,a in enumerate(selection['allocations']):
            basis=a['allocated_cost']-stock[a['fee_id']]
            if basis!=0.:
                events.append((0.,0,dict(event_id=f"native-fee:{bound['path_key']}:{i}:acquire",fee_id=a['fee_id'],source=deepcopy(fees[a['fee_id']]['source']),review=deepcopy(a['review']),ordinal=origin,sequence=0,action='acquire',reason='other_reviewed',amount=float(basis))))
        for i,r in enumerate(h.get('acquisition_releases',[])):
            events.append(((r['ordinal']-origin)/365.,2,dict(event_id=f"native-fee:{bound['path_key']}:{i}:release-acquired-cost",fee_id=r['fee_id'],source=deepcopy(fees[r['fee_id']]['source']),review=deepcopy(r['review']),ordinal=r['ordinal'],sequence=0,action='release_acquired_cost',reason='other_reviewed',amount=float(r['amount']))))
        events.sort(key=lambda x:(x[0],x[1]));source['events']=[];bound['native_years']=[]
        for i,(year,_,event)in enumerate(events):event['sequence']=i;source['events'].append(event);bound['native_years'].append(year)
        bound['source']=replay_fee(source,check,transfer=h['policy'].get('transfer'))
        if h['policy'].get('transfer'):bound['native_years']=[0.]*len(h['policy']['fees'])+bound['native_years']
    # The independent contractual rights arithmetic is measure-neutral. Physical
    # verification remains independently selected; Pricing cannot publish a GL.
    bound['source']['raw_request']['measure']='physical'if role=='physical'else'pricing'
    cash=[]
    for year,group in zip(bound['native_years'],bound['source']['groups'],strict=True):
        amount=0. if group['event']['action']in('acquire','acquire_transferred_stock')else math.fsum(p['amount']for p in group['postings']if p['account']=='cash')
        if amount!=0.:cash.append(dict(event_id='separate-fee:'+group['event']['event_id'],kind='separate_nonintegral_fee',year=year,principal=0.,interest=0.,fees=amount))
    expected=dict(raw_fee_history=graph_typed(h),bound_path=bound,cash=cash,discount_spread=float(selection['discount_spread']),allocations=graph_typed(selection['allocations']),loan_original_cost_yield_unchanged=True)
    if actual is not None:
        graph_identity(h,actual.get('raw_fee_history'))
        graph_identity(selection['allocations'],actual.get('allocations'))
        graph_identity(bound['source']['raw_request'],actual.get('bound_path',{}).get('source',{}).get('raw_request'))
        compare(expected,actual,check=check)
    return expected


def price(projection,curve,asof,mark,bump=0.,original=False):
    t=(mark-asof)/365.;spread=projection['discount_spread'];df0=_df(curve,t)
    return math.fsum(c['fees']*_df(curve,c['year'])/df0*math.exp(-(spread+bump)*(c['year']-t))for c in projection['cash']if c['year']>t or original and c['year']==t)


def income_period(projection,left,right):
    bound=projection['bound_path'];origin=bound['source']['raw_request']['asof_ordinal']
    inside=lambda year:(left<year or left==0. and year==0.)and year<=right
    return -math.fsum(p['amount']for year,g in zip(bound['native_years'],bound['source']['groups'],strict=True)if inside(year)for p in g['postings']if p['account']in('fee_income','fee_writeoff_expense','fee_recovery_income','fee_acquisition_expense'))
