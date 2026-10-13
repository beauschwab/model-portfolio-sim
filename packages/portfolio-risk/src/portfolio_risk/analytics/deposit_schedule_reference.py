"""Independent supplied deposit runoff source verifier; never an execution backend.
Gross earned expense and bank cash settlement are reconstructed separately.
"""
from copy import deepcopy
import math
from .credit_path_verification import equal

def checkpoint(check):
    if check():raise InterruptedError('deposit source verification cancelled')

def project(raw,check=lambda:False):
    """Reviewed physical dated-ledger boundary; no signed-rate accounting inferred."""
    if any(s['opening_paid_rate']<0. or s['repricing']['floor']<0. for s in raw['segments']):raise ArithmeticError('deposit negative-rate accounting unsupported')
    return project_declared(raw,measure='physical',check=check)

def project_declared(raw,*,measure,check=lambda:False):
    """Independent complete supplied history for analysis/product-unit verification.

    The measure is a required declaration, never converted from P to Q. Signed
    earned interest is supported here; the ledger wrapper retains its own guard.
    ACT/ACT, migrations, external inflows and opening accrued remain unsupported.
    """
    checkpoint(check)
    if measure not in {'physical','pricing'} or raw['schema']!='deposit-state-1' or raw['identity']['measure']!=measure:raise ArithmeticError('deposit declared source measure required')
    if raw['migrations'] or any(any(v!=0. for v in d['external_inflows']) for d in raw['drivers']):raise ArithmeticError('deposit migration/inflow ownership unsupported')
    segments=raw['segments'];n=len(segments);asof=raw['as_of_ordinal'];end=asof+raw['horizon_days']
    if raw['day_count'] not in {'act360','act365_fixed'}:raise ArithmeticError('deposit actual accrual convention unsupported')
    divisor=360. if raw['day_count']=='act360' else 365.
    if any(s['opening_accrued_interest']!=0. for s in segments):raise ArithmeticError('deposit opening rights unsupported')
    if raw['terminal']['kind'] not in {'retain_residual','assumed_liquidation'}:raise ArithmeticError('deposit terminal value unsupported')
    observations={d['ordinal']:d for d in raw['drivers']}
    if len(observations)!=len(raw['drivers']):raise ArithmeticError('deposit duplicate observation date')
    balance=[s['opening_balance'] for s in segments];accrued=[0.]*n;rates=[s['opening_paid_rate'] for s in segments]
    cash=[];interest=[];points=[];withdrawals=closures=maturities=terminal=0.
    def point(day,outside=None,assessed=None):return dict(ordinal=day,balances=balance.copy(),accrued_interest=accrued.copy(),paid_rates=rates.copy(),outside_supported_rate_regime=outside or [False]*n,rate_regime_assessed=assessed or [False]*n)
    def pay(day,i,kind,p,c):
        if p!=0. or c!=0.:cash.append(dict(event_id=f"{segments[i]['segment_id']}-{kind}-{day}",ordinal=day,segment_id=segments[i]['segment_id'],kind=kind,principal=p,cash_interest=c))
    points.append(point(asof))
    for day in range(asof,end):
        checkpoint(check);driver=observations[day];active=[s['maturity_ordinal'] is None or s['maturity_ordinal']>day for s in segments];outside=[False]*n;assessed=[False]*n
        for i,s in enumerate(segments):
            if s['maturity_ordinal']==day:
                maturities+=balance[i];pay(day,i,'contractual_maturity',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.
            policy=s['repricing'];observed=observations[day-policy['observation_lag_days']];reference=observed['reference_rate'];competitor=observed['competitor_rates'][i];rule=policy['target']
            if rule['kind']=='linear':beta=rule['beta']
            elif rule['kind']=='logistic_beta':
                x=rule['steepness']*(reference-rule['midpoint']);sigmoid=1./(1.+math.exp(-x)) if x>=0. else math.exp(x)/(1.+math.exp(x));beta=rule['beta_min']+(rule['beta_max']-rule['beta_min'])*sigmoid
            else:raise ArithmeticError('deposit target rule unsupported')
            desired=min(max(rule['intercept']+beta*reference+rule['competitor_weight']*(competitor-reference)+observed['offered_spread_stress'][i],policy['floor']),policy['ceiling'])
            half=policy['rising_half_life_days'] if desired>=rates[i] else policy['falling_half_life_days'];adjustment=1. if half==0. else -math.expm1(-math.log(2.)/half)
            rates[i]=min(max(rates[i]+adjustment*(desired-rates[i]),policy['floor']),policy['ceiling'])
            regime=policy['supported_observation_range']
            if regime is not None:assessed[i]=True;outside[i]=not regime['minimum_reference_rate']<=reference<=regime['maximum_reference_rate']
        new_balance=[0.]*n;new_accrued=[0.]*n
        for i,s in enumerate(segments):
            if not active[i]:continue
            p=s['retention'];c=s['characteristics'];gap=max(driver['competitor_rates'][i]-rates[i],0.);velocity=abs(driver['reference_rate']-observations[day-1]['reference_rate'])
            exposure=p['uninsured_stress_sensitivity']*c['uninsured_fraction']+p['concentration_stress_sensitivity']*c['concentration']+p['digital_stress_sensitivity']*c['digital_fraction']-p['relationship_protection']*c['relationship_fraction']-p['operational_protection']*c['operational_fraction']
            withdrawal=max(p['withdrawal_per_year']+p['opportunity_gap_sensitivity']*gap+p['absolute_daily_velocity_sensitivity']*velocity+driver['confidence_stress'][i]*exposure,0.)
            closure=p['closure_per_year']+p['closure_stress_sensitivity']*driver['confidence_stress'][i];total=withdrawal+closure
            if not math.isfinite(total) or not 0.<=total<=100.:raise ArithmeticError('deposit competing intensity outside bounds')
            exited=-math.expm1(-total/365.) if total else 0.;share=exited/total if total else 0.
            wd=balance[i]*withdrawal*share;cl=balance[i]*closure*share;wi=accrued[i]*withdrawal*share;ci=accrued[i]*closure*share
            withdrawals+=wd;closures+=cl;pay(day,i,'withdrawal',-wd,-wi);pay(day,i,'closure',-cl,-ci)
            new_balance[i]+=balance[i]*(1.-exited);new_accrued[i]+=accrued[i]*(1.-exited)
        for i,s in enumerate(segments):
            earned=new_balance[i]*rates[i]/divisor;new_accrued[i]+=earned
            if earned:interest.append(dict(start_ordinal=day,end_ordinal=day+1,segment_id=s['segment_id'],amount=earned))
            if day+1 in s['interest_payment_ordinals']:pay(day+1,i,'interest_payment',0.,-new_accrued[i]);new_accrued[i]=0.
        balance=new_balance;accrued=new_accrued;points.append(point(day+1,outside,assessed))
    for i,s in enumerate(segments):
        if s['maturity_ordinal']==end:maturities+=balance[i];pay(end,i,'contractual_maturity',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.
    if raw['terminal']['kind']=='assumed_liquidation':
        for i in range(n):terminal+=balance[i];pay(end,i,'assumed_terminal_payout',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.
    if any(v!=0. for v in balance+accrued):raise ArithmeticError('deposit source requires complete zero tail')
    points[-1]['balances']=balance.copy();points[-1]['accrued_interest']=accrued.copy();cash.sort(key=lambda e:e['ordinal'])
    opening=sum(s['opening_balance'] for s in segments);expense=sum(v['amount'] for v in interest);paid=sum(v['cash_interest'] for v in cash)
    equal(opening,withdrawals+closures+maturities+terminal,'deposit/principal_conservation');equal(expense+paid,0.,'deposit/interest_conservation')
    return dict(schema='deposit-state-1',identity=deepcopy(raw['identity']),parameter_status='supplied_assumption',segment_ids=[s['segment_id'] for s in segments],points=points,cashflows=cash,migrations=[],interest=interest,total_external_inflows=0.,total_withdrawals=withdrawals,total_closures=closures,total_contractual_maturities=maturities,total_assumed_terminal_payout=terminal,closing_balance=sum(balance),closing_accrued_interest=sum(accrued),total_interest_expense=expense,total_bank_cash_interest=paid,supplied_franchise_value=None)

def verify_preparation(raw,prepared,receipt,check=lambda:False):
    checkpoint(check)
    if receipt['raw_request']!=raw or raw['schema']!='deposit-graph-dated-ledger-1' or receipt['schema']!='deposit-graph-dated-ledger-source-1' or receipt['normalization_policy']!='native_par_deposit_opening_anchors-1':raise ArithmeticError('deposit immutable source schema/identity differs')
    graph=raw['graph'];capture=receipt['capture'];owners={o['position_id']:o for o in raw['owners']};policy=raw['capture_policy']
    if graph['schema']!='deposit-product-graph-1' or receipt['graph']['schema']!=graph['schema'] or receipt['graph']['identity']!=graph['identity'] or capture['schema']!='deposit-scheduled-capture-1':raise ArithmeticError('deposit graph/capture identity differs')
    if len(owners)!=len(raw['owners']) or set(owners)!={p['position_id'] for p in graph['positions']} or len({o['source_id'] for o in owners.values()})!=len(owners):raise ArithmeticError('deposit complete unique ownership differs')
    if policy['recognition_model']!='deposit_daily_par-1' or policy['book_basis']!='par_deposit_simple_interest' or policy['fees']!='reject':raise ArithmeticError('deposit gross expense/par policy differs')
    for owner in owners.values():
        if owner['direction']!='payment' or owner['ownership']!='fully_scenario_generated' or owner['source_revision']!=graph['identity']['input_revision']:raise ArithmeticError('deposit bank payment ownership differs')
    mapping={m['run_id']:m['ledger_scenario_id'] for m in raw['run_mapping']}
    if len(mapping)!=len(raw['run_mapping']) or set(mapping)!={s['run_id'] for s in graph['scenarios']}:raise ArithmeticError('deposit complete unique run mapping differs')
    for s in graph['scenarios']:
        base=s['scenario_id']==graph['base_scenario_id']
        if (mapping[s['run_id']]=='baseline')!=base or (base and s['severity']!=1.):raise ArithmeticError('deposit baseline mapping differs')
    physical=[];sources=[];plans=[];cash_origins=[];openings=[]
    for position in graph['positions']:
        checkpoint(check);owner=owners[position['position_id']];events=[];coverage=[];opening=None;horizon=None
        for scenario in graph['scenarios']:
            histories=[h for h in position['histories'] if h['scenario_id']==scenario['scenario_id']]
            if len(histories)!=1 or len(histories[0]['physical'])!=1 or histories[0]['physical'][0]['weight']!=1.:raise ArithmeticError('deposit one physical history weight one required')
            h=histories[0]['physical'][0];contract=h['request'];output=project(contract,check);principal=sum(s['opening_balance'] for s in contract['segments'])
            if opening is not None and (opening!=principal or horizon!=contract['horizon_days']):raise ArithmeticError('deposit opening/horizon differs across runs')
            opening=principal;horizon=contract['horizon_days'];physical.append(dict(position_id=position['position_id'],scenario_id=scenario['scenario_id'],run_id=scenario['run_id'],path_id=h['path_id'],weight=1.,role='physical',projection=output))
            selector=dict(kind='named',scenario_id=scenario['run_id']);coverage.append(dict(position_id=position['position_id'],run=selector,status=dict(kind='complete_event_set') if output['cashflows'] else dict(kind='no_events',rationale='complete native physical deposit projection has no cash events')))
            remaining=[v['amount'] for v in output['interest']];index={s['segment_id']:i for i,s in enumerate(contract['segments'])};origins={s['segment_id']:[i for i,o in enumerate(output['interest']) if o['segment_id']==s['segment_id']] for s in contract['segments']};cursor=dict.fromkeys(origins,0)
            for cash in output['cashflows']:
                checkpoint(check);segment=cash['segment_id'];si=index[segment];event_id=f"deposit:{si}:{cash['kind']}:{cash['ordinal']}";needed=-cash['cash_interest'];allocation=[]
                while needed>0. and cursor[segment]<len(origins[segment]):
                    checkpoint(check);oi=origins[segment][cursor[segment]];origin=output['interest'][oi]
                    if origin['end_ordinal']>cash['ordinal']:raise ArithmeticError('deposit future rights cannot settle early')
                    amount=min(needed,remaining[oi])
                    if amount>0.:allocation.append(dict(origin_id=f"deposit-interest:{si}:{origin['start_ordinal']}:{origin['end_ordinal']}",start_ordinal=origin['start_ordinal'],end_ordinal=origin['end_ordinal'],accrued_interest=-amount,book_amortization=0.))
                    needed-=amount;remaining[oi]-=amount
                    if remaining[oi]==0.:cursor[segment]+=1
                if needed>max(1e-9,abs(cash['cash_interest'])*1e-12):raise ArithmeticError('deposit cash lacks earned origins')
                if allocation:allocation[-1]['accrued_interest']+=cash['cash_interest']-sum(s['accrued_interest'] for s in allocation)
                if allocation:plans.append(dict(position_id=position['position_id'],event_id=event_id,run=selector,segments=allocation))
                period=dict(start_ordinal=allocation[0]['start_ordinal'],end_ordinal=allocation[-1]['end_ordinal']) if allocation else None
                provenance=dict(kind='assumed',assumption_id=contract['identity']['assumption_id'],rationale=contract['terminal']['rationale']) if cash['kind']=='assumed_terminal_payout' else dict(kind='modeled',model_id='deposit-state-1',model_version='deposit-state-1',input_revision=graph['identity']['input_revision'])
                events.append(dict(position_id=position['position_id'],event_id=event_id,scenario=selector,payment_ordinal=cash['ordinal'],day_offset=cash['ordinal']-graph['as_of_ordinal'],accrual_period=period,currency=graph['currency'],components=dict(principal=cash['principal'],cash_interest=cash['cash_interest'],accrued_interest=cash['cash_interest'],book_amortization=0.,fees=0.),measure='physical',provenance=provenance,adjustment=dict(kind='none')))
                cash_origins.append(dict(position_id=position['position_id'],scenario_id=scenario['scenario_id'],run_id=scenario['run_id'],event_id=event_id,native_event_id=cash['event_id'],segment_id=segment,kind=cash['kind'],ordinal=cash['ordinal']))
            if sum(abs(v) for v in remaining)>max(1e-9,abs(output['total_interest_expense'])*1e-12):raise ArithmeticError('deposit earned rights omitted')
        events.sort(key=lambda e:e['payment_ordinal']);sources.append(dict(source_id=owner['source_id'],source_revision=owner['source_revision'],schedule=dict(schema='dated-cashflow-1',as_of_ordinal=graph['as_of_ordinal'],horizon_days=horizon,scenarios=[s['run_id'] for s in graph['scenarios']],events=events),coverage=coverage));openings.append(dict(position_id=position['position_id'],principal=opening,accrued_interest=0.,gross_carrying_value=opening))
    equal([s for s in receipt['graph']['native_sources'] if s['role']=='physical'],physical,'deposit/physical_sources')
    for key,want in [('identity',graph['identity']),('policy',policy),('scenarios',graph['scenarios']),('owners',raw['owners']),('openings',openings),('sources',sources),('accrual_plans',plans)]:equal(capture[key],want,'deposit/capture/'+key)
    equal(receipt['cash_origins'],cash_origins,'deposit/cash_origins')
    specification=deepcopy(raw['specification']);anchors={o['position_id']:o for o in openings};ids=[p['id'] for p in specification['positions']]
    if len(ids)!=len(set(ids)) or not anchors.keys()<=set(ids):raise ArithmeticError('deposit missing/duplicate ledger positions')
    for p in specification['positions']:
        if p['id'] not in anchors:continue
        owner=owners[p['id']]
        if p.get('kind')!='deposit' or p.get('account')!=owner['account_id']:raise ArithmeticError('deposit explicit account/kind differs')
        additions=dict(balance=anchors[p['id']]['principal'],opening_accrued=0.,book_adjustment=0.,source_id=owner['source_id'])
        if any(k in p and p[k]!=v for k,v in additions.items()):raise ArithmeticError('deposit explicit opening/source anchor conflicts')
        p.update(additions)
    dated=dict(schema='dated-store-1',as_of_ordinal=graph['as_of_ordinal'],reporting_horizon_days=graph['reporting_ordinals'][-1]-graph['as_of_ordinal'],runs=[dict(run_id=s['run_id'],scenario_id=mapping[s['run_id']],severity=s['severity'],input_revision=graph['identity']['input_revision']) for s in graph['scenarios']],owners=[dict(o,currency=graph['currency']) for o in raw['owners']],sources=sources,accrual_plans=[dict(p,segments=[{k:v for k,v in s.items() if k!='origin_id'} for s in p['segments']]) for p in plans])
    equal(prepared,dict(schema='dated-ledger-1',monetary_unit=raw['monetary_unit'],recognition_policy=raw['recognition_policy'],specification=specification,dated=dated),'deposit/prepared')
    return dict(physical_deposit_source_recomputed=True,accrual_origins_retained=True,pricing_graph_independently_verified=False,empirical_validation=False)
