"""Independent test/reference CD sequence recurrence and graph pricing.

No production engine, native callback, NumPy backend, or producer cash is used.
The public proposal fails closed for indexed-term rows until their full independent
source verifier is composed; the native graph retains its existing indexed owner.
"""
from copy import deepcopy
from datetime import date
from math import copysign, exp, isfinite, log

ROLES=('physical','pricing','pricing_down','pricing_up')
FLOATS=frozenset(('principal','accrued_interest','annual_rate','grace_annual_rate',
 'minimum_remaining_principal','annual_penalty_rate','fraction','amount','penalty_cap',
 'cash_out_principal','renew_principal','switch_principal','cash_out_interest',
 'renew_interest','switch_interest','base_oas','signed_cash_price','bump_bp','severity','weight'))
ENUMS=frozenset(('direction','measure','day_count','right_tail','cash_timing','channel','penalty_limit'))
def normalize(v,key=''):
    """Only declared binary64 financial cells normalize browser integer spelling."""
    if isinstance(v,dict):
        out={}
        for k,x in v.items():
            if k in ENUMS and type(x) is not str:raise ValueError(f'{k} must be an enum string')
            out[k]=normalize(x,k)
        return out
    if isinstance(v,list):return [normalize(x,key) for x in v]
    if key in FLOATS or key in ('times_years','discount_factors'):
        if v is None and key=='penalty_cap':return None
        if type(v) not in (int,float) or not isfinite(float(v)):raise ValueError(f'{key} must be a finite monetary/rate float')
        return float(v)
    return v
def exact(a,b,path='source'):
    if type(a) is not type(b):raise ValueError(f'{path} immutable type changed')
    if isinstance(b,dict):
        if a.keys()!=b.keys():raise ValueError(f'{path} immutable keys changed')
        for k in b:exact(a[k],b[k],f'{path}.{k}')
    elif isinstance(b,list):
        if len(a)!=len(b):raise ValueError(f'{path} length changed')
        for i,(x,y) in enumerate(zip(a,b)):exact(x,y,f'{path}[{i}]')
    elif isinstance(b,float):
        if a!=b or (b==0. and copysign(1.,a)!=copysign(1.,b)):raise ValueError(f'{path} immutable financial/source changed')
    elif a!=b:raise ValueError(f'{path} immutable value changed')
def close(a,b,path='money'):
    if type(a) not in (int,float) or not isfinite(a) or abs(a-b)>max(1e-9,1e-12*max(abs(a),abs(b))):raise ValueError(f'{path}: {a} != {b}')
def fraction(start,end,basis):
    if basis=='act360':return (end-start)/360.
    if basis=='act365_fixed':return (end-start)/365.
    if basis!='act_act_isda':raise ValueError('unsupported day-count')
    value=0.;cursor=start
    while cursor<end:
        y=date.fromordinal(cursor).year;year_days=366 if date(y,12,31).timetuple().tm_yday==366 else 365;stop=min(end,date(y,1,1).toordinal()+year_days)
        value+=(stop-cursor)/(366. if date(y,12,31).timetuple().tm_yday==366 else 365.)
        cursor=stop
    return value
def sequence(raw):
    r=normalize(raw);live={r['opening']['position_id']:deepcopy(r['opening'])}
    bindings={b['position_id']:b for b in r['policies']};events=[];receipts=[]
    side=1. if r['direction']=='receipt' else -1.
    def cash(day,owner,event_id,p,a,fee,source,accrual=None):
        return dict(position_id=owner,event_id=event_id,scenario=dict(kind='named',scenario_id=r['scenario_id']),
         payment_ordinal=day,day_offset=day-r['as_of_ordinal'],accrual_period=accrual,currency=r['currency'],
         components=dict(principal=side*p,cash_interest=side*a,fees=side*fee,accrued_interest=side*0.,book_amortization=side*0.),
         measure=r['measure'],provenance=dict(kind='modeled',model_id=source['id'],model_version='term-contracts-1',input_revision=source['revision']),adjustment=dict(kind='none'))
    for i,action in enumerate(r['actions']):
        opening=live.pop(action['position_id']);s=deepcopy(opening);policy=bindings[s['position_id']]['policy'];day=action['ordinal'];election=action['election']
        delta=0.
        if s['last_accrual_ordinal']<min(day,s['maturity_ordinal']):delta=s['principal']*s['annual_rate']*fraction(s['last_accrual_ordinal'],min(day,s['maturity_ordinal']),s['day_count'])
        if day>s['maturity_ordinal']:delta+=s['principal']*policy['grace_annual_rate']*fraction(max(s['last_accrual_ordinal'],s['maturity_ordinal']),day,s['day_count'])
        earned=s['accrued_interest']+delta;s['accrued_interest']=earned;s['last_accrual_ordinal']=day
        source=action['source'];settlements=[];new=[];penalty=0.;pi=0.;pp=0.
        if election['kind']=='withdraw':
            removed=election['principal'];allocated=earned*removed/s['principal']
            waived=any(w['start_ordinal']<=day<=w['end_ordinal'] for w in policy['penalty_waivers'])
            if not waived:
                rule=policy['penalty'];kind=rule['kind']
                penalty=(rule['amount'] if kind=='fixed_amount' else removed*rule['fraction'] if kind=='principal_fraction' else removed*(rule['annual_penalty_rate']*rule['days']/rule['denominator']) if kind=='interest_days' else 0.)
                if policy['penalty_cap'] is not None:penalty=min(penalty,policy['penalty_cap'])
                limit=max(allocated,0.) if policy['penalty_limit']=='accrued_interest_only' else max(removed+allocated,0.)
                penalty=min(penalty,limit)
            pi=min(penalty,max(allocated,0.));pp=penalty-pi
            principal_cash=removed-pp;interest_cash=allocated-pi
            s['principal']-=removed;s['accrued_interest']-=allocated
            settlements=[cash(day,s['position_id'],f'action-{i}:cd-withdrawal',removed,allocated,-penalty,source)]
        elif election['kind']=='maturity':
            removed=s['principal'];principal_cash=election['cash_out_principal'];interest_cash=election['cash_out_interest']
            settlements=[cash(day,s['position_id'],f'action-{i}:cd-maturity-settlement',s['principal'],earned,0.,source)]
            for prefix,name in [('renew','renewal'),('switch','switching')]:
                p=election[prefix+'_principal'];a=election[prefix+'_interest'];target=election[name]
                if p or a:
                    current=dict(contract=deepcopy(target['contract']),position_id=target['position_id'],principal=p+a,accrued_interest=0.,last_accrual_ordinal=day,maturity_ordinal=target['maturity_ordinal'],annual_rate=target['annual_rate'],day_count=target['day_count'])
                    new.append(current);settlements.append(cash(day,current['position_id'],f'action-{i}:cd-{name if name=="renewal" else "switch"}',-(p+a),0.,0.,source))
            s['principal']=0.;s['accrued_interest']=0.
        else:raise ValueError('unsupported election')
        if s['principal'] or s['accrued_interest']:live[s['position_id']]=s
        for t in new:live[t['position_id']]=t
        events.extend(settlements)
        receipts.append(dict(action=action,opening=opening,closing=s,new_contracts=new,policy=deepcopy(bindings[s['position_id']]['policy']['identity']),earned_interest_delta=delta,
         gross_principal_removed=removed,principal_cash_returned=principal_cash,interest_cash_paid=interest_cash,penalty=penalty,
         penalty_from_accrued_interest=pi,penalty_from_principal=pp,net_external_cash=sum(sum(x['components'][k] for k in ('principal','cash_interest','fees')) for x in settlements),events=settlements))
    states=[live[k] for k in sorted(live)]
    for i,s in enumerate(states):
        total=s['accrued_interest']+s['principal']*s['annual_rate']*fraction(s['last_accrual_ordinal'],s['maturity_ordinal'],s['day_count'])
        period=dict(start_ordinal=s['last_accrual_ordinal'],end_ordinal=s['maturity_ordinal']) if s['last_accrual_ordinal']<s['maturity_ordinal'] else None
        events.append(cash(s['maturity_ordinal'],s['position_id'],f'terminal-{i}:cd-no-election-maturity',s['principal'],total,0.,r['terminal_projection_source'],period))
    events.sort(key=lambda e:e['payment_ordinal'])
    return dict(events=events,states=states,receipts=receipts)
def discount(curve,t):
    times=curve['times_years'];dfs=curve['discount_factors']
    for i,end in enumerate(times):
        if t==end:return dfs[i]
        if t<end:
            w=(t-times[i-1])/(end-times[i-1]);return exp(log(dfs[i-1])+(log(dfs[i])-log(dfs[i-1]))*w)
    if curve['right_tail']=='reject':raise ValueError('curve tail rejects')
    end=times[-1];a=log(dfs[-1])
    return exp(a*t/end) if curve['right_tail']=='flat_zero' else exp(a+(a-log(dfs[-2]))*(t-end)/(end-times[-2]))
def price(events,curve,asof,oas,bump=0.):
    total=0.;correction=0.
    for e in events:
        if e['payment_ordinal']<=asof:continue
        t=(e['payment_ordinal']-asof)/365.;c=e['components'];cash=c['principal']+c['cash_interest']+c['fees']
        value=cash*discount(curve,t)*exp(-(oas+bump)*t)
        adjusted=value-correction;n=total+adjusted;correction=(n-total)-adjusted;total=n
    return total
def sources(raw,original=False):
    r=normalize(raw);out=[]
    scenarios=[r['original_base']['scenario']] if original else r['scenarios']
    owners=r['original_base']['histories'] if original else r['positions']
    for owner in owners:
        sets=[owner['histories']] if original else owner['histories']
        for s,h in zip(scenarios,sets):
            for role in ROLES:
                for path in h[role]:
                    if path['product']['kind']!='cd_sequence':raise ValueError('independent graph2 CD-only publication scope')
                    q=path['product']['request'];projection=sequence(q)
                    out.append(dict(position_id=owner['position_id'],scenario_id=s['scenario_id'],run_id=s['run_id'],path_id=path['path_id'],weight=path['weight'],role=role,product=path['product'],projection=projection))
    return out
def _financial_tree(actual,expected,path):
    if isinstance(expected,dict):
        if actual.keys()!=expected.keys():raise ValueError(f'{path} event/state keys')
        for k,w in expected.items():_financial_tree(actual[k],w,f'{path}.{k}')
    elif isinstance(expected,list):
        if len(actual)!=len(expected):raise ValueError(f'{path} event/state count')
        for i,(a,b) in enumerate(zip(actual,expected)):_financial_tree(a,b,f'{path}[{i}]')
    elif isinstance(expected,float):close(actual,expected,path)
    else:exact(actual,expected,path)
def verify_source(actual,expected):
    for k in ('position_id','scenario_id','run_id','path_id','role','weight','product'):exact(actual[k],expected[k],k)
    projection=actual['projection'];exact(projection['kind'],'cd_sequence')
    result=projection['result'];exact(result['schema'],'cd-election-sequence-2');exact(result['admitted_request'],expected['product']['request'])
    request=expected['product']['request']
    for k,w in dict(schema='dated-cashflow-1',as_of_ordinal=request['as_of_ordinal'],horizon_days=request['tail_end_ordinal']-request['as_of_ordinal'],scenarios=[request['scenario_id']]).items():exact(result['schedule'][k],w)
    _financial_tree(result['schedule']['events'],expected['projection']['events'],'events')
    _financial_tree(result['live_before_terminal_settlement'],expected['projection']['states'],'states')
    if len(result['action_receipts'])!=len(expected['projection']['receipts']):raise ValueError('native receipt count')
    for actual_receipt,w in zip(result['action_receipts'],expected['projection']['receipts']):
        exact(actual_receipt['action'],w['action']);_financial_tree(actual_receipt['opening'],w['opening'],'opening')
        native=actual_receipt['native']
        for k,want in dict(schema='term-enhancements-1',model_version='term-contracts-1',policy=w['policy'],election_source=w['action']['source']).items():exact(native[k],want,k)
        for k in ('closing','new_contracts'):_financial_tree(native[k],w[k],k)
        for k in ('earned_interest_delta','gross_principal_removed','principal_cash_returned','interest_cash_paid','penalty','penalty_from_accrued_interest','penalty_from_principal','net_external_cash'):close(native[k],w[k],k)
        _financial_tree(native['settlement_schedule']['events'],[{**e,'event_id':e['event_id'].split(':',1)[1],'day_offset':0} for e in w['events']],'actual settlement')
        for name in ('settlement_schedule','remaining_schedule'):
            for k,want in dict(schema='dated-cashflow-1',as_of_ordinal=w['action']['ordinal'],horizon_days=request['tail_end_ordinal']-w['action']['ordinal'],scenarios=[request['scenario_id']]).items():exact(native[name][k],want,name+'.'+k)
    for flag in ('accounting_adoption','empirical_validation','joint_optimal_exercise'):exact(result[flag],False)
    exact(result['source_status'],'explicit_supplied_chronological_contract_elections')
def verify(raw,result):
    r=normalize(raw);expected=sources(r);original=sources(r,True)
    if result['schema']!='term-product-graph-2' or len(expected)!=len(result['native_sources']) or len(original)!=len(result['original_native_sources']):raise ValueError('schema/source coverage')
    for a,b in zip(result['native_sources'],expected):verify_source(a,b)
    for a,b in zip(result['original_native_sources'],original):verify_source(a,b)
    anchor=[r['schema'],r['identity']['model_revision'],r['identity']['paired_history_source'],r['identity']['paired_history_revision'],r['identity']['randomness_domain'],r['as_of_ordinal'],r['currency'],r['cash_timing'],[[p['position_id'],p['known'],p['oas_target']]for p in r['positions']],r['original_base'],r['original_quotes']]
    exact(result['identity'],r['identity']);exact(result['base_anchor']['schema'],'term-base-anchor-2');exact(result['base_anchor']['calibration_input'],anchor,'complete original anchor')
    exact(result['base_anchor']['oas'],[dict(position_id=p['position_id'],base_oas=p['oas_target']['base_oas'],calibration_kind='supplied_locked_oas')for p in r['positions']])
    if len(result['metrics'])!=len(r['positions'])*len(r['scenarios']):raise ValueError('metric coverage')
    for position,quote in zip(r['positions'],r['original_quotes']):
        oas=position['oas_target']['base_oas'];s=r['original_base']['scenario']
        q=[p for p in original if p['position_id']==position['position_id'] and p['role']=='pricing']
        pv=sum(p['weight']*price(p['projection']['events'],s['collateral_curve'],r['as_of_ordinal'],oas) for p in q)
        close(pv,quote['signed_cash_price'],'original-only quote')
    for (position,s),actual in zip(((p,s)for p in r['positions'] for s in r['scenarios']),result['metrics']):
        exact(actual['position_id'],position['position_id']);exact(actual['scenario_id'],s['scenario_id']);oas=position['oas_target']['base_oas']
        def role_price(role,bump):return sum(p['weight']*price(p['projection']['events'],s['collateral_curve'],r['as_of_ordinal'],oas,bump) for p in expected if p['position_id']==position['position_id'] and p['scenario_id']==s['scenario_id'] and p['role']==role)
        close(actual['pv'],role_price('pricing',0.),'PV')
        close(actual['supplied_history_dv01'],(role_price('pricing_down',-r['bump_bp']/10000.)-role_price('pricing_up',r['bump_bp']/10000.))/(2.*r['bump_bp']),'DV01')
        for key,bounds in [('opening_actions',[(r['as_of_ordinal'],r['as_of_ordinal'],True)]),('physical_periods',[(a,b,False)for a,b in zip(r['reporting_ordinals'],r['reporting_ordinals'][1:])]),('physical_tail',[(r['reporting_ordinals'][-1],position['known']['contract']['tail_end_ordinal'],False)])]:
            rows=actual[key] if key=='physical_periods' else [actual[key]]
            if len(rows)!=len(bounds):raise ValueError('complete physical report coverage')
            for row,(start,end,opening) in zip(rows,bounds):
                sums={k:0. for k in row}
                for p in expected:
                    if p['position_id']!=position['position_id'] or p['scenario_id']!=s['scenario_id'] or p['role']!='physical':continue
                    weight=p['weight'];inside=lambda d:(d==start) if opening else (start<d<=end)
                    for e in p['projection']['events']:
                        if inside(e['payment_ordinal']):
                            for k,m in [('principal_cash','principal'),('interest_cash','cash_interest'),('fee_cash','fees')]:sums[k]+=weight*e['components'][m]
                    sign=1. if p['product']['request']['direction']=='receipt' else -1.
                    for a in p['projection']['receipts']:
                        if inside(a['action']['ordinal']):
                            fields=dict(cd_principal_extinguished=a['gross_principal_removed'],cd_penalty=a['penalty'],cd_penalty_from_principal=a['penalty_from_principal'],cd_penalty_from_accrued_interest=a['penalty_from_accrued_interest'],cd_interest_accrued_through_election=a['earned_interest_delta'],cd_new_contract_principal=sum(t['principal']for t in a['new_contracts']),cd_net_external_cash=a['net_external_cash'],cd_interest_cash_paid=sign*a['interest_cash_paid'],cd_principal_cash_returned=sign*a['principal_cash_returned'])
                            for k,v in fields.items():sums[k]+=weight*v
                for k,v in sums.items():close(row[k],v,k)
    status=result['sequence_policy']
    for k in ('accounting_adoption','empirical_validation','joint_optimal_exercise','saved_library_cohort_adoption','capital_ftp_eligible'):exact(status[k],False)
    exact(status['oas_calibration_calls'],0);exact(status['single_election_fallbacks'],0)
    exact(status['native_sequence_sources'],len(expected));exact(status['original_native_sequence_sources'],len(original))
    exact(status['locked_original_oas_sources'],len(r['positions']));exact(status['original_quote_reconciled'],True)
    return dict(schema='independent-cd-sequence-graph-reference-1',current_sources=len(expected),original_sources=len(original),raw_source_cash_risk_verified=True,production_adoption=False)
