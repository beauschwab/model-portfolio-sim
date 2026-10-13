"""Independent raw histories, original-yield recurrence and gross rights proof.

Artifact reference only. This module never invokes a native callback.
"""
from copy import deepcopy
import json,math,struct,sys
from decimal import Decimal

SCHEMA='deposit-performing-continuation-2'
POLICIES=dict(expectation_policy='later_date_original_yield_catchup-1',pnl_policy='liability_revision_separate_pnl-1',phase_policy='all_date_recognition_settlement_then_revision-1',tail_policy='complete_original_and_revised_tail-1')
INTEGERS={'purchase_ordinal','through_ordinal','revision_ordinal','as_of_ordinal','ordinal','horizon_days','maturity_ordinal','interest_payment_ordinals','observation_lag_days'}
def near(a,b):
    if type(a) not in (int,float) or type(b) not in (int,float) or not math.isfinite(a) or not math.isfinite(b) or abs(a-b)>max(1e-9,1e-12*max(abs(a),abs(b))):raise ArithmeticError(f'unchanged finite amount gate: {a!r} != {b!r}')
def exact(a,b):
    if type(a)!=type(b):raise ArithmeticError('exact metadata type')
    if type(b) is float:
        if struct.pack('>d',a)!=struct.pack('>d',b):raise ArithmeticError('exact binary64 identity')
    elif type(b) is dict:
        if a.keys()!=b.keys():raise ArithmeticError('exact object fields')
        for k,v in b.items():exact(a[k],v)
    elif type(b) is list:
        if len(a)!=len(b):raise ArithmeticError('exact ordered array')
        for x,y in zip(a,b):exact(x,y)
    elif a!=b:raise ArithmeticError('exact metadata value')
def equal(a,b,path='continuation'):
    if type(b) is dict:
        if type(a) is not dict or a.keys()!=b.keys():raise ArithmeticError(path+': fields')
        for k,v in b.items():equal(a[k],v,path+'/'+k)
    elif type(b) is list:
        if type(a) is not list or len(a)!=len(b):raise ArithmeticError(path+': array')
        for i,(x,y) in enumerate(zip(a,b)):equal(x,y,path+'/'+str(i))
    elif type(b) is float:near(a,b)
    else:
        try:exact(a,b)
        except ArithmeticError as error:raise ArithmeticError(path+": "+str(error)+f" ({a!r} / {b!r})") from error
def typed(v,key=''):
    if type(v) is dict:return {k:typed(x,k) for k,x in v.items()}
    if type(v) is list:return [typed(x,key) for x in v]
    if type(v) is int and key not in INTEGERS:return float(v)
    return v
def native_json_length(v):
    """Metadata-only serde binary64 spelling; never a financial callback."""
    if type(v) is dict:return 2+max(0,len(v)-1)+sum(len(json.dumps(k,ensure_ascii=False).encode())+1+native_json_length(x) for k,x in v.items())
    if type(v) is list:return 2+max(0,len(v)-1)+sum(native_json_length(x) for x in v)
    if type(v) is float:
        if not math.isfinite(v):raise ArithmeticError('nonfinite serialization')
        text=repr(v)
        if 'e' in text:
            if 1e-5<=abs(v)<1e16:text=format(Decimal(text),'f')
            else:
                significand,exponent=text.split('e');text=significand+'e'+str(int(exponent))
        return len(text)
    return len(json.dumps(v,ensure_ascii=False).encode())
def money(v):
    if type(v) not in (int,float) or not math.isfinite(v) or abs(v)>10000.:raise ArithmeticError('component monetary domain')
def keys(v,names):
    if type(v) is not dict or v.keys()!=set(names.split()):raise ArithmeticError('unknown/missing fields')
def tick(check):
    if check():raise InterruptedError('continuation reference canceled')
def identity(v):
    keys(v,'id revision')
    for s in v.values():
        if type(s) is not str or not s.strip() or len(s.encode())>256 or any(ord(c)<32 for c in s):raise ArithmeticError('bounded identity')
def source_domain(s):
    keys(s,'schema identity as_of_ordinal horizon_days day_count segments migrations drivers terminal')
    if s['schema']!='deposit-state-1' or s['identity']['measure']!='physical' or type(s['horizon_days']) is not int or not 1<=s['horizon_days']<=365 or len(s['segments'])!=1 or s['migrations'] or s['terminal']!={'kind':'retain_residual'}:raise ArithmeticError('explicit narrow source domain')
    p=s['segments'][0];start=s['as_of_ordinal'];end=start+s['horizon_days']
    if type(start) is not int or type(p['maturity_ordinal']) is not int or not start<p['maturity_ordinal']<=end:raise ArithmeticError('complete actual maturity')
    if p['opening_accrued_interest']!=0. or p['opening_balance']<=0. or p['opening_paid_rate']<0. or p['repricing']['floor']<0. or p['repricing']['ceiling']>1.:raise ArithmeticError('narrow original opening')
    for v in [p['opening_balance'],p['opening_accrued_interest']]:money(v)
    lag=p['repricing']['observation_lag_days'];first=start-lag-1
    if type(lag) is not int or not 0<=lag<=64 or len(s['drivers'])!=end-first:raise ArithmeticError('complete lag calendar')
    for n,d in enumerate(s['drivers']):
        keys(d,'ordinal reference_rate competitor_rates offered_spread_stress confidence_stress external_inflows')
        if type(d['ordinal']) is not int or d['ordinal']!=first+n:raise ArithmeticError('strict source observations')
        if any(len(d[k])!=1 for k in ('competitor_rates','offered_spread_stress','confidence_stress','external_inflows')) or d['external_inflows']!=[0.]:raise ArithmeticError('inflow/dimensions')
        for v,lo,hi in [(d['reference_rate'],-.2,2.),(d['competitor_rates'][0],-.2,2.),(d['offered_spread_stress'][0],-2.,2.),(d['confidence_stress'][0],0.,100.)]:
            if type(v) not in (int,float) or not math.isfinite(v) or not lo<=v<=hi:raise ArithmeticError('driver domain')
    if s['day_count'] not in ('act360','act365_fixed'):raise ArithmeticError('day-count form')
    if p['interest_payment_ordinals']!=sorted(set(p['interest_payment_ordinals'])) or any(type(d) is not int or not start<d<=end for d in p['interest_payment_ordinals']):raise ArithmeticError('cash dates')
    def finite_numeric(v,key=''):
        if type(v) is dict:
            for k,x in v.items():finite_numeric(x,k)
        elif type(v) is list:
            for x in v:finite_numeric(x,key)
        elif type(v) is bool and key!='logistic_plateau_identified':raise ArithmeticError('boolean financial alias')
        elif type(v) in (int,float) and not math.isfinite(v):raise ArithmeticError('nonfinite source')
    finite_numeric(p)
def admit(r):
    keys(r,'schema contract anchor policy revision_ordinal prefix original current')
    if r['schema']!=SCHEMA:raise ArithmeticError('schema')
    for v in (r['contract'],r['anchor']['purchase_quote'],r['anchor']['original_yield_anchor'],r['policy']['review'],r['prefix']['identity']):identity(v)
    keys(r['policy'],'review expectation_policy pnl_policy phase_policy tail_policy changed_driver_dimensions')
    for k,v in POLICIES.items():exact(r['policy'][k],v)
    keys(r['prefix'],'identity through_ordinal drivers')
    source_domain(r['original']);source_domain(r['current'])
    a=r['anchor'];o=r['original'];c=r['current'];start=o['as_of_ordinal'];rev=r['revision_ordinal'];end=start+o['horizon_days']
    if type(rev) is not int or not start<rev<o['segments'][0]['maturity_ordinal']<=end or r['prefix']['through_ordinal']!=rev:raise ArithmeticError('revision domain')
    keys(a,'purchase_ordinal original_principal original_price_per_100 original_gross_carrying_value original_annual_effective_rate purchase_quote original_yield_anchor')
    for k in ('original_principal','original_gross_carrying_value','original_annual_effective_rate'):money(a[k])
    if type(a['original_price_per_100']) not in (int,float) or not math.isfinite(a['original_price_per_100']) or not 0<a['original_price_per_100']<=1e6:raise ArithmeticError('original purchase quote domain')
    if a['purchase_ordinal']!=start or a['original_principal']!=o['segments'][0]['opening_balance'] or not 0<=a['original_annual_effective_rate']<=10. or a['original_gross_carrying_value']<=0.:raise ArithmeticError('original anchor')
    near(a['original_gross_carrying_value'],a['original_principal']*a['original_price_per_100']/100.)
    for k in ('schema','as_of_ordinal','horizon_days','day_count','segments','migrations','terminal'):exact(o[k],c[k])
    for k in ('entity_id','cohort_id','currency','model_version'):exact(o['identity'][k],c['identity'][k])
    exact(r['prefix']['drivers'],[d for d in o['drivers'] if d['ordinal']<=rev])
    dimensions=r['policy']['changed_driver_dimensions'];allowed={'reference_rate','competitor_rates','offered_spread_stress','confidence_stress'}
    if len(dimensions)!=len(set(dimensions)) or not set(dimensions)<=allowed:raise ArithmeticError('declared dimensions')
    for old,new in zip(o['drivers'],c['drivers']):
        exact(old['ordinal'],new['ordinal']);exact(old['external_inflows'],new['external_inflows'])
        if old['ordinal']<=rev:exact(old,new)
        for k in allowed:
            if k not in dimensions:exact(old[k],new[k])
    return start,rev,end
def obligations(s):
    out={}
    near(s['closing_balance'],0.);near(s['closing_accrued_interest'],0.)
    for p in s['points']:
        for v in p['balances']+p['accrued_interest']:money(v)
    for c in s['cashflows']:
        p,i=out.setdefault(c['ordinal'],[0.,0.]);out[c['ordinal']]=[p-c['principal'],i-c['cash_interest']]
        for v in out[c['ordinal']]+[sum(out[c['ordinal']])]:money(v)
    return dict(sorted(out.items()))
def income_request(r,s,changed):
    cash=obligations(s);start=r['original']['as_of_ordinal'];end=start+r['original']['horizon_days'];earned={v['end_ordinal']:v['amount'] for v in s['interest']};events=[]
    if start in cash:raise ArithmeticError('purchase-date actual cash unsupported')
    for d in range(start+1,end+1):
        p,i=cash.get(d,(0.,0.));change=None
        if changed and d==r['revision_ordinal']:change=dict(kind='catchup_original',review=deepcopy(r['policy']['review']),complete_future_cash=[dict(payment_ordinal=j,amount=sum(v)) for j,v in cash.items() if j>d])
        events.append(dict(event_id=f'day_{d}',source=deepcopy(r['contract']),payment_ordinal=d,contractual_interest_earned=earned.get(d,0.),principal_cash=p,interest_cash=i,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=change))
    a=r['anchor']
    return dict(schema='dated-income-1',contract=deepcopy(r['contract']),original_yield_anchor=deepcopy(a['original_yield_anchor']),policy=deepcopy(r['policy']['review']),currency=r['original']['identity']['currency'],monetary_unit='currency_units',measure='physical',as_of_ordinal=start,full_contract_end_ordinal=end,opening_principal=a['original_principal'],opening_accrued_interest=0.,opening_gross_carrying_value=a['original_gross_carrying_value'],original_annual_effective_rate=a['original_annual_effective_rate'],opening_annual_effective_rate=a['original_annual_effective_rate'],events=events)
def income(request,check):
    p=request['opening_principal'];a=request['opening_accrued_interest'];c=request['opening_gross_carrying_value'];y=request['original_annual_effective_rate'];date=request['as_of_ordinal'];rows=[]
    for e in request['events']:
        tick(check)
        if e['principal_cash']<0. or e['principal_cash']>p:raise ArithmeticError('principal repayment exceeds source outstanding')
        opening=c;eir=c*math.expm1(math.log1p(y)*(e['payment_ordinal']-date)/365.);amort=eir-e['contractual_interest_earned'];cash=e['principal_cash']+e['interest_cash'];c+=eir-cash;p-=e['principal_cash'];a+=e['contractual_interest_earned']-e['interest_cash'];mod=0.;review=None
        if e['yield_change'] is not None:
            review=deepcopy(e['yield_change']['review']);pv=sum(v['amount']*math.exp(-math.log1p(y)*(v['payment_ordinal']-e['payment_ordinal'])/365.) for v in e['yield_change']['complete_future_cash']);mod=pv-c;c=pv
        bound=64.*sys.float_info.epsilon*(abs(opening)+abs(cash)+abs(eir))
        if c<0. and p==0. and abs(a)<=bound and abs(c)<=bound:c=0.
        if c<0.:raise ArithmeticError('negative gross carry')
        row=dict(event_id=e['event_id'],source=deepcopy(e['source']),payment_ordinal=e['payment_ordinal'],opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=eir,contractual_interest_earned=e['contractual_interest_earned'],basis_amortization=amort,modification_gain_loss=mod,recognized_nonintegral_fee_income=0.,cash_received=cash,closing_principal=p,closing_accrued_interest=a,closing_gross_carrying_value=c,closing_book_adjustment=c-p-a,yield_review=review)
        for k,v in row.items():
            if type(v) is float:money(v)
        rows.append(row);date=e['payment_ordinal']
    return dict(schema='dated-income-1',**{k:deepcopy(request[k]) for k in ('contract','policy','original_yield_anchor','currency','monetary_unit','measure')},rows=rows,remaining_principal=p,remaining_accrued_interest=a,remaining_gross_carrying_value=c)
def rights(s,through,check):
    origins=[dict(native_interest_index=i,start_ordinal=x['start_ordinal'],end_ordinal=x['end_ordinal'],earned=x['amount'],allocated=0.,remaining=x['amount']) for i,x in enumerate(s['interest']) if x['end_ordinal']<=through];settlements=[]
    for i,c in enumerate(s['cashflows']):
        tick(check)
        if c['ordinal']>through or c['cash_interest']==0.:continue
        full=c['kind'] in ('interest_payment','contractual_maturity');needed=-c['cash_interest'];allocations=[]
        for o in origins:
            if o['end_ordinal']>c['ordinal']:break
            amount=o['remaining'] if full else max(0.,min(needed,o['remaining']))
            if amount>0.:o['remaining']-=amount;o['allocated']+=amount;needed-=amount;allocations.append(dict(native_interest_index=o['native_interest_index'],amount=amount))
            if not full and needed<=0.:break
        allocated=sum(a['amount'] for a in allocations);near(allocated,-c['cash_interest'])
        settlements.append(dict(native_cash_index=i,native_event_id=c['event_id'],ordinal=c['ordinal'],full_native_retirement=full,actual_bank_cash_interest=c['cash_interest'],allocations=allocations,retained_cash_roundoff_right=allocated+c['cash_interest']))
    remaining=sum((o['remaining'] for o in origins),0.);residual=sum((s['retained_cash_roundoff_right'] for s in settlements),0.)
    return dict(origins=origins,settlements=settlements,remaining_origin_rights=remaining,retained_cash_roundoff_right=residual,total_right=remaining+residual)
def reconstruct(raw,check=lambda:False):
    from deposit_source_reference import project
    tick(check);r=typed(raw);start,rev,end=admit(r)
    original=project(r['original'],check);current=project(r['current'],check)
    # Empty earned arrays still emit the declared financial binary64 amount.
    for s in (original,current):s['total_interest_expense']=float(s['total_interest_expense'])
    oc=obligations(original);cc=obligations(current)
    for d in range(start,rev+1):exact(oc.get(d),cc.get(d))
    exact([i for i in original['interest'] if i['end_ordinal']<=rev],[i for i in current['interest'] if i['end_ordinal']<=rev])
    y=r['anchor']['original_annual_effective_rate']
    pv=lambda cash,at:sum(sum(v)*math.exp(-math.log1p(y)*(d-at)/365.) for d,v in cash.items() if d>at)
    original_pv=pv(oc,start);near(original_pv,r['anchor']['original_gross_carrying_value'])
    oq=income_request(r,original,False);oi=income(oq,check);cq=income_request(r,current,True);ci=income(cq,check);idx=rev-start-1;row=ci['rows'][idx]
    before=row['opening_gross_carrying_value']+row['effective_interest']-row['cash_received'];near(before,oi['rows'][idx]['closing_gross_carrying_value']);prefix=rights(current,rev,check)
    near(prefix['total_right'],row['closing_accrued_interest']);point=deepcopy(current['points'][idx+1]);indices=[i for i,c in enumerate(current['cashflows']) if c['ordinal']==rev and c['kind'] in ('withdrawal','closure')];np=point['balances'][0];na=point['accrued_interest'][0]
    for i in indices:np+=current['cashflows'][i]['principal'];na+=current['cashflows'][i]['cash_interest']
    near(np,row['closing_principal']);near(na,row['closing_accrued_interest']);lag=r['current']['segments'][0]['repricing']['observation_lag_days']
    opening=dict(native_point=point,same_date_start_cash_indices=indices,next_interval_paid_rate=current['points'][idx+2]['paid_rates'][0],lag_observations=[deepcopy(d) for d in r['current']['drivers'] if rev-lag-1<=d['ordinal']<=rev],principal_after_all_date_cash=row['closing_principal'],accrued_after_all_date_cash=row['closing_accrued_interest'],gross_carrying_before_revision=before,book_basis_before_revision=before-row['closing_principal']-row['closing_accrued_interest'],accumulated_original_eir=sum(x['effective_interest'] for x in ci['rows'][:idx+1]),actual_bank_principal_cash_to_revision=-sum((v[0] for d,v in cc.items() if d<=rev),0.),actual_bank_interest_cash_to_revision=-sum((v[1] for d,v in cc.items() if d<=rev),0.),unretired_earned_origins=prefix)
    ort=rights(original,end,check);crt=rights(current,end,check)
    for rt in (ort,crt):
        for o in rt['origins']:near(o['remaining'],0.)
        for s in rt['settlements']:near(s['retained_cash_roundoff_right'],0.)
        near(rt['total_right'],0.)
    for result in (oi,ci):
        for k in ('remaining_principal','remaining_accrued_interest','remaining_gross_carrying_value'):near(result[k],0.)
    groups=[]
    def group(n,kind,review,lines):
        values=[dict(account=k,amount=v) for k,v in lines]
        if abs(sum(x['amount'] for x in values))>1e-9:raise ArithmeticError('absolute atomic journal balance')
        groups.append(dict(ordinal=ci['rows'][n]['payment_ordinal'],kind=kind,source_income_row=n,review=deepcopy(review),lines=values))
    for n,x in enumerate(ci['rows']):
        tick(check);e=cq['events'][n]
        group(n,'original_eir_and_earned',r['contract'],[('interest_payable',-x['contractual_interest_earned']),('deposit_book_basis',-x['basis_amortization']),('pnl:deposit_original_eir_expense',x['effective_interest'])])
        group(n,'actual_cash_settlement',r['contract'],[('deposit_principal',e['principal_cash']),('interest_payable',e['interest_cash']),('cash',-x['cash_received'])])
        if x['yield_review'] is not None:group(n,'reviewed_expectation_revision',r['policy']['review'],[('deposit_book_basis',-x['modification_gain_loss']),('pnl:deposit_basis_revision',x['modification_gain_loss'])])
    return dict(schema=SCHEMA,raw_request=r,original_source=original,current_source=current,original_pv=original_pv,original_income_request=oq,original_income=oi,current_income_request=cq,current_income=ci,revision_opening=opening,remaining_expected_cash_pv=pv(cc,rev),basis_revision=row['modification_gain_loss'],original_full_rights=ort,current_full_rights=crt,journal_deltas=groups,accounting_standard_approval=False,empirical_validation=False,installed_adoption=False,saved_book_adoption=False,cohort_adoption=False,capital_ftp_eligible=False)
def verify(raw,actual,check=lambda:False):
    expected=reconstruct(raw,check)
    actual=deepcopy(actual);a=actual.pop('admission');keys(a,'raw_bytes work_units reserved_bytes full_end_ordinal prefix_days allocation_slots_reserved')
    for v in a.values():
        if type(v) is not int:raise ArithmeticError('admission integer metadata')
    days=raw['original']['horizon_days'];slots=days*(3*days+2);rb=a['raw_bytes']
    if rb!=native_json_length(typed(raw)):raise ArithmeticError('raw byte admission identity')
    if not 0<rb<=1_048_576 or a['allocation_slots_reserved']!=slots or a['work_units']!=4*4096*days+slots*48+rb*8 or a['reserved_bytes']!=(days+2)*4*32768+slots*512+rb*12 or a['work_units']>8_000_000 or a['reserved_bytes']>32*1024*1024:raise ArithmeticError('joined preflight reservation')
    exact(a['prefix_days'],raw['revision_ordinal']-raw['original']['as_of_ordinal']);exact(a['full_end_ordinal'],raw['original']['as_of_ordinal']+days)
    exact(actual['raw_request'],expected['raw_request']);equal(actual,expected)
    if len(json.dumps(actual,separators=(',',':')).encode())>a['reserved_bytes']:raise ArithmeticError('actual retained bytes')
    return dict(raw_physical_source_recomputed=True,purchase_prefix_original_eir_recomputed=True,ordered_delta_journal_recomputed=True,unweighted_gross_right_tails_recomputed=True,empirical_validation=False,installed_adoption=False,capital_ftp_eligible=False)
