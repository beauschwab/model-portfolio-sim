"""Independent acquired-mortgage source and original-carry publication verification.

Raw mortgage economics are reconstructed by the frozen independent projector.
SciPy augmented expm propagates one opening G through all actual boundaries.
No native producer arrays or financial callbacks supply economic truth.
"""
from copy import deepcopy
import hashlib
import json
import math
import numpy as np
from . import mortgage_credit_math_reference as model
from . import mortgage_credit_source_reference as sequential_source

SCHEMA='mortgage-credit-accounting-1'
POLICY=dict(original_income='stop_at_exit-1',remittance_float='reject_nonzero_remittance_lag-1',
 live_cash='actual_native_cash_reduces_carry-1',exit_basis='signed_native_exit_cash_minus_exact_carry-1',
 default_basis='dispose_signed_basis_at_default-1',default_principal_book='chargeoff_native_loss_at_default-1',
 tail='reject_unclosed_gross_rights-1')


def typed(raw):
    r=deepcopy(raw);r['contract']=model.typed_contract(r['contract']);r['original_contract']=model.typed_contract(r['original_contract']);a=r['acquisition'];i=r['policy']['interest']
    for k in ('actual_cost','original_annual_effective_rate'):a[k]=float(a[k])
    a['opening_gross_carrying']=[float(x) for x in a['opening_gross_carrying']]
    i['collection_fraction']=[float(x) for x in i['collection_fraction']]
    for tail in i['complete_collection_tails']:
        for row in tail:row['weight']=float(row['weight'])
    return r


def admit(raw,check=lambda:False):
    model.checkpoint(check);model.fields(raw,'schema contract original_contract original_economic_source pairing path_id economic_source acquisition policy')
    model.fields(raw['pairing'],'review original_run_id current_run_id policy')
    model.fields(raw['acquisition'],'identity original_yield_anchor income_review purchase_ordinal actual_cost original_annual_effective_rate opening_gross_carrying accounting_basis')
    model.fields(raw['policy'],'identity original_income remittance_float live_cash exit_basis default_basis default_principal_book default_principal_book_review economic_principal_disposal_review economic_principal_disposal_lag_days interest tail')
    r=typed(raw);checked_equal(raw,r,check,'raw-numeric-types');c=r['contract'];a=r['acquisition'];p=r['policy'];i=p['interest'];model.validate_contract(c);model.validate_contract(r['original_contract'])
    model.fields(i,'review accounting_accrued collection_fraction complete_collection_tails economic_remainder economic_disposal_review disposal_lag_days')
    for identity in (r['economic_source'],r['original_economic_source'],r['pairing']['review'],a['identity'],a['original_yield_anchor'],a['income_review'],p['identity'],p['default_principal_book_review'],p['economic_principal_disposal_review'],i['review'],i['economic_disposal_review']):model.identity(identity)
    model.text(r['path_id']);model.ordinal(a['purchase_ordinal']);model.finite(a['actual_cost']);model.finite(a['original_annual_effective_rate'],-.99,10.);model.vector(a['opening_gross_carrying'],3)
    if (r['schema']!=SCHEMA or c['identity']['measure']!='physical' or c['monetary_unit']!='currency_units' or c['horizon_days']>1080
        or c['ownership']['kind']!='whole_loan' or c['ownership']['remittance_lag_days']!=0
        or c['opening_borrower_principal']!=c['opening_investor_principal'] or c['opening_borrower_interest']!=[0.,0.,0.]
        or c['opening_investor_interest']!=[0.,0.,0.] or c['opening_contractual_interest']!=0.
        or a['purchase_ordinal']!=c['as_of_ordinal'] or a['actual_cost']<=0. or a['accounting_basis']!='supplied_non_pcd_gross_cost-1'
        or r['economic_source']['revision']!=c['identity']['source_revision'] or c['payment_dates'][-1]>c['as_of_ordinal']+c['horizon_days']):raise ArithmeticError('explicit acquired Physical whole-loan zero-float scope')
    original=r['original_contract'];pair=r['pairing']
    if paired_terms(c)!=paired_terms(original) or original['identity']['measure']!='physical' or r['original_economic_source']['revision']!=original['identity']['source_revision'] or pair['original_run_id']!=original['identity']['run_id'] or pair['current_run_id']!=c['identity']['run_id'] or pair['policy']!='supplied_physical_original_contract_unchanged-1':raise ArithmeticError('immutable original terms and explicit supplied physical pairing')
    model.equal(sum(a['opening_gross_carrying']),a['actual_cost'],'opening/acquisition-cost')
    for origin,g in enumerate(a['opening_gross_carrying']):
        if c['opening_investor_principal'][origin]==0. and g!=0.:raise ArithmeticError('orphan acquired carry origin')
    if any(p[k]!=v for k,v in POLICY.items()) or i['accounting_accrued']!='reverse_income_at_default' or i['economic_remainder']!='reviewed_economic_disposal':raise ArithmeticError('reviewed book versus economic disposal policies')
    model.integer(p['economic_principal_disposal_lag_days'],0,1080);model.integer(i['disposal_lag_days'],0,1080);model.vector(i['collection_fraction'],3,0.,1.)
    if not isinstance(i['complete_collection_tails'],list) or len(i['complete_collection_tails'])!=3:raise ArithmeticError('complete origin collection tails')
    for origin,tail in enumerate(i['complete_collection_tails']):
        if not isinstance(tail,list) or len(tail)>32 or bool(tail)!=(i['collection_fraction'][origin]>0.):raise ArithmeticError('explicit complete bounded collection evidence')
        previous=-1;weight=0.
        for t in tail:
            model.fields(t,'lag_days weight');model.integer(t['lag_days'],0,i['disposal_lag_days']);model.finite(t['weight'],0.,1.)
            if t['lag_days']<=previous:raise ArithmeticError('collection chronological ordering')
            previous=t['lag_days'];weight+=t['weight']
        if tail and abs(weight-1.)>1e-12:raise ArithmeticError('collection weights cannot normalize missing mass')
        if any(t['lag_days']>p['economic_principal_disposal_lag_days'] for t in c['complete_recovery_tails'][origin]+original['complete_recovery_tails'][origin]):raise ArithmeticError('principal disposal before native recovery tail')
    for contract in (c,original):
        for tail in contract['complete_recovery_tails']:
            if tail and abs(sum(t['weight'] for t in tail)-1.)>8.*np.finfo(float).eps:raise ArithmeticError('native principal-tail mass domain')
    count=len(model.boundaries(c));original_count=len(model.boundaries(original));encoded=len(canonical_bytes(r))
    metadata=escaped_strings(r);interest_rows=sum(len(t) for t in i['complete_collection_tails'])
    rows_for=lambda c,n:n*(8*sum(len(t) for t in c['complete_recovery_tails'])+4*interest_rows+128)
    rows=rows_for(c,count)+rows_for(original,original_count)
    capacity=rows*(metadata+4096)+(count+original_count)*(metadata+65536)+4*encoded
    work=count*(140*14**3+4096)+original_count*4096+4000000+model.GENERATOR_CODE_HASH_BYTES
    growth=math.exp(max(math.log1p(a['original_annual_effective_rate']),0.)*c['horizon_days']/365.)
    if count>16 or original_count>16 or encoded>1024*1024 or capacity>64*1024*1024 or work>9000000 or a['actual_cost']*growth>model.MAX_AMOUNT:raise ArithmeticError('sequential aggregate pre-generation limits')
    for contract in (c,original):
        if len(contract['identity']['position_id'].encode())+len(contract['identity']['run_id'].encode())+32>256:raise ArithmeticError('bounded combined generator source identity')
    model.ordinal(c['as_of_ordinal']+c['horizon_days']+p['economic_principal_disposal_lag_days']);model.ordinal(c['as_of_ordinal']+c['horizon_days']+i['disposal_lag_days'])
    return r,dict(interval_count=count,original_interval_count=original_count,retained_row_bound=rows,retained_byte_bound=capacity,work_bound=work,generator_code_hash_bytes=model.GENERATOR_CODE_HASH_BYTES)


def paired_terms(contract):
    c=deepcopy(contract)
    for key in ('path','borrower_collection_fraction','loss_given_default','complete_recovery_tails'):c.pop(key)
    for key in ('run_id','assumption_id'):c['identity'].pop(key)
    for reset in c['rate'].get('resets',[]):reset.pop('input')
    return c


def escaped_strings(v):
    if isinstance(v,str):return len(json.dumps(v,ensure_ascii=False).encode())
    if isinstance(v,list):return sum(escaped_strings(x) for x in v)
    if isinstance(v,dict):return sum(escaped_strings(x) for x in v.values())
    return 0


def canonical_bytes(value):
    """Canonical sorted JSON matching native supplied-input Value serialization."""
    from decimal import Decimal
    def encode(v):
        if isinstance(v,dict):return '{'+','.join(encode(k)+':'+encode(v[k]) for k in sorted(v))+'}'
        if isinstance(v,list):return '['+','.join(encode(x) for x in v)+']'
        if isinstance(v,str):return json.dumps(v,ensure_ascii=False,separators=(',',':'))
        if type(v) is float:
            if not math.isfinite(v):raise ArithmeticError('finite canonical source')
            s=repr(v)
            if 'e' in s:
                m,e=s.split('e');n=int(e)
                if -5<=n<16:return format(Decimal(s),'f')
                return m+'e'+('-' if n<0 else '+')+str(abs(n))
            return s
        return json.dumps(v,separators=(',',':'),allow_nan=False)
    return encode(value).encode()


def source_hash(value):return hashlib.sha256(canonical_bytes(value)).hexdigest()


def _take(entitlements,used,kind,ordinal,principal,interest):
    if principal==interest==0.:return None
    for row in sorted(entitlements,key=lambda e:int(e['event_id'].rsplit('-',1)[1])):
        if row['event_id'] not in used and row['kind']==kind and row['entitlement_ordinal']==ordinal:
            try:model.equal([row['principal'],row['interest']],[float(principal),float(interest)],'raw-native-entitlement')
            except ArithmeticError:continue
            used.add(row['event_id']);return row['event_id']
    raise ArithmeticError('independently generated entitlement ownership absent')


def boundary_source(contract,check=lambda:False):
    """Raw independent interval projector plus explicit per-origin cash ownership."""
    generated=sequential_source.project(contract,check);projection=generated['native_projection'];intervals=generated['intervals'];capture=[];used=set()
    c=generated['raw_request'];entitlements=projection['entitlements'];previous_claim=0
    for index,r in enumerate(intervals):
        model.checkpoint(check);t=np.array(r['loan_transition']);f=np.array(r['loan_earned_interest']);it=np.array(r['investor_transition']);iff=np.array(r['investor_earned_interest'])
        bp=np.r_[r['opening_borrower_principal'],[0.]*4];ba=np.r_[r['opening_borrower_interest'],[0.]*4]
        ip=np.r_[r['opening_investor_principal'],[0.]*4];ia=np.r_[r['opening_investor_interest'],[0.]*4]
        lp=bp@t;la=ba@t+bp@f;p=ip@it;a=ia@it+ip@iff;day=r['end_ordinal'];point=projection['points'][index]
        claim_end=previous_claim+sum(d['default_ordinal']==day for d in projection['defaults']);recoveries=[]
        for ci in range(previous_claim,claim_end):
            claim=projection['defaults'][ci]
            for ti,w in enumerate(claim['complete_recovery_tail']):
                event=_take(entitlements,used,'whole_loan_default_recovery',w['ordinal'],w['principal'],0.)
                recoveries.append(dict(claim_index=ci,origin=claim['origin'],tail_index=ti,ordinal=w['ordinal'],principal=w['principal'],entitlement_id=event))
        prepayments=[];prepay=point['turnover_hazard_per_year']+point['refinance_hazard_per_year']
        if prepay>0.:
            for kind,fraction in [('turnover',point['turnover_hazard_per_year']/prepay),('refinance',point['refinance_hazard_per_year']/prepay)]:
                principal=float(p[6]*fraction);interest=float(la[6]*fraction);recognized=float(a[6]*fraction)
                prepayments.append(dict(kind=kind,fraction=fraction,principal=principal,interest=interest,recognized_interest=recognized,entitlement_id=_take(entitlements,used,kind,day,principal,interest)))
        scheduled=[]
        if r['payment_is_boundary']:
            probability=np.array(r['opening_probability'])@t;final=day==c['payment_dates'][-1]
            for origin in range(3):
                available=(lp[origin]+la[origin])*c['borrower_collection_fraction'][origin] if final else r['payment_before_boundary']*probability[origin]*c['borrower_collection_fraction'][origin]
                interest=float(min(available,la[origin]));principal=float(min(available-interest,lp[origin]));recognized=float(min(interest,a[origin]))
                scheduled.append(dict(origin=origin,principal=principal,interest=interest,recognized_interest=recognized,entitlement_id=_take(entitlements,used,'scheduled_investor_entitlement',day,principal,interest)))
        capture.append(dict(interval_index=index,ordinal=day,transported_loan_principal=lp.tolist(),transported_loan_interest=la.tolist(),transported_investor_principal=p.tolist(),transported_investor_interest=a.tolist(),default_claim_start=previous_claim,default_claim_end=claim_end,default_recoveries=recoveries,prepayments=prepayments,scheduled=scheduled,closing_investor_principal=point['investor_principal'],closing_investor_interest=point['investor_interest']))
        previous_claim=claim_end
    if len(used)!=len(entitlements):raise ArithmeticError('unowned raw native cash entitlement')
    links=[dict(entitlement_id=e['event_id'],cash_event_id=f"{e['kind']}-{index}") for index,e in enumerate(entitlements)]
    return dict(native_projection=projection,intervals=intervals,capture=dict(boundaries=capture,cash_links=links))



def original_economics(r, source, check):
    """Independent original gross-claim cash/disposal ownership, without G refit."""
    contract=r['original_contract'];policy=r['policy'];interest=policy['interest']
    projection=source['native_projection'];links={x['entitlement_id']:x['cash_event_id'] for x in source['capture']['cash_links']}
    full=contract['as_of_ordinal']+contract['horizon_days'];cash=[];closeouts=[]
    last=projection['points'][-1]
    if last['contractual_principal']!=0. or any(any(v!=0. for v in last[k]) for k in ('investor_principal','investor_interest','borrower_principal','borrower_interest')):
        raise ArithmeticError('complete original live source tail without refit/plug')
    for boundary in source['capture']['boundaries']:
        for ci in range(boundary['default_claim_start'],boundary['default_claim_end']):
            model.checkpoint(check);claim=projection['defaults'][ci];origin=claim['origin'];cid=claim['claim_id'];target_collected=claim['borrower_contract_interest']*interest['collection_fraction'][origin];collected=0.;interest_ids=[]
            for n,t in enumerate(interest['complete_collection_tails'][origin]):
                day=claim['default_ordinal']+t['lag_days'];cashid=f'{cid}:reviewed-interest:{n}';model.ordinal(day)
                value=target_collected*t['weight'];collected+=value
                cash.append(dict(cash_id=cashid,claim_id=cid,ordinal=day,amount=value,review=deepcopy(interest['review'])));interest_ids.append(cashid);full=max(full,day)
            recoveries=[x for x in boundary['default_recoveries'] if x['claim_index']==ci]
            recovered=sum(x['principal'] for x in claim['complete_recovery_tail'])
            principal_ids=[links[x['entitlement_id']] for x in recoveries if x['entitlement_id'] is not None]
            pd=claim['default_ordinal']+policy['economic_principal_disposal_lag_days'];ad=claim['default_ordinal']+interest['disposal_lag_days'];model.ordinal(pd);model.ordinal(ad)
            full=max(full,pd,ad,*[x['ordinal'] for x in claim['complete_recovery_tail']])
            principal_disposal=claim['borrower_principal']-recovered
            if principal_disposal<0.:raise ArithmeticError('original principal overcollection requires unsupported excess ownership')
            accrued_disposal=claim['borrower_contract_interest']-collected
            if accrued_disposal<0.:raise ArithmeticError('original interest overcollection requires unsupported excess ownership')
            closeouts.append(dict(claim_id=cid,origin=origin,gross_principal=claim['borrower_principal'],gross_contract_interest=claim['borrower_contract_interest'],principal_cash=recovered,interest_cash=collected,principal_economic_disposal=principal_disposal,principal_disposal_ordinal=pd,principal_disposal_review=deepcopy(policy['economic_principal_disposal_review']),interest_economic_disposal=accrued_disposal,interest_disposal_ordinal=ad,interest_disposal_review=deepcopy(interest['economic_disposal_review']),native_principal_cash_ids=principal_ids,reviewed_interest_cash_ids=interest_ids,closing_economic_principal=claim['borrower_principal']-recovered-principal_disposal,closing_economic_interest=claim['borrower_contract_interest']-collected-accrued_disposal))
    return cash,closeouts,full


def checked_equal(actual, expected, check, path='sequential'):
    """Check cancellation at every complete-output node; exact typed metadata."""
    model.checkpoint(check)
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or set(actual)!=set(expected):raise ArithmeticError(path+': keys')
        for key in expected:checked_equal(actual[key],expected[key],check,path+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': shape')
        for index,value in enumerate(expected):checked_equal(actual[index],value,check,f'{path}/{index}')
    else:model.equal(actual,expected,path)


def checked_identity(actual, expected, check, path='immutable'):
    """Retained input copies have exact normalized types and values."""
    model.checkpoint(check)
    if type(actual) is not type(expected):
        raise ArithmeticError(path+': exact input type')
    if isinstance(expected,dict):
        if set(actual)!=set(expected):raise ArithmeticError(path+': exact input keys')
        for key in expected:checked_identity(actual[key],expected[key],check,path+'/'+key)
    elif isinstance(expected,list):
        if len(actual)!=len(expected):raise ArithmeticError(path+': exact input shape')
        for index,value in enumerate(expected):checked_identity(actual[index],value,check,f'{path}/{index}')
    elif actual!=expected or (type(expected) is float and expected==0. and
            math.copysign(1.,actual)!=math.copysign(1.,expected)):
        raise ArithmeticError(path+': exact input identity')


def verify_immutable_clones(actual, expected, check):
    checked_identity(actual['raw_request'],expected['raw_request'],check,'raw_request')
    for name in ('source','original_source'):
        a=actual[name];e=expected[name]
        for key in ('schema','identity','ownership','monetary_unit','original_principal','original_payment','origination_ordinal'):
            checked_identity(a['native_projection'][key],e['native_projection'][key],check,name+'/native_projection/'+key)
        if len(a['intervals'])!=len(e['intervals']):raise ArithmeticError(name+': complete interval count')
        for index,(ai,ei) in enumerate(zip(a['intervals'],e['intervals'],strict=True)):
            for key in ('identity','ownership'):
                checked_identity(ai[key],ei[key],check,f'{name}/interval/{index}/{key}')
    for index,(ai,ei) in enumerate(zip(actual['intervals'],expected['intervals'],strict=True)):
        for key in ('original_annual_effective_rate','policy'):
            checked_identity(ai['native_carry_request'][key],ei['native_carry_request'][key],check,f'interval/{index}/native_carry_request/{key}')


def reconstruct(raw,check=lambda:False):
    r,admission=admit(raw,check);source=boundary_source(r['contract'],check);original_source=boundary_source(r['original_contract'],check);c=r['contract'];p=r['policy'];i=p['interest'];acq=r['acquisition'];projection=source['native_projection'];end=c['as_of_ordinal']+c['horizon_days'];last=projection['points'][-1]
    if last['contractual_principal']!=0. or any(any(v!=0. for v in last[k]) for k in ('investor_principal','investor_interest','borrower_principal','borrower_interest')):raise ArithmeticError('complete actual live tail without terminal cash plug')
    links={l['entitlement_id']:l['cash_event_id'] for l in source['capture']['cash_links']};g=list(acq['opening_gross_carrying']);live_earned=[0.]*3;disposed_earned=0.;intervals=[];actions=[];interest_cash=[];closeouts=[];earned=0.;full_end=end;used=set()
    def action(day,index,origin,kind,source_id,review,income,disposed,cash):
        model.checkpoint(check);model.ordinal(day)
        for x in (income,disposed,cash):model.finite(x,-model.MAX_AMOUNT,model.MAX_AMOUNT)
        actions.append(dict(ordinal=day,interval_index=index,origin=origin,kind=kind,source_id=source_id,review=deepcopy(review),income=float(income),gross_carry_disposed=float(disposed),actual_cash=float(cash)))
    for index,(receipt,boundary) in enumerate(zip(source['intervals'],source['capture']['boundaries'],strict=True)):
        model.checkpoint(check)
        opening=dict(measure='physical',monetary_unit=c['monetary_unit'],probability=deepcopy(receipt['opening_probability']),principal=receipt['opening_investor_principal']+[0.]*4,accrued_interest=receipt['opening_investor_interest']+[0.]*4,gross_carrying=deepcopy(g)+[0.]*4,earned_eir=deepcopy(live_earned)+[0.]*4)
        carry=model.carry_request(c,receipt,dict(annual_effective_rate=acq['original_annual_effective_rate'],policy=dict(identity=deepcopy(p['identity']),original_income=p['original_income'],remittance_float='external_explicit_owner_required-1')))
        operator=model.carry_operator(carry,check);transport=model.carry_apply(carry,opening,check)
        model.equal(transport['closing']['principal'],boundary['transported_investor_principal'],'sequential/actual-principal');model.equal(transport['closing']['accrued_interest'],boundary['transported_investor_interest'],'sequential/actual-accrued')
        earned+=transport['total_interval_earned_eir'];g=transport['closing']['gross_carrying'][:3];live_earned=transport['closing']['earned_eir'][:3]
        disposed_earned+=sum(transport['closing']['earned_eir'][3:])
        for claim in projection['defaults'][boundary['default_claim_start']:boundary['default_claim_end']]:
            origin=claim['origin'];slot=origin+3;cg=transport['closing']['gross_carrying'][slot];basis=cg-claim['investor_principal']-claim['investor_accrued_interest'];day=claim['default_ordinal'];cid=claim['claim_id']
            model.equal(transport['closing']['principal'][slot],claim['investor_principal'],'default/exact-principal');model.equal(transport['closing']['accrued_interest'][slot],claim['investor_accrued_interest'],'default/exact-accrued')
            action(day,index,origin,'default_accrued_reversal',cid,i['review'],-claim['investor_accrued_interest'],claim['investor_accrued_interest'],0.)
            action(day,index,origin,'default_signed_basis_disposal',cid,p['identity'],-basis,basis,0.)
            action(day,index,origin,'default_native_principal_loss_book_chargeoff',cid,p['default_principal_book_review'],-claim['borrower_principal_loss'],claim['borrower_principal_loss'],0.)
            recovered=sum(w['principal'] for w in claim['complete_recovery_tail']);principal_disposal=claim['borrower_principal']-recovered;pd=day+p['economic_principal_disposal_lag_days'];idate=day+i['disposal_lag_days'];target_collected=claim['borrower_contract_interest']*i['collection_fraction'][origin];collected=0.;interest_ids=[];principal_ids=[]
            model.ordinal(pd);model.ordinal(idate)
            if principal_disposal<0.:raise ArithmeticError('current principal overcollection requires unsupported excess ownership')
            for n,t in enumerate(i['complete_collection_tails'][origin]):
                date=day+t['lag_days'];value=target_collected*t['weight'];collected+=value;cashid=f'{cid}:reviewed-interest:{n}'
                action(date,index,origin,'reviewed_default_interest_cash_income',cashid,i['review'],value,0.,value)
                interest_cash.append(dict(cash_id=cashid,claim_id=cid,ordinal=date,amount=value,review=deepcopy(i['review'])));interest_ids.append(cashid);full_end=max(full_end,date)
            for recovery in claim['complete_recovery_tail']:
                if recovery['principal']==0.:continue
                eid=_take(projection['entitlements'],used,'whole_loan_default_recovery',recovery['ordinal'],recovery['principal'],0.);cashid=links[eid];principal_ids.append(cashid)
                action(recovery['ordinal'],index,origin,'native_default_principal_cash',cashid,p['default_principal_book_review'],0.,recovery['principal'],recovery['principal']);full_end=max(full_end,recovery['ordinal'])
            full_end=max(full_end,pd,idate);interest_disposal=claim['borrower_contract_interest']-collected
            if interest_disposal<0.:raise ArithmeticError('current interest overcollection requires unsupported excess ownership')
            closeouts.append(dict(claim_id=cid,origin=origin,default_ordinal=day,gross_principal=claim['borrower_principal'],gross_contract_interest=claim['borrower_contract_interest'],recognized_interest=claim['investor_accrued_interest'],exact_default_carry=cg,signed_default_basis=basis,principal_cash=recovered,interest_cash=collected,principal_economic_disposal=principal_disposal,principal_disposal_ordinal=pd,principal_disposal_review=deepcopy(p['economic_principal_disposal_review']),interest_economic_disposal=interest_disposal,interest_disposal_ordinal=idate,interest_disposal_review=deepcopy(i['economic_disposal_review']),native_principal_cash_ids=principal_ids,reviewed_interest_cash_ids=interest_ids,closing_economic_principal=claim['borrower_principal']-recovered-principal_disposal,closing_economic_interest=claim['borrower_contract_interest']-collected-interest_disposal,closing_book_principal=claim['investor_principal']-claim['borrower_principal_loss']-recovered,closing_book_accrued=0.,closing_book_basis=0.))
        for prepay in boundary['prepayments']:
            cg=transport['closing']['gross_carrying'][6]*prepay['fraction'];cash=prepay['principal']+prepay['interest'];eid=prepay['entitlement_id']
            if eid is not None:used.add(eid);action(boundary['ordinal'],index,None,prepay['kind']+'_exact_exit',links[eid],p['identity'],cash-cg,cg,cash)
            elif cash!=0. or cg!=0.:raise ArithmeticError('prepaid carry without actual cash ownership')
        for scheduled in boundary['scheduled']:
            origin=scheduled['origin'];cash=scheduled['principal']+scheduled['interest'];g[origin]-=cash;eid=scheduled['entitlement_id']
            if eid is not None:used.add(eid);action(boundary['ordinal'],index,origin,'scheduled_native_cash',links[eid],p['identity'],0.,cash,cash)
            elif cash!=0.:raise ArithmeticError('scheduled cash identity absent')
            if boundary['closing_investor_principal'][origin]==0. and boundary['closing_investor_interest'][origin]==0.:
                action(boundary['ordinal'],index,origin,'scheduled_full_exit_signed_basis',f'scheduled:{index}:{origin}',p['identity'],-g[origin],g[origin],0.);g[origin]=0.;disposed_earned+=live_earned[origin];live_earned[origin]=0.
            elif g[origin]<0.:raise ArithmeticError('negative remaining live gross carrying')
        basis=[g[j]-boundary['closing_investor_principal'][j]-boundary['closing_investor_interest'][j] for j in range(3)]
        intervals.append(dict(interval_index=index,opening=opening,native_carry_request=carry,operator=operator,transport=transport,closing_live_gross_carrying=deepcopy(g),closing_live_basis=basis,closing_live_earned_eir=deepcopy(live_earned)))
    if len(used)!=len(projection['entitlements']) or g!=[0.,0.,0.]:raise ArithmeticError('complete acquired cash ownership/live tail')
    for d in closeouts:
        model.equal(d['closing_book_principal'],0.,'book/principal-closeout')
        if d['closing_economic_principal']!=0. or d['closing_economic_interest']!=0.:raise ArithmeticError('unclosed gross economic claims')
    k=math.log1p(acq['original_annual_effective_rate']);discount=lambda d,x:x*math.exp(-k*(d-c['as_of_ordinal'])/365.)
    original_cash,original_closeouts,original_end=original_economics(r,original_source,check)
    full_end=max(full_end,original_end)
    quote=sum(discount(x['ordinal'],x['principal']+x['interest']) for x in original_source['native_projection']['cashflows'])
    for x in original_cash:quote+=discount(x['ordinal'],x['amount'])
    model.equal(quote,acq['actual_cost'],'original/full-tail-quote')
    cash=sum(x['principal']+x['interest'] for x in projection['cashflows'])+sum(x['amount'] for x in interest_cash);income=earned+sum(x['income'] for x in actions);error=acq['actual_cost']+income-cash
    model.equal(acq['actual_cost']+income,cash,'whole-life/cost-income-cash')
    actions.sort(key=lambda a:(a['ordinal'],a['interval_index']))
    result=dict(schema=SCHEMA,raw_request=r,admission=admission,source=source,original_source=original_source,original_source_raw_hash=source_hash(r['original_contract']),current_source_raw_hash=source_hash(c),original_source_receipt_hash=source_hash(original_source),current_source_receipt_hash=source_hash(source),intervals=intervals,actions=actions,reviewed_interest_cash=interest_cash,default_closeouts=closeouts,original_reviewed_interest_cash=original_cash,original_economic_closeouts=original_closeouts,original_full_end_ordinal=original_end,original_quote_pv=quote,actual_acquisition_cost=acq['actual_cost'],total_earned_eir=earned,total_accounting_income=income,total_actual_cash=cash,cumulative_earned_eir_conservation_error=sum(live_earned)+disposed_earned-earned,quote_basis='complete_immutable_original_actual_cash_only-1',full_end_ordinal=full_end,closing_live_gross_carrying=g,closing_reconciliation_error=error,principal_recovery_owner='native_mortgage_projection_only-1',sequential_carry_adoption=False,production_adoption=False,installed_independent_verification=False,mixed_ledger_adoption=False)
    if model.serialized_size(result)>admission['retained_byte_bound']:raise ArithmeticError('complete output capacity admission')
    return json.loads(json.dumps(result,separators=(',',':'),allow_nan=False))


def ordered_sum(values):
    total=0.
    for value in values:total+=value
    return total


def verify(raw,actual,check=lambda:False):
    model.checkpoint(check);expected=reconstruct(raw,check)
    output=deepcopy(actual)
    if len(output.get('intervals',[]))!=len(expected['intervals']):raise ArithmeticError('complete sequential interval count')
    verify_immutable_clones(output,expected,check)
    for i,(a,e) in enumerate(zip(output['intervals'],expected['intervals'],strict=True)):
        model.checkpoint(check);model.verify_operator(e['native_carry_request'],a['operator'],check);a['operator'].pop('work_multiplications')
        # Verify every raw-derived state/reward first. The conservation receipt
        # is a deterministic reduction of those already checked money vectors;
        # expm versus uniformization roundoff must not become a new tolerance.
        at=deepcopy(a['transport']);et=deepcopy(e['transport']);error=at.pop('gross_conservation_error');et.pop('gross_conservation_error')
        checked_equal(at,et,check,f'sequential/interval/{i}/transport')
        opening=ordered_sum(a['opening']['gross_carrying'])
        reduction=ordered_sum(at['closing']['gross_carrying'])-opening-ordered_sum(at['interval_earned_eir'])
        if type(error) is not float or error!=reduction or abs(error)>1e-9+1e-12*abs(opening):raise ArithmeticError('exact verified-vector gross conservation reduction')
        e['transport']['gross_conservation_error']=reduction
    # Hashes bind the already independently checked receipt bytes, not the
    # independent expm's different last bits. All financial receipt contents are
    # still compared below against raw reconstruction with unchanged tolerance.
    for key,field in [('original_source_receipt_hash','original_source'),('current_source_receipt_hash','source')]:
        if output[key]!=source_hash(output[field]):raise ArithmeticError('complete receipt hash mismatch')
        expected[key]=output[key]
    checked_equal(output,expected,check)
    model.checkpoint(check)
    return dict(verified=True,raw_mortgage_and_boundaries_recomputed=True,sequential_single_acquisition_carry_recomputed=True,
        original_yield_quote_recomputed=True,complete_gross_claim_tail_verified=True,analytical_mortgage_source_only=True,production_adoption=False,mixed_ledger_adoption=False,capital_ftp_eligible=False,empirical_validation=False)
