"""Independent test/reference CD sequence recurrence and graph pricing.

No production engine, native callback, NumPy backend, or producer cash is used.
The public proposal fails closed for indexed-term rows until their full independent
source verifier is composed; the native graph retains its existing indexed owner.
"""
import contextvars
from copy import deepcopy
from datetime import date
from math import copysign, exp, isfinite, log

_CHECK=contextvars.ContextVar('cd_sequence_check',default=lambda:False)
def _probe():
    if _CHECK.get()():raise InterruptedError('independent CD source verification cancelled')

ROLES=('physical','pricing','pricing_down','pricing_up')
FLOATS=frozenset(('principal','accrued_interest','annual_rate','grace_annual_rate',
 'minimum_remaining_principal','annual_penalty_rate','fraction','amount','penalty_cap',
 'cash_out_principal','renew_principal','switch_principal','cash_out_interest',
 'renew_interest','switch_interest','base_oas','signed_cash_price','bump_bp','severity','weight'))
ENUMS=frozenset(('direction','measure','day_count','right_tail','cash_timing','channel','penalty_limit'))
def normalize(v,key=''):
    _probe()
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
    _probe()
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
    _probe()
    if type(a) not in (int,float) or not isfinite(a) or abs(a-b)>max(1e-9,1e-12*max(abs(a),abs(b))):raise ValueError(f'{path}: {a} != {b}')
def fraction(start,end,basis):
    _probe()
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
    _probe()
    r=normalize(raw);live={r['opening']['position_id']:deepcopy(r['opening'])}
    bindings={b['position_id']:b for b in r['policies']};events=[];receipts=[]
    side=1. if r['direction']=='receipt' else -1.
    def cash(day,owner,event_id,p,a,fee,source,accrual=None):
        return dict(position_id=owner,event_id=event_id,scenario=dict(kind='named',scenario_id=r['scenario_id']),
         payment_ordinal=day,day_offset=day-r['as_of_ordinal'],accrual_period=accrual,currency=r['currency'],
         components=dict(principal=side*p,cash_interest=side*a,fees=side*fee,accrued_interest=side*0.,book_amortization=side*0.),
         measure=r['measure'],provenance=dict(kind='modeled',model_id=source['id'],model_version='term-contracts-1',input_revision=source['revision']),adjustment=dict(kind='none'))
    for i,action in enumerate(r['actions']):
        _probe()
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
    _probe()
    times=curve['times_years'];dfs=curve['discount_factors']
    for i,end in enumerate(times):
        if t==end:return dfs[i]
        if t<end:
            w=(t-times[i-1])/(end-times[i-1]);return exp(log(dfs[i-1])+(log(dfs[i])-log(dfs[i-1]))*w)
    if curve['right_tail']=='reject':raise ValueError('curve tail rejects')
    end=times[-1];a=log(dfs[-1])
    return exp(a*t/end) if curve['right_tail']=='flat_zero' else exp(a+(a-log(dfs[-2]))*(t-end)/(end-times[-2]))
def price(events,curve,asof,oas,bump=0.):
    _probe()
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
    _probe()
    if isinstance(expected,dict):
        if actual.keys()!=expected.keys():raise ValueError(f'{path} event/state keys')
        if expected.get('principal')==0. and expected.get('cash_interest',0.)!=0. and actual.get('cash_interest')==0.:raise ValueError(f'{path} erased retained zero-principal cash right')
        if expected.get('principal')==0. and expected.get('accrued_interest',0.)!=0. and actual.get('accrued_interest')==0.:raise ValueError(f'{path} erased retained zero-principal accrued right')
        for k,w in expected.items():_financial_tree(actual[k],w,f'{path}.{k}')
    elif isinstance(expected,list):
        if len(actual)!=len(expected):raise ValueError(f'{path} event/state count')
        for i,(a,b) in enumerate(zip(actual,expected)):_financial_tree(a,b,f'{path}[{i}]')
    elif isinstance(expected,float):close(actual,expected,path)
    else:exact(actual,expected,path)
def verify_source(actual,expected):
    _probe()
    if set(actual)!=set('position_id scenario_id run_id path_id weight role product projection'.split()):raise ValueError('complete native source fields')
    for k in ('position_id','scenario_id','run_id','path_id','role','weight','product'):exact(actual[k],expected[k],k)
    projection=actual['projection']
    if set(projection)!=set(('kind','result')):raise ValueError('complete typed projection fields')
    exact(projection['kind'],'cd_sequence')
    result=projection['result']
    if set(result)!=set('schema admitted_request action_receipts schedule live_before_terminal_settlement source_status accounting_adoption empirical_validation joint_optimal_exercise'.split()):raise ValueError('complete sequence result fields')
    exact(result['schema'],'cd-election-sequence-2');exact(result['admitted_request'],expected['product']['request'])
    request=expected['product']['request']
    if set(result['schedule'])!=set('schema as_of_ordinal horizon_days scenarios events'.split()):raise ValueError('complete actual source schedule fields')
    for k,w in dict(schema='dated-cashflow-1',as_of_ordinal=request['as_of_ordinal'],horizon_days=request['tail_end_ordinal']-request['as_of_ordinal'],scenarios=[request['scenario_id']]).items():exact(result['schedule'][k],w)
    _financial_tree(result['schedule']['events'],expected['projection']['events'],'events')
    _financial_tree(result['live_before_terminal_settlement'],expected['projection']['states'],'states')
    if len(result['action_receipts'])!=len(expected['projection']['receipts']):raise ValueError('native receipt count')
    for actual_receipt,w in zip(result['action_receipts'],expected['projection']['receipts']):
        if set(actual_receipt)!=set(('action','opening','native')):raise ValueError('complete action receipt fields')
        exact(actual_receipt['action'],w['action']);_financial_tree(actual_receipt['opening'],w['opening'],'opening')
        native=actual_receipt['native']
        for k,want in dict(schema='term-enhancements-1',model_version='term-contracts-1',policy=w['policy'],election_source=w['action']['source']).items():exact(native[k],want,k)
        for k in ('closing','new_contracts'):_financial_tree(native[k],w[k],k)
        for k in ('earned_interest_delta','gross_principal_removed','principal_cash_returned','interest_cash_paid','penalty','penalty_from_accrued_interest','penalty_from_principal','net_external_cash'):close(native[k],w[k],k)
        _financial_tree(native['settlement_schedule']['events'],[{**e,'event_id':e['event_id'].split(':',1)[1],'day_offset':0} for e in w['events']],'actual settlement')
        preview=[]
        # Reconstruct old step preview exactly, separately from the actual
        # sequence's retained final cash. The old step emits a closing tail only
        # when principal>0; sequence final right ownership also keeps nonzero A.
        states=[]
        if w['closing']['principal']>0.:states.append((w['closing'],'cd-remaining-maturity'))
        for new_state in w['new_contracts']:
            target=w['action']['election'].get('renewal');name='cd-renewal-maturity' if target is not None and new_state['position_id']==target['position_id'] else 'cd-switch-maturity'
            states.append((new_state,name))
        side=1. if request['direction']=='receipt' else -1.
        for state,event_id in states:
            _probe();day=state['maturity_ordinal'];earned=state['accrued_interest']+state['principal']*state['annual_rate']*fraction(state['last_accrual_ordinal'],day,state['day_count'])
            preview.append(dict(position_id=state['position_id'],event_id=event_id,scenario=dict(kind='named',scenario_id=request['scenario_id']),payment_ordinal=day,day_offset=day-w['action']['ordinal'],currency=request['currency'],measure=request['measure'],provenance=dict(kind='modeled',model_id=w['action']['source']['id'],model_version='term-contracts-1',input_revision=w['action']['source']['revision']),adjustment=dict(kind='none'),accrual_period=dict(start_ordinal=state['last_accrual_ordinal'],end_ordinal=day) if state['last_accrual_ordinal']<day else None,components=dict(principal=side*state['principal'],cash_interest=side*earned,fees=side*0.,accrued_interest=side*0.,book_amortization=side*0.)))
        preview.sort(key=lambda e:e['payment_ordinal'])
        _financial_tree(native['remaining_schedule']['events'],preview,'complete native preview tail')
        physical='Supplied Physical election, not observed transaction or calibrated behavior. Penalty is separated into forfeited accrued interest and principal-funded fee; gross face extinction is distinct from net principal returned. Grace interest and renewal/switch contracts are explicit. Renewal funding uses same-date gross old settlement and separate reinvestment with one net external cash amount. Remaining fixed bullet tails are separate, complete and untruncated; no future elections, automatic renewal or accounting recognition is inferred.'
        disclosure=physical if request['measure']=='physical' else 'Explicit supplied Pricing election/probabilities; no Physical-to-Pricing conversion or calibrated behavior. '+physical.split('. ',1)[1]
        exact(native['disclosure'],disclosure,'native disclosure')
        if set(native)!=set('schema model_version policy election_source closing new_contracts settlement_schedule remaining_schedule earned_interest_delta gross_principal_removed principal_cash_returned interest_cash_paid penalty penalty_from_accrued_interest penalty_from_principal net_external_cash disclosure'.split()):raise ValueError('complete native step fields')
        for name in ('settlement_schedule','remaining_schedule'):
            if set(native[name])!=set('schema as_of_ordinal horizon_days scenarios events'.split()):raise ValueError('complete preview/settlement schedule fields')
            for k,want in dict(schema='dated-cashflow-1',as_of_ordinal=w['action']['ordinal'],horizon_days=request['tail_end_ordinal']-w['action']['ordinal'],scenarios=[request['scenario_id']]).items():exact(native[name][k],want,name+'.'+k)
    for flag in ('accounting_adoption','empirical_validation','joint_optimal_exercise'):exact(result[flag],False)
    exact(result['source_status'],'explicit_supplied_chronological_contract_elections')
def _verify_calculation(raw,result):
    r=normalize(raw);_admit(r);expected=sources(r);original=sources(r,True)
    if set(result)!=set('schema identity base_anchor metrics native_sources income_policy pricing_policy source_status sequence_policy original_native_sources'.split()):raise ValueError('complete graph result fields')
    if result['schema']!='term-product-graph-2' or len(expected)!=len(result['native_sources']) or len(original)!=len(result['original_native_sources']):raise ValueError('schema/source coverage')
    exact(result['income_policy'],'Physical signed native contractual cash, explicit CD penalty/face-extinction/renewal and earned-through-election diagnostics; no daily coupon proration, effective-yield NII, allowance or journal inferred')
    exact(result['pricing_policy'],'complete future supplied-Q native CD sequence schedule and indexed cash, supplied collateral and immutable original locked OAS; paired down/up histories and actual-date bumps; no P-to-Q conversion')
    exact(result['source_status'],'raw supplied full native CD sequence states/action receipts and exclusive unsuperseded maturity cash; signed rights, explicit policies/authorized renewal terms retained; no optimal/joint/empirical/saved/ledger/treasury adoption')
    if set(result['base_anchor'])!=set(('schema','calibration_input','oas')):raise ValueError('complete original anchor fields')
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
        if set(actual)!=set('position_id scenario_id pv supplied_history_dv01 physical_periods physical_tail opening_actions'.split()):raise ValueError('complete risk metric fields')
        exact(actual['position_id'],position['position_id']);exact(actual['scenario_id'],s['scenario_id']);oas=position['oas_target']['base_oas']
        def role_price(role,bump):return sum(p['weight']*price(p['projection']['events'],s['collateral_curve'],r['as_of_ordinal'],oas,bump) for p in expected if p['position_id']==position['position_id'] and p['scenario_id']==s['scenario_id'] and p['role']==role)
        close(actual['pv'],role_price('pricing',0.),'PV')
        close(actual['supplied_history_dv01'],(role_price('pricing_down',-r['bump_bp']/10000.)-role_price('pricing_up',r['bump_bp']/10000.))/(2.*r['bump_bp']),'DV01')
        for key,bounds in [('opening_actions',[(r['as_of_ordinal'],r['as_of_ordinal'],True)]),('physical_periods',[(a,b,False)for a,b in zip(r['reporting_ordinals'],r['reporting_ordinals'][1:])]),('physical_tail',[(r['reporting_ordinals'][-1],position['known']['contract']['tail_end_ordinal'],False)])]:
            rows=actual[key] if key=='physical_periods' else [actual[key]]
            if len(rows)!=len(bounds):raise ValueError('complete physical report coverage')
            for row,(start,end,opening) in zip(rows,bounds):
                keys=set('principal_cash interest_cash fee_cash cd_principal_extinguished cd_penalty cd_penalty_from_principal cd_penalty_from_accrued_interest cd_interest_accrued_through_election cd_new_contract_principal cd_net_external_cash cd_interest_cash_paid cd_principal_cash_returned'.split())
                if set(row)!=keys:raise ValueError('complete physical cash/diagnostic fields')
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
    if set(status)!=set('original_native_sequence_sources native_sequence_sources locked_original_oas_sources oas_calibration_calls single_election_fallbacks original_quote_reconciled joint_optimal_exercise accounting_adoption empirical_validation saved_library_cohort_adoption capital_ftp_eligible'.split()):raise ValueError('complete sequence policy fields')
    for k in ('accounting_adoption','empirical_validation','joint_optimal_exercise','saved_library_cohort_adoption','capital_ftp_eligible'):exact(status[k],False)
    exact(status['oas_calibration_calls'],0);exact(status['single_election_fallbacks'],0)
    exact(status['native_sequence_sources'],len(expected));exact(status['original_native_sequence_sources'],len(original))
    exact(status['locked_original_oas_sources'],len(r['positions']));exact(status['original_quote_reconciled'],True)
    return dict(schema='independent-cd-sequence-graph-reference-1',current_sources=len(expected),original_sources=len(original),raw_source_cash_risk_verified=True,production_adoption=False)


def verify(raw,result,check=lambda:False):
    token=_CHECK.set(check)
    try:
        _probe();receipt=_verify_calculation(raw,result);_probe()
        receipt.update(complete_preview_tails_verified=True,raw_input_retained_separately=True,persisted_source_verified=False)
        return receipt
    finally:_CHECK.reset(token)

def verify_request(schema,request,result,check=lambda:False):
    token=_CHECK.set(check)
    try:
        if schema=='term-product-graph-2':raw=request
        elif schema=='term-product-reprice-2':
            if type(request)is not dict or set(request)!={'base','current'}:raise ValueError('explicit reprice base/current only')
            a,b=normalize(request['base']),normalize(request['current'])
            # The native base key's full typed original-parent identity, including
            # reviewed locked OAS and quote, survives every current scenario edit.
            def parent(r):return [r['schema'],r['identity']['model_revision'],r['identity']['paired_history_source'],r['identity']['paired_history_revision'],r['identity']['randomness_domain'],r['as_of_ordinal'],r['currency'],r['cash_timing'],[[p['position_id'],p['known'],p['oas_target']]for p in r['positions']],r['original_base'],r['original_quotes']]
            exact(parent(a),parent(b),'immutable original reprice parent');raw=request['current']
        else:raise ValueError('explicit graph2/reprice2 verifier required')
        receipt=verify(raw,result,check);receipt['operation_schema']=schema
        return receipt
    finally:_CHECK.reset(token)

def _admit(r):
    _probe()
    if type(r)is not dict or r.get('schema')!='term-product-graph-2':raise ValueError('explicit CD graph2 raw required')
    if set(r)!=set('schema identity as_of_ordinal currency reporting_ordinals base_scenario_id bump_bp cash_timing scenarios positions original_quotes original_base'.split()):raise ValueError('graph raw fields differ')
    if not 1<=len(r['positions'])<=256 or not 1<=len(r['scenarios'])<=13 or not 2<=len(r['reporting_ordinals'])<=121:raise ValueError('graph raw dimensions')
    if any(type(d)is not int or not 1<=d<=3652059 for d in [r['as_of_ordinal'],*r['reporting_ordinals']]):raise ValueError('actual integer date required')
    if r['reporting_ordinals'][0]!=r['as_of_ordinal'] or any(a>=b for a,b in zip(r['reporting_ordinals'],r['reporting_ordinals'][1:])):raise ValueError('increasing report dates required')
    if len(r['original_quotes'])!=len(r['positions']) or len(r['original_base']['histories'])!=len(r['positions']):raise ValueError('complete original coverage required')
    if len({p['position_id']for p in r['positions']})!=len(r['positions']) or len({s['scenario_id']for s in r['scenarios']})!=len(r['scenarios']):raise ValueError('unique graph owners/scenarios required')
    if r['base_scenario_id'] not in [s['scenario_id']for s in r['scenarios']]:raise ValueError('missing base scenario')
    for i,p in enumerate(r['positions']):
        _probe()
        if p['known']['kind']!='cd_sequence' or p['oas_target']['kind']!='locked':raise ValueError('public CD sequence and reviewed locked OAS only')
        c=p['known']['contract'];original=r['original_base']['histories'][i]
        exact(original['position_id'],p['position_id']);exact(r['original_quotes'][i]['position_id'],p['position_id'])
        if len(p['histories'])!=len(r['scenarios']):raise ValueError('complete current scene coverage')
        paired=[[x['path_id'],x['weight']]for x in original['histories']['pricing']]
        for scenario,h in [(r['original_base']['scenario'],original['histories']),*zip(r['scenarios'],p['histories'])]:
            _probe();exact(h['scenario_id'],scenario['scenario_id'])
            if scenario['collateral_curve']['currency']!=r['currency']:raise ValueError('collateral currency differs')
            for role in ROLES:
                paths=h[role]
                if not 1<=len(paths)<=128 or len({x['path_id']for x in paths})!=len(paths):raise ValueError('complete unique path coverage')
                if abs(sum(x['weight']for x in paths)-1.)>1e-12 or any(x['weight']<0. for x in paths):raise ValueError('supplied unit probabilities required')
                if role!='physical':exact([[x['path_id'],x['weight']]for x in paths],paired,'paired path order/weight')
                for path in paths:
                    _probe()
                    if path['product']['kind']!='cd_sequence':raise ValueError('CD-only independently verified source')
                    q=path['product']['request']
                    for k,w in dict(schema='cd-election-sequence-2',measure='physical' if role=='physical' else 'pricing',currency=r['currency'],monetary_unit='currency_units',direction=c['direction'],scenario_id=scenario['run_id'],as_of_ordinal=r['as_of_ordinal'],reporting_end_ordinal=r['reporting_ordinals'][-1],tail_end_ordinal=c['tail_end_ordinal'],opening=c['opening']).items():exact(q[k],w,'raw source '+k)
                    if len(q['actions'])>256 or len(q['policies'])>256:raise ValueError('bounded sequence domain')
                    if any(type(a['ordinal'])is not int for a in q['actions']):raise ValueError('integer action date')
                    if any(a['ordinal']>b['ordinal']for a,b in zip(q['actions'],q['actions'][1:])):raise ValueError('chronological actions required')
                    known={b['position_id']:b for b in c['policies']}
                    for binding in q['policies']:exact(binding,known[binding['position_id']],'reviewed policy owner')
                    terms={t['position_id']:t for t in c['authorized_new_contracts']}
                    for action in q['actions']:
                        e=action['election']
                        if e['kind']=='maturity':
                            for key in ('renewal','switching'):
                                target=e[key]
                                if target is not None:exact(target,terms[target['position_id']],'authorized renewal/switch terms')
