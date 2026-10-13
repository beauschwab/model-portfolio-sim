"""Independent gross legal/recognized commercial coupon subledger equations."""
from copy import deepcopy
from .commercial_credit_reference import STATES, add_earned, apply_cash, sync


def groups(replay,check=lambda:False):
    policy=replay['policy'];atoms=replay['earned_origin_contributions'];cash=replay['native_source_cash'];paid=replay['native_coupon_recoveries'];paths=replay['credit']['result']['paths']
    result=[];closing=[];ci=si=ri=0;lots=[];previous=None;default=False
    def code(kind,is_default):return ('default_' if is_default else 'live_')+kind
    for p in replay['carry_points']:
        check();source_id=None;kind=p['kind']
        if kind=='reviewed_opening_snapshot':lots=[];previous=None;default=False;ri=0
        elif kind=='live_sojourn':
            while ci<len(atoms) and atoms[ci]['path_key']==p['path_key'] and atoms[ci]['end_year']==p['year']:
                c=atoms[ci];mode=policy['recognition'][c['interval_index']]['by_origin'][c['origin_index']];add_earned(lots,c,mode=='full_accrual');ci+=1
        elif kind=='contractual_cash':
            row=cash[si];path=next(v for v in paths if v['path_key']==p['path_key']);event=next(e for e in path['events'] if e['event_id']==row['cash']['source_event_id']);apply_cash(lots,row,STATES.index(event['origin']));source_id=row['cash']['source_event_id'];si+=1
        elif kind=='default_gross_transfer':
            default=True
            for lot in lots:lot['default_claim']=True
        elif kind=='default_actual_cash':
            for r in paid:
                if r['path_key']==p['path_key'] and r['recovery_index']==ri:
                    lot=next(l for l in lots if l['coupon_id']==r['coupon_id'] and l['earning_origin_index']==r['earning_origin_index']);lot['legal_cash_collected']+=r['interest'];lot['recognized_cash_applied']+=r['interest'];sync(lot)
            ri+=1
        elif kind=='full_live_exit':
            path=next(v for v in paths if v['path_key']==p['path_key']);source_id=next(e['event_id'] for e in path['events'] if e['year']==p['year'] and e['kind'] in ('maturity','prepayment'))
            for lot in lots:lot['legal_cash_collected']+=lot['legal_remaining'];lot['recognized_cash_applied']+=lot['recognized_remaining'];sync(lot)
        elif kind not in ('funded_draw','closing_claims_retained'):raise ArithmeticError('unsupported independent commercial action')
        origin_legal=sum((l['legal_remaining'] for l in lots),0.);recognized=sum((l['recognized_remaining'] for l in lots),0.)
        canonical=p.get('canonical_legal_coupons',[]);legal=sum((c['legal_remaining'] for c in canonical),0.) if canonical else origin_legal
        next_=dict(path_key=p['path_key'],year=p['year'],principal=p['principal'],recognized_interest=recognized,legal_interest=legal,
            native_legal_reduction_difference=p['contractual_accrued_interest']-origin_legal,book_adjustment=p['gross_carry']-p['principal']-recognized,gross_carry=p['gross_carry'],in_default=default,lots=deepcopy(lots))
        if canonical:next_['canonical_legal_coupons']=deepcopy(canonical)
        lines=[]
        def add(account,value,lot=None):
            if value!=0.:lines.append(dict(coupon_id=lot['coupon_id'] if lot else None,earning_origin_index=lot['earning_origin_index'] if lot else None,posting=dict(account=account,amount=value)))
        if previous is None:
            add('live_principal',next_['principal']);add('live_book_adjustment',next_['book_adjustment'])
        elif previous['in_default']==default:
            add(code('principal',default),next_['principal']-previous['principal']);add(code('book_adjustment',default),next_['book_adjustment']-previous['book_adjustment'])
        else:
            add(code('principal',previous['in_default']),-previous['principal']);add(code('book_adjustment',previous['in_default']),-previous['book_adjustment']);add(code('principal',default),next_['principal']);add(code('book_adjustment',default),next_['book_adjustment'])
        for lot in lots:
            before=next((b for b in previous['lots'] if b['coupon_id']==lot['coupon_id'] and b['earning_origin_index']==lot['earning_origin_index']),None) if previous else None
            if before is None or before['default_claim']==lot['default_claim']:add(code('accrued_interest',lot['default_claim']),lot['recognized_remaining']-(before['recognized_remaining'] if before else 0.),lot)
            else:add(code('accrued_interest',before['default_claim']),-before['recognized_remaining'],before);add(code('accrued_interest',lot['default_claim']),lot['recognized_remaining'],lot)
        money=-next_['gross_carry'] if previous is None else p['cash'];add('cash',money);add('interest_income',-p['effective_interest']);add('basis_recognition_expense',p['basis_realization'])
        result.append(dict(path_key=p['path_key'],year=p['year'],phase=kind,policy=deepcopy(policy['identity']),source_event_id=source_id,effective_interest=p['effective_interest'],basis_realization=p['basis_realization'],cash=money,postings=lines,next=next_))
        if kind=='closing_claims_retained':closing.append(next_)
        previous=next_
    if replay.get('reviewed_credit_actions') is not None:
        from .commercial_reviewed_reference import apply
        return apply(replay,result,check)
    return result,closing
