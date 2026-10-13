"""Independent reviewed book actions over independently reconstructed source paths.

Economic carry never supplies a booked allowance or an implicit legal waiver.
Actual fractional source recoveries retain their order within the reporting day.
"""
from collections import defaultdict
from copy import deepcopy
import math


def _day(year):
    days=year*365.;n=round(days)
    day=n if n/365.==year else math.ceil(days)
    return day+1 if year>day/365. else day


def _subtract(total,part):
    if part!=0. and (total==0. or math.copysign(1.,total)!=math.copysign(1.,part)) or abs(part)>abs(total):
        raise ArithmeticError('review exceeds exact signed independent claim')
    return total-part


def _book_subtract(total,part):
    near=abs(total-part)<=1e-9+1e-12*abs(total)
    if part!=0. and(total==0. or math.copysign(1.,total)!=math.copysign(1.,part))or abs(part)>abs(total)and not near:raise ArithmeticError('review exceeds supplied signed book claim')
    value=total-part
    return 0. if total!=0. and math.copysign(1.,value)!=math.copysign(1.,total)and near else value


def _collect(total,paid):
    return 0. if total==0. else total-math.copysign(min(abs(total),abs(paid)),paid)


def apply(replay,source_groups,check=lambda:False):
    actions=replay['reviewed_credit_actions'];policy=actions['policy'];asof=replay['asof_ordinal'];paths=replay['credit']['result']['paths'];result=[];closing=[]
    reverse=policy['accrued']=='reverse_income_at_default';expense=policy['basis']=='recognize_in_earnings_at_default'
    if policy['accrued'] not in ('reverse_income_at_default','transfer_until_reviewed_reversal') or policy['basis'] not in ('recognize_in_earnings_at_default','transfer_until_reviewed_chargeoff') or policy['allowance_shortfall'] not in ('reject','expense_at_reviewed_chargeoff') or policy['excess_cash_recovery'] not in ('credit_allowance','recognize_in_earnings'):raise ArithmeticError('unknown reviewed credit policy')
    rows=sum((len(actions.get(k,[])) for k in ('allowance_reviews','chargeoffs','legal_waivers')),0)
    if rows>4096:raise ArithmeticError('review admission exceeds4096')
    seen=set()
    for kind in ('allowance_reviews','chargeoffs','legal_waivers'):
        for row in actions.get(kind,[]):
            review=row if kind=='legal_waivers' else row['review'];identity=review['review'];key=(row['path_key'],identity['id'],identity['revision'])
            if key in seen or row['path_key'] not in [p['path_key']for p in paths] or type(review['recognition_ordinal'])is not int or not asof<=review['recognition_ordinal']<=asof+_day(replay['carry_points'][-1]['year']):raise ArithmeticError('review identity/path/date not independently admitted')
            seen.add(key)
    for path in paths:
        key=path['path_key'];claim_prefix=f'commercial-default:{key}';claims=[];legal={};legal_p=0.;allowance=0.;waivers=[];base=None;recovery_index=0
        events=[]
        for g in source_groups:
            if g['path_key']==key:events.append((g['year'],0 if g['phase']=='reviewed_opening_snapshot' else 4 if g['phase']=='closing_claims_retained' else 2,'source',g))
        for name,kind in [('allowance_reviews','allowance'),('chargeoffs','chargeoff'),('legal_waivers','waiver')]:
            for row in actions.get(name,[]):
                if row['path_key']!=key:continue
                r=row if kind=='waiver' else row['review'];phase=1 if kind=='allowance' and r['phase']=='before_credit' else 3
                events.append(((r['recognition_ordinal']-asof)/365.,phase,kind,row))
        events.sort(key=lambda e:(e[0],e[1]))
        def claim(identity):
            try:return next(c for c in claims if c['claim_id']==identity)
            except StopIteration as exc:raise ArithmeticError('review unknown independent default claim')from exc
        def add(lines,account,value,identity=None):
            if value==0.:return
            lot=next((l for l in base['lots'] if f"commercial-interest:{key}:{l['coupon_id']}:{l['earning_origin_index']}"==identity),None) if base else None
            lines.append(dict(coupon_id=lot['coupon_id']if lot else None,earning_origin_index=lot['earning_origin_index']if lot else None,posting=dict(account=account,amount=float(value))))
        def initial(identity,recognized,p,b,loss,tail,at):
            return dict(claim_id=identity,economic_source=dict(id=replay['credit']['result']['source_id'],revision=replay['credit']['result']['source_revision']),recovery_schedule_review=deepcopy(policy['identity']),policy=deepcopy(policy),default_ordinal=at,initial_principal=p,economic_principal_loss=loss,initial_accrued_interest=recognized,initial_book_adjustment=b,outstanding_principal=p,outstanding_accrued_interest=0. if reverse else recognized,outstanding_book_adjustment=0. if expense else b,principal_charged_off=0.,accrued_reversed=recognized if reverse else 0.,basis_recognized=b if expense else 0.,recovery_cash_collected=0.,recovery_applied_to_principal=0.,interest_recovery_cash_collected=0.,interest_recovery_applied=0.,settled_interest_recovery_ids=[],remaining_recoveries=tail)
        def snapshot(year):
            s=deepcopy(base);s['year']=year
            if s['in_default']:
                s['principal']=sum((c['outstanding_principal']for c in claims),0.);s['book_adjustment']=sum((c['outstanding_book_adjustment']for c in claims),0.)
                for lot in s['lots']:
                    identity=f"commercial-interest:{key}:{lot['coupon_id']}:{lot['earning_origin_index']}";lot['recognized_remaining']=claim(identity)['outstanding_accrued_interest'];lot['recognized_cash_applied']=lot['recognized_earned']-lot['recognized_remaining'];lot['legal_remaining']=legal[identity]
                s['recognized_interest']=sum((l['recognized_remaining']for l in s['lots']),0.);s['legal_interest']=sum((l['legal_remaining']for l in s['lots']),0.);s['gross_carry']=s['principal']+s['recognized_interest']+s['book_adjustment']
            s['reviewed_credit_state']=dict(legal_principal=legal_p,model_gross_carry=base['gross_carry'],allowance=allowance,claims=deepcopy(claims),legal_waivers=deepcopy(waivers))
            return s
        for year,_,kind,row in events:
            check()
            if kind=='source':
                g=deepcopy(row)
                if g['phase']=='default_gross_transfer':
                    base=deepcopy(g['next']);d=path['default'];legal_p=base['principal'];tail=defaultdict(float)
                    for r in d['recoveries']:tail[asof+_day(r['year'])]+=r['principal']
                    tail=[dict(payment_ordinal=at,amount=float(v))for at,v in sorted(tail.items())if v!=0.]
                    claims.append(initial(claim_prefix,0.,base['principal'],base['book_adjustment'],d['economic_principal_loss'],tail,asof+_day(year)))
                    if expense:add(g['postings'],'default_book_adjustment',-base['book_adjustment']);add(g['postings'],'basis_recognition_expense',base['book_adjustment'])
                    for lot in base['lots']:
                        identity=f"commercial-interest:{key}:{lot['coupon_id']}:{lot['earning_origin_index']}";legal[identity]=lot['legal_remaining'];claims.append(initial(identity,lot['recognized_remaining'],0.,0.,0.,[],asof+_day(year)))
                        cash_only=initial(f"commercial-legal-cash:{key}:{lot['coupon_id']}:{lot['earning_origin_index']}",lot['legal_remaining'],0.,0.,0.,[],asof+_day(year));cash_only['outstanding_accrued_interest']=0.;cash_only['accrued_reversed']=lot['legal_remaining'];claims.append(cash_only)
                        if reverse:add(g['postings'],'default_accrued_interest',-lot['recognized_remaining'],identity);add(g['postings'],'interest_income',lot['recognized_remaining'])
                elif g['phase']=='default_actual_cash':
                    base=deepcopy(g['next']);r=path['default']['recoveries'][recovery_index];g['postings']=[];c=claim(claim_prefix);due=r['principal']
                    if due!=0.:
                        point=next(p for p in c['remaining_recoveries']if p['payment_ordinal']==asof+_day(year));point['amount']-=due;c['remaining_recoveries']=[p for p in c['remaining_recoveries']if p['amount']!=0.]
                        applied=min(due,c['outstanding_principal']);extra=due-applied;contra=extra if policy['excess_cash_recovery']=='credit_allowance' else 0.
                        for account,value in [('cash',due),('default_principal',-applied),('allowance',-contra),('recovery_income',-(extra-contra))]:add(g['postings'],account,value,claim_prefix)
                        c['outstanding_principal']-=applied;c['recovery_applied_to_principal']+=applied;c['recovery_cash_collected']+=due;allowance+=contra
                    legal_p=_collect(legal_p,due)
                    for paid in replay['native_coupon_recoveries']:
                        if paid['path_key']!=key or paid['recovery_index']!=recovery_index:continue
                        identity=f"commercial-interest:{key}:{paid['coupon_id']}:{paid['earning_origin_index']}";legal[identity]=_collect(legal[identity],paid['interest']);book=claim(identity);capacity=book['initial_accrued_interest']-book['interest_recovery_cash_collected'];money=math.copysign(min(abs(capacity),abs(paid['interest'])),paid['interest'])if capacity!=0. and math.copysign(1.,capacity)==math.copysign(1.,paid['interest'])else 0.
                        for suffix,identity,cash in [('book',identity,money),('legal',f"commercial-legal-cash:{key}:{paid['coupon_id']}:{paid['earning_origin_index']}",paid['interest']-money)]:
                            if cash==0.:continue
                            c=claim(identity);applied=math.copysign(min(abs(cash),abs(c['outstanding_accrued_interest'])),cash)if c['outstanding_accrued_interest']!=0. else 0.;extra=cash-applied;account='allowance'if policy['excess_cash_recovery']=='credit_allowance'else'recovery_income'
                            if account=='allowance':allowance+=extra
                            if allowance<0.:raise ArithmeticError('signed reviewed recovery produces negative allowance')
                            c['outstanding_accrued_interest']=_book_subtract(c['outstanding_accrued_interest'],applied);c['interest_recovery_applied']+=applied;c['interest_recovery_cash_collected']+=cash;c['settled_interest_recovery_ids'].append(dict(id=f"native-interest:{key}:{recovery_index}:{paid['native_audit_index']}:{paid['earning_origin_index']}:{suffix}",revision='1'))
                            for account,value in [('cash',cash),('default_accrued_interest',-applied),(account,-extra)]:add(g['postings'],account,value,identity)
                    recovery_index+=1
                else:
                    base=deepcopy(g['next'])
                    if not base['in_default']:legal_p=base['principal']
            else:
                if base is None:raise ArithmeticError('review precedes independent acquisition')
                r=row if kind=='waiver'else row['review'];lines=[];review=deepcopy(policy['identity'])
                if kind=='allowance':
                    target=float(r['target_amount']);delta=target-allowance
                    if target<0.:raise ArithmeticError('negative reviewed allowance')
                    add(lines,'credit_loss_expense',delta);add(lines,'allowance',-delta);allowance=target;phase='reviewed_allowance_target'
                elif kind=='chargeoff':
                    c=claim(r['claim_id']);p=float(r['principal_amount']);b=float(r['book_adjustment_amount']);a=float(r['accrued_interest_reversal']);net=p+b
                    if net<0. or p<0.:raise ArithmeticError('negative reviewed chargeoff')
                    c['outstanding_principal']=_book_subtract(c['outstanding_principal'],p);c['outstanding_book_adjustment']=_book_subtract(c['outstanding_book_adjustment'],b);c['outstanding_accrued_interest']=_book_subtract(c['outstanding_accrued_interest'],a);covered=min(net,allowance);short=net-covered
                    if short!=0. and policy['allowance_shortfall']=='reject':raise ArithmeticError('reviewed chargeoff exceeds available allowance')
                    for account,value in [('default_principal',-p),('default_book_adjustment',-b),('allowance',covered),('credit_loss_expense',short),('default_accrued_interest',-a),('interest_income',a)]:add(lines,account,value,r['claim_id'])
                    allowance-=covered;c['principal_charged_off']+=p;c['basis_recognized']+=b;c['accrued_reversed']+=a;phase='reviewed_credit_chargeoff'
                else:
                    if not base['in_default']or r['future_cash_disposition']!='retain_supplied_voluntary_recovery_cash-1':raise ArithmeticError('waiver lacks independent default/cash authority')
                    if r['claim_id']==claim_prefix:
                        if r['accrued_interest_amount']!=0.:raise ArithmeticError('principal waiver substitutes coupon')
                        legal_p=_subtract(legal_p,float(r['principal_amount']))
                    else:
                        if r['principal_amount']!=0.:raise ArithmeticError('coupon waiver substitutes principal')
                        legal[r['claim_id']]=_subtract(legal[r['claim_id']],float(r['accrued_interest_amount']))
                    waivers.append(deepcopy(r));review=deepcopy(r['review']);phase='reviewed_legal_waiver'
                g=dict(path_key=key,year=year,phase=phase,policy=review,source_event_id=None,effective_interest=0.,basis_realization=0.,cash=0.,postings=lines,next=None)
            g['next']=snapshot(year);result.append(g)
            if g['phase']=='closing_claims_retained':closing.append(deepcopy(g['next']))
    return result,closing
