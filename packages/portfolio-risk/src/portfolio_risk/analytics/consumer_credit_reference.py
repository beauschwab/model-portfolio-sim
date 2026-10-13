"""Independent supplied consumer-credit accounting source/publication oracle.

This explicit verifier is never a production financial execution backend or an
empirical default model. A primitive receipt does not verify mixed ledger adoption.
"""
from copy import deepcopy
import math
import sys
import json

from portfolio_risk.analytics.consumer_schedule_reference import year_fraction, checkpoint


def equal(actual, expected, label='consumer-credit'):
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys()!=expected.keys():
            raise ArithmeticError(label+': exact source fields differ')
        for key,value in expected.items():
            equal(actual[key],value,label+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):
            raise ArithmeticError(label+': complete ordered source differs')
        for index,value in enumerate(expected):
            equal(actual[index],value,label+'/'+str(index))
    elif isinstance(expected,int) and not isinstance(expected,bool) and (
            label.split('/')[-1].endswith('ordinal') or any(token in label.split('/') for token in
                ('payment_dates','horizon_days','recovery_delay_days','intent_index','admitted_work'))):
        if type(actual) is not int or actual!=expected:
            raise ArithmeticError(label+': exact integer identity differs')
    elif isinstance(expected,(int,float)) and not isinstance(expected,bool):
        if isinstance(actual,bool) or not isinstance(actual,(int,float)) or not math.isfinite(actual) or abs(actual-expected)>1e-9+1e-12*abs(expected):
            raise ArithmeticError(label+': financial source differs')
    elif actual!=expected or type(actual) is not type(expected):
        raise ArithmeticError(label+': exact source identity differs')


def amount(v):
    if isinstance(v,bool) or not isinstance(v,(int,float)) or not math.isfinite(v) or not 0.<=v<=1e15:
        raise ArithmeticError('consumer-credit finite nonnegative actual-money domain')
    return v


def fields(value, names):
    if not isinstance(value,dict) or set(value)!=set(names.split()):
        raise ArithmeticError('consumer-credit strict versioned fields required')


def text(value):
    if not isinstance(value,str) or not value.strip() or len(value.encode('utf-8'))>256 or any(ord(c)<32 or 127<=ord(c)<160 for c in value):
        raise ArithmeticError('consumer-credit bounded source identity required')


def identity(value):
    fields(value,'id revision')
    text(value['id']);text(value['revision'])


def finite(value, low, high):
    if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value) or not low<=value<=high:
        raise ArithmeticError('consumer-credit finite bounded numeric domain')


def subtract_owned(total,delta):
    """Reviewed signed slice allocation; only the shared roundoff crossing rule."""
    finite(total,-1e15,1e15);finite(delta,-1e15,1e15)
    near=abs(total-delta)<=max(1e-9,64*sys.float_info.epsilon*max(abs(total),abs(delta)))
    if delta!=0. and (total==0. or math.copysign(1.,total)!=math.copysign(1.,delta)) or abs(delta)>abs(total) and not near:
        raise ArithmeticError('consumer-credit chargeoff exceeds owned signed claims')
    result=total-delta
    return 0. if total!=0. and math.copysign(1.,result)!=math.copysign(1.,total) and near else result


def _admit(raw):
    fields(raw,'schema contract path_id economic_source acquisition policy claim_id reporting_end_ordinal opening_allowance allowance_reviews chargeoffs')
    c=raw['contract'];a=raw['acquisition'];policy=raw['policy'];p=policy['accounting']
    fields(c,'schema identity product as_of_ordinal horizon_days original_principal opening_principal opening_accrued_interest opening_unrecognized_interest opening_fees initial_state note_rate disclosed_apr day_count original_payment payment_dates prepayment_penalty_fraction accrue_contract_interest_in_nonaccrual steps')
    fields(c['identity'],'position_id original_loan_id cohort_id currency source_revision model_version assumption_id run_id measure')
    for key,value in c['identity'].items():
        text(value)
    currency=c['identity']['currency']
    if len(currency)!=3 or not currency.isascii() or not currency.isalpha() or not currency.isupper():
        raise ArithmeticError('consumer-credit explicit ISO-style currency required')
    fields(a,'identity original_yield_anchor income_review accounting_basis purchase_ordinal principal cost original_annual_effective_rate')
    if a['accounting_basis']!='supplied_non_pcd_gross_cost-1' or raw['economic_source']['revision']!=c['identity']['source_revision']:
        raise ArithmeticError('consumer-credit reviewed non-PCD original source basis required')
    fields(policy,'accounting recovery_review cost_review original_income terminal_basis recovery_cost')
    fields(p,'identity accrued basis allowance_shortfall excess_cash_recovery')
    for value in (raw['economic_source'],a['identity'],a['original_yield_anchor'],a['income_review'],p['identity'],policy['recovery_review'],policy['cost_review']):
        identity(value)
    text(raw['path_id']);text(raw['claim_id'])
    if (p['accrued'] not in {'reverse_income_at_default','transfer_until_reviewed_reversal'}
        or p['basis'] not in {'transfer_until_reviewed_chargeoff','recognize_in_earnings_at_default'}
        or p['allowance_shortfall'] not in {'reject','expense_at_reviewed_chargeoff'}
        or p['excess_cash_recovery'] not in {'credit_allowance','recognize_in_earnings'}):
        raise ArithmeticError('consumer-credit explicit reviewed accounting policy required')
    for key in ('as_of_ordinal','horizon_days'):
        if type(c[key]) is not int:
            raise ArithmeticError('consumer-credit exact integer calendar required')
    if type(a['purchase_ordinal']) is not int or type(raw['reporting_end_ordinal']) is not int or type(c['accrue_contract_interest_in_nonaccrual']) is not bool:
        raise ArithmeticError('consumer-credit exact integer/bool source fields required')
    finite(a['original_annual_effective_rate'],-.99,10.)
    for key in ('principal','cost'):
        amount(a[key])
    for key in ('opening_accrued_interest','opening_unrecognized_interest','opening_fees','prepayment_penalty_fraction'):
        amount(c[key])
    for collection in (c['steps'],c['payment_dates'],raw['allowance_reviews'],raw['chargeoffs']):
        if not isinstance(collection,list):
            raise ArithmeticError('consumer-credit ordered source arrays required')
    for step in c['steps']:
        fields(step,'ordinal action')
    seen=set();days=set();start=c['as_of_ordinal'];end=start+c['horizon_days']
    for collection,kind in ((raw['allowance_reviews'],'allowance'),(raw['chargeoffs'],'chargeoff')):
        for r in collection:
            fields(r,'review recognition_ordinal phase target_amount' if kind=='allowance' else 'review claim_id recognition_ordinal principal_amount book_adjustment_amount accrued_interest_reversal')
            identity(r['review']);key=(r['review']['id'],r['review']['revision'])
            if key in seen or type(r['recognition_ordinal']) is not int or not start<r['recognition_ordinal']<=end:
                raise ArithmeticError('consumer-credit duplicate/outside reviewed instruction')
            seen.add(key)
            if kind=='allowance':
                if r['phase'] not in {'before_credit','after_credit'} or r['recognition_ordinal'] in days:
                    raise ArithmeticError('consumer-credit ambiguous allowance phase/day')
                days.add(r['recognition_ordinal']);amount(r['target_amount'])
            else:
                if r['claim_id']!=raw['claim_id']:
                    raise ArithmeticError('consumer-credit exact chargeoff claim identity required')
                amount(r['principal_amount']);finite(r['book_adjustment_amount'],-1e15,1e15);finite(r['accrued_interest_reversal'],-1e15,1e15)
    if len(json.dumps(raw,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode('utf-8'))>1024*1024:
        raise ArithmeticError('consumer-credit raw source byte budget exceeded')


def project_supplied(raw, check=lambda:False):
    """Reconstruct exact performing payment/default/payoff source independently.

    Default is an explicit supplied event after accrual over (previous,date].
    It suppresses the same-day contractual due and ends subsequent interest.
    Recovery economics are principal-only and include the complete delayed tail.
    No probability weighting, stochastic draw, allowance or chargeoff is inferred.
    """
    checkpoint(check)
    if (raw['schema']!='consumer-installment-1' or raw['identity']['measure']!='physical'
            or raw['identity']['model_version']!=raw['schema'] or raw['initial_state']!='performing'
            or any(raw[k]!=0. for k in ('opening_accrued_interest','opening_unrecognized_interest','opening_fees','prepayment_penalty_fraction'))
            or raw['day_count'] not in {'act360','act365_fixed'}):
        raise ArithmeticError('consumer-credit explicit physical performing source required')
    start=raw['as_of_ordinal'];end=start+raw['horizon_days'];dues=raw['payment_dates'];steps=raw['steps']
    if (type(start) is not int or type(raw['horizon_days']) is not int or not 1<=start<end<=3652059
            or raw['horizon_days']>4095 or not dues or len(dues)+len(steps)>4096
            or any(type(day) is not int or not start<day<=end for day in dues)
            or dues!=sorted(set(dues))):
        raise ArithmeticError('consumer-credit bounded exact full-tail source calendar')
    dates=[s['ordinal'] for s in steps]
    if dates!=sorted(set(dates)) or any(type(d) is not int or not start<d<=end for d in dates):
        raise ArithmeticError('consumer-credit exact ordered supplied action dates')
    actions={s['ordinal']:s['action'] for s in steps}
    if any(a['kind'] not in {'payment','payoff','default','change_payment'} for a in actions.values()):
        raise ArithmeticError('consumer-credit nonaccrual/fees/draws/modifications require separate reviewed ownership')
    for a in actions.values():
        action_fields={'kind','amount'} if a['kind'] in {'payment','change_payment'} else {'kind'}
        if set(a)!=action_fields:
            raise ArithmeticError('consumer-credit exact supplied action fields')
        if 'amount' in a:
            amount(a['amount'])
    for k in ('opening_principal','original_principal','original_payment','note_rate','disclosed_apr'):
        amount(raw[k])
    if raw['opening_principal']>raw['original_principal'] or raw['original_principal']==0. or raw['original_payment']==0. or raw['note_rate']>2.:
        raise ArithmeticError('consumer-credit retained original principal/payment domain')
    product=raw['product'];kind=product['kind']
    if kind not in {'auto','personal'}:
        raise ArithmeticError('consumer-credit supplied auto/personal product required')
    delay=product['recovery_delay_days']
    if type(delay) is not int or not 0<=delay<=4095:
        raise ArithmeticError('consumer-credit bounded exact recovery delay')
    if kind=='auto':
        fields(product,'kind new_vehicle vintage_ordinal original_ltv channel collateral_value_as_of annual_depreciation sale_haircut repossession_cost recovery_delay_days')
        if type(product['new_vehicle']) is not bool:
            raise ArithmeticError('consumer-credit exact vehicle flag required')
        finite(product['original_ltv'],0.,10.)
        if product['original_ltv']==0.:
            raise ArithmeticError('consumer-credit positive original LTV required')
        for k in ('collateral_value_as_of','repossession_cost'):
            amount(product[k])
        finite(product['annual_depreciation'],0.,1.);finite(product['sale_haircut'],0.,1.)
    else:
        fields(product,'kind vintage_ordinal dti dti_definition channel collection_fraction collection_cost recovery_delay_days')
        finite(product['dti'],0.,10.);text(product['dti_definition'])
        amount(product['collection_cost'])
        finite(product['collection_fraction'],0.,1.)
    text(product['channel'])
    if type(product['vintage_ordinal']) is not int or not 1<=product['vintage_ordinal']<=start or raw['disclosed_apr']>2.:
        raise ArithmeticError('consumer-credit original vintage/APR domain required')
    principal=raw['opening_principal'];interest=0.;payment=raw['original_payment'];state='performing';previous=start
    recognition=[];cash=[];points=[];default=None;collected=0.
    for day in sorted(set(dues)|set(actions)|{end}):
        checkpoint(check)
        if principal>0. and state=='performing':
            earned=principal*raw['note_rate']*year_fraction(previous,day,raw['day_count']);amount(earned);interest+=earned
            if earned!=0.:
                recognition.append(dict(origin_id='interest-'+str(len(recognition)),start_ordinal=previous,end_ordinal=day,interest=earned))
        previous=day;action=actions.get(day)
        if action is not None and state!='performing':
            raise ArithmeticError('consumer-credit supplied action follows terminal exit')
        if action is not None and action['kind']=='default':
            recovery_day=day+delay
            if recovery_day>end:
                raise ArithmeticError('consumer-credit complete recovery tail is missing')
            if kind=='personal':
                gross=principal*product['collection_fraction'];cost=product['collection_cost']
            else:
                gross=product['collateral_value_as_of']*(1.-product['annual_depreciation'])**((recovery_day-start)/365.)*(1.-product['sale_haircut'])
                cost=product['repossession_cost']
            amount(gross);net=min(max(gross-cost,0.),principal);unreimbursed=max(cost-gross,0.)
            default=dict(ordinal=day,principal=principal,accrued_interest=interest,recognized_accrued_interest=interest,
                unrecognized_contract_interest=0.,fees=0.,gross_recovery=gross,recovery_cost=cost,net_recovery=net,
                principal_loss=principal-net,recovery_ordinal=recovery_day)
            if net!=0. or unreimbursed!=0.:
                cash.append(dict(event_id='recovery-0',ordinal=recovery_day,kind='recovery',principal=net,interest=0.,fees=-unreimbursed))
            principal=interest=0.;state='default'
        else:
            payoff=action is not None and action['kind']=='payoff';pay=None
            if payoff:
                pay=principal+interest
            elif action is not None and action['kind']=='payment':
                pay=action['amount']
            elif action is not None and action['kind']=='change_payment':
                payment=action['amount']
            if pay is None and day in dues and state=='performing':
                pay=principal+interest if day==dues[-1] else min(payment,principal+interest)
            if pay is not None:
                if pay>principal+interest+1e-9 or (principal+interest==0. and pay!=0.):
                    raise ArithmeticError('consumer-credit payment exceeds original contractual payoff')
                paid_interest=min(pay,interest);interest-=paid_interest
                paid_principal=min(max(pay-paid_interest,0.),principal);principal-=paid_principal;collected+=paid_principal
                if pay>0.:
                    cash.append(dict(event_id='payment-'+str(len(cash)),ordinal=day,kind='prepayment' if payoff else 'payment',principal=paid_principal,interest=paid_interest,fees=0.))
                if principal==0. and interest==0.:
                    state='paid_off'
        points.append(dict(ordinal=day,state=state,principal=principal,accrued_interest=interest,unrecognized_interest=0.,fees=0.,scheduled_payment=payment))
    return dict(schema=raw['schema'],identity=deepcopy(raw['identity']),original_principal=raw['original_principal'],original_payment=raw['original_payment'],
        disclosed_apr=raw['disclosed_apr'],payment_dates=deepcopy(dues),cashflows=cash,recognition=recognition,points=points,default=default,
        principal_collected=collected,closing_principal=principal,closing_accrued_interest=interest,closing_unrecognized_interest=0.,closing_fees=0.)


def _prefix(raw,economic,exit_day,terminal,check):
    c=raw['contract'];a=raw['acquisition'];y=a['original_annual_effective_rate'];events=[];rows=[]
    principal=a['principal'];carry=a['cost'];accrued=0.
    for day in range(c['as_of_ordinal']+1,exit_day+1):
        checkpoint(check);earned=0.;pc=ic=0.
        for s in economic['recognition']:
            if s['start_ordinal']<day<=s['end_ordinal']:
                earned+=s['interest']*year_fraction(day-1,day,c['day_count'])/year_fraction(s['start_ordinal'],s['end_ordinal'],c['day_count'])
        if day!=exit_day or terminal is None:
            for cash in economic['cashflows']:
                if cash['ordinal']==day and cash['kind']!='recovery':
                    pc+=cash['principal'];ic+=cash['interest']
        event=dict(event_id=f'income:{day}',source=deepcopy(raw['economic_source']),payment_ordinal=day,
            contractual_interest_earned=earned,principal_cash=pc,interest_cash=ic,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None)
        opening=carry;effective=opening*math.expm1(math.log1p(y)/365.)
        carry+=effective-pc-ic;principal-=pc;accrued+=earned-ic
        rounding=64*sys.float_info.epsilon*(abs(opening)+abs(pc+ic)+abs(effective))
        if carry<0. and principal==0. and abs(accrued)<=rounding and abs(carry)<=rounding:
            carry=0.
        if carry<0.:
            raise ArithmeticError('consumer-credit negative prefix carrying requires a reviewed owner')
        row=dict(event_id=event['event_id'],source=deepcopy(raw['economic_source']),payment_ordinal=day,
            opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=effective,
            contractual_interest_earned=earned,basis_amortization=effective-earned,modification_gain_loss=0.,
            recognized_nonintegral_fee_income=0.,cash_received=pc+ic,closing_principal=principal,closing_accrued_interest=accrued,
            closing_gross_carrying_value=carry,closing_book_adjustment=carry-principal-accrued,yield_review=None)
        events.append(event);rows.append(row)
    common=dict(schema='dated-income-1',contract=dict(id=c['identity']['original_loan_id'],revision=c['identity']['source_revision']),
        original_yield_anchor=deepcopy(a['original_yield_anchor']),policy=deepcopy(a['income_review']),currency=c['identity']['currency'],
        monetary_unit='currency_units',measure='physical')
    request=dict(common,as_of_ordinal=c['as_of_ordinal'],full_contract_end_ordinal=c['as_of_ordinal']+c['horizon_days'],
        opening_principal=a['principal'],opening_accrued_interest=0.,opening_gross_carrying_value=a['cost'],
        original_annual_effective_rate=y,opening_annual_effective_rate=y,events=events)
    result=dict(common,rows=rows,remaining_principal=principal,remaining_accrued_interest=accrued,remaining_gross_carrying_value=carry)
    return request,result


def _empty_live():
    return dict(principal=0.,undrawn=0.,accrued_interest=0.,book_adjustment=0.)


def reconstruct(raw,check=lambda:False):
    """Complete independent primitive reconstruction; no native finance callbacks."""
    checkpoint(check)
    _admit(raw)
    c=raw['contract'];a=raw['acquisition'];policy=raw['policy'];p=policy['accounting'];start=c['as_of_ordinal'];end=start+c['horizon_days']
    if (raw['schema']!='consumer-credit-accounting-1' or c['horizon_days']>1080
            or c['opening_principal']!=c['original_principal'] or a['principal']!=c['opening_principal']
            or a['purchase_ordinal']!=start or not start<=raw['reporting_end_ordinal']<=end
            or policy['original_income']!='frozen_original_eir_until_exit-1'
            or policy['terminal_basis']!='recognize_in_earnings_at_payoff-1'
            or policy['recovery_cost']!='gross_recovery_with_credit_loss_cost-1'
            or a['cost']<=0. or not -.99<=a['original_annual_effective_rate']<=10.
            or len(raw['allowance_reviews'])+len(raw['chargeoffs'])>64):
        raise ArithmeticError('consumer-credit explicit acquisition/accounting/full-tail contract required')
    if any(s['action']['kind'] not in {'payment','payoff','default'} for s in c['steps']):
        raise ArithmeticError('consumer-credit unsupported supplied transition')
    amount(raw['opening_allowance']);amount(a['cost'])
    work=c['horizon_days']*(len(c['steps'])+len(c['payment_dates'])+len(raw['allowance_reviews'])+len(raw['chargeoffs'])+64)
    if work>2000000:
        raise ArithmeticError('consumer-credit bounded work exceeded before source generation')
    economic=project_supplied(c,check);default=economic['default'];terminal=next((cash for cash in economic['cashflows'] if cash['kind']!='recovery' and cash['principal']!=0. and any(pt['ordinal']==cash['ordinal'] and pt['principal']==0. for pt in economic['points'])),None)
    if economic['closing_principal']!=0. or economic['closing_accrued_interest']!=0.:
        raise ArithmeticError('consumer-credit unfinished live source full tail rejected')
    exit_day=default['ordinal'] if default else terminal['ordinal'] if terminal else end
    recovery_origin_ids=([f"default:{default['ordinal']}"] if default else [])+[cash['event_id'] for cash in economic['cashflows'] if cash['kind']=='recovery']
    recovery=None
    if default:
        if default['gross_recovery']>default['principal'] or default['recovery_cost']>default['gross_recovery']:
            raise ArithmeticError('consumer-credit surplus/uncovered gross recovery cost unsupported')
        recovery=dict(ordinal=default['recovery_ordinal'],gross_cash=default['gross_recovery'],cost_cash=default['recovery_cost'],net_cash=default['net_recovery'],
            principal_only_loss=default['principal']-default['gross_recovery'],total_economic_loss=default['principal_loss'],cost_review=deepcopy(policy['cost_review']))
    prefix_request,prefix=_prefix(raw,economic,exit_day,terminal,check)
    state=dict(live=[dict(principal=a['principal'],undrawn=0.,accrued_interest=0.,book_adjustment=a['cost']-a['principal']),_empty_live(),_empty_live()],
        allowance=raw['opening_allowance'],claims=[])
    opening=deepcopy(state);intents=[];lineage=[]
    daily=[dict(ordinal=start,economic_state='performing',owned=deepcopy(state),effective_interest=0.,contractual_interest=0.,cash_delta=0.,earnings_delta=0.)]

    def emit(day,event,lines,ids,source=None,review=None,claim=None):
        postings=[dict(account=code,amount=value) for code,value in lines if value!=0.]
        if not postings:
            return
        for entry in postings:
            finite(entry['amount'],-1e15,1e15)
        tolerance=1e-9 if event.startswith('product_') else max(1e-9,64*sys.float_info.epsilon*sum(abs(x['amount']) for x in postings))
        if abs(sum(x['amount'] for x in postings))>tolerance:
            raise ArithmeticError('consumer-credit independent intent fails balance')
        lineage.append(dict(ordinal=day,phase=event,intent_index=len(intents),product_event_ids=ids,
            original_yield_anchor=deepcopy(a['original_yield_anchor']),economic_source=deepcopy(raw['economic_source']),path_id=raw['path_id']))
        intents.append(dict(recognition_ordinal=day,event=event,position_id=c['identity']['position_id'],claim_id=claim,
            policy=deepcopy(review or p['identity']),source=deepcopy(source or raw['economic_source']),postings=postings))

    def allowance(review,day,ids):
        target=amount(review['target_amount']);change=target-state['allowance'];state['allowance']=target
        emit(day,'reviewed_allowance_target',[('credit_loss_expense',change),('allowance',-change)],ids,source=review['review'])

    for day in range(start+1,end+1):
        checkpoint(check);begin=len(intents);effective=earned=0.;live=state['live'][0]
        if day<=exit_day:
            row=prefix['rows'][day-start-1];earned=row['contractual_interest_earned'];effective=row['effective_interest']
            live['accrued_interest']+=earned;live['book_adjustment']+=row['basis_amortization']
            ids=[s['origin_id'] for s in economic['recognition'] if s['start_ordinal']<day<=s['end_ordinal']]
            emit(day,'product_original_effective_interest',[('live_accrued_interest',earned),('live_book_adjustment',row['basis_amortization']),('interest_income',-effective)],ids,review=a['income_review'])
            for cash in economic['cashflows']:
                if cash['ordinal']!=day or cash['kind']=='recovery':
                    continue
                live['principal']-=cash['principal'];live['accrued_interest']-=cash['interest']
                emit(day,'product_contractual_settlement',[('cash',cash['principal']+cash['interest']),('live_principal',-cash['principal']),('live_accrued_interest',-cash['interest'])],[cash['event_id']])
                if terminal is not None and terminal['event_id']==cash['event_id']:
                    basis=live['book_adjustment'];live['book_adjustment']=0.
                    emit(day,'product_prepayment_basis_retirement' if cash['kind']=='prepayment' else 'product_contractual_paid_off_basis_retirement',
                        [('live_book_adjustment',-basis),('basis_recognition_expense',basis)],[cash['event_id']])
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
            emit(day,'product_reviewed_recovery_cost',[('cash',-recovery['cost_cash']),('credit_loss_expense',recovery['cost_cash'])],recovery_origin_ids,review=policy['cost_review'],claim=raw['claim_id'])
        postings=[p for i in intents[begin:] for p in i['postings']]
        cash_delta=sum((p['amount'] for p in postings if p['account']=='cash'),0.)
        earnings=-sum((p['amount'] for p in postings if p['account'] in {'interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'}),0.)
        current=next((pt['state'] for pt in reversed(economic['points']) if pt['ordinal']<=day),'performing')
        daily.append(dict(ordinal=day,economic_state=current,owned=deepcopy(state),effective_interest=effective,contractual_interest=earned,cash_delta=cash_delta,earnings_delta=earnings))
    equal(sum((d['cash_delta'] for d in daily),0.),sum((x['principal']+x['interest']+x['fees'] for x in economic['cashflows']),0.))
    equal(sum(s['principal'] for s in state['live']),economic['closing_principal'])
    equal(sum(s['accrued_interest'] for s in state['live']),economic['closing_accrued_interest'])
    result=dict(schema=raw['schema'],raw_request=deepcopy(raw),economic=economic,income_prefix_request=prefix_request,income_prefix=prefix,
        acquisition_opening=opening,intents=intents,lineage=lineage,daily=daily,reporting=deepcopy(daily[raw['reporting_end_ordinal']-start]),closing=state,recovery=recovery,
        admitted_work=work,mixed_ledger_adoption=False,
        scope='one acquired supplied physical installment economic owner with original-EIR prefix and reviewed shared credit intents; no second principal projection, stochastic hazard fit, pricing history, mixed ledger adoption or independently persisted verification')
    checkpoint(check)
    if len(json.dumps(result,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode('utf-8'))>64*1024*1024:
        raise ArithmeticError('consumer-credit complete retained output byte budget exceeded')
    return result


def verify(raw,result,check=lambda:False):
    expected=reconstruct(raw,check);equal(result,expected);checkpoint(check)
    return dict(verified=True,supplied_consumer_source_recomputed=True,original_yield_prefix_recomputed=True,
        reviewed_credit_intents_recomputed=True,gross_recovery_cost_separated=True,mixed_ledger_adoption=False,empirical_validation=False)
