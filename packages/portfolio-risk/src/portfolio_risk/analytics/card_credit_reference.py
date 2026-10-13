"""Independent acquired non-grace card credit/accounting source verifier.

Raw supplied actions own economics; reviewed original EIR and credit policy own
accounting. No native financial callback or production calculation backend. A standalone
source receipt does not prove mixed ledger adoption or empirical validation.
"""
from copy import deepcopy
import json
import math
import sys
from portfolio_risk.analytics import consumer_credit_reference as shared

fields=shared.fields;identity=shared.identity;text=shared.text;amount=shared.amount
finite=shared.finite;equal=shared.equal;checkpoint=shared.checkpoint
subtract_owned=shared.subtract_owned;_empty_live=shared._empty_live
CLASSES=('purchase','cash_advance','promotional')
SCHEMA='card-credit-accounting-1'
POLICY=dict(contractual_recognition='raw_card_non_grace_earned_then_billed-1',
    original_income='frozen_original_account_eir_until_exit-1',
    terminal_basis='recognize_in_earnings_at_reviewed_closure-1',
    recovery_cost='gross_recovery_with_credit_loss_cost-1',
    commitment='retained_source_undrawn_no_ccf_no_draw-1',unsupported_grace_fees_draws='reject')


def ordinal(v):
    if type(v) is not int or not 1<=v<=3652059:raise ArithmeticError('card exact bounded ordinal required')


def _admit(raw):
    fields(raw,'schema contract path_id economic_source acquisition policy termination claim_id reporting_end_ordinal opening_allowance allowance_reviews chargeoffs')
    c=raw['contract'];a=raw['acquisition'];p=raw['policy'];t=raw['termination']
    fields(c,'schema identity as_of_ordinal horizon_days opening_principal opening_billed_interest opening_unbilled_interest opening_unrecognized_billed_interest opening_fees opening_contingent_purchase_interest opening_statement_remaining opening_minimum_remaining opening_due_ordinal initial_limit initial_state rates day_count initial_purchase_grace regain_grace_after_full_statement_payment compound_billed_interest minimum cycles steps collection_fraction collection_cost recovery_delay_days')
    fields(c['identity'],'position_id original_loan_id cohort_id currency source_revision model_version assumption_id run_id measure')
    for value in c['identity'].values():text(value)
    fields(a,'identity original_yield_anchor income_review accounting_basis purchase_ordinal principal cost original_annual_effective_rate')
    fields(p,'identity accounting recovery_review cost_review contractual_recognition original_income terminal_basis recovery_cost commitment unsupported_grace_fees_draws')
    q=p['accounting'];fields(q,'identity accrued basis allowance_shortfall excess_cash_recovery')
    for value in (raw['economic_source'],a['identity'],a['original_yield_anchor'],a['income_review'],p['identity'],q['identity'],p['recovery_review'],p['cost_review']):identity(value)
    for key,value in POLICY.items():
        if p[key]!=value:raise ArithmeticError('card explicit reviewed accounting policy required')
    if (q['accrued'] not in {'reverse_income_at_default','transfer_until_reviewed_reversal'} or q['basis'] not in {'transfer_until_reviewed_chargeoff','recognize_in_earnings_at_default'}
            or q['allowance_shortfall'] not in {'reject','expense_at_reviewed_chargeoff'} or q['excess_cash_recovery'] not in {'credit_allowance','recognize_in_earnings'}):raise ArithmeticError('card shared policy unsupported')
    text(raw['path_id']);text(raw['claim_id']);ordinal(c['as_of_ordinal']);ordinal(a['purchase_ordinal']);ordinal(raw['reporting_end_ordinal'])
    if type(c['horizon_days']) is not int or not 1<=c['horizon_days']<=1080:raise ArithmeticError('card complete days bound')
    start=c['as_of_ordinal'];end=start+c['horizon_days'];ordinal(end)
    for key in ('initial_purchase_grace','regain_grace_after_full_statement_payment','compound_billed_interest'):
        if type(c[key]) is not bool:raise ArithmeticError('card exact grace/compound boolean')
    for key in ('opening_principal','opening_billed_interest','opening_unbilled_interest','opening_unrecognized_billed_interest','opening_fees','opening_statement_remaining'):
        if not isinstance(c[key],list) or len(c[key])!=3:raise ArithmeticError('card three-class opening state required')
        for value in c[key]:amount(value)
    for value in (a['principal'],a['cost'],raw['opening_allowance'],c['initial_limit'],c['opening_minimum_remaining'],c['opening_contingent_purchase_interest'],c['collection_cost']):amount(value)
    finite(a['original_annual_effective_rate'],-.99,10.);finite(c['collection_fraction'],0.,1.)
    if (raw['schema']!=SCHEMA or c['schema']!='consumer-card-1' or c['identity']['model_version']!=c['schema'] or c['identity']['measure']!='physical'
            or c['initial_state']!='performing' or c['initial_purchase_grace'] or c['regain_grace_after_full_statement_payment']
            or any(c['opening_billed_interest']+c['opening_unbilled_interest']+c['opening_unrecognized_billed_interest']+c['opening_fees'])
            or c['opening_contingent_purchase_interest']!=0. or a['principal']!=sum(c['opening_principal']) or a['principal']<=0. or a['cost']<=0.
            or a['purchase_ordinal']!=start or a['accounting_basis']!='supplied_non_pcd_gross_cost-1'
            or raw['economic_source']['revision']!=c['identity']['source_revision'] or not start<=raw['reporting_end_ordinal']<=end
            or c['day_count'] not in {'act360','act365_fixed'}):raise ArithmeticError('card physical acquired non-grace no-fee source required')
    fields(c['rates'],'purchase cash_advance promotional promotional_expiry_ordinal post_promotion')
    for key in ('purchase','cash_advance','promotional','post_promotion'):finite(c['rates'][key],0.,2.)
    ordinal(c['rates']['promotional_expiry_ordinal'])
    m=c['minimum'];fields(m,'floor principal_fraction include_billed_interest include_fees allocation_order');amount(m['floor']);finite(m['principal_fraction'],0.,1.)
    if type(m['include_billed_interest']) is not bool or type(m['include_fees']) is not bool or not isinstance(m['allocation_order'],list) or sorted(m['allocation_order'])!=sorted(CLASSES):raise ArithmeticError('card exact minimum convention')
    if any(v>c['opening_principal'][i] for i,v in enumerate(c['opening_statement_remaining'])) or c['opening_minimum_remaining']>sum(c['opening_statement_remaining']):raise ArithmeticError('card opening statement exceeds debt')
    due=c['opening_due_ordinal']
    if due is not None:
        ordinal(due)
        if not start<=due<=end:raise ArithmeticError('card opening due calendar')
    elif any(c['opening_statement_remaining']) or c['opening_minimum_remaining']!=0.:raise ArithmeticError('card opening statement needs due date')
    if not isinstance(c['cycles'],list) or not isinstance(c['steps'],list) or len(c['cycles'])+len(c['steps'])>4096:raise ArithmeticError('card event count bound')
    if (c['horizon_days']+1)*(3+len(c['cycles']))>250000:raise ArithmeticError('card daily source work admission')
    previous=due if due is not None else start-1
    for cycle in c['cycles']:
        fields(cycle,'statement_ordinal due_ordinal');ordinal(cycle['statement_ordinal']);ordinal(cycle['due_ordinal'])
        if not previous<cycle['statement_ordinal']<cycle['due_ordinal']<=end or cycle['statement_ordinal']<start:raise ArithmeticError('card cycles overlap/outside source')
        previous=cycle['due_ordinal']
    closure=None
    if t=={'kind':'source_default'}:pass
    else:
        fields(t,'kind ordinal review');identity(t['review']);ordinal(t['ordinal'])
        if t['kind']!='reviewed_zero_rights_closure' or not start<t['ordinal']<=end:raise ArithmeticError('card explicit closure review')
        closure=t['ordinal']
    previous=start;limits=0
    for step in c['steps']:
        fields(step,'ordinal action');ordinal(step['ordinal']);action=step['action']
        if not start<step['ordinal']<=end or step['ordinal']<previous or closure is not None and step['ordinal']>closure:raise ArithmeticError('card ordered postacquisition actions required')
        previous=step['ordinal']
        if action=={'kind':'default'}:continue
        fields(action,'kind amount');amount(action['amount'])
        if action['kind']=='payment':continue
        if action['kind']=='set_limit' and action['amount']==0. and step['ordinal']==closure:limits+=1
        else:raise ArithmeticError('card unsupported draw/fee/state/rate/limit action')
    if closure is not None and limits!=1:raise ArithmeticError('card closure actual limit retirement required')
    if type(c['recovery_delay_days']) is not int or not 0<=c['recovery_delay_days']<=36600:raise ArithmeticError('card exact recovery delay')
    seen=set();days=set()
    for collection,kind in ((raw['allowance_reviews'],'allowance'),(raw['chargeoffs'],'chargeoff')):
        if not isinstance(collection,list):raise ArithmeticError('card ordered reviewed instructions')
        for r in collection:
            fields(r,'review recognition_ordinal phase target_amount' if kind=='allowance' else 'review claim_id recognition_ordinal principal_amount book_adjustment_amount accrued_interest_reversal')
            identity(r['review']);ordinal(r['recognition_ordinal']);key=(r['review']['id'],r['review']['revision'])
            if key in seen or not start<r['recognition_ordinal']<=end:raise ArithmeticError('card duplicate/outside review')
            seen.add(key)
            if kind=='allowance':
                amount(r['target_amount'])
                if r['phase'] not in {'before_credit','after_credit'} or r['recognition_ordinal'] in days:raise ArithmeticError('card ambiguous allowance phase/day')
                days.add(r['recognition_ordinal'])
            else:
                amount(r['principal_amount']);finite(r['book_adjustment_amount'],-1e15,1e15);finite(r['accrued_interest_reversal'],-1e15,1e15)
                if r['claim_id']!=raw['claim_id']:raise ArithmeticError('card reviewed claim identity')
    reviews=len(raw['allowance_reviews'])+len(raw['chargeoffs']);work=(c['horizon_days']+1)*(len(c['steps'])+len(c['cycles'])+reviews+72)
    if reviews>64 or work>2000000 or len(json.dumps(raw,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())>1024*1024:raise ArithmeticError('card bounded raw work/metadata admission')
    return closure,work


def project(c,check=lambda:False):
    """Supplied three-class accrual, minimum/excess waterfall and exact default."""
    p=list(c['opening_principal']);billed=[0.]*3;unbilled=[0.]*3;remaining=list(c['opening_statement_remaining'])
    minimum=c['opening_minimum_remaining'];limit=c['initial_limit'];state='performing';collected=0.;default=None
    points=[];cash=[];recognition=[];statements=[];actions={};cycles={x['statement_ordinal']:x for x in c['cycles']}
    for step in c['steps']:actions.setdefault(step['ordinal'],[]).append(step['action'])
    def apr(i,day):return c['rates']['post_promotion' if i==2 and day>=c['rates']['promotional_expiry_ordinal'] else CLASSES[i]]
    start=c['as_of_ordinal'];end=start+c['horizon_days']
    for day in range(start,end+1):
        checkpoint(check)
        if day>start and state!='default':
            for i in range(3):
                earned=(p[i]+(billed[i] if c['compound_billed_interest'] else 0.))*apr(i,day-1)*shared.year_fraction(day-1,day,c['day_count'])
                unbilled[i]+=earned
                if earned!=0.:recognition.append(dict(origin_id=f'lot-{i}',start_ordinal=day-1,end_ordinal=day,interest=earned))
        for action in actions.get(day,[]):
            if state=='default':raise ArithmeticError('card action after default')
            if action['kind']=='set_limit':limit=0.;continue
            if action['kind']=='default':
                principal=sum(p);interest=sum(billed)+sum(unbilled);gross=principal*c['collection_fraction'];cost=c['collection_cost'];net=max(gross-cost,0.)
                recovery=day+c['recovery_delay_days']
                if recovery>end:raise ArithmeticError('card full recovery tail required')
                default=dict(ordinal=day,principal=principal,accrued_interest=interest,recognized_accrued_interest=interest,unrecognized_contract_interest=0.,fees=0.,gross_recovery=gross,recovery_cost=cost,net_recovery=net,principal_loss=principal-net,recovery_ordinal=recovery)
                if net!=0. or cost>gross:cash.append(dict(event_id='recovery-0',ordinal=recovery,kind='recovery',principal=net,interest=0.,fees=-max(cost-gross,0.)))
                p=[0.]*3;billed=[0.]*3;unbilled=[0.]*3;remaining=[0.]*3;minimum=limit=0.;state='default';continue
            value=action['amount']
            if value>sum(p)+sum(billed)+1e-9:raise ArithmeticError('card payment exceeds billed debt')
            first=min(value,minimum);minimum-=first;paid_p=paid_i=0.
            orders=[([CLASSES.index(x) for x in c['minimum']['allocation_order']],first),(sorted(range(3),key=lambda i:(-apr(i,day),i)),value-first)]
            for order,available in orders:
                for i in order:
                    if available<=0.:break
                    ic=min(available,billed[i]);billed[i]-=ic;available-=ic;paid_i+=ic
                    pc=min(available,p[i]);p[i]-=pc;available-=pc;paid_p+=pc;remaining[i]=max(remaining[i]-ic-pc,0.)
            collected+=paid_p;cash.append(dict(event_id='payment-'+str(len(cash)),ordinal=day,kind='payment',principal=paid_p,interest=paid_i,fees=0.))
        if state!='default':
            if day==c['opening_due_ordinal'] or any(x['due_ordinal']==day for x in c['cycles']):remaining=[0.]*3
            if day in cycles:
                for i in range(3):billed[i]+=unbilled[i];unbilled[i]=0.
                total=sum(p)+sum(billed);rule=c['minimum'];minimum=min(max(rule['floor'],sum(p)*rule['principal_fraction']+(sum(billed) if rule['include_billed_interest'] else 0.)),total)
                remaining=[p[i]+billed[i] for i in range(3)]
                statements.append(dict(ordinal=day,due_ordinal=cycles[day]['due_ordinal'],principal=p.copy(),billed_interest=billed.copy(),fees=[0.]*3,minimum_payment=minimum,statement_total=total))
        debt=sum(p)+sum(billed)
        for value in p+billed+unbilled+[debt,collected]:amount(value)
        points.append(dict(ordinal=day,state=state,principal=p.copy(),billed_interest=billed.copy(),unbilled_interest=unbilled.copy(),fees=[0.]*3,limit=limit,utilization=debt/limit if limit>0. else None,undrawn=max(limit-debt,0.),purchase_grace=False))
    equal(sum(c['opening_principal']),collected+(default['principal'] if default is not None else 0.)+sum(p),'card/principal-conservation')
    return dict(schema=c['schema'],identity=deepcopy(c['identity']),statements=statements,cashflows=sorted(cash,key=lambda x:x['ordinal']),recognition=recognition,points=points,default=default,
        opening_principal=sum(c['opening_principal']),drawn_principal=0.,collected_principal=collected,closing_principal=sum(p),closing_interest=sum(billed),closing_unbilled_interest=sum(unbilled),closing_unrecognized_billed_interest=0.,closing_fees=0.,closing_grace_contingent_interest=0.)


def _shared_day(raw,state,default,recovery,recovery_origin_ids,day,emit,allowance):
    p=raw['policy']['accounting'];policy=raw['policy'];live=state['live'][0]
    ids=[f'default:{day}'] if default is not None and default['ordinal']==day else recovery_origin_ids if recovery is not None and recovery['ordinal']==day else []
    reviews=[r for r in raw['allowance_reviews'] if r['recognition_ordinal']==day]
    if len(reviews)>1:
        raise ArithmeticError('consumer-credit ambiguous same-day allowance review')
    if reviews and reviews[0]['phase']=='before_credit':
        allowance(reviews[0],day,ids)
    if default is not None and default['ordinal']==day:
        principal=live['principal'];accrued=live['accrued_interest'];basis=live['book_adjustment'];claimid=raw['claim_id']
        equal(principal,default['principal']);equal(accrued,default['recognized_accrued_interest'])
        if principal+basis<0.:
            raise ArithmeticError('consumer-credit default principal plus allocated basis cannot be negative')
        reverse=p['accrued']=='reverse_income_at_default';basis_expense=p['basis']=='recognize_in_earnings_at_default'
        emit(day,'economic_default_transfer',[('live_principal',-principal),('default_principal',principal)],ids,claim=claimid)
        emit(day,'default_accrued_income_reversal' if reverse else 'default_accrued_transfer',
            [('live_accrued_interest',-accrued),('interest_income' if reverse else 'default_accrued_interest',accrued)],ids,claim=claimid)
        emit(day,'default_basis_earnings' if basis_expense else 'default_basis_transfer',
            [('live_book_adjustment',-basis),('basis_recognition_expense' if basis_expense else 'default_book_adjustment',basis)],ids,claim=claimid)
        state['claims'].append(dict(claim_id=claimid,economic_source=deepcopy(raw['economic_source']),recovery_schedule_review=deepcopy(policy['recovery_review']),policy=deepcopy(p),default_ordinal=day,
            initial_principal=principal,economic_principal_loss=recovery['principal_only_loss'],initial_accrued_interest=accrued,initial_book_adjustment=basis,
            outstanding_principal=principal,outstanding_accrued_interest=0. if reverse else accrued,outstanding_book_adjustment=0. if basis_expense else basis,
            principal_charged_off=0.,accrued_reversed=accrued if reverse else 0.,basis_recognized=basis if basis_expense else 0.,recovery_cash_collected=0.,recovery_applied_to_principal=0.,
            interest_recovery_cash_collected=0.,interest_recovery_applied=0.,settled_interest_recovery_ids=[],
            remaining_recoveries=[] if recovery['gross_cash']==0. else [dict(payment_ordinal=recovery['ordinal'],amount=recovery['gross_cash'])]))
        state['live'][0]=_empty_live()
    for review in raw['chargeoffs']:
        if review['recognition_ordinal']!=day:
            continue
        claim=next((q for q in state['claims'] if q['claim_id']==review['claim_id']),None)
        if claim is None:
            raise ArithmeticError('consumer-credit reviewed chargeoff lacks actual default claim')
        pc=review['principal_amount'];book=review['book_adjustment_amount'];ar=review['accrued_interest_reversal'];net=pc+book
        if pc<0. or net<0.:
            raise ArithmeticError('consumer-credit chargeoff exceeds owned signed claims')
        remaining_pc=subtract_owned(claim['outstanding_principal'],pc)
        remaining_book=subtract_owned(claim['outstanding_book_adjustment'],book)
        remaining_accrued=subtract_owned(claim['outstanding_accrued_interest'],ar)
        covered=min(net,state['allowance']);shortfall=net-covered
        if shortfall!=0. and p['allowance_shortfall']=='reject':
            raise ArithmeticError('consumer-credit reviewed chargeoff exceeds allowance')
        emit(day,'reviewed_credit_chargeoff',[('default_principal',-pc),('default_book_adjustment',-book),('allowance',covered),('credit_loss_expense',shortfall)],ids,source=review['review'],claim=claim['claim_id'])
        emit(day,'reviewed_accrued_income_reversal',[('default_accrued_interest',-ar),('interest_income',ar)],ids,source=review['review'],claim=claim['claim_id'])
        claim['outstanding_principal']=remaining_pc;claim['outstanding_book_adjustment']=remaining_book;claim['outstanding_accrued_interest']=remaining_accrued
        claim['principal_charged_off']+=pc;claim['basis_recognized']+=book;claim['accrued_reversed']+=ar;state['allowance']-=covered
    for claim in state['claims']:
        due=next((q['amount'] for q in claim['remaining_recoveries'] if q['payment_ordinal']==day),0.)
        if due!=0.:
            applied=min(due,claim['outstanding_principal']);excess=due-applied
            allowance_credit=excess if p['excess_cash_recovery']=='credit_allowance' else 0.;income=excess-allowance_credit
            emit(day,'modeled_physical_recovery_cash',[('cash',due),('default_principal',-applied),('allowance',-allowance_credit),('recovery_income',-income)],ids,source=policy['recovery_review'],claim=claim['claim_id'])
            claim['outstanding_principal']-=applied;claim['recovery_applied_to_principal']+=applied;claim['recovery_cash_collected']+=due;state['allowance']+=allowance_credit
        claim['remaining_recoveries']=[q for q in claim['remaining_recoveries'] if q['payment_ordinal']>day]
    if reviews and reviews[0]['phase']=='after_credit':
        allowance(reviews[0],day,ids)
    if recovery is not None and recovery['ordinal']==day and recovery['cost_cash']!=0.:
        emit(day,'card_reviewed_recovery_cost',[('cash',-recovery['cost_cash']),('credit_loss_expense',recovery['cost_cash'])],recovery_origin_ids,review=policy['cost_review'],claim=raw['claim_id'])


def _day(economic,index,state,exit_day,effective=0.,coupon=0.,cash=0.,earnings=0.):
    point=economic['points'][index]
    return dict(ordinal=point['ordinal'],economic_state=point['state'],
        accounting_status='acquired' if index==0 else ('default_claims' if economic['default'] is not None else 'reviewed_closed') if point['ordinal']>=exit_day else 'live',
        owned=deepcopy(state),source_billed_interest=deepcopy(point['billed_interest']),source_unbilled_interest=deepcopy(point['unbilled_interest']),
        source_limit=point['limit'],source_undrawn=point['undrawn'],effective_interest=effective,contractual_interest=coupon,cash_delta=cash,earnings_delta=earnings)


def reconstruct(raw,check=lambda:False):
    checkpoint(check);closure,work=_admit(raw);c=raw['contract'];a=raw['acquisition'];policy=raw['policy'];p=policy['accounting']
    start=c['as_of_ordinal'];end=start+c['horizon_days'];economic=project(c,check);default=economic['default'];recovery=None
    if closure is None:
        if default is None:raise ArithmeticError('card declared source default missing')
        exit_day=default['ordinal']
    else:
        if default is not None:raise ArithmeticError('card default cannot masquerade as closure')
        point=economic['points'][closure-start]
        if any(point['principal']+point['billed_interest']+point['unbilled_interest']+point['fees']) or point['limit']!=0. or point['undrawn']!=0.:raise ArithmeticError('card reviewed closure has financial rights')
        exit_day=closure
    if any(economic[key]!=0. for key in ('drawn_principal','closing_principal','closing_interest','closing_unbilled_interest','closing_unrecognized_billed_interest','closing_fees','closing_grace_contingent_interest')) or economic['points'][-1]['limit']!=0. or economic['points'][-1]['undrawn']!=0.:
        raise ArithmeticError('card complete zero financial/commitment source tail required')
    if default is not None:
        if default['gross_recovery']>default['principal'] or default['recovery_cost']>default['gross_recovery']:raise ArithmeticError('card surplus/uncovered gross recovery cost unsupported')
        recovery=dict(ordinal=default['recovery_ordinal'],gross_cash=default['gross_recovery'],cost_cash=default['recovery_cost'],net_cash=default['net_recovery'],
            principal_only_loss=default['principal']-default['gross_recovery'],total_economic_loss=default['principal_loss'],cost_review=deepcopy(policy['cost_review']))
    # Reuse the independent frozen-yield recurrence, never a native result.
    prefix_request,prefix=shared._prefix(raw,economic,exit_day,None if default is not None else {},check)
    for event in prefix_request['events']:event['event_id']='card-'+event['event_id']
    for row in prefix['rows']:row['event_id']='card-'+row['event_id']
    state=dict(live=[dict(principal=a['principal'],undrawn=0.,accrued_interest=0.,book_adjustment=a['cost']-a['principal']),_empty_live(),_empty_live()],allowance=raw['opening_allowance'],claims=[])
    opening=deepcopy(state);intents=[];lineage=[];daily=[_day(economic,0,state,exit_day)]
    cash_by_day={};recognition_by_day={}
    for x in economic['cashflows']:cash_by_day.setdefault(x['ordinal'],[]).append(x)
    for index,x in enumerate(economic['recognition']):recognition_by_day.setdefault(x['end_ordinal'],[]).append(index)
    recovery_origin_ids=([f'default:{default["ordinal"]}'] if default is not None else [])+[x['event_id'] for x in economic['cashflows'] if x['kind']=='recovery']
    def emit(day,event,lines,ids,source=None,review=None,claim=None,recognition=None):
        postings=[dict(account=code,amount=value) for code,value in lines if value!=0.]
        if not postings:return
        for entry in postings:finite(entry['amount'],-1e15,1e15)
        scale=sum(abs(x['amount']) for x in postings)
        tolerance=1e-9+1e-12*scale if event.startswith('card_') else max(1e-9,64*sys.float_info.epsilon*scale)
        if abs(sum(x['amount'] for x in postings))>tolerance:raise ArithmeticError('card independent intent balance fails')
        lineage.append(dict(ordinal=day,phase=event,intent_index=len(intents),source_event_ids=deepcopy(ids),source_recognition_indices=list(recognition or []),
            original_yield_anchor=deepcopy(a['original_yield_anchor']),economic_source=deepcopy(raw['economic_source']),path_id=raw['path_id']))
        intents.append(dict(recognition_ordinal=day,event=event,position_id=c['identity']['position_id'],claim_id=claim,policy=deepcopy(review or p['identity']),source=deepcopy(source or raw['economic_source']),postings=postings))
    def allowance(review,day,ids):
        target=amount(review['target_amount']);change=target-state['allowance'];state['allowance']=target
        emit(day,'reviewed_allowance_target',[('credit_loss_expense',change),('allowance',-change)],ids,source=review['review'])
    for day in range(start+1,end+1):
        checkpoint(check);begin=len(intents);effective=coupon=0.;live=state['live'][0]
        if day<=exit_day:
            row=prefix['rows'][day-start-1];coupon=row['contractual_interest_earned'];effective=row['effective_interest']
            live['accrued_interest']+=coupon;live['book_adjustment']+=row['basis_amortization']
            emit(day,'card_original_effective_interest',[('live_accrued_interest',coupon),('live_book_adjustment',row['basis_amortization']),('interest_income',-effective)],[],review=a['income_review'],recognition=recognition_by_day.get(day,[]))
            for cash in cash_by_day.get(day,[]):
                if cash['kind']=='recovery':continue
                live['principal']-=cash['principal'];live['accrued_interest']-=cash['interest']
                emit(day,'card_contractual_settlement',[('cash',cash['principal']+cash['interest']),('live_principal',-cash['principal']),('live_accrued_interest',-cash['interest'])],[cash['event_id']],review=policy['identity'])
        if day==closure:
            basis=live['book_adjustment'];live['book_adjustment']=0.
            emit(day,'card_reviewed_closure_basis_retirement',[('live_book_adjustment',-basis),('basis_recognition_expense',basis)],[f'closure:{day}'],review=raw['termination']['review'])
        if live['principal']+live['book_adjustment']<0.:raise ArithmeticError('card opening live principal plus basis cannot be negative')
        _shared_day(raw,state,default,recovery,recovery_origin_ids,day,emit,allowance)
        point=economic['points'][day-start]
        equal(sum(x['principal'] for x in state['live']),sum(point['principal']),'card/source-principal')
        equal(sum(x['accrued_interest'] for x in state['live']),sum(point['billed_interest'])+sum(point['unbilled_interest']),'card/source-earned-right')
        postings=[x for intent in intents[begin:] for x in intent['postings']]
        cash=sum((x['amount'] for x in postings if x['account']=='cash'),0.)
        earnings=-sum((x['amount'] for x in postings if x['account'] in {'interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'}),0.)
        daily.append(_day(economic,day-start,state,exit_day,effective,coupon,cash,earnings))
    equal(sum(d['cash_delta'] for d in daily),sum(x['principal']+x['interest']+x['fees'] for x in economic['cashflows']),'card/source-cash-conservation')
    result=dict(schema=SCHEMA,raw_request=deepcopy(raw),economic=economic,income_prefix_request=prefix_request,income_prefix=prefix,acquisition_opening=opening,intents=intents,lineage=lineage,
        statement_lineage=[dict(source_index=i,ordinal=s['ordinal'],due_ordinal=s['due_ordinal'],interest_recognized_again=False) for i,s in enumerate(economic['statements'])],
        commitment_source=dict(policy=policy['commitment'],policy_review=deepcopy(policy['identity']),retirement_ordinal=exit_day,retirement_kind=raw['termination']['kind'],funded_contingent_draw=0.),
        daily=daily,reporting=deepcopy(daily[raw['reporting_end_ordinal']-start]),closing=deepcopy(state),recovery=recovery,admitted_work=work,mixed_ledger_adoption=False,empirical_validation=False)
    if len(json.dumps(result,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())>64*1024*1024:raise ArithmeticError('card full retained result capacity')
    checkpoint(check);return result


def _equal(actual,expected,path='card'):
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():raise ArithmeticError(path+': exact fields differ')
        for k,v in expected.items():_equal(actual[k],v,path+'/'+k)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': ordered complete array differs')
        for i,v in enumerate(expected):_equal(actual[i],v,path+'/'+str(i))
    elif type(expected) is int and (path.rsplit('/',1)[-1].endswith('ordinal') or path.rsplit('/',1)[-1] in {'horizon_days','recovery_delay_days','source_index','intent_index','admitted_work'} or 'source_recognition_indices' in path.split('/')):
        if type(actual) is not int or actual!=expected:raise ArithmeticError(path+': exact integer differs')
    else:equal(actual,expected,path)


def verify(raw,result,check=lambda:False):
    expected=reconstruct(raw,check);_equal(result,expected);checkpoint(check)
    return dict(verified=True,supplied_card_source_recomputed=True,original_yield_prefix_recomputed=True,
        reviewed_credit_intents_recomputed=True,gross_recovery_cost_separated=True,statement_income_not_repeated=True,
        mixed_ledger_adoption=False,empirical_validation=False)
