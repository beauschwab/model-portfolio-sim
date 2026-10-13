"""Independent raw-card original-EIR and compact gross/basis-right verifier.

Explicit publication/test reference only; never a production financial backend.
No artifact imports or native financial callbacks. Events are sole postings;
shared intents retain reviewed evidence and are never posted a second time.
"""
from copy import deepcopy
from decimal import Decimal
import json
import math
import sys
from portfolio_risk.analytics import card_credit_reference as card
from portfolio_risk.analytics import consumer_credit_reference as shared

fields=shared.fields;identity=shared.identity;equal=shared.equal;text=shared.text
SCHEMA='card-credit-accounting-2'
POLICY=dict(principal_basis='actual_terminal_principal_transfer_to_unpaid_interest-1',
    live_original_yield='frozen_original_yield_on_whole_live_carrying-1',
    interest_cash_basis='proportional_basis_realization_separate_from_original_eir-1',
    default_original_yield='stop_at_source_default_no_claim_eir-1')

def money(v):
    if isinstance(v,bool) or not isinstance(v,(int,float)) or not math.isfinite(v) or abs(v)>1e15:raise ArithmeticError('finite bounded basis-rights amount')
    return v


def near(a,b):return abs(a-b)<=1e-9+1e-12*abs(b)


def day(v):
    if type(v) is not int or not 1<=v<=3652059:raise ArithmeticError('basis-rights exact ordinal')


def _policy(p):
    fields(p,'identity accrued basis allowance_shortfall excess_cash_recovery');identity(p['identity'])
    if p['accrued'] not in ('reverse_income_at_default','transfer_until_reviewed_reversal') or p['basis'] not in ('recognize_in_earnings_at_default','transfer_until_reviewed_chargeoff') or p['allowance_shortfall'] not in ('reject','expense_at_reviewed_chargeoff') or p['excess_cash_recovery'] not in ('credit_allowance','recognize_in_earnings'):raise ArithmeticError('basis-rights declared credit policy')


def _new_zero(review,check=lambda:False):
    shared.checkpoint(check);fields(review,'identity original_yield_anchor economic_source position_id currency principal cost original_annual_effective_rate acquisition_ordinal policy accounting_basis')
    for k in ('identity','original_yield_anchor','economic_source'):identity(review[k])
    for k in ('position_id','currency'):text(review[k])
    if len(review['currency'])!=3 or not review['currency'].isascii() or not review['currency'].isupper() or not review['currency'].isalpha():raise ArithmeticError('actual ISO currency required')
    _policy(review['policy']);day(review['acquisition_ordinal']);money(review['principal']);money(review['cost']);money(review['original_annual_effective_rate'])
    if review['principal']<=0. or review['cost']<=0. or review['original_annual_effective_rate']!=0.:raise ArithmeticError('first proof positive acquisition and explicit zero original EIR')
    state=dict(live=[shared._empty_live() for _ in range(3)],allowance=0.,claims=[])
    state['live'][0].update(principal=review['principal'],book_adjustment=review['cost']-review['principal'])
    b=dict(review=deepcopy(review),core=state,live_interest_basis=0.,claim_interest_basis={},events=[],shared_intents=[],interest_cash_reviews=[],cash_received=0.,original_eir_income=0.,reviewed_basis_income=0.,shared_default_income=0.,last_ordinal=review['acquisition_ordinal'],action_count=0)
    validate(b,check);return b


def carrying(book):
    core=book['core'];return sum((s['principal']+s['accrued_interest']+s['book_adjustment'] for s in core['live']),0.)+book['live_interest_basis']+sum((c['outstanding_principal']+c['outstanding_accrued_interest']+c['outstanding_book_adjustment'] for c in core['claims']),0.)+sum(book['claim_interest_basis'].values(),0.)


def complete(book):
    c=book['core']
    return near(c['allowance'],0.) and all(near(v,0.) for s in c['live'] for v in s.values()) and near(book['live_interest_basis'],0.) and all(not c['remaining_recoveries'] and all(near(c[k],0.) for k in ('outstanding_principal','outstanding_accrued_interest','outstanding_book_adjustment')) for c in c['claims']) and all(near(v,0.) for v in book['claim_interest_basis'].values())


def validate(book,check=lambda:False):
    shared.checkpoint(check)
    for state in book['core']['live']:
        for v in state.values():money(v)
        if state['principal']<0. or state['undrawn']<0. or state['principal']+state['book_adjustment']<0.:raise ArithmeticError('unchanged shared principal plus basis guard')
    money(book['live_interest_basis']);a=book['core']['live'][0]['accrued_interest']
    if a<0. or a+book['live_interest_basis'] < -1e-9:raise ArithmeticError('gross interest plus associated basis negative')
    ids=set()
    for c in book['core']['claims']:
        if c['claim_id'] in ids:raise ArithmeticError('duplicate claim')
        ids.add(c['claim_id'])
        if c['outstanding_principal']+c['outstanding_book_adjustment']<0.:raise ArithmeticError('unchanged shared default principal plus basis guard')
        for k in ('outstanding_principal','outstanding_accrued_interest','outstanding_book_adjustment'):money(c[k])
    for claim,b in book['claim_interest_basis'].items():
        text(claim);money(b)
        c=next((c for c in book['core']['claims'] if c['claim_id']==claim),None)
        if c is None or c['outstanding_accrued_interest']+b < -1e-9:raise ArithmeticError('orphan/negative claim interest basis')
    expected=book['review']['cost']-book['cash_received']+book['original_eir_income']+book['reviewed_basis_income']+book['shared_default_income']
    equal(carrying(book),expected,'basis-rights/cash-carry-income')
    for event in book['events']:
        if type(event['atomic_action']) is not int or not 1<=event['atomic_action']<=book['action_count']:raise ArithmeticError('basis-rights exact atomic action')
        equal(sum((l['amount'] for l in event['lines']),0.),0.,'basis-rights/transaction-balance')


def _emit(b,d,name,claim,lines,income=0.):
    if len(b['events'])>=256:raise ArithmeticError('basis-rights event retention')
    for _,v in lines:money(v)
    equal(sum((v for _,v in lines),0.),0.,'basis-rights/event-balance')
    r=b['review'];b['events'].append(dict(atomic_action=b['action_count'],ordinal=d,event=name,claim_id=claim,review=deepcopy(r['identity']),source=deepcopy(r['economic_source']),original_yield_anchor=deepcopy(r['original_yield_anchor']),lines=[dict(account=k,amount=v) for k,v in lines if v!=0.],original_eir_income=0.,reviewed_basis_income=income))
    b['reviewed_basis_income']=money(b['reviewed_basis_income']+income)


def _shared_intent(b,d,name,claim,source,lines,policy=None):
    postings=[dict(account=k,amount=v) for k,v in lines if v!=0.]
    if not postings:return
    intent=dict(recognition_ordinal=d,event=name,position_id=b['review']['position_id'],claim_id=claim,policy=deepcopy(policy or b['review']['policy']['identity']),source=deepcopy(source),postings=postings)
    b['shared_intents'].append(intent)
    _emit(b,d,name,claim,[(p['account'],p['amount']) for p in postings])


def _apply_disposal(book,action,check=lambda:False):
    """Clone/stage one complete source action and commit only a validated group."""
    shared.checkpoint(check);b=deepcopy(book);kind=action['kind'];d=action['ordinal'];day(d)
    if d<b['last_ordinal'] or d-b['review']['acquisition_ordinal']>64:raise ArithmeticError('basis-rights chronological 64-day proof')
    b['last_ordinal']=d;b['action_count']+=1;s=b['core']['live'][0];r=b['review']
    if kind=='earned':
        fields(action,'kind ordinal amount');amount=money(action['amount'])
        if amount<0. or b['core']['claims']:raise ArithmeticError('positive live source recognition required')
        s['accrued_interest']=money(s['accrued_interest']+amount);s['book_adjustment']=money(s['book_adjustment']-amount)
        _emit(b,d,'original_zero_eir_contractual_accrual',None,[('live_accrued_interest',amount),('live_book_adjustment',-amount)])
    elif kind=='terminal_principal_cash':
        fields(action,'kind ordinal amount');p=money(action['amount']);basis=s['book_adjustment'];a=s['accrued_interest']
        if p<=0. or p!=s['principal'] or b['core']['claims']:raise ArithmeticError('exact actual terminal principal cash')
        if a<=0. and basis!=0.:raise ArithmeticError('no earned interest right for basis')
        if a+b['live_interest_basis']+basis<0.:raise ArithmeticError('transfer makes negative interest carrying')
        s['book_adjustment']=0.;b['live_interest_basis']=money(b['live_interest_basis']+basis)
        _emit(b,d,'reviewed_principal_basis_to_interest_right',None,[('live_book_adjustment',-basis),('live_interest_basis',basis)])
        s['principal']=0.;b['cash_received']=money(b['cash_received']+p)
        _emit(b,d,'actual_terminal_principal_cash',None,[('cash',p),('live_principal',-p)])
    elif kind=='live_interest_cash':
        fields(action,'kind ordinal amount');q=money(action['amount']);a=s['accrued_interest']
        if q<=0. or q>a or a<=0.:raise ArithmeticError('actual cash exceeds live gross right')
        released=b['live_interest_basis'] if q==a else b['live_interest_basis']*(q/a)
        s['accrued_interest']-=q;b['live_interest_basis']-=released;b['cash_received']=money(b['cash_received']+q)
        _emit(b,d,'reviewed_live_interest_cash_and_basis_realization',None,[('cash',q),('live_accrued_interest',-q),('live_interest_basis',-released),('pnl:reviewed_basis_realization',released)],-released)
    elif kind=='interest_only_default':
        fields(action,'kind ordinal claim_id');claim=action['claim_id'];text(claim)
        if s['principal']!=0. or s['book_adjustment']!=0. or s['accrued_interest']<=0. or b['core']['claims']:raise ArithmeticError('actual interest-only default boundary')
        policy=r['policy'];transfer=policy['accrued']=='transfer_until_reviewed_reversal'
        if policy['basis']!=('transfer_until_reviewed_chargeoff' if transfer else 'recognize_in_earnings_at_default'):raise ArithmeticError('paired gross/contra default policy required')
        a=s['accrued_interest'];s['accrued_interest']=0.
        _shared_intent(b,d,'default_accrued_transfer' if transfer else 'default_accrued_income_reversal',claim,r['economic_source'],[('live_accrued_interest',-a),('default_accrued_interest' if transfer else 'interest_income',a)])
        if not transfer:b['shared_default_income']=money(b['shared_default_income']-a)
        b['core']['claims'].append(dict(claim_id=claim,economic_source=deepcopy(r['economic_source']),recovery_schedule_review=deepcopy(r['identity']),policy=deepcopy(policy),default_ordinal=d,initial_principal=0.,economic_principal_loss=0.,initial_accrued_interest=a,initial_book_adjustment=0.,outstanding_principal=0.,outstanding_accrued_interest=a if transfer else 0.,outstanding_book_adjustment=0.,principal_charged_off=0.,accrued_reversed=0. if transfer else a,basis_recognized=0.,recovery_cash_collected=0.,recovery_applied_to_principal=0.,interest_recovery_cash_collected=0.,interest_recovery_applied=0.,settled_interest_recovery_ids=[],remaining_recoveries=[]))
        basis=b['live_interest_basis'];b['live_interest_basis']=0.
        if transfer:
            b['claim_interest_basis'][claim]=basis;_emit(b,d,'reviewed_default_interest_basis_transfer',claim,[('live_interest_basis',-basis),('default_interest_basis',basis)])
        else:_emit(b,d,'reviewed_default_interest_basis_realization',claim,[('live_interest_basis',-basis),('pnl:reviewed_default_basis_realization',basis)],-basis)
    elif kind=='claim_interest_cash':
        fields(action,'kind ordinal settlement');settlement=action['settlement'];fields(settlement,'recovery claim_id recognition_ordinal amount policy_review source excess_cash')
        for k in ('recovery','policy_review','source'):identity(settlement[k])
        if settlement['recognition_ordinal']!=d:raise ArithmeticError('reviewed settlement date mismatch')
        claim=settlement['claim_id'];text(claim);c=next((c for c in b['core']['claims'] if c['claim_id']==claim),None)
        if c is None or claim not in b['claim_interest_basis']:raise ArithmeticError('actual paired claim missing')
        q=money(settlement['amount']);a=c['outstanding_accrued_interest'];basis=b['claim_interest_basis'][claim]
        if q<=0. or q>a:raise ArithmeticError('reviewed collection exceeds gross claim')
        if settlement['recovery'] in c['settled_interest_recovery_ids']:raise ArithmeticError('reviewed recovery ID already settled')
        if settlement['excess_cash'] not in ('credit_allowance','recognize_in_earnings'):raise ArithmeticError('explicit excess disposition')
        released=basis if q==a else basis*q/a
        b['interest_cash_reviews'].append(deepcopy(settlement));c['outstanding_accrued_interest']=shared.subtract_owned(a,q)
        c['interest_recovery_cash_collected']+=q;c['interest_recovery_applied']+=q;c['settled_interest_recovery_ids'].append(deepcopy(settlement['recovery']))
        b['cash_received']=money(b['cash_received']+q)
        _shared_intent(b,d,'reviewed_physical_interest_recovery_cash',claim,settlement['source'],[('cash',q),('default_accrued_interest',-q)],settlement['policy_review'])
        b['claim_interest_basis'][claim]=basis-released
        _emit(b,d,'reviewed_recovery_interest_basis_realization',claim,[('default_interest_basis',-released),('pnl:reviewed_recovery_basis_realization',released)],-released)
    else:raise ArithmeticError('unsupported Card2 owned basis-rights action')
    validate(b,check);book.clear();book.update(b)



def new(review,check=lambda:False):
    shared.checkpoint(check);money(review['original_annual_effective_rate'])
    if review['accounting_basis']!='supplied_non_pcd_gross_cost-1':raise ArithmeticError('explicit reviewed non-PCD acquisition basis')
    if not -.99<=review['original_annual_effective_rate']<=10.:raise ArithmeticError('bounded explicitly supplied original yield')
    r=deepcopy(review);r['original_annual_effective_rate']=0.
    b=_new_zero(r,check);b['review']=deepcopy(review);b['income_steps']=[]
    validate(b,check);return b


def apply(book,action,check=lambda:False):
    if action['kind']!='earned':return _apply_disposal(book,action,check)
    shared.checkpoint(check);fields(action,'kind ordinal amount');b=deepcopy(book);d=action['ordinal'];day(d)
    if d<b['last_ordinal'] or d-b['review']['acquisition_ordinal']>64:raise ArithmeticError('chronological bounded actual date')
    previous=b['last_ordinal'];b['last_ordinal']=d;b['action_count']+=1
    a=money(action['amount']);s=b['core']['live'][0];r=b['review']
    if a<0. or b['core']['claims']:raise ArithmeticError('live recognition only; original yield stops at default')
    channel='live_book_adjustment' if s['principal']>0. else 'live_interest_basis'
    if channel=='live_interest_basis' and (s['book_adjustment']!=0. or s['accrued_interest']<=0. and a>0.):
        raise ArithmeticError('principal-free yield requires an existing gross right and reviewed transfer')
    opening=carrying(b);y=r['original_annual_effective_rate']
    effective=money(opening*math.expm1(math.log1p(y)*(d-previous)/365.));amort=money(effective-a)
    closing=money(opening+effective);accrued=money(s['accrued_interest']+a)
    rounding=64*sys.float_info.epsilon*(abs(opening)+abs(effective))
    if closing<0. and s['principal']==0. and abs(accrued)<=rounding and abs(closing)<=rounding:closing=0.
    if closing<0.:raise ArithmeticError('negative gross carrying requires explicit policy')
    event=dict(event_id=f'original-rights-yield:{d}:{b["action_count"]}',source=deepcopy(r['economic_source']),
        payment_ordinal=d,contractual_interest_earned=a,principal_cash=0.,interest_cash=0.,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None)
    common=dict(schema='dated-income-1',contract=deepcopy(r['economic_source']),original_yield_anchor=deepcopy(r['original_yield_anchor']),
        policy=deepcopy(r['identity']),currency=r['currency'],monetary_unit='currency_units',measure='physical')
    request=dict(common,as_of_ordinal=previous,full_contract_end_ordinal=d,opening_principal=s['principal'],opening_accrued_interest=s['accrued_interest'],
        opening_gross_carrying_value=opening,original_annual_effective_rate=y,opening_annual_effective_rate=y,events=[event])
    row=dict(event_id=event['event_id'],source=deepcopy(r['economic_source']),payment_ordinal=d,
        opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=effective,contractual_interest_earned=a,
        basis_amortization=amort,modification_gain_loss=0.,recognized_nonintegral_fee_income=0.,cash_received=0.,
        closing_principal=s['principal'],closing_accrued_interest=accrued,closing_gross_carrying_value=closing,
        closing_book_adjustment=closing-s['principal']-accrued,yield_review=None)
    result=dict(common,rows=[row],remaining_principal=s['principal'],remaining_accrued_interest=accrued,remaining_gross_carrying_value=closing)
    s['accrued_interest']=accrued
    if channel=='live_book_adjustment':s['book_adjustment']=money(s['book_adjustment']+amort)
    else:b['live_interest_basis']=money(b['live_interest_basis']+amort)
    b['original_eir_income']=money(b['original_eir_income']+effective)
    _emit(b,d,'native_original_eir_and_contractual_accrual',None,
        [('live_accrued_interest',a),(channel,amort),('pnl:original_effective_interest',-effective)])
    b['events'][-1]['original_eir_income']=effective
    b['income_steps'].append(dict(channel=channel,request=request,result=result))
    validate(b,check);shared.checkpoint(check);book.clear();book.update(b)


def _size(value):
    """Compact typed JSON bytes, retaining UTF-8 and shortest-float spelling.

    Rust's serializer emits fixed notation down to1e-5; Python's JSON chooses
    scientific notation there and retains leading exponent zeros. Capacity
    admission must use the same declared f64 wire representation, not relax
    exact integer byte metadata to a financial tolerance.
    """
    if isinstance(value,dict):return 2+max(0,len(value)-1)+sum(_size(k)+1+_size(v) for k,v in value.items())
    if isinstance(value,list):return 2+max(0,len(value)-1)+sum(_size(v) for v in value)
    if isinstance(value,str):return len(json.dumps(value,ensure_ascii=False).encode())
    if type(value) is float:
        money(value);s=repr(value)
        if 'e' in s:
            mantissa,exponent=s.split('e');n=int(exponent)
            if -5<=n<16:
                s=format(Decimal(s),'f')
                if '.' not in s:s+='.0'
            else:s=mantissa+'e'+('-' if n<0 else '+')+str(abs(n))
        return len(s)
    return len(json.dumps(value,separators=(',',':'),allow_nan=False).encode())


def _typed(raw):
    """Normalize only explicit f64 fields; exact date/range integers stay typed."""
    out=deepcopy(raw);c=out['contract'];r=out['review']
    number=lambda v:float(money(v))
    for k in ('principal','cost','original_annual_effective_rate'):r[k]=number(r[k])
    for k in ('opening_principal','opening_billed_interest','opening_unbilled_interest','opening_unrecognized_billed_interest','opening_fees','opening_statement_remaining'):c[k]=[number(v) for v in c[k]]
    for k in ('opening_contingent_purchase_interest','opening_minimum_remaining','initial_limit','collection_fraction','collection_cost'):c[k]=number(c[k])
    for k in ('purchase','cash_advance','promotional','post_promotion'):c['rates'][k]=number(c['rates'][k])
    for k in ('floor','principal_fraction'):c['minimum'][k]=number(c['minimum'][k])
    for step in c['steps']:
        if 'amount' in step['action']:step['action']['amount']=number(step['action']['amount'])
    for recovery in out['interest_recoveries']:recovery['amount']=number(recovery['amount'])
    return out


def _admit(raw,check=lambda:False):
    shared.checkpoint(check);fields(raw,'schema contract review policy termination claim_id reporting_ordinal interest_recoveries')
    raw=_typed(raw)
    if raw['schema']!=SCHEMA or _size(raw)>1024*1024:raise ArithmeticError('explicit card2 schema/raw bytes')
    c=raw['contract'];r=raw['review'];p=raw['policy'];new(r,check)
    fields(p,'review principal_basis live_original_yield interest_cash_basis default_original_yield');identity(p['review'])
    if any(p[k]!=v for k,v in POLICY.items()):raise ArithmeticError('reviewed principal/interest/yield/default ownership')
    text(raw['claim_id']);day(raw['reporting_ordinal']);closure=raw['termination']
    if closure=={'kind':'source_default'}:close=None
    else:
        fields(closure,'kind ordinal review');identity(closure['review']);day(closure['ordinal']);close=closure['ordinal']
        if closure['kind']!='reviewed_zero_rights_closure':raise ArithmeticError('reviewed actual source closure required')
    start=c['as_of_ordinal'];end=start+c['horizon_days']
    if type(c['horizon_days']) is not int or not 1<=c['horizon_days']<=64 or len(c['steps'])+len(c['cycles'])>64 or not isinstance(raw['interest_recoveries'],list) or len(raw['interest_recoveries'])>16:
        raise ArithmeticError('bounded card2 source/actions/collections')
    if not start<=raw['reporting_ordinal']<=end or close is not None and not start<close<=end:raise ArithmeticError('exact report/full/closure clock')
    if c['identity']['position_id']!=r['position_id'] or c['identity']['currency']!=r['currency'] or c['identity']['source_revision']!=r['economic_source']['revision'] or start!=r['acquisition_ordinal'] or sum(c['opening_principal'],0.)!=r['principal']:
        raise ArithmeticError('immutable actual acquisition/source anchor')
    # Reuse only independent raw card domain checks. No schema1 income lifecycle
    # or finance execution is called, and its narrower tail guard is unchanged.
    dummy=deepcopy(r['identity'])
    policy=dict(identity=dummy,accounting=deepcopy(r['policy']),recovery_review=dummy,cost_review=dummy,
        contractual_recognition='raw_card_non_grace_earned_then_billed-1',original_income='frozen_original_account_eir_until_exit-1',
        terminal_basis='recognize_in_earnings_at_reviewed_closure-1',recovery_cost='gross_recovery_with_credit_loss_cost-1',
        commitment='retained_source_undrawn_no_ccf_no_draw-1',unsupported_grace_fees_draws='reject')
    acquisition=dict(identity=dummy,original_yield_anchor=deepcopy(r['original_yield_anchor']),income_review=dummy,
        accounting_basis=r['accounting_basis'],purchase_ordinal=start,principal=r['principal'],cost=r['cost'],original_annual_effective_rate=r['original_annual_effective_rate'])
    card._admit(dict(schema='card-credit-accounting-1',contract=deepcopy(c),path_id='card2-domain',economic_source=deepcopy(r['economic_source']),
        acquisition=acquisition,policy=policy,termination=deepcopy(closure),claim_id=raw['claim_id'],reporting_end_ordinal=raw['reporting_ordinal'],opening_allowance=0.,allowance_reviews=[],chargeoffs=[]))
    if c['collection_fraction']!=0. or c['collection_cost']!=0.:raise ArithmeticError('card2 principal recovery ownership unsupported')
    paired=(r['policy']['accrued']=='transfer_until_reviewed_reversal' and r['policy']['basis']=='transfer_until_reviewed_chargeoff') or (r['policy']['accrued']=='reverse_income_at_default' and r['policy']['basis']=='recognize_in_earnings_at_default')
    if not paired:raise ArithmeticError('paired gross and contra default policies')
    default=False;limits=0
    for step in c['steps']:
        shared.checkpoint(check)
        if step['ordinal']<=start or close is not None and step['ordinal']>close or default:raise ArithmeticError('source actions outside reviewed owner')
        a=step['action']
        if a['kind']=='payment':pass
        elif a['kind']=='default' and close is None:default=True
        elif a=={'kind':'set_limit','amount':0.} and step['ordinal']==close:limits+=1
        else:raise ArithmeticError('unsupported fee/grace/draw/rate/state/limit')
    if close is None and not default or close is not None and limits!=1:raise ArithmeticError('explicit actual default or zero-limit closure')
    seen=set()
    for recovery in raw['interest_recoveries']:
        shared.checkpoint(check);fields(recovery,'recovery claim_id recognition_ordinal amount policy_review source excess_cash')
        for k in ('recovery','policy_review','source'):identity(recovery[k])
        day(recovery['recognition_ordinal']);money(recovery['amount']);text(recovery['claim_id'])
        key=(recovery['recovery']['id'],recovery['recovery']['revision'])
        if recovery['amount']<=0. or not start<recovery['recognition_ordinal']<=end or recovery['claim_id']!=raw['claim_id'] or key in seen or recovery['excess_cash'] not in {'credit_allowance','recognize_in_earnings'}:
            raise ArithmeticError('unique reviewed bounded actual interest recovery')
        seen.add(key)
    days=c['horizon_days'];steps=len(c['steps']);collections=len(raw['interest_recoveries'])
    max_actions=days+2*steps+1+collections;max_events=days+3*steps+2+2*collections
    if max_actions>256 or max_events>256:raise ArithmeticError('card2 atomic arena bound')
    raw_bytes=_size(raw);metadata=_size(r)+_size(p)
    reserved=2*raw_bytes+(days+1)*(8192+metadata)+max_events*(4096+3*metadata)+max_actions*(8192+2*metadata)+days*(16384+4*metadata)+512*1024
    work=(days+1)*(steps+len(c['cycles'])+128)+max_events*128+collections*1024
    if reserved>16*1024*1024 or work>2_000_000:raise ArithmeticError('card2 aggregate byte/work admission')
    return dict(start=start,end=end,closure=close,reserved_bytes=reserved,work=work,max_actions=max_actions,max_events=max_events)


SNAPSHOT_KEYS=('core','live_interest_basis','claim_interest_basis','cash_received','original_eir_income','reviewed_basis_income','shared_default_income','last_ordinal','action_count')


def snapshot(book):return {k:deepcopy(book[k]) for k in SNAPSHOT_KEYS}


def reconstruct(raw,check=lambda:False):
    admitted=_admit(raw,check);raw=_typed(raw);start=admitted['start'];end=admitted['end'];closure=admitted['closure'];economic=card.project(raw['contract'],check);book=new(raw['review'],check)
    if economic['drawn_principal']!=0. or any(x['fees']!=0. or x['principal']<0. or x['interest']<0. for x in economic['cashflows']):raise ArithmeticError('unsupported actual cash ownership')
    default=economic['default']
    if closure is None and (default is None or default['principal']!=0.) or closure is not None and default is not None:raise ArithmeticError('actual interest-only default or reviewed live closure')
    earned={};cash={};recoveries={}
    for i,row in enumerate(economic['recognition']):
        if row['end_ordinal']-row['start_ordinal']!=1:raise ArithmeticError('actual daily recognition intervals')
        earned.setdefault(row['end_ordinal'],[]).append(i)
    for i,row in enumerate(economic['cashflows']):cash.setdefault(row['ordinal'],[]).append(i)
    for i,row in enumerate(raw['interest_recoveries']):recoveries.setdefault(row['recognition_ordinal'],[]).append(i)
    opening=snapshot(book);daily=[dict(ordinal=start,snapshot=deepcopy(opening),actions=dict(start=0,end=0))];actions=[];stopped=False
    def action(d,kind,source,indices=None,event_id=None,recovery_index=None,**kwargs):
        cursors=[len(book[k]) for k in ('events','shared_intents','income_steps')]
        apply(book,dict(kind=kind,ordinal=d,**kwargs),check)
        ranges={k:dict(start=left,end=len(book[k])) for k,left in zip(('events','shared_intents','income_steps'),cursors)}
        actions.append(dict(atomic_action=book['action_count'],ordinal=d,**ranges,
            source=dict(kind=source,recognition_indices=indices or [],cash_event_id=event_id,interest_recovery_index=recovery_index),committed=snapshot(book)))
        if len(actions)>256 or len(book['events'])>256:raise ArithmeticError('bounded atomic arenas')
    for d in range(start+1,end+1):
        shared.checkpoint(check);first=len(actions)
        if not stopped:
            indices=earned.get(d,[]);value=sum((economic['recognition'][i]['interest'] for i in indices),0.)
            action(d,'earned','native_recognition_and_original_yield',indices=indices,amount=value)
        for i in cash.get(d,[]):
            if stopped:raise ArithmeticError('post-exit actual cash needs explicit ownership')
            row=economic['cashflows'][i]
            if row['interest']!=0.:action(d,'live_interest_cash','native_interest_cash',event_id=row['event_id'],amount=row['interest'])
            if row['principal']!=0.:action(d,'terminal_principal_cash','native_terminal_principal_cash',event_id=row['event_id'],amount=row['principal'])
        if default is not None and default['ordinal']==d:
            equal(book['core']['live'][0]['accrued_interest'],default['accrued_interest'],'source-default/gross-right')
            action(d,'interest_only_default','native_interest_only_default',claim_id=raw['claim_id']);stopped=True
        for i in recoveries.get(d,[]):action(d,'claim_interest_cash','reviewed_actual_interest_recovery',recovery_index=i,settlement=raw['interest_recoveries'][i])
        point=economic['points'][d-start]
        if closure==d:
            if not complete(book) or any(v!=0. for k in ('principal','billed_interest','unbilled_interest','fees') for v in point[k]) or point['limit']!=0. or point['undrawn']!=0.:
                raise ArithmeticError('closure retains gross source rights or commitment')
            stopped=True
        if not stopped:
            equal(book['core']['live'][0]['principal'],sum(point['principal'],0.),'source/live-principal')
            equal(book['core']['live'][0]['accrued_interest'],sum(point['billed_interest']+point['unbilled_interest'],0.),'source/gross-accrued')
        daily.append(dict(ordinal=d,snapshot=snapshot(book),actions=dict(start=first,end=len(actions))))
    out=dict(schema='card-credit-accounting-result-2',raw_request=deepcopy(raw),economic=economic,acquisition_opening=opening,actions=actions,
        events=deepcopy(book['events']),shared_intents=deepcopy(book['shared_intents']),income_steps=deepcopy(book['income_steps']),daily=daily,
        reporting=deepcopy(daily[raw['reporting_ordinal']-start]['snapshot']),closing=snapshot(book),complete=complete(book),
        admitted_work=admitted['work'],reserved_bytes=admitted['reserved_bytes'],mixed_ledger_adoption=False,accounting_standard_approval=False,empirical_validation=False)
    if _size(out)>admitted['reserved_bytes']:raise ArithmeticError('complete raw economics/arenas exceed reservation')
    shared.checkpoint(check);return out


def _equal(actual,expected,path='card2'):
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or set(actual)!=set(expected):raise ArithmeticError(path+': exact field set')
        for k,v in expected.items():_equal(actual[k],v,path+'/'+k)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': complete ordered shape')
        for i,v in enumerate(expected):_equal(actual[i],v,path+'/'+str(i))
    elif type(expected) is int and (path.rsplit('/',1)[-1].endswith('ordinal') or path.rsplit('/',1)[-1] in {'start','end','atomic_action','action_count','interest_recovery_index','admitted_work','reserved_bytes'} or any(k in path.split('/') for k in ('recognition_indices','horizon_days'))):
        if type(actual) is not int or actual!=expected:raise ArithmeticError(path+': exact integer identity')
    else:card._equal(actual,expected,path)


def verify(raw,result,check=lambda:False):
    expected=reconstruct(raw,check)
    if set(result)!=set(expected)|{'retained_capacity_charge'}:raise ArithmeticError('complete card2 result fields')
    charge=result['retained_capacity_charge']
    if type(charge) is not int or not 0<charge<=expected['reserved_bytes']:raise ArithmeticError('bounded exact retained capacity metadata')
    # Allocator-specific vector capacities are not financial source truth. The
    # full serialized payload is an independent lower bound on their charge.
    charged=deepcopy(result);charged['retained_capacity_charge']=0
    if charge<_size(charged):raise ArithmeticError('retained charge below complete escaped payload')
    _equal({k:result[k] for k in expected},expected);shared.checkpoint(check)
    return dict(verified=True,raw_card_source_recomputed=True,original_yield_income_recomputed=True,
        atomic_gross_basis_rights_recomputed=True,compact_arena_ranges_recomputed=True,full_gross_tail_retained=True,
        original_eir_distinct_from_basis_realization=True,mixed_ledger_adoption=False,empirical_validation=False)


