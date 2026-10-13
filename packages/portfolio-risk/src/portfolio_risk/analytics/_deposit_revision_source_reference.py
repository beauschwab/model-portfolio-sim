"""Independent purchase-time liability revision oracle; artifact analysis only.

No native financial callback. Raw deposit histories own physical cash; reviewed
original yield owns one catchup and subsequent carrying recurrence.
"""
from copy import deepcopy
import math
import sys
from portfolio_risk.analytics import deposit_schedule_reference as deposit
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as checks

SCHEMA='deposit-revised-income-source-1'
POLICIES=dict(expectation_policy='purchase_asof_original_yield_catchup-1',pnl_policy='liability_revision_separate_pnl-1',phase_policy='original_opening_then_revision_then_daily_recognition_settlement-1',tail_policy='complete_original_and_revised_tail-1')
DIMENSIONS=frozenset(('reference_rate','competitor_rates','offered_spread_stress','confidence_stress'))

def typed_source(value,key=''):
    """Declared typed raw normalization, retaining signed binary64 zero."""
    integers={'purchase_ordinal','as_of_ordinal','horizon_days','ordinal','observation_lag_days','maturity_ordinal','interest_payment_ordinals'}
    if isinstance(value,dict):return {k:typed_source(v,k) for k,v in value.items()}
    if isinstance(value,list):return [typed_source(v,key) for v in value]
    if type(value) is int and key not in integers:return float(value)
    return value

def near(a,b,path='deposit conservation'):
    if type(a) not in (int,float) or type(b) not in (int,float) or not math.isfinite(a) or not math.isfinite(b) or abs(a-b)>max(1e-9,1e-12*max(abs(a),abs(b))):
        raise ArithmeticError(path+': original finite amount tolerance')

def equal(actual,expected,path='deposit'):
    if isinstance(expected,dict):
        if type(actual) is not dict or actual.keys()!=expected.keys():raise ArithmeticError(path+': complete fields differ')
        for k,v in expected.items():equal(actual[k],v,path+'/'+k)
    elif isinstance(expected,list):
        if type(actual) is not list or len(actual)!=len(expected):raise ArithmeticError(path+': complete ordered rows differ')
        for i,v in enumerate(expected):equal(actual[i],v,path+'/'+str(i))
    elif type(expected) is float:near(actual,expected,path)
    else:checks.exact(actual,expected,path)

def tick(check):
    if check():raise InterruptedError('deposit revision independent verification cancelled')

def amount(v):
    if type(v) not in (int,float) or not math.isfinite(v) or abs(v)>10000.:
        raise ArithmeticError('deposit revision monetary domain exceeds10000')
    return float(v)

def shape(raw):
    checks.keys(raw,'schema identity as_of_ordinal horizon_days day_count segments migrations drivers terminal')
    identity=raw['identity'];checks.keys(identity,'entity_id cohort_id currency source_revision model_version assumption_id assumption_rationale run_id measure')
    for k,v in identity.items():
        if k=='measure':continue
        if type(v) is not str or not v.strip() or len(v.encode())>(1024 if k=='assumption_rationale' else 256) or any(ord(c)<32 for c in v):raise ArithmeticError('bounded source identity')
    if raw['schema']!='deposit-state-1' or raw['identity']['measure']!='physical' or type(raw['horizon_days']) is not int or not 1<=raw['horizon_days']<=64 or len(raw['segments'])!=1 or raw['migrations'] or raw['terminal']!={'kind':'retain_residual'}:
        raise ArithmeticError('one physical actual maturity deposit with <=64 days required')
    start=raw['as_of_ordinal'];s=raw['segments'][0];end=start+raw['horizon_days']
    checks.keys(s,'segment_id opening_balance opening_accrued_interest opening_paid_rate characteristics repricing retention maturity_ordinal interest_payment_ordinals')
    if type(s['segment_id']) is not str or not s['segment_id'].strip() or len(s['segment_id'].encode())>64:raise ArithmeticError('segment identity')
    if type(start) is not int or type(s['maturity_ordinal']) is not int or not start<s['maturity_ordinal']<=end:
        raise ArithmeticError('actual maturity within complete tail required')
    if amount(s['opening_balance'])<=0. or amount(s['opening_accrued_interest'])!=0. or s['opening_paid_rate']<0. or s['repricing']['floor']<0. or s['repricing']['ceiling']>1.:
        raise ArithmeticError('positive principal zero opening rights nonnegative rates required')
    if type(s['repricing']['observation_lag_days']) is not int or not 0<=s['repricing']['observation_lag_days']<=64:
        raise ArithmeticError('strict bounded observation lag')
    def rng(v,lo,hi):
        if type(v) not in (int,float) or not math.isfinite(v) or not lo<=v<=hi:raise ArithmeticError('deposit numeric domain')
    policy=s['repricing'];checks.keys(policy,'target observation_lag_days rising_half_life_days falling_half_life_days floor ceiling supported_observation_range')
    rng(policy['floor'],0.,1.);rng(policy['ceiling'],policy['floor'],1.);rng(s['opening_paid_rate'],policy['floor'],policy['ceiling'])
    for k in ('rising_half_life_days','falling_half_life_days'):rng(policy[k],0.,36600.)
    rule=policy['target']
    if rule['kind']=='linear':checks.keys(rule,'kind intercept beta competitor_weight');rng(rule['beta'],0.,2.)
    elif rule['kind']=='logistic_beta':
        checks.keys(rule,'kind intercept beta_min beta_max midpoint steepness competitor_weight');rng(rule['beta_min'],0.,2.);rng(rule['beta_max'],rule['beta_min'],2.);rng(rule['midpoint'],-.2,2.);rng(rule['steepness'],0.,10000.)
    else:raise ArithmeticError('explicit target model')
    rng(rule['intercept'],-2.,2.);rng(rule['competitor_weight'],0.,2.)
    if policy['supported_observation_range'] is not None:
        regime=policy['supported_observation_range'];checks.keys(regime,'minimum_reference_rate maximum_reference_rate history_revision logistic_plateau_identified');rng(regime['minimum_reference_rate'],-.2,2.);rng(regime['maximum_reference_rate'],regime['minimum_reference_rate'],2.)
        if type(regime['logistic_plateau_identified']) is not bool:raise ArithmeticError('explicit plateau declaration')
    checks.keys(s['characteristics'],'uninsured_fraction concentration digital_fraction relationship_fraction operational_fraction')
    for v in s['characteristics'].values():rng(v,0.,1.)
    checks.keys(s['retention'],'withdrawal_per_year closure_per_year opportunity_gap_sensitivity absolute_daily_velocity_sensitivity uninsured_stress_sensitivity concentration_stress_sensitivity digital_stress_sensitivity relationship_protection operational_protection closure_stress_sensitivity')
    for v in s['retention'].values():rng(v,0.,10000.)
    first=start-policy['observation_lag_days']-1
    if len(raw['drivers'])!=end-first:raise ArithmeticError('complete lag history')
    for j,d in enumerate(raw['drivers']):
        checks.keys(d,'ordinal reference_rate competitor_rates offered_spread_stress confidence_stress external_inflows')
        if type(d['ordinal']) is not int or d['ordinal']!=first+j:raise ArithmeticError('exact ordered driver date')
        rng(d['reference_rate'],-.2,2.)
        for k,lo,hi in (('competitor_rates',-.2,2.),('offered_spread_stress',-2.,2.),('confidence_stress',0.,100.),('external_inflows',0.,10000.)):
            if len(d[k])!=1:raise ArithmeticError('one segment driver vector')
            rng(d[k][0],lo,hi)
    if any(type(v) is not int or not start<v<=end for v in s['interest_payment_ordinals']):raise ArithmeticError('no day0 interest cash or incomplete calendar')
    if s['interest_payment_ordinals']!=sorted(set(s['interest_payment_ordinals'])):raise ArithmeticError('strict interest date order')
    if any(any(v!=0. for v in d['external_inflows']) for d in raw['drivers']):raise ArithmeticError('external inflow unsupported')

def admit(raw,check=lambda:False):
    tick(check);checks.keys(raw,'schema contract anchor policy original runs')
    if raw['schema']!=SCHEMA or not 1<=len(raw['runs'])<=8:raise ArithmeticError('bounded declared revision source schema/runs')
    checks.domain.identity(raw['contract']);a=raw['anchor'];p=raw['policy']
    checks.keys(a,'purchase_ordinal original_principal original_price_per_100 original_gross_carrying_value original_annual_effective_rate purchase_quote original_yield_anchor')
    checks.keys(p,'review expectation_policy pnl_policy phase_policy tail_policy changed_driver_dimensions')
    for x in (a['purchase_quote'],a['original_yield_anchor'],p['review']):checks.domain.identity(x)
    for k,v in POLICIES.items():checks.exact(p[k],v,'reviewed '+k)
    dims=p['changed_driver_dimensions']
    if len(dims)!=len(set(dims)) or not set(dims)<=DIMENSIONS:raise ArithmeticError('duplicate or unknown driver dimensions')
    shape(raw['original']);original=raw['original']
    if type(a['purchase_ordinal']) is not int or a['purchase_ordinal']!=original['as_of_ordinal']:raise ArithmeticError('purchase time original anchor required')
    checks.exact(float(a['original_principal']),float(original['segments'][0]['opening_balance']),'original principal identity')
    principal=amount(a['original_principal']);cost=amount(a['original_gross_carrying_value'])
    if any(type(a[k]) not in (int,float) or not math.isfinite(a[k]) for k in ('original_price_per_100','original_annual_effective_rate')) or not 0.<a['original_price_per_100']<=1e6 or not 0.<=a['original_annual_effective_rate']<=10. or cost<=0.:raise ArithmeticError('positive purchase and nonnegative original EIR required')
    near(cost,principal*a['original_price_per_100']/100.,'original quote/cost')
    ids=set();scenarios=set()
    for run in raw['runs']:
        tick(check);checks.keys(run,'run_id scenario_id severity revised');checks.domain.identity(dict(id=run['run_id'],revision=run['scenario_id']))
        if run['run_id'] in ids or run['scenario_id'] in scenarios or run['severity']!=1. or type(run['severity']) is bool:raise ArithmeticError('duplicate or incompatible revision run')
        ids.add(run['run_id']);scenarios.add(run['scenario_id']);current=run['revised'];shape(current)
        checks.exact(current['identity']['run_id'],run['run_id'],'explicit native run')
        for k in ('as_of_ordinal','horizon_days','day_count','segments','terminal'):
            checks.exact(current[k],original[k],'immutable contract '+k)
        for k in ('entity_id','cohort_id','currency','model_version'):
            checks.exact(current['identity'][k],original['identity'][k],'immutable source '+k)
        if len(current['drivers'])!=len(original['drivers']):raise ArithmeticError('driver calendar count')
        for old,new in zip(original['drivers'],current['drivers'],strict=True):
            checks.exact(new['ordinal'],old['ordinal'],'driver calendar');checks.exact(new['external_inflows'],old['external_inflows'],'inflows')
            if old['ordinal']<original['as_of_ordinal']:checks.exact(new,old,'retained past observation')
            for dim in DIMENSIONS-set(dims):checks.exact(new[dim],old[dim],'undeclared driver '+dim)
    return deepcopy(raw)

def obligations(source):
    out={}
    for cash in source['cashflows']:
        p=amount(cash['principal']);i=amount(cash['cash_interest'])
        if p>0. or i>0.:raise ArithmeticError('liability outflow/nonnegative coupon ownership')
        slot=out.setdefault(cash['ordinal'],[0.,0.]);slot[0]-=p;slot[1]-=i
        amount(slot[0]+slot[1])
    return out

def cash_origin_allocations(source,check=lambda:False):
    """Reviewed FIFO source-origin rights, with explicit zero calendar gaps.

    Each actual cash event is retained once. Earned lots are independently
    generated source.interest entries, not producer-supplied allocation arrays.
    Principal never enters the interest allocation. Monetary residuals remain
    visible and are checked under the unchanged amount contract.
    """
    remaining=[i['amount'] for i in source['interest']];cursor=0;allocations=[]
    for cash in source['cashflows']:
        tick(check);needed=-cash['cash_interest'];lots=[]
        full=cash['kind'] in ('interest_payment','contractual_maturity')
        while (full or needed>0.) and cursor<len(remaining):
            tick(check);origin=source['interest'][cursor]
            if origin['end_ordinal']>cash['ordinal']:
                # Retain tiny source summation residual; never take future rights
                # merely to force a floating-point equality or add a cash plug.
                near(needed,0.,'cannot settle future earned origin')
                break
            paid=remaining[cursor] if full else min(needed,remaining[cursor])
            if paid>0.:
                if lots and lots[-1]['end_ordinal']<origin['start_ordinal']:
                    lots.append(dict(origin_index=None,start_ordinal=lots[-1]['end_ordinal'],end_ordinal=origin['start_ordinal'],amount=0.))
                lots.append(dict(origin_index=cursor,start_ordinal=origin['start_ordinal'],end_ordinal=origin['end_ordinal'],amount=paid))
            needed-=paid;remaining[cursor]-=paid
            if remaining[cursor]==0.:cursor+=1
        near(needed,0.,'cash interest must consume actual earned origins')
        allocations.append(dict(native_event_id=cash['event_id'],ordinal=cash['ordinal'],principal=cash['principal'],cash_interest=cash['cash_interest'],retirement='complete_eligible_origins' if full else 'partial_fifo_origins',lots=lots))
    near(sum(abs(x) for x in remaining),0.,'complete gross earned source rights')
    return allocations

def income(raw,projection,check):
    start=raw['original']['as_of_ordinal'];end=start+raw['original']['horizon_days'];anchor=raw['anchor'];cash=obligations(projection)
    if start in cash:raise ArithmeticError('asof settlement unsupported')
    y=anchor['original_annual_effective_rate'];z=math.log1p(y)/365.
    # Cash amounts/dates come solely from the independent physical projection.
    future=[dict(payment_ordinal=d,amount=sum(cash[d])) for d in sorted(cash)]
    earned={i['end_ordinal']:i['amount'] for i in projection['interest']}
    base=dict(source=deepcopy(raw['contract']),principal_cash=0.,interest_cash=0.,contractual_interest_earned=0.,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None)
    events=[dict(base,event_id='asof_revision',payment_ordinal=start,yield_change=dict(kind='catchup_original',review=deepcopy(raw['policy']['review']),complete_future_cash=future))]
    for d in range(start+1,end+1):
        p,i=cash.get(d,(0.,0.));events.append(dict(base,event_id='day_'+str(d),payment_ordinal=d,principal_cash=p,interest_cash=i,contractual_interest_earned=earned.get(d,0.)))
    request=dict(schema='dated-income-1',contract=deepcopy(raw['contract']),original_yield_anchor=deepcopy(anchor['original_yield_anchor']),policy=deepcopy(raw['policy']['review']),currency=projection['identity']['currency'],monetary_unit='currency_units',measure='physical',as_of_ordinal=start,full_contract_end_ordinal=end,opening_principal=anchor['original_principal'],opening_accrued_interest=0.,opening_gross_carrying_value=anchor['original_gross_carrying_value'],original_annual_effective_rate=y,opening_annual_effective_rate=y,events=events)
    principal=float(anchor['original_principal']);carrying=float(anchor['original_gross_carrying_value']);accrued=0.;last=start;rows=[]
    revised_pv=sum(f['amount']*math.exp(-z*(f['payment_ordinal']-start)) for f in future)
    for e in events:
        tick(check);opening=carrying;effective=opening*math.expm1(z*(e['payment_ordinal']-last));amort=effective-e['contractual_interest_earned'];payment=e['principal_cash']+e['interest_cash']
        carrying+=effective-payment;principal-=e['principal_cash'];accrued+=e['contractual_interest_earned']-e['interest_cash'];modification=0.;review=None
        if e['yield_change'] is not None:modification=revised_pv-carrying;carrying=revised_pv;review=deepcopy(raw['policy']['review'])
        roundoff=64.*sys.float_info.epsilon*(abs(opening)+abs(payment)+abs(effective))
        if carrying<0. and principal==0. and abs(accrued)<=roundoff and abs(carrying)<=roundoff:carrying=0.
        if carrying<0.:raise ArithmeticError('negative carrying needs reviewed ownership')
        row=dict(event_id=e['event_id'],source=deepcopy(e['source']),payment_ordinal=e['payment_ordinal'],opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=effective,contractual_interest_earned=e['contractual_interest_earned'],basis_amortization=amort,modification_gain_loss=modification,recognized_nonintegral_fee_income=0.,cash_received=payment,closing_principal=principal,closing_accrued_interest=accrued,closing_gross_carrying_value=carrying,closing_book_adjustment=carrying-principal-accrued,yield_review=review)
        for k,v in row.items():
            if type(v) is float:amount(v)
        rows.append(row);last=e['payment_ordinal']
    for v in (principal,accrued,carrying):near(v,0.,'full independent liability tail')
    result={k:deepcopy(request[k]) for k in ('schema','contract','policy','original_yield_anchor','currency','monetary_unit','measure')}
    result.update(rows=rows,remaining_principal=principal,remaining_accrued_interest=accrued,remaining_gross_carrying_value=carrying)
    return request,result,revised_pv

def reconstruct_source(raw,check=lambda:False):
    r=typed_source(admit(raw,check));original=deposit.project(r['original'],check)
    # Pure projector Python empty sums are integers; these result fields are
    # computed currency amounts, never immutable integer identities.
    for k in ('total_principal_paid','total_interest_expense','unsettled_accrued_interest'):
        if k in original:original[k]=float(original[k])
    cash=obligations(original);start=r['original']['as_of_ordinal'];z=math.log1p(r['anchor']['original_annual_effective_rate'])/365.
    original_pv=sum(sum(v)*math.exp(-z*(d-start)) for d,v in sorted(cash.items()))
    near(original_pv,r['anchor']['original_gross_carrying_value'],'complete original physical quote/EIR')
    paths=[]
    for run in r['runs']:
        tick(check);source=deposit.project(run['revised'],check);request,result,pv=income(r,source,check)
        for k in ('total_principal_paid','total_interest_expense','unsettled_accrued_interest'):
            if k in source:source[k]=float(source[k])
        paths.append(dict(run_id=run['run_id'],scenario_id=run['scenario_id'],severity=run['severity'],source=source,income_request=request,income=result,original_cost=r['anchor']['original_gross_carrying_value'],revised_carrying_value=pv,basis_revision=pv-r['anchor']['original_gross_carrying_value']))
    return dict(original_source=original,original_pv=original_pv,paths=paths)

def verify_source(raw,actual,check=lambda:False):
    expected=reconstruct_source(raw,check)
    for k,value in expected.items():equal(actual[k],value,'deposit revision source/'+k)
    for got,want in zip(actual['paths'],expected['paths'],strict=True):
        for k in ('contract','policy','original_yield_anchor','original_annual_effective_rate','opening_annual_effective_rate'):
            checks.exact(got['income_request'][k],want['income_request'][k],'frozen original income '+k)
    return dict(physical_deposit_source_recomputed=True,original_yield_revision_recomputed=True,accounting_standard_approval=False,capital_ftp_eligible=False,installed_adoption=False)
