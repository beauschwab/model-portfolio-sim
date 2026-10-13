"""Independent scalar source replay for publication verification, never a backend.

The oracle integrates cumulative hazard and realized currency rights directly.
It does not call native financial functions or consume producer event amounts.
"""
import math
from collections import defaultdict
from copy import deepcopy

STATES=('performing','watch','delinquent','default','prepaid')
MASK=(1<<64)-1


def _mix(value):
    value=(value+0x9e3779b97f4a7c15)&MASK
    value=((value^(value>>30))*0xbf58476d1ce4e5b9)&MASK
    value=((value^(value>>27))*0x94d049bb133111eb)&MASK
    return value^(value>>31)


def _pair(raw,path_key,index):
    stream=raw['draw_stream']
    if stream['kind']=='supplied':
        pair=next(p for p in stream['paths'] if p['path_key']==path_key)['pairs'][index]
        return pair['waiting_uniform'],pair['destination_uniform']
    if stream['kind']!='counter_splitmix64_v1':raise ValueError('unknown verification draw stream')
    key=(_mix(stream['seed']^_mix(path_key^0xa0761d6478bd642f))+index*0xe7037ed1a0b428db)&MASK
    return tuple(((_mix(v)>>12)+.5)/2**52 for v in (key,key^0xd2b74407b1ce6e93))


def simulate_path(raw,opening,check=lambda:False):
    """Return independent economic result and exact live occupations for one path."""
    state=STATES.index(opening['initial_live_state'])
    principal,undrawn,accrued,basis=(opening[k] for k in ('principal','undrawn_commitment','accrued_interest','book_adjustment'))
    out=dict(path_id=opening['path_id'],path_key=opening['path_key'],outcome=STATES[state],
        earned_interest=0.,earned_interest_by_origin=[0.]*3,cash_received=0.,cash_funded=0.,
        book_adjustment_recognized=0.,random_pairs_used=0,events=[],default=None)
    horizon=raw['intervals'][-1]['end_year'];time=0.;interval=boundary=jumps=0
    waiting=None;sojourns=[];work=0
    while time<horizon or boundary<len(raw['boundaries']) and raw['boundaries'][boundary]['year']==time:
        check();work+=1
        if work>2_000_000:raise ValueError('independent path work bound exceeded')
        terms=raw['intervals'][interval]
        end=min(terms['end_year'],raw['boundaries'][boundary]['year'] if boundary<len(raw['boundaries']) else horizon)
        hazard=sum(terms['hazards_per_year'][state])
        if hazard>0. and waiting is None:
            u,v=_pair(raw,opening['path_key'],out['random_pairs_used'])
            waiting=(-math.log(u),v);out['random_pairs_used']+=1
        capacity=hazard*(end-time)
        jump=waiting is not None and hazard>0. and waiting[0]<=capacity
        next_time=min(time+waiting[0]/hazard,end) if jump else end
        interest=principal*terms['accrual_rates'][state]*(next_time-time)
        if next_time>time:
            sojourns.append(dict(start_year=time,end_year=next_time,origin=STATES[state],principal=principal,
                undrawn_commitment=undrawn,opening_accrued_interest=accrued,book_adjustment=basis,
                accrual_rate=terms['accrual_rates'][state],earned_interest=interest))
        accrued+=interest;out['earned_interest']+=interest;out['earned_interest_by_origin'][state]+=interest
        time=next_time
        if jump:
            jumps+=1
            if jumps>raw['max_jumps_per_path']:raise ValueError('independent jump limit exceeded')
            threshold=waiting[1]*hazard;cum=0.
            for destination,rate in enumerate(terms['hazards_per_year'][state]):
                cum+=rate
                if threshold<cum:break
            waiting=None
            event=dict(year=time,event_id=f'jump:{jumps}',policy_id=raw['model_policy_id'],origin=STATES[state],
                destination=STATES[destination],kind='transition',cash=0.,principal=0.,interest=0.,book_adjustment_recognition=0.)
            if destination==3:
                draw=undrawn*terms['credit_conversion_factor'][state]
                principal+=draw;undrawn=0.;out['cash_funded']+=draw
                event.update(cash=-draw,principal=principal,interest=accrued)
                claim=dict(year=time,origin=STATES[state],principal=principal,accrued_interest=accrued,
                    book_adjustment=basis,contingent_draw=draw,economic_principal_loss=principal*terms['loss_given_default'][state],recoveries=[])
                weight=0.
                for point in terms['complete_recovery_tails'][state]:
                    recovery=dict(year=time+point['lag_years'],principal=principal*(1.-terms['loss_given_default'][state])*point['weight'],
                        interest=accrued*terms['interest_recovery_fraction'][state]*point['weight'])
                    claim['recoveries'].append(recovery)
                    if recovery['year']<=horizon:
                        weight+=point['weight'];out['cash_received']+=recovery['principal']+recovery['interest']
                principal=claim['principal']*(terms['loss_given_default'][state]+(1.-terms['loss_given_default'][state])*(1.-weight))
                accrued=claim['accrued_interest']*(1.-terms['interest_recovery_fraction'][state]*weight)
                out['default']=claim;out['outcome']='default'
            elif destination==4:
                event.update(kind='prepayment',principal=principal,interest=accrued,cash=principal+accrued,book_adjustment_recognition=basis)
                out['cash_received']+=event['cash'];out['book_adjustment_recognized']+=basis
                principal=accrued=basis=undrawn=0.;out['outcome']='prepaid'
            else:state=destination;out['outcome']=STATES[state]
            out['events'].append(event)
            if destination>=3:break
        else:
            if waiting is not None:waiting=(waiting[0]-capacity,waiting[1])
            while boundary<len(raw['boundaries']) and raw['boundaries'][boundary]['year']==time:
                source=raw['boundaries'][boundary];action=source['action'];kind=action['kind']
                event=dict(year=time,event_id=source['event_id'],policy_id=source['policy_id'],origin=STATES[state],
                    destination=None,kind=kind,cash=0.,principal=0.,interest=0.,book_adjustment_recognition=0.)
                if kind=='draw':
                    amount=undrawn*action['undrawn_fraction'][state];principal+=amount;undrawn-=amount
                    event.update(principal=amount,cash=-amount);out['cash_funded']+=amount
                else:
                    pf=1. if kind=='maturity' else action['principal_fraction'][state]
                    af=1. if kind=='maturity' else action['interest_fraction'][state]
                    uf=1. if kind=='maturity' else action['cancel_undrawn_fraction'][state]
                    p=principal*pf;a=accrued*af;k=basis*pf
                    event.update(principal=p,interest=a,cash=p+a,book_adjustment_recognition=k)
                    principal-=p;accrued-=a;basis-=k;undrawn*=1.-uf
                    out['cash_received']+=p+a;out['book_adjustment_recognized']+=k
                    if kind=='maturity':out['outcome']='matured'
                out['events'].append(event);boundary+=1
                if out['outcome']=='matured':break
            if out['outcome']=='matured':break
            if time==terms['end_year'] and interval+1<len(raw['intervals']):interval+=1
            if time==horizon:break
    out.update(closing_principal=principal,closing_undrawn_commitment=undrawn,
        closing_accrued_interest=accrued,closing_book_adjustment=basis)
    return out,sojourns


def clock(request,year):
    elapsed=year*(365.*86400.)
    absolute=request['path']['origin_time']['seconds_of_day']+elapsed
    day=math.floor(absolute/86400.);seconds=absolute-day*86400.
    tol=request['timestamp_tolerance_seconds']
    if seconds<=tol:seconds=0.
    elif 86400.-seconds<=tol:day+=1;seconds=0.
    ordinal=request['path']['origin_time']['ordinal']+day
    return dict(elapsed_year=year,elapsed_seconds=elapsed,ordinal=ordinal,seconds_of_day=seconds),ordinal+(seconds!=0.)


def account_path(request,opening,binding,check=lambda:False):
    """Independent realized exposures, explicit policy decisions and journal intents."""
    economic,sojourns=simulate_path(request['path'],opening,check)
    start=request['path']['origin_time']['ordinal'];end=request['reporting_end_ordinal']
    live=[dict(principal=0.,undrawn=0.,accrued_interest=0.,book_adjustment=0.) for _ in range(3)]
    origin=STATES.index(opening['initial_live_state'])
    live[origin]=dict(principal=opening['principal'],undrawn=opening['undrawn_commitment'],
        accrued_interest=opening['accrued_interest'],book_adjustment=opening['book_adjustment'])
    allowance=binding['opening_allowance'];claim=None;gl=defaultdict(float)
    gl.update(live_principal=opening['principal'],live_accrued_interest=opening['accrued_interest'],
        live_book_adjustment=opening['book_adjustment'],allowance=-allowance)
    probability=[0.]*7;probability[origin]=1.;matured=0.;intents=[];daily=[]
    complete=[];ops=defaultdict(list)
    for sojourn in sojourns:
        a,b=sojourn['start_year'],sojourn['end_year']
        while a<b:
            _,day=clock(request,a)
            if clock(request,a)[0]['seconds_of_day']==0.:day+=1
            day_end=((day-start)*86400.-request['path']['origin_time']['seconds_of_day'])/(365.*86400.)
            limit=min(b,day_end)
            if limit<=a:raise ArithmeticError('independent accrual clock failed to advance')
            earned=sojourn['principal']*sojourn['accrual_rate']*(limit-a)
            slice_=dict(start_year=a,end_year=limit,origin=sojourn['origin'],principal=sojourn['principal'],
                accrual_rate=sojourn['accrual_rate'],earned_interest=earned)
            ops[day].append((limit,0,('accrual',slice_)));a=limit
    for index,event in enumerate(economic['events']):ops[clock(request,event['year'])[1]].append((event['year'],1,('event',index)))
    if economic['default']:
        for index,recovery in enumerate(economic['default']['recoveries']):
            exact,day=clock(request,recovery['year'])
            complete.append(dict(recovery_id=dict(id=f"{binding['claim_id']}:recovery:{index}",revision=binding['recovery_schedule_review']['revision']),
                exact_time=exact,recognition_ordinal=day,principal=recovery['principal'],interest=recovery['interest'],after_reporting=day>end))
    policy=binding['policy'];reviews={r['recognition_ordinal']:r for r in binding['allowance_reviews']}
    chargeoffs=defaultdict(list)
    for row in binding['chargeoffs']:chargeoffs[row['recognition_ordinal']].append(row)
    boundaries={b['event_id']:b for b in request['path']['boundaries']}
    def emit(event,lines,*,source=None,claim_id=None,exact=None,ids=None,timing='reviewed_daily_policy',contract=None,accrual=None,policy_id=None,retain_empty=False):
        postings=[dict(account=k,amount=float(v)) for k,v in lines if v!=0.]
        if not postings and not retain_empty:return
        for posting in postings:gl[posting['account']]+=posting['amount']
        if abs(math.fsum(p['amount'] for p in postings))>1e-9*max(1.,math.fsum(abs(p['amount']) for p in postings)):
            raise ArithmeticError('independent source intent unbalanced')
        intents.append(dict(intent=dict(recognition_ordinal=day,event=event,position_id=binding['position_id'],
            claim_id=claim_id,policy=policy_id or policy['identity'],source=source or binding['economic_source'],postings=postings),
            exact_event=exact,event_ids=ids or [],timing=timing,contractual_policy_id=contract,accrual=accrual))
    def target(review,after=False):
        nonlocal allowance
        delta=review['target_amount']-allowance
        emit('reviewed_allowance_target',[('credit_loss_expense',delta),('allowance',-delta)],source=review['review'],timing='reviewed_after_all_credit_cash' if after else policy_timing)
        allowance=review['target_amount']
    for day in range(start+1,end+1):
        check();cash_before=gl['cash'];profit_before=-sum(gl[k] for k in ('interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'))
        earned=0.;default=None
        for _,_,(kind,value) in sorted(ops[day],key=lambda op:(op[0],op[1])):
            if kind=='accrual':
                slice_=value;i=STATES.index(slice_['origin']);interest=slice_['earned_interest'];live[i]['accrued_interest']+=interest;earned+=interest
                emit('realized_earned_interest',[('live_accrued_interest',interest),('interest_income',-interest)],
                    exact=clock(request,slice_['end_year'])[0],timing='exact_sojourn_ending',accrual=slice_)
                continue
            event=economic['events'][value];i=STATES.index(event['origin']);row=live[i]
            if event['kind']=='transition':
                if event['destination']=='default':
                    default=economic['default'];probability=[0.]*7;probability[3+i]=1.
                else:
                    j=STATES.index(event['destination']);live[j]=row;live[i]=dict.fromkeys(row,0.)
                    probability=[0.]*7;probability[j]=1.
            elif event['kind']=='draw':
                row['principal']+=event['principal'];row['undrawn']-=event['principal']
                emit('realized_scheduled_draw',[('live_principal',event['principal']),('cash',-event['principal'])],
                    exact=clock(request,event['year'])[0],ids=[event['event_id']],timing='exact_boundary',retain_empty=True)
            else:
                row['principal']-=event['principal'];row['accrued_interest']-=event['interest'];row['book_adjustment']-=event['book_adjustment_recognition']
                if event['kind']=='repayment':row['undrawn']*=1.-boundaries[event['event_id']]['action']['cancel_undrawn_fraction'][i]
                else:
                    row['undrawn']=0.;probability=[0.]*7
                    if event['kind']=='prepayment':probability[6]=1.
                    else:matured=1.
                emit('realized_'+event['kind'],[('cash',event['cash']),('live_principal',-event['principal']),
                    ('live_accrued_interest',-event['interest']),('live_book_adjustment',-event['book_adjustment_recognition']),
                    ('basis_recognition_expense',event['book_adjustment_recognition'])],exact=clock(request,event['year'])[0],
                    ids=[event['event_id']],timing='exact_boundary',contract=event['policy_id'],retain_empty=True)
        policy_timing='reviewed_daily_credit_policy' if default else 'reviewed_daily_policy'
        review=reviews.get(day)
        if review and review['phase']=='before_credit':target(review)
        if default:
            d=default;exact=clock(request,d['year'])[0];draw=d['contingent_draw'];p=d['principal'];a=d['accrued_interest'];k=d['book_adjustment']
            name=binding['claim_id'];base_args=dict(claim_id=name,exact=exact,timing=policy_timing)
            emit('default_contingent_draw',[('live_principal',draw),('cash',-draw)],**base_args)
            emit('economic_default_transfer',[('live_principal',-p),('default_principal',p)],**base_args)
            reverse=policy['accrued']=='reverse_income_at_default';recognize=policy['basis']=='recognize_in_earnings_at_default'
            emit('default_accrued_income_reversal' if reverse else 'default_accrued_transfer',
                [('live_accrued_interest',-a),('interest_income' if reverse else 'default_accrued_interest',a)],**base_args)
            emit('default_basis_earnings' if recognize else 'default_basis_transfer',
                [('live_book_adjustment',-k),('basis_recognition_expense' if recognize else 'default_book_adjustment',k)],**base_args)
            live[STATES.index(d['origin'])]=dict.fromkeys(live[0],0.)
            tail=defaultdict(float)
            for r in complete:tail[r['recognition_ordinal']]+=r['principal']
            claim=dict(claim_id=name,economic_source=binding['economic_source'],recovery_schedule_review=binding['recovery_schedule_review'],policy=policy,
                default_ordinal=day,initial_principal=p,economic_principal_loss=d['economic_principal_loss'],initial_accrued_interest=a,initial_book_adjustment=k,
                outstanding_principal=p,outstanding_accrued_interest=0. if reverse else a,outstanding_book_adjustment=0. if recognize else k,
                principal_charged_off=0.,accrued_reversed=a if reverse else 0.,basis_recognized=k if recognize else 0.,
                recovery_cash_collected=0.,recovery_applied_to_principal=0.,interest_recovery_cash_collected=0.,interest_recovery_applied=0.,
                settled_interest_recovery_ids=[],remaining_recoveries=[dict(payment_ordinal=o,amount=v) for o,v in sorted(tail.items())])
        for instruction in chargeoffs[day]:
            if claim is None:raise ArithmeticError('chargeoff before independent default')
            p=instruction['principal_amount'];k=instruction['book_adjustment_amount'];a=instruction['accrued_interest_reversal'];covered=min(p+k,allowance)
            emit('reviewed_credit_chargeoff',[('default_principal',-p),('default_book_adjustment',-k),('allowance',covered),('credit_loss_expense',p+k-covered)],
                claim_id=claim['claim_id'],source=instruction['review'],timing=policy_timing)
            emit('reviewed_accrued_income_reversal',[('default_accrued_interest',-a),('interest_income',a)],
                claim_id=claim['claim_id'],source=instruction['review'],timing=policy_timing)
            claim['outstanding_principal']-=p;claim['outstanding_book_adjustment']-=k;claim['outstanding_accrued_interest']-=a
            claim['principal_charged_off']+=p;claim['basis_recognized']+=k;claim['accrued_reversed']+=a;allowance-=covered
        recovered=[r for r in complete if r['recognition_ordinal']==day]
        if claim is not None:
            due=sum(p['amount'] for p in claim['remaining_recoveries'] if p['payment_ordinal']==day)
            applied=min(due,claim['outstanding_principal']);excess=due-applied
            contra=excess if policy['excess_cash_recovery']=='credit_allowance' else 0.
            emit('modeled_physical_recovery_cash',[('cash',due),('default_principal',-applied),('allowance',-contra),('recovery_income',-(excess-contra))],
                claim_id=claim['claim_id'],source=binding['recovery_schedule_review'],ids=[r['recovery_id']['id'] for r in recovered],timing='daily_principal_recovery_coalesced')
            claim['outstanding_principal']-=applied;claim['recovery_cash_collected']+=due;claim['recovery_applied_to_principal']+=applied;allowance+=contra
            claim['remaining_recoveries']=[p for p in claim['remaining_recoveries'] if p['payment_ordinal']>day]
            for recovery in recovered:
                cash=recovery['interest'];right=claim['outstanding_accrued_interest'];applied=math.copysign(min(abs(cash),abs(right)),cash) if right!=0. else 0.
                excess=cash-applied;contra=excess if binding['interest_recovery_excess']=='credit_allowance' else 0.
                emit('reviewed_physical_interest_recovery_cash',[('cash',cash),('default_accrued_interest',-applied),
                    ('allowance' if contra!=0. else 'recovery_income',-excess)],claim_id=claim['claim_id'],source=binding['recovery_schedule_review'],
                    ids=[r['recovery_id']['id'] for r in recovered],timing='reviewed_daily_interest_recovery',policy_id=binding['interest_recovery_policy'])
                claim['outstanding_accrued_interest']-=applied;claim['interest_recovery_applied']+=applied;claim['interest_recovery_cash_collected']+=cash
                claim['settled_interest_recovery_ids'].append(recovery['recovery_id']);allowance+=contra
        if review and review['phase']=='after_credit':target(review,after=True)
        profit=-sum(gl[k] for k in ('interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'))
        daily.append(dict(recognition_ordinal=day,probability=probability.copy(),matured_probability=matured,live=deepcopy(live),
            default_principal=claim['outstanding_principal'] if claim else 0.,default_accrued_interest=claim['outstanding_accrued_interest'] if claim else 0.,
            default_book_adjustment=claim['outstanding_book_adjustment'] if claim else 0.,allowance=allowance,earned_interest=earned,
            cash_delta=gl['cash']-cash_before,profit_delta=profit-profit_before,cumulative_cash=gl['cash'],cumulative_profit=profit,
            net_assets_including_cash=sum(gl[k] for k in ('cash','live_principal','default_principal','live_accrued_interest','default_accrued_interest','live_book_adjustment','default_book_adjustment','allowance'))))
    return dict(binding=binding,economic=economic,daily=daily,intents=intents,complete_recoveries=complete,
        closing=dict(live=live,allowance=allowance,claims=[claim] if claim else []))
