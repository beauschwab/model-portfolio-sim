"""Independent raw mortgage and coupled-carry publication reference.

SciPy's scaling/squaring exponential is independent of native uniformization.
This is a verifier, never a financial execution backend or adoption permission.
"""
from copy import deepcopy
import json
import math
import numpy as np
from scipy.linalg import expm

# Exact compiled source receipt identity; queued/executed artifact hashes bind execution.
SOURCE_SHA = 'c07d2a5bf213c5d3860a722ce27079bd4a10120b1fc7e3b62118437a51571e03'
GENERATOR_CODE_HASH_BYTES = 141787
ORDER = 'interval_transport_default_capture_prepayment_payment_arm_reset_snapshot-1'
MAX_AMOUNT = 1e15


def checkpoint(check):
    if check():
        raise InterruptedError('mortgage source reference cancelled')


def equal(a, b, path='result'):
    """Complete shape/order equality; integers/identities are exact typed values."""
    if isinstance(b, dict):
        if not isinstance(a, dict) or set(a) != set(b):
            raise ArithmeticError(f'{path}: keys')
        for k in b:
            equal(a[k], b[k], path + '/' + k)
    elif isinstance(b, list):
        if not isinstance(a, list) or len(a) != len(b):
            raise ArithmeticError(f'{path}: shape')
        for i, v in enumerate(b):
            equal(a[i], v, f'{path}/{i}')
    elif isinstance(b, bool) or b is None or isinstance(b, str) or type(b) is int:
        if type(a) is not type(b) or a != b:
            raise ArithmeticError(f'{path}: exact identity')
    elif isinstance(b, (int, float)):
        if type(a) not in (int, float) or not math.isfinite(a) or not math.isfinite(b) or abs(a-b) > 1e-9 + 1e-12 * abs(b):
            raise ArithmeticError(f'{path}: financial value {a!r} != {b!r}')
    elif a != b:
        raise ArithmeticError(f'{path}: value')


def fields(value, names):
    if not isinstance(value, dict) or set(value) != set(names.split()):
        raise ArithmeticError('unknown/missing raw fields')


def text(v):
    if not isinstance(v, str) or not v.strip() or len(v.encode()) > 256 or any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in v):
        raise ArithmeticError('bounded immutable identity required')


def identity(v):
    fields(v, 'id revision')
    for s in v.values():
        text(s)


def finite(v, low=0., high=MAX_AMOUNT):
    if not isinstance(v, (int, float, np.floating)) or isinstance(v, bool) or not math.isfinite(v) or not low <= v <= high:
        raise ArithmeticError('finite bounded domain')
    return float(v)


def integer(v, low=0, high=36600):
    if type(v) is not int or not low <= v <= high:
        raise ArithmeticError('exact bounded integer')
    return v


def ordinal(v):
    return integer(v, 1, 3652059)


def vector(v, n, low=0., high=MAX_AMOUNT):
    if not isinstance(v, list) or len(v) != n:
        raise ArithmeticError('fixed vector shape')
    for x in v:
        finite(x, low, high)
    if sum(abs(x) for x in v) > high and high == MAX_AMOUNT:
        raise ArithmeticError('aggregate monetary domain')


def tau(a, b, convention):
    ordinal(a); ordinal(b)
    if b < a or convention not in {'act360', 'act365_fixed'}:
        raise ArithmeticError('explicit mortgage simple-interest clock')
    return (b-a)/(360. if convention == 'act360' else 365.)


def hazard_generator(h):
    if not isinstance(h, list) or len(h) != 3:
        raise ArithmeticError('hazard rows')
    q = np.zeros((7, 7))
    for i, row in enumerate(h):
        vector(row, 5, 0., 100.)
        if row[i] != 0. or sum(row) > 100.:
            raise ArithmeticError('zero diagonal and bounded outgoing hazard')
        q[i, i] = -sum(row)
        for j in range(3):
            if j != i:
                q[i, j] = row[j]
        q[i, 3+i] = row[3]
        q[i, 6] = row[4]
    return q


def coefficients(q, rates, duration):
    """T and F from exp(t [[Q,R],[0,Q]]); signed coupon reward remains signed."""
    r = np.diag(list(rates) + [0.]*4)
    block = np.block([[q, r], [np.zeros_like(q), q]])
    exponential = expm(duration * block)
    return exponential[:7, :7], exponential[:7, 7:]


def typed_contract(raw):
    """Normalize only declared f64 fields, retaining integer dates and all identities."""
    c=deepcopy(raw)
    def number(v):
        finite(v,-MAX_AMOUNT,MAX_AMOUNT)
        return float(v)
    for k in ['original_principal','original_payment','opening_contractual_principal','opening_contractual_interest','current_payment','current_note_rate','opening_burnout_incentive_years']:
        c[k]=number(c[k])
    for k in ['opening_probability','opening_borrower_principal','opening_borrower_interest','opening_investor_principal','opening_investor_interest','borrower_collection_fraction','loss_given_default']:
        c[k]=[number(v) for v in c[k]]
    for tail in c['complete_recovery_tails']:
        for p in tail:p['weight']=number(p['weight'])
    for p in c['path']:
        for k in ['refinancing_market_rate','turnover_hazard_per_year']:p[k]=number(p[k])
        p['credit_hazards_per_year']=[[number(v) for v in row] for row in p['credit_hazards_per_year']]
    for k in ['incentive_slope','incentive_threshold','burnout_sensitivity']:c['refinance'][k]=number(c['refinance'][k])
    c['refinance']['prepay_eligibility']=[number(v) for v in c['refinance']['prepay_eligibility']]
    if c['ownership']['kind']=='agency':c['ownership']['servicing_and_guarantee_spread']=number(c['ownership']['servicing_and_guarantee_spread'])
    else:c['ownership']['recognized_accrual_fraction']=[number(v) for v in c['ownership']['recognized_accrual_fraction']]
    if c['rate']['kind']=='arm':
        terms=c['rate']['terms']
        for k in ['margin','original_note_rate','initial_up_cap','initial_down_cap','subsequent_up_cap','subsequent_down_cap','lifetime_up_cap','lifetime_down_cap','note_floor']:terms[k]=number(terms[k])
        for reset in c['rate']['resets']:reset['input']['fixing']['rate']=number(reset['input']['fixing']['rate'])
    return c


def typed_proof(raw):
    out=deepcopy(raw);out['contract']=typed_contract(raw['contract'])
    if out['original_yield'] is not None:
        finite(out['original_yield']['annual_effective_rate'],-.99,10.)
        out['original_yield']['annual_effective_rate']=float(out['original_yield']['annual_effective_rate'])
    for opening in out['interval_openings']:
        vector(opening['gross_carrying'],3);opening['gross_carrying']=[float(v) for v in opening['gross_carrying']]
    return out


def boundaries(c):
    start = c['as_of_ordinal']; end = start+c['horizon_days']
    dates = set(c['payment_dates']) | {p['start_ordinal'] for p in c['path']} | {end}
    if c['rate']['kind'] == 'arm':
        dates |= {r['ordinal'] for r in c['rate']['resets']}
    return sorted(d for d in dates if start < d <= end)


def validate_contract(c):
    fields(c, 'schema identity monetary_unit as_of_ordinal horizon_days origination_ordinal original_principal original_payment opening_contractual_principal opening_contractual_interest current_payment current_note_rate opening_probability opening_borrower_principal opening_borrower_interest opening_investor_principal opening_investor_interest opening_burnout_incentive_years day_count rate ownership publication_cutoff_unix_seconds payment_dates borrower_collection_fraction refinance path loss_given_default complete_recovery_tails')
    fields(c['identity'], 'position_id original_loan_id cohort_id currency source_revision model_version assumption_id run_id measure')
    if c['schema'] != 'mortgage-state-1' or c['monetary_unit'] != 'currency_units' or c['identity']['model_version'] != 'mortgage-state-1' or c['identity']['measure'] not in {'physical', 'pricing'}:
        raise ArithmeticError('explicit mortgage schema/units/measure')
    for k,v in c['identity'].items():
        text(v)
    currency = c['identity']['currency']
    if len(currency) != 3 or not currency.isascii() or not currency.isupper() or not currency.isalpha():
        raise ArithmeticError('currency code')
    start = ordinal(c['as_of_ordinal']); end = ordinal(start+integer(c['horizon_days'], 1))
    origin = ordinal(c['origination_ordinal'])
    if not 0 <= start-origin <= 36600:
        raise ArithmeticError('actual retained seasoning')
    for k in ['original_principal','original_payment','opening_contractual_principal','opening_contractual_interest','current_payment']:
        finite(c[k])
    if not c['original_principal'] or not c['original_payment'] or not c['current_payment'] or c['opening_contractual_principal'] > c['original_principal']:
        raise ArithmeticError('contractual original anchors')
    finite(c['current_note_rate'], 0., 2.); finite(c['opening_burnout_incentive_years'], 0., 200.)
    for k in ['opening_borrower_principal','opening_borrower_interest','opening_investor_principal','opening_investor_interest']:
        vector(c[k],3)
    if sum(c['opening_borrower_principal']) > c['original_principal'] or any(i > b for i,b in zip(c['opening_investor_principal'], c['opening_borrower_principal'])):
        raise ArithmeticError('opening actual money domain')
    vector(c['opening_probability'],3,0.,1.)
    if sum(c['opening_probability']) > 1.+1e-11:
        raise ArithmeticError('absolute population mass')
    vector(c['borrower_collection_fraction'],3,0.,1.)
    integer(c['publication_cutoff_unix_seconds'],0,253402300799)
    if not 1 <= len(c['payment_dates']) <= 4096 or not 1 <= len(c['path']) <= 4096:
        raise ArithmeticError('bounded complete calendar')
    last = start
    for d in c['payment_dates']:
        ordinal(d)
        if d <= last or d-start > 36600:
            raise ArithmeticError('complete ordered remaining dates')
        last = d
    if last-origin > 36600:
        raise ArithmeticError('original-to-final term')
    last = start-1
    for p in c['path']:
        fields(p,'start_ordinal refinancing_market_rate turnover_hazard_per_year credit_hazards_per_year')
        d=ordinal(p['start_ordinal'])
        if not last < d <= end:
            raise ArithmeticError('ordered path dates')
        last=d
        finite(p['refinancing_market_rate'],0.,2.); finite(p['turnover_hazard_per_year'],0.,100.)
        hazard_generator(p['credit_hazards_per_year'])
        if any(row[4] != 0. for row in p['credit_hazards_per_year']):
            raise ArithmeticError('prepay sole behavioral owner')
    if c['path'][0]['start_ordinal'] != start:
        raise ArithmeticError('path start anchor')
    ref=c['refinance']
    fields(ref,'incentive_slope incentive_threshold turnover_seasoning_days refinance_seasoning_days burnout_sensitivity prepay_eligibility')
    finite(ref['incentive_slope'],0.,10000.); finite(ref['incentive_threshold'],0.,2.); finite(ref['burnout_sensitivity'],0.,100.)
    integer(ref['turnover_seasoning_days']); integer(ref['refinance_seasoning_days']); vector(ref['prepay_eligibility'],3,0.,1.)
    vector(c['loss_given_default'],3,0.,1.)
    if len(c['complete_recovery_tails']) != 3:
        raise ArithmeticError('origin recovery tails')
    for i,tail in enumerate(c['complete_recovery_tails']):
        if len(tail)>32:
            raise ArithmeticError('bounded recovery tail')
        last=-1
        for p in tail:
            fields(p,'lag_days weight'); integer(p['lag_days']); finite(p['weight'],0.,1.)
            if p['lag_days']<=last:
                raise ArithmeticError('ordered recovery lag')
            last=p['lag_days']
        if tail and abs(sum(p['weight'] for p in tail)-1.)>8*np.finfo(float).eps or not tail and c['loss_given_default'][i]!=1.:
            raise ArithmeticError('complete unit recovery weights')
    owner=c['ownership']; agency=owner['kind']=='agency'
    if agency:
        fields(owner,'kind issuer guarantee remittance_lag_days default_buyout_lag_days servicing_and_guarantee_spread default_coupon_policy')
        text(owner['issuer']); identity(owner['guarantee']); integer(owner['default_buyout_lag_days']); finite(owner['servicing_and_guarantee_spread'],0.,2.)
        if owner['default_coupon_policy'] != 'frozen_at_default':
            raise ArithmeticError('agency frozen coupon')
    else:
        fields(owner,'kind remittance_lag_days recognized_accrual_fraction')
        if owner['kind']!='whole_loan' or c['opening_investor_principal'] != c['opening_borrower_principal']:
            raise ArithmeticError('whole loan ownership')
        vector(owner['recognized_accrual_fraction'],3,0.,1.)
    integer(owner['remittance_lag_days'])
    finite(c['current_note_rate']-(owner['servicing_and_guarantee_spread'] if agency else 0.),0.,2.)
    tau(start,end,c['day_count'])
    rate=c['rate']
    if rate['kind']=='fixed':
        fields(rate,'kind')
        if c['current_payment']!=c['original_payment']:
            raise ArithmeticError('fixed original payment')
    elif rate['kind']=='arm':
        fields(rate,'kind terms resets'); t=rate['terms']
        fields(t,'index reference_calendar margin original_note_rate initial_up_cap initial_down_cap subsequent_up_cap subsequent_down_cap lifetime_up_cap lifetime_down_cap note_floor prior_reset_count payment_recast')
        identity(t['reference_calendar']); fields(t['index'],'index_id kind tenor_months average_calendar_days'); text(t['index']['index_id'])
        idx=t['index']
        if idx['kind']=='term':
            integer(idx['tenor_months'],1,120)
            if idx['average_calendar_days'] is not None: raise ArithmeticError('term descriptor')
        elif idx['kind']=='published_average':
            integer(idx['average_calendar_days'],1,365)
            if idx['tenor_months'] is not None: raise ArithmeticError('average descriptor')
        else: raise ArithmeticError('supplied ARM index required')
        for k in ['margin','original_note_rate','initial_up_cap','initial_down_cap','subsequent_up_cap','subsequent_down_cap','lifetime_up_cap','lifetime_down_cap','note_floor']:
            finite(t[k],0.,2.)
        integer(t['prior_reset_count'],0,4096)
        if t['payment_recast'] not in {'retain_original','reamortize_remaining'} or len(rate['resets'])>4096:
            raise ArithmeticError('ARM recast/reset domain')
        lo=max(t['original_note_rate']-t['lifetime_down_cap'],t['note_floor']); hi=t['original_note_rate']+t['lifetime_up_cap']
        if lo>hi or hi>2. or not lo<=c['current_note_rate']<=hi: raise ArithmeticError('ARM lifetime cap')
        last=start;ids=set()
        for r in rate['resets']:
            fields(r,'ordinal input'); d=ordinal(r['ordinal'])
            if not last<d<=c['payment_dates'][-1]: raise ArithmeticError('reset calendar')
            last=d; inp=r['input']; f=inp['fixing']
            fields(f,'fixing_id revision index_id value_ordinal publication_unix_seconds rate')
            if inp['kind']=='observed':
                fields(inp,'kind fixing reset_cutoff_unix_seconds'); available=f['publication_unix_seconds']
            elif inp['kind']=='projected':
                fields(inp,'kind fixing projection_source available_unix_seconds reset_cutoff_unix_seconds'); identity(inp['projection_source']); available=inp['available_unix_seconds']
            else: raise ArithmeticError('explicit fixing kind')
            for k in ['fixing_id','revision']: text(f[k])
            cutoff=inp['reset_cutoff_unix_seconds']; integer(cutoff,0,253402300799); integer(available,0,253402300799); integer(f['publication_unix_seconds'],0,c['publication_cutoff_unix_seconds']); ordinal(f['value_ordinal']); finite(f['rate'],-1.,2.)
            if f['fixing_id'] in ids or f['index_id']!=idx['index_id'] or f['value_ordinal']>d or available>min(c['publication_cutoff_unix_seconds'],cutoff) or cutoff//86400+719163>d or c['publication_cutoff_unix_seconds']//86400+719163>start:
                raise ArithmeticError('fixing identity/availability leakage')
            ids.add(f['fixing_id'])
    else: raise ArithmeticError('mortgage rate kind')
    if len(boundaries(c))>4096: raise ArithmeticError('aggregate calendar')


def project(c, check=lambda:False):
    """Raw path generator, including interval-end approximation and full cash tails."""
    c=typed_contract(c);validate_contract(c); checkpoint(check)
    start=c['as_of_ordinal']; end=start+c['horizon_days']; owner=c['ownership']; agency=owner['kind']=='agency'
    prob=np.array(c['opening_probability']+[0.]*4); b=np.array(c['opening_borrower_principal'],float); ba=np.array(c['opening_borrower_interest'],float)
    p=np.array(c['opening_investor_principal'],float); a=np.array(c['opening_investor_interest'],float)
    note=c['current_note_rate']; payment=c['current_payment']; contractual=c['opening_contractual_principal']; ci=c['opening_contractual_interest']; burnout=c['opening_burnout_incentive_years']; matured=0.; previous=start
    reset_count=0 if c['rate']['kind']=='fixed' else c['rate']['terms']['prior_reset_count']
    resets={} if c['rate']['kind']=='fixed' else {r['ordinal']:r for r in c['rate']['resets']}
    out={k:deepcopy(c[k]) for k in ['schema','identity','ownership','monetary_unit','original_principal','original_payment','origination_ordinal']}
    out.update({k:[] for k in ['borrower_cashflows','cashflows','entitlements','recognition','points','defaults','resets']})
    totals=['borrower_scheduled_principal','borrower_turnover_principal','borrower_refinance_principal','investor_scheduled_principal','investor_turnover_principal','investor_refinance_principal','guarantee_principal_advance','guarantee_interest_advance']
    out.update({k:0. for k in totals}); out.update(calendar_mapping_verified=False,work_units=0)
    intervals=[]
    def cash(rows,day,kind,principal,interest):
        for v in [principal,interest,principal+interest]: finite(v)
        ordinal(day)
        if principal or interest: rows.append(dict(event_id=f'{kind}-{len(rows)}',ordinal=day,kind=kind,principal=float(principal),interest=float(interest),fees=0.))
    def entitlement(day,kind,principal,interest):
        ordinal(day);ordinal(day+owner['remittance_lag_days'])
        for v in [principal,interest,principal+interest]: finite(v)
        rows=out['entitlements']
        if principal or interest: rows.append(dict(event_id=f'{kind}-{len(rows)}',entitlement_ordinal=day,remittance_ordinal=day+owner['remittance_lag_days'],kind=kind,principal=float(principal),interest=float(interest)))
    def recognition(prefix,left,right,value):
        if value: out['recognition'].append(dict(origin_id=f'{prefix}-{len(out["recognition"])}',start_ordinal=left,end_ordinal=right,interest=float(value)))
    def buyout(day,principal,interest,coupon):
        if not agency:return None
        final=day+owner['default_buyout_lag_days'];ordinal(final);since=day
        if day in c['payment_dates'] and final>day:
            entitlement(day,'agency_default_guaranteed_coupon',0.,interest);interest=0.
        for due in [d for d in c['payment_dates'] if day<d<final]:
            out['work_units']+=1; interest+=principal*coupon*tau(since,due,c['day_count'])
            entitlement(due,'agency_default_guaranteed_coupon',0.,interest)
            if since<end:recognition('agency-default-earned',since,min(due,end),principal*coupon*tau(since,min(due,end),c['day_count']))
            interest=0.;since=due
        interest+=principal*coupon*tau(since,final,c['day_count'])
        if since<end and final>since:recognition('agency-default-earned',since,min(final,end),principal*coupon*tau(since,min(final,end),c['day_count']))
        entitlement(final,'agency_default_buyout',principal,interest)
        return final
    for day in boundaries(c):
        checkpoint(check); out['work_units']+=2+len(out['defaults'])
        path=max((x for x in c['path'] if x['start_ordinal']<=previous),key=lambda x:x['start_ordinal'])
        years=(day-previous)/365.; fraction=tau(previous,day,c['day_count']); ci+=contractual*note*fraction;finite(ci)
        age=previous-c['origination_ordinal'];ref=c['refinance'];incentive=max(note-path['refinancing_market_rate']-ref['incentive_threshold'],0.)
        ramp=lambda n: min(age/n,1.) if n else 1.
        turnover=path['turnover_hazard_per_year']*ramp(ref['turnover_seasoning_days'])
        refinancing=ref['incentive_slope']*incentive*ramp(ref['refinance_seasoning_days'])*math.exp(-ref['burnout_sensitivity']*burnout)
        finite(refinancing,0.,100.)
        h=deepcopy(path['credit_hazards_per_year'])
        for i in range(3):h[i][4]=(turnover+refinancing)*ref['prepay_eligibility'][i]
        q=hazard_generator(h);coupon=note-(owner['servicing_and_guarantee_spread'] if agency else 0.);finite(coupon,0.,2.)
        factors=[1.]*3 if agency else owner['recognized_accrual_fraction']
        lr=[note*fraction/years]*3;ir=[coupon*f*fraction/years for f in factors]
        t,lf=coefficients(q,lr,years);it,iff=coefficients(q,ir,years)
        live=np.array(list(prob[:3])+[0.]*4)
        op=lambda rates:dict(schema='credit-operator-1',measure=c['identity']['measure'],duration_years=years,transition_hazards_per_year=h,accrual_rates=rates)
        intervals.append(dict(schema='mortgage-loop-interval-1',financial_source_sha256=SOURCE_SHA,identity=deepcopy(c['identity']),ownership=deepcopy(owner),interval_index=len(intervals),start_ordinal=previous,end_ordinal=day,path_start_ordinal=path['start_ordinal'],duration_years_act365f=years,contractual_day_fraction=fraction,note_rate_before_boundary=note,investor_coupon_before_boundary=coupon,payment_before_boundary=payment,reset_count_before_boundary=reset_count,burnout_before_interval=burnout,loan_operator_request=op(lr),investor_operator_request=op(ir),loan_transition=t.tolist(),loan_earned_interest=lf.tolist(),investor_transition=it.tolist(),investor_earned_interest=iff.tolist(),opening_probability=live.tolist(),opening_borrower_principal=b.tolist(),opening_borrower_interest=ba.tolist(),opening_investor_principal=p.tolist(),opening_investor_interest=a.tolist(),default_claim_start_index=len(out['defaults']),payment_is_boundary=day in c['payment_dates'],arm_reset_is_boundary=day in resets,event_order=ORDER))
        bp=np.r_[b,[0.]*4]; bi=np.r_[ba,[0.]*4]; ip=np.r_[p,[0.]*4]; ia=np.r_[a,[0.]*4]
        lp=bp@t;la=bi@t+bp@lf;owned=ip@it;oi=ia@it+ip@iff
        recognition('mortgage-earned',previous,day,float(sum(ip@iff)))
        newprob=live@t; prob[:3]=newprob[:3];prob[3:]+=newprob[3:]
        b=lp[:3].copy();ba=la[:3].copy();p=owned[:3].copy();a=oi[:3].copy()
        for i in range(3):
            j=i+3
            if lp[j]==la[j]==owned[j]==oi[j]==0.:continue
            gross=lp[j];loss=gross*c['loss_given_default'][i];tail=[];out['work_units']+=len(c['complete_recovery_tails'][i])+1
            for w in c['complete_recovery_tails'][i]:
                due=day+w['lag_days'];amount=(gross-loss)*w['weight'];tail.append(dict(ordinal=due,principal=float(amount)))
                cash(out['borrower_cashflows'],due,'borrower_default_recovery',amount,0.)
                if not agency:entitlement(due,'whole_loan_default_recovery',amount,0.)
            bought=buyout(day,owned[j],oi[j],coupon)
            out['defaults'].append(dict(claim_id=f'mortgage-default-{len(out["defaults"])}',origin=i,default_ordinal=day,borrower_principal=float(gross),borrower_contract_interest=float(la[j]),investor_principal=float(owned[j]),investor_accrued_interest=float(oi[j]),borrower_principal_loss=float(loss),complete_recovery_tail=tail,investor_buyout_ordinal=bought))
        if turnover+refinancing>0.:
            for kind,part in [('turnover',turnover/(turnover+refinancing)),('refinance',refinancing/(turnover+refinancing))]:
                lb=lp[6]*part;li=la[6]*part;ib=owned[6]*part;ii=oi[6]*part if agency else li
                cash(out['borrower_cashflows'],day,kind,lb,li);entitlement(day,kind,ib,ii)
                if not agency:recognition('nonaccrual-payoff',day,day,li-oi[6]*part)
                out[f'borrower_{kind}_principal']+=float(lb);out[f'investor_{kind}_principal']+=float(ib)
        burnout+=incentive*years;finite(burnout,0.,200.)
        if day in c['payment_dates']:
            if payment+1e-9<ci:raise ArithmeticError('unsupported negative amortization')
            final=day==c['payment_dates'][-1];scheduled=contractual if final else min(payment-ci,contractual);contractual-=scheduled
            for i in range(3):
                available=(b[i]+ba[i])*c['borrower_collection_fraction'][i] if final else payment*prob[i]*c['borrower_collection_fraction'][i]
                actual_i=min(available,ba[i]);actual_p=min(available-actual_i,b[i]);b[i]-=actual_p;ba[i]-=actual_i
                cash(out['borrower_cashflows'],day,'scheduled_borrower_collection',actual_p,actual_i)
                owner_p=(p[i] if final else min(scheduled*prob[i],p[i])) if agency else actual_p;owner_i=a[i] if agency else actual_i
                if agency:out['guarantee_principal_advance']+=float(max(owner_p-actual_p,0.));out['guarantee_interest_advance']+=float(max(a[i]-actual_i,0.))
                recognized=min(owner_i,a[i]);p[i]-=owner_p;a[i]-=recognized
                entitlement(day,'scheduled_investor_entitlement',owner_p,owner_i)
                if not agency and owner_i>recognized:recognition('nonaccrual-collection',day,day,owner_i-recognized)
                out['borrower_scheduled_principal']+=float(actual_p);out['investor_scheduled_principal']+=float(owner_p)
                if final and b[i]==ba[i]==p[i]==a[i]==0.:matured+=float(prob[i]);prob[i]=0.
            ci=0.
        if day in resets:
            reset=resets[day];terms=c['rate']['terms'];fix=reset['input']['fixing'];old=note
            up=terms['initial_up_cap'] if not reset_count else terms['subsequent_up_cap'];down=terms['initial_down_cap'] if not reset_count else terms['subsequent_down_cap']
            lo=max(terms['original_note_rate']-terms['lifetime_down_cap'],terms['note_floor'],note-down);hi=min(terms['original_note_rate']+terms['lifetime_up_cap'],note+up)
            if lo>hi:raise ArithmeticError('nonintersecting reset caps')
            note=min(max(fix['rate']+terms['margin'],lo),hi);finite(note-(owner['servicing_and_guarantee_spread'] if agency else 0.),0.,2.);reset_count+=1
            if terms['payment_recast']=='retain_original':payment=c['original_payment']
            elif contractual>0.:
                remaining=[d for d in c['payment_dates'] if d>day]
                if not remaining:raise ArithmeticError('complete recast tail required')
                dates=[day]+remaining;fractions=[tau(l,r,c['day_count']) for l,r in zip(dates,dates[1:])]
                discounts=np.cumprod([1/(1+note*f) for f in fractions])
                payment=(contractual+ci/(1+note*fractions[0]))/float(sum(discounts))
            out['resets'].append(dict(ordinal=day,previous_rate=old,selected_index_rate=fix['rate'],new_rate=note,new_payment=payment,fixing_id=fix['fixing_id'],fixing_revision=fix['revision'],projected=reset['input']['kind']=='projected'))
        borrower_default=sum((d['borrower_principal']-sum((w['principal'] for w in d['complete_recovery_tail'] if w['ordinal']<=day),0.) for d in out['defaults']),0.)
        investor_default=sum((d['investor_principal'] for d in out['defaults'] if d['investor_buyout_ordinal']>day),0.) if agency else borrower_default
        for v in list(b)+list(ba)+list(p)+list(a):finite(float(v))
        out['points'].append(dict(ordinal=day,seasoning_days=day-c['origination_ordinal'],burnout_incentive_years=burnout,note_rate=note,current_payment=payment,contractual_principal=contractual,probability=prob.tolist(),matured_probability=matured,borrower_principal=b.tolist(),borrower_interest=ba.tolist(),investor_principal=p.tolist(),investor_interest=a.tolist(),turnover_hazard_per_year=turnover,refinance_hazard_per_year=refinancing,borrower_default_principal=borrower_default,investor_default_principal=investor_default));previous=day
    out['borrower_cashflows'].sort(key=lambda r:(r['ordinal'],r['event_id']));out['entitlements'].sort(key=lambda r:(r['entitlement_ordinal'],r['event_id']))
    for e in out['entitlements']:cash(out['cashflows'],e['remittance_ordinal'],e['kind'],e['principal'],e['interest'])
    out['cashflows'].sort(key=lambda r:(r['ordinal'],r['event_id']))
    out['borrower_principal_conservation_error']=sum(c['opening_borrower_principal'])-sum(e['principal'] for e in out['borrower_cashflows'] if e['ordinal']<=end)-sum(b)-borrower_default
    out['investor_principal_conservation_error']=sum(c['opening_investor_principal'])-sum(e['principal'] for e in out['entitlements'] if e['entitlement_ordinal']<=end)-sum(p)-investor_default
    for k in ['borrower_principal_conservation_error','investor_principal_conservation_error']:
        if abs(out[k])>1e-8:raise ArithmeticError('principal conservation')
    if out['work_units']>2_000_000:raise ArithmeticError('source work bound')
    return dict(raw_request=deepcopy(c),native_projection=out,intervals=intervals,production_adoption=False,installed_independent_verification=False)


def carry_operator(r, check=lambda:False):
    fields(r,'schema generator_source measure duration_years generator recognized_contractual_rates original_annual_effective_rate policy');identity(r['generator_source'])
    fields(r['policy'],'identity original_income remittance_float');identity(r['policy']['identity'])
    if r['schema']!='mortgage-coupled-carry-1' or r['measure'] not in {'physical','pricing'} or r['policy']['original_income']!='stop_at_exit-1' or r['policy']['remittance_float']!='external_explicit_owner_required-1':raise ArithmeticError('explicit carry policies')
    duration=finite(r['duration_years'],0.,1080/365); y=finite(r['original_annual_effective_rate'],-.99,10.);vector(r['recognized_contractual_rates'],3,-100.,100.)
    q=np.asarray(r['generator'],dtype=float)
    if q.shape!=(7,7) or not np.isfinite(q).all():raise ArithmeticError('generator shape/finite')
    hazards=[]
    for i,row in enumerate(q):
        if i>=3:
            if np.any(row!=0.):raise ArithmeticError('absorbing generator rows')
        else:
            h=[float(row[j]) if j!=i else 0. for j in range(3)]+[float(row[3+i]),float(row[6])];hazards.append(h)
            if row[i]!=-sum(h) or any(row[j]!=0. for j in range(3,6) if j!=3+i):raise ArithmeticError('default origins/diagonal')
    hazard_generator(hazards);checkpoint(check)
    t,f=coefficients(q,r['recognized_contractual_rates'],duration);k=math.log1p(y);live=np.diag([1.]*3+[0.]*4)
    coupled=expm(duration*np.block([[q+k*live,abs(k)*live],[np.zeros_like(q),q]]))
    return dict(request=deepcopy(r),base=dict(schema='credit-operator-1',measure=r['measure'],duration_years=duration,transition=t.tolist(),earned_interest=f.tolist()),growth=coupled[:7,:7].tolist(),absolute_reward=coupled[:7,7:].tolist(),reward_sign=math.copysign(1.,k),growth_bound=math.exp(max(k,0.)*duration))


def carry_apply(r,s,check=lambda:False):
    op=carry_operator(r,check);fields(s,'measure monetary_unit probability principal accrued_interest gross_carrying earned_eir')
    if s['measure']!=r['measure'] or s['monetary_unit']!='currency_units':raise ArithmeticError('carry state identity')
    vector(s['probability'],7,0.,1.)
    if sum(s['probability'])>1.+1e-11:raise ArithmeticError('absolute probability mass')
    for k in ['principal','accrued_interest','earned_eir']:vector(s[k],7,-MAX_AMOUNT,MAX_AMOUNT)
    vector(s['gross_carrying'],7)
    vector([g-p-a for g,p,a in zip(s['gross_carrying'],s['principal'],s['accrued_interest'])],7,-MAX_AMOUNT,MAX_AMOUNT)
    if sum(s['gross_carrying'])*op['growth_bound']>MAX_AMOUNT:raise ArithmeticError('growth monetary preflight')
    if r['duration_years']==0.:
        return dict(closing=deepcopy(s),book_adjustment=[g-p-a for g,p,a in zip(s['gross_carrying'],s['principal'],s['accrued_interest'])],interval_earned_eir=[0.]*7,total_interval_earned_eir=0.,gross_conservation_error=0.,native_mortgage_generator_verified=False)
    t=np.array(op['base']['transition']);f=np.array(op['base']['earned_interest']);g=np.array(s['gross_carrying'])@np.array(op['growth']);reward=np.array(s['gross_carrying'])@np.array(op['absolute_reward'])*op['reward_sign']
    p=np.array(s['principal'])@t;a=np.array(s['accrued_interest'])@t+np.array(s['principal'])@f
    earned=np.array(s['earned_eir'])@t+reward;closing=dict(measure=s['measure'],monetary_unit=s['monetary_unit'],probability=(np.array(s['probability'])@t).tolist(),principal=p.tolist(),accrued_interest=a.tolist(),gross_carrying=g.tolist(),earned_eir=earned.tolist())
    for v in [g,reward,earned,g-p-a]:vector(v.tolist(),7,-MAX_AMOUNT,MAX_AMOUNT)
    return dict(closing=closing,book_adjustment=(g-p-a).tolist(),interval_earned_eir=reward.tolist(),total_interval_earned_eir=float(sum(reward)),gross_conservation_error=float(sum(g)-sum(s['gross_carrying'])-sum(reward)),native_mortgage_generator_verified=False)


def verify_operator(raw,actual,check=lambda:False):
    expected=carry_operator(raw,check)
    if set(actual)!=set(expected)|{'work_multiplications'}:raise ArithmeticError('complete immutable operator fields')
    subset={k:actual[k] for k in expected};equal(subset,expected,'operator')
    n=actual.get('work_multiplications');integer(n,0,140)
    if (raw['original_annual_effective_rate']==0. or raw['duration_years']==0.) and n!=0:raise ArithmeticError('zero work coefficient identity')


def carry_request(c,interval,y):
    q=hazard_generator(interval['investor_operator_request']['transition_hazards_per_year'])
    return dict(schema='mortgage-coupled-carry-1',generator_source=dict(id=f'{c["identity"]["position_id"]}:{c["identity"]["run_id"]}:interval:{interval["interval_index"]}',revision=c['identity']['source_revision']),measure=c['identity']['measure'],duration_years=interval['duration_years_act365f'],generator=q.tolist(),recognized_contractual_rates=interval['investor_operator_request']['accrual_rates'],original_annual_effective_rate=y['annual_effective_rate'],policy=deepcopy(y['policy']))


def serialized_size(v):
    """UTF-8 compact JSON capacity, with shortest-float exponent spelling.

    This checks retained-byte admission, not a cryptographic canonical hash.
    Financial fields are normalized to their declared f64 input types first.
    """
    if isinstance(v,dict):return 2+max(0,len(v)-1)+sum(serialized_size(k)+1+serialized_size(x) for k,x in v.items())
    if isinstance(v,list):return 2+max(0,len(v)-1)+sum(serialized_size(x) for x in v)
    if isinstance(v,str):return len(json.dumps(v,ensure_ascii=False).encode())
    if type(v) is float:
        if not math.isfinite(v):raise ArithmeticError('finite JSON values required')
        s=repr(v)
        if 'e' in s:
            mantissa,exponent=s.split('e');n=int(exponent)
            s=mantissa+'e'+('-' if n<0 else '+')+str(abs(n))
        return len(s)
    return len(json.dumps(v,separators=(',',':'),allow_nan=False).encode())


def admit(raw):
    fields(raw,'schema contract original_yield interval_openings')
    if raw['schema']!='mortgage-interval-proof-draft-1' or len(raw['interval_openings'])>16:raise ArithmeticError('typed proof schema/cardinality')
    raw=typed_proof(raw);c=raw['contract'];validate_contract(c);count=len(boundaries(c));size=serialized_size(raw)
    if size>1024*1024:raise ArithmeticError('raw proof bytes')
    metadata=serialized_size([c['identity'],c['ownership']])
    if count>4096 or count*(metadata+8192)>64*1024*1024:raise ArithmeticError('receipt capacity')
    y=raw['original_yield']
    if y is None:
        if raw['interval_openings']:raise ArithmeticError('missing original yield')
    else:
        fields(y,'identity income_review annual_effective_rate policy');identity(y['identity']);identity(y['income_review']);finite(y['annual_effective_rate'],-.99,10.)
        fields(y['policy'],'identity original_income remittance_float');identity(y['policy']['identity'])
        if c['identity']['measure']!='physical' or c['ownership']['kind']!='whole_loan' or y['policy']['original_income']!='stop_at_exit-1' or y['policy']['remittance_float']!='external_explicit_owner_required-1':raise ArithmeticError('reviewed physical whole-loan income policy')
    seen=set();dates=[c['as_of_ordinal']]+boundaries(c)
    for opening in raw['interval_openings']:
        fields(opening,'interval_index carry_anchor gross_carrying');identity(opening['carry_anchor']);vector(opening['gross_carrying'],3);i=integer(opening['interval_index'],0,count-1)
        if i in seen:raise ArithmeticError('duplicate interval anchor')
        seen.add(i);text(f'{c["identity"]["position_id"]}:{c["identity"]["run_id"]}:interval:{i}')
        t=(dates[i+1]-dates[i])/365.
        if t>1080/365. or sum(opening['gross_carrying'])*math.exp(max(math.log1p(y['annual_effective_rate']),0.)*t)>MAX_AMOUNT:raise ArithmeticError('interval/growth preflight')
    rows=count*count*16+count*256
    retained=rows*(size+4096)+(count+len(raw['interval_openings']))*(size+32768)
    work=2_000_000+len(raw['interval_openings'])*140*14**3+count*1024
    if retained>64*1024*1024 or work>8_000_000:raise ArithmeticError('aggregate proof byte/work preflight')
    return retained,work


def verify_receipt_carry(raw,actual,check=lambda:False):
    """Descriptor-only fixture check; this alone does not verify mortgage generation."""
    receipt=raw['receipt']
    if receipt['schema']!='mortgage-loop-interval-1' or receipt['financial_source_sha256']!=SOURCE_SHA:raise ArithmeticError('receipt source identity')
    loan=receipt['loan_operator_request'];owner=receipt['investor_operator_request']
    equal(loan['transition_hazards_per_year'],owner['transition_hazards_per_year'])
    for name,r in [('loan',loan),('investor',owner)]:
        equal(r['duration_years'],receipt['duration_years_act365f']);q=hazard_generator(r['transition_hazards_per_year']);t,f=coefficients(q,r['accrual_rates'],r['duration_years']);equal(receipt[name+'_transition'],t.tolist());equal(receipt[name+'_earned_interest'],f.tolist())
    c={'identity':receipt['identity']};y=dict(annual_effective_rate=raw['original_annual_effective_rate'],policy=raw['policy'])
    expected=carry_request(c,receipt,y);carry_operator(expected,check);equal(actual,expected)


def verify(raw,result,check=lambda:False):
    retained,work=admit(raw)
    raw=typed_proof(raw)
    fields(result,'schema raw_request source proofs admitted_bytes admitted_work same_loop_instrumentation_parity sequential_carry_adoption original_quote_reconciled remittance_float_accounting_adopted principal_recovery_owner production_adoption installed_independent_verification')
    c=raw['contract'];source=project(c,check);equal(result['raw_request'],raw,'raw');equal(result['source'],source,'source')
    if result['schema']!=raw['schema']:raise ArithmeticError('proof schema')
    y=raw['original_yield'];indices=set()
    if y is None:
        if raw['interval_openings']:raise ArithmeticError('missing reviewed original yield')
    else:
        fields(y,'identity income_review annual_effective_rate policy');identity(y['identity']);identity(y['income_review']);finite(y['annual_effective_rate'],-.99,10.)
        if c['identity']['measure']!='physical' or c['ownership']['kind']!='whole_loan':raise ArithmeticError('carry proof physical whole loan only')
    if len(result['proofs'])!=len(raw['interval_openings']):raise ArithmeticError('complete proof coverage')
    for opening,actual in zip(raw['interval_openings'],result['proofs']):
        checkpoint(check);fields(actual,'interval_index carry_anchor original_yield raw_carry_request opening operator result');fields(opening,'interval_index carry_anchor gross_carrying');identity(opening['carry_anchor']);vector(opening['gross_carrying'],3);i=integer(opening['interval_index'],0,len(source['intervals'])-1)
        if i in indices:raise ArithmeticError('duplicate interval');
        indices.add(i);interval=source['intervals'][i];request=carry_request(c,interval,y)
        s=dict(measure=c['identity']['measure'],monetary_unit=c['monetary_unit'],probability=interval['opening_probability'],principal=interval['opening_investor_principal']+[0.]*4,accrued_interest=interval['opening_investor_interest']+[0.]*4,gross_carrying=opening['gross_carrying']+[0.]*4,earned_eir=[0.]*7)
        equal(actual['interval_index'],i);equal(actual['carry_anchor'],opening['carry_anchor']);equal(actual['original_yield'],y);equal(actual['raw_carry_request'],request);equal(actual['opening'],s);verify_operator(request,actual['operator'],check);equal(actual['result'],carry_apply(request,s,check),'carry')
        # Default slices have native economic ownership and exactly one monetary transport.
        defaults=[d for d in source['native_projection']['defaults'] if d['default_ordinal']==interval['end_ordinal']]
        for d in defaults:
            j=3+d['origin'];equal(actual['result']['closing']['principal'][j],d['investor_principal']);equal(actual['result']['closing']['accrued_interest'][j],d['investor_accrued_interest'])
    integer(result['admitted_bytes'],1,64*1024*1024);integer(result['admitted_work'],1,8_000_000)
    equal(result['admitted_work'],work);equal(result['admitted_bytes'],retained)
    flags=dict(same_loop_instrumentation_parity=True,sequential_carry_adoption=False,original_quote_reconciled=False,remittance_float_accounting_adopted=False,principal_recovery_owner='native_mortgage_projection_only-1',production_adoption=False,installed_independent_verification=False)
    for k,v in flags.items():equal(result[k],v,k)
    return dict(verified=True,raw_mortgage_source_recomputed=True,interval_generator_recomputed=True,coupled_expm_recomputed=True,production_adoption=False,empirical_validation=False)
