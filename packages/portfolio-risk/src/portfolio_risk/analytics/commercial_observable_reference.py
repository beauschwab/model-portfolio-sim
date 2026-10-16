"""Independent frozen served rights and chronological post-exit reconstruction."""
from copy import deepcopy
from .commercial_source_reference import observable_factor
from .credit_path_reference import STATES


def continue_terminal(raw,out,prepared,source,segments,audits,views,active,points,contributions,source_cash,cash_indices,principal,accrued,gross,check):
    terminal=raw['factor_policy'].get('terminal_obligations')
    if terminal is None:return accrued,gross
    default=out['default'];exit_=next((e for e in out['events'] if e['kind'] in ('prepayment','maturity')),None)
    if default is None and exit_ is None:return accrued,gross
    cut=default['year'] if default else exit_['year'];origin=STATES.index(default['origin'] if default else exit_['origin']);is_default=default is not None
    interval=next(i for i,t in enumerate(prepared['raw_credit_path']['intervals']) if cut<=t['end_year']);terms=prepared['raw_credit_path']['intervals'][interval]
    clocks=sorted(set(s['observable_information_unix_seconds'] for s in segments if 'observable_information_unix_seconds' in s))
    steps=[]
    for audit_index,audit in enumerate(audits):
        view=next(v for v in views if v[2]<=audit_index<v[3])
        if view[1] is not None and view[1] not in active:continue
        owned=[s for s in segments if s['native_audit_index']==audit_index];old=observable_factor(owned,source,cut,cut)
        period=next(p for p in view[0]['periods'] if p['period_id']==audit['period_id']);cash_index=cash_indices[audit['period_id']]
        for information in clocks:
            year=(information-(source['asof_ordinal']-719163)*86400)/31536000.
            check()
            if year<=cut:continue
            value=observable_factor(owned,source,cut,year);delta=value-old;old=value
            if delta==0.:continue
            steps.append(dict(year=year,original_settlement_year=(period['payment_ordinal']-source['asof_ordinal'])/365.,coupon_id=f'pending-coupon:{audit_index}:{information}',source_event_id=audit['period_id'],source_id=view[0]['contract']['id'],source_revision=view[0]['contract']['revision'],policy_id=terminal['review']['id'],captured_segment_index=next(i for i,s in enumerate(segments) if s['native_audit_index']==audit_index),delta_factor=delta,
                entitlement=dict(native_audit_index=audit_index,native_cash_index=cash_index,opening_entitlement_principal=audit['opening_principal']),tranche_id=view[1]))
    steps.sort(key=lambda s:s['year'])
    if not steps:return accrued,gross
    events=[]
    for i,step in enumerate(steps):
        amount=step['entitlement']['opening_entitlement_principal']*step['delta_factor'];events.append((step['year'],0,'fix',i,amount))
        if default:
            for recovery in terms['complete_recovery_tails'][origin]:events.append((max(cut+recovery['lag_years'],step['year']),1,'cash',i,amount*terms['interest_recovery_fraction'][origin]*recovery['weight']))
        else:events.append((step['original_settlement_year'],1,'cash',i,amount))
    events.sort(key=lambda e:(e[0],e[1]));begin=next(i for i,p in enumerate(points) if p['path_key']==out['path_key'] and p['year']==cut and p['kind']==('default_gross_transfer' if default else 'full_live_exit'))
    baseline=deepcopy(points[begin]);old=points[begin+1:];del points[begin+1:];cursor=0;extra_gross=extra_legal=0.
    def merge(point):
        value=deepcopy(point);value['gross_carry']+=extra_gross;value['contractual_accrued_interest']+=extra_legal;value['book_adjustment']=value['gross_carry']-value['principal']-value['contractual_accrued_interest'];return value
    for year,_,kind,index,amount in events:
        check();step=steps[index]
        while cursor<len(old) and old[cursor]['year']<=year:
            baseline=old[cursor];cursor+=1;points.append(merge(baseline))
        point=deepcopy(baseline);point.update(year=year,origin_index=origin,accounting_rate=None,effective_interest=0.,cash=0.,basis_realization=0.);point.pop('canonical_legal_coupons',None);point.pop('draw_tranches',None);point.pop('pending_coupon_obligation_indices',None)
        memo=pending_indices(segments,source,cut,year,views,active)
        if memo:point['pending_coupon_obligation_indices']=memo
        if kind=='fix':
            extra_legal+=amount
            if not default:extra_gross+=amount;point['effective_interest']=amount
            point['kind']='pending_coupon_fixing';contributions.append(dict(path_key=out['path_key'],interval_index=interval,origin_index=origin,start_year=cut,end_year=year,coupon_id=step['coupon_id'],source_event_id=step['source_event_id'],source_id=step['source_id'],source_revision=step['source_revision'],captured_segment_index=step['captured_segment_index'],delta_factor=step['delta_factor'],actual_principal=step['entitlement']['opening_entitlement_principal'],contractual_earned=amount,native_entitlement=deepcopy(step['entitlement'])))
            out['earned_interest']+=amount;out['earned_interest_by_origin'][origin]+=amount
        else:
            extra_gross-=amount;extra_legal-=amount;point.update(cash=amount,kind='pending_coupon_cash')
            cash=dict(coupon_id=step['coupon_id'],source_event_id=step['source_event_id'],native_cash_index=step['entitlement']['native_cash_index'],principal=0.,interest=amount)
            if step['tranche_id'] is not None:cash['tranche_id']=step['tranche_id']
            source_cash.append(dict(path_key=out['path_key'],year=year,cash=cash));out['cash_received']+=amount
            out['events'].append(dict(year=year,event_id=f"pending-coupon:{step['coupon_id']}:{len(source_cash)}",policy_id=step['policy_id'],origin=STATES[origin],destination=None,kind='pending_coupon_cash',cash=amount,principal=0.,interest=amount,book_adjustment_recognition=0.))
        points.append(merge(point))
    for baseline in old[cursor:]:check();points.append(merge(baseline))
    return accrued+extra_legal,baseline['gross_carry']+extra_gross


def pending_indices(segments,source,served_year,information_year,views=None,active=()):
    elapsed=served_year*365.;rounded=round(elapsed)
    days=rounded if served_year==rounded/365. else elapsed
    at=source['asof_ordinal']+days;result=[]
    for i,segment in enumerate(segments):
        if views is not None:
            view=next(v for v in views if v[2]<=segment['native_audit_index']<v[3])
            if view[1] is not None and view[1] not in active:continue
        information=segment.get('observable_information_unix_seconds')
        if information is not None and at>segment['interest_start_ordinal'] and information_year<(information-(source['asof_ordinal']-719163)*86400)/31536000.:result.append(i)
    return result
