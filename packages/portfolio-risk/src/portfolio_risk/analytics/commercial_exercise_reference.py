"""Independent finite-distribution stopping from raw observed commercial paths.

Only validation uses this module. No supplied monetary tree, native carry or
native decision determines conditional continuation or a Physical-path action.
"""
from copy import deepcopy
import json
import math
from .commercial_credit_reference import replay, typed
from .commercial_source_reference import compare
from .commercial_reference_math import _cash,_df


def _key(value):return json.dumps(value,sort_keys=True,separators=(',',':'),allow_nan=False)


def _observed(event,year,timing):return event['year']<year or event['year']==year and (timing=='after_scheduled_cash' or event['kind'] not in ('repayment','maturity','draw','funded_draw'))


def _state(raw,projection,year,timing='after_scheduled_cash'):
    path=projection['credit']['result']['paths'][0];state=raw['credit']['paths'][0]['initial_live_state']
    for event in path['events']:
        if _observed(event,year,timing):
            if event['destination'] is not None:state=event['destination']
            if event['kind']=='maturity':state='prepaid'
    if path['default'] and path['default']['year']<=year:state='default'
    points=projection['carry_points']
    if timing=='before_scheduled_cash':
        end=next((i for i,p in enumerate(points) if p['year']==year and p['kind'] in ('contractual_cash','full_live_exit','funded_draw')),len(points));points=points[:end]
    point=next((p for p in reversed(points) if p['year']<=year),None)
    if point is None or point['year']!=year and state not in ('default','prepaid'):raise ArithmeticError('exact independent exercise point absent')
    return state,float(point['principal']),float(point['contractual_accrued_interest'])


def _surviving_rights(raw, projection, ordinal, timing):
    """Observed gross rights and known inventory; future realized cash is hidden."""
    source = raw['source']['current']
    year = (ordinal - source['asof_ordinal']) / 365.
    points = projection['carry_points']
    if timing == 'before_scheduled_cash':
        end = next((i for i, p in enumerate(points) if p['year'] == year
                    and p['kind'] in ('contractual_cash', 'funded_draw', 'full_live_exit')), len(points))
        points = points[:end]
    point = next(p for p in reversed(points) if p['year'] <= year)
    if point['contractual_accrued_interest'] != 0. or any(
            c['legal_remaining'] != 0. for c in point.get('canonical_legal_coupons', [])):
        return True
    def pending(day):
        return day > ordinal or day == ordinal and timing == 'before_scheduled_cash'
    return any(pending(p['payment_ordinal']) and (p['fee'] != 0.
                or p['coupon']['kind'] == 'indexed'
                or p['coupon']['annual_rate'] != 0.) for p in source['periods']) or any(
                pending(d['draw_ordinal']) for d in raw.get('performing_draw_tranches', []))


def _coupon_observed(period, ordinal, timing):
    return period['start_ordinal'] <= ordinal or period['payment_ordinal'] < ordinal or (
        period['payment_ordinal'] == ordinal and timing == 'after_scheduled_cash')


def information(raw,projection,ordinal,timing='after_scheduled_cash'):
    normalized=typed(raw);source=normalized['source']['current'];year=(ordinal-source['asof_ordinal'])/365.;cutoff=(ordinal-719163)*86400
    def observed_periods(current):
        periods=deepcopy(current['periods'])
        for period in periods:
            for observation in period['coupon'].get('observations',[]):
                if observation['selection_cutoff_unix_seconds']>cutoff or observation['fixing_ordinal']>ordinal:observation.pop('rate',None)
                observation.pop('source',None)
        return periods
    periods=observed_periods(source);draws=[]
    for draw in normalized.get('performing_draw_tranches',[]):
        current=draw['source']['current'];draws.append({k:deepcopy(current[k]) for k in ('contract','currency','monetary_unit','opening_principal','principal_plan')}|dict(draw_ordinal=draw['draw_ordinal'],eligible_origins=deepcopy(draw['eligible_origins']),periods=observed_periods(current),cost=draw['source']['acquisition']['cost'],original_annual_effective_rate=draw['source']['acquisition']['original_annual_effective_rate']))
    points=projection['carry_points']
    if timing=='before_scheduled_cash':points=points[:next((i for i,p in enumerate(points) if p['year']==year and p['kind'] in ('contractual_cash','full_live_exit','funded_draw')),len(points))]
    gross=next(p for p in reversed(points) if p['year']<=year).get('canonical_legal_coupons',[])
    terms=source['periods']+[p for d in normalized.get('performing_draw_tranches',[]) for p in d['source']['current']['periods']]
    gross=[c for c in gross if any(p['period_id']==c['coupon_id'] and _coupon_observed(p,ordinal,timing) for p in terms)]
    state,principal,legal=_state(raw,projection,year,timing)
    events=[{k:e[k] for k in ('year','kind','origin','destination','cash','principal','interest')} for e in projection['credit']['result']['paths'][0]['events'] if _observed(e,year,timing)]
    return dict(contract=source['contract'],currency=source['currency'],monetary_unit=source['monetary_unit'],opening_principal=source['opening_principal'],principal_plan=source['principal_plan'],periods=periods,performing_draw_tranches=draws,gross_legal_coupons=deepcopy(gross),state=state,principal=principal,legal_interest=legal,events=events)


def compile_policy(policy,histories,curve,oas,bump=0.,check=lambda:False):
    check();policy=deepcopy(policy)
    if policy.get('schema')!='commercial-native-exercise-1' or policy.get('information_policy')!='native_full_observed_prefix-1' or not 0<len(histories)<=64 or len(policy['exercise_dates'])>128:raise ArithmeticError('bounded independent exercise policy required')
    first=histories[0]['source']['source']['current'];asof=first['asof_ordinal'];prior=asof
    rights=policy.get('rights_only_termination')
    if rights is None:policy.pop('rights_only_termination',None)
    elif type(rights)is not dict or set(rights)!= {'review','disposition'} or rights['disposition']!='settle_earned_cancel_future-1' or type(rights['review'])is not dict or set(rights['review'])!= {'id','revision'} or any(type(v)is not str or not v.strip() for v in rights['review'].values()):raise ArithmeticError('explicit reviewed rights-only disposition required')
    for date in policy['exercise_dates']:
        check()
        if type(date['ordinal']) is not int or not prior<date['ordinal']<=first['tail_end_ordinal'] or date['timing'] not in ('after_scheduled_cash','before_scheduled_cash'):raise ArithmeticError('exact increasing exercise date/timing required')
        for name in ('call','put'):
            factor=date[name+'_clean_price_factor'];fee=date[name+'_fee']
            if factor is not None and (type(factor) not in (int,float) or not math.isfinite(factor) or not 0<=factor<=10):raise ArithmeticError('exercise clean factor required')
            if type(fee) not in (int,float) or not math.isfinite(fee) or fee<0 or factor is None and fee!=0:raise ArithmeticError('exercise fee/right differs')
            if factor is not None:date[name+'_clean_price_factor']=float(factor)
            date[name+'_fee']=float(fee)
        if date['call_clean_price_factor'] is None and date['put_clean_price_factor'] is None:raise ArithmeticError('exercise right absent')
        prior=date['ordinal']
    weights=[h['weight'] for h in histories]
    if any(type(w) not in (int,float) or not math.isfinite(w) or not 0<w<=1 for w in weights) or abs(sum(weights)-1.)>1e-9+1e-12:raise ArithmeticError('exercise complete probability required')
    if policy['simultaneous_priority'] not in ('issuer','holder'):raise ArithmeticError('exercise simultaneous priority required')
    raws=[h['source'] for h in histories];projections=[replay(r,check) for r in raws];cash=[_cash(p['credit']) for p in projections]
    if len({r['credit']['paths'][0]['path_key'] for r in raws})!=len(raws) or any(len(r['credit']['paths'])!=1 for r in raws):raise ArithmeticError('one unique exercise path required')
    root=_key(information(raws[0],projections[0],asof))
    if any(_key(information(r,p,asof))!=root for r,p in zip(raws,projections)):raise ArithmeticError('Pricing initial observable information differs')
    def df(t):return _df(curve,t)*math.exp(-(oas+bump)*t)
    nodes=[];lookup={};next_values=[0.]*len(raws);next_time=None;next_timing=None
    for date in reversed(policy['exercise_dates']):
        check();year=(date['ordinal']-asof)/365.;discount=df(year);groups={}
        for i,(r,p) in enumerate(zip(raws,projections)):
            info=information(r,p,date['ordinal'],date['timing']);groups.setdefault(_key(info),(info,[]))[1].append(i)
        values=[0.]*len(raws)
        for key,(info,members) in sorted(groups.items()):
            check();probability=sum(weights[i] for i in members);continuation=0.
            for i in members:
                value=next_values[i]*df(next_time)/discount if next_time is not None else 0.
                for c in cash[i]:
                    check()
                    after=c['year']>year or c['year']==year and date['timing']=='before_scheduled_cash'
                    before=next_time is None or c['year']<next_time or c['year']==next_time and next_timing=='after_scheduled_cash'
                    if after and before:value+=(c['principal']+c['interest']+c['fees'])*df(c['year'])/discount
                continuation+=weights[i]/probability*value
            state,principal,legal=_state(raws[members[0]],projections[members[0]],year,date['timing']);decision='continue';value=continuation
            rights_only=principal==0. and _surviving_rights(raws[members[0]],projections[members[0]],date['ordinal'],date['timing'])
            if state in ('performing','watch','delinquent') and rights_only and rights is None:raise ArithmeticError('rights-only termination requires explicit owner')
            if state in ('performing','watch','delinquent') and (principal>0 or rights_only):
                for action in (('put','call') if policy['simultaneous_priority']=='issuer' else ('call','put')):
                    factor=date[action+'_clean_price_factor']
                    if factor is None:continue
                    exercise=principal*factor+legal+date[action+'_fee']
                    if action=='call' and exercise<value or action=='put' and exercise>value:value=exercise;decision=action
            for i in members:values[i]=value
            node=dict(ordinal=date['ordinal'],timing=date['timing'],information=info,probability=float(probability),continuation_value=float(continuation),value=float(value),decision=decision,live_principal=principal,legal_interest=legal)
            lookup[date['ordinal'],key]=node;nodes.append(node)
        next_time=year;next_timing=date['timing'];next_values=values
    value=0.
    for i,w in enumerate(weights):
        check();pv=next_values[i]*df(next_time) if next_time is not None else 0.
        for c in cash[i]:
            if c['year']>0 and (next_time is None or c['year']<next_time or c['year']==next_time and next_timing=='after_scheduled_cash'):pv+=(c['principal']+c['interest']+c['fees'])*df(c['year'])
        value+=w*pv
    result=dict(policy=policy,value=float(value),nodes=nodes,fixed_base_oas=float(oas),parallel_bump=float(bump),finite_distribution=True,continuous_model_convergence=False)
    return dict(result=result,lookup=lookup,asof=asof,position_id=first['position_id'])


def stops_for(compiled,raw,check=lambda:False):
    check();projection=replay(raw,check);source=raw['source']['current'];policy=compiled['result']['policy']
    if source['asof_ordinal']!=compiled['asof'] or source['position_id']!=compiled['position_id'] or len(raw['credit']['paths'])!=1:raise ArithmeticError('exercise Physical identity differs')
    for date in policy['exercise_dates']:
        state,principal,_=_state(raw,projection,(date['ordinal']-compiled['asof'])/365.,date['timing'])
        rights_only=principal==0. and _surviving_rights(raw,projection,date['ordinal'],date['timing'])
        if state in ('performing','watch','delinquent') and rights_only and policy.get('rights_only_termination') is None:raise ArithmeticError('rights-only termination requires explicit owner')
        if state not in ('performing','watch','delinquent') or principal==0. and not rights_only:continue
        node=compiled['lookup'].get((date['ordinal'],_key(information(raw,projection,date['ordinal'],date['timing']))))
        if node is None:raise ArithmeticError('Physical observed state absent from Pricing policy')
        action=node['decision']
        if action!='continue':
            stop=dict(path_key=raw['credit']['paths'][0]['path_key'],event_id=f"native-exercise:{policy['identity']['id']}:{date['ordinal']}",ordinal=date['ordinal'],action=action,principal_clean_price_factor=date[action+'_clean_price_factor'],fee=date[action+'_fee'],review=deepcopy(policy['identity']))
            stop['timing']=date['timing']
            if principal==0:stop['rights_only_review']=deepcopy(policy['rights_only_termination']['review'])
            return [stop]
    return []


def verify_policy(policy,histories,curve,oas,bump,actual,check=lambda:False):
    compiled=compile_policy(policy,histories,curve,oas,bump,check);compare(compiled['result'],actual,check=check);return compiled
