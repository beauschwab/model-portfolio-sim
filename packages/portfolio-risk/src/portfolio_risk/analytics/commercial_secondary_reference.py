"""Independent raw contractual-origin -> acquired buyer-right inventory.

Validation only. Seller monetary checkpoints, carrying values and EIR are never
inputs. Native transfer inventories are compared after this raw replay.
"""
from copy import deepcopy
from .commercial_source_reference import project, factor, observable_factor, observable_policy, compare
from .commercial_dto import typed
from .modeled_json_identity import exact_json_identity


def loan_transfer(raw,check=lambda:False):
    source=raw['source'];t=source.get('transfer')
    if t is None:return None
    check();origin=t['contract_origin'];at=source['current']['asof_ordinal']
    if t['schema']!='commercial-performing-secondary-acquisition-1' or t['economic_ordinal']!=at or t['recognition_ordinal']!=at or t['settlement_ordinal']!=at or origin['asof_ordinal']>=at:
        raise ArithmeticError('secondary exact contract/acquisition clocks differ')
    if t['conveyance']!='all_remaining_principal_and_unsettled_coupon_rights-1' or t['coincident_order']!='transfer_before_acquisition_day_service_and_cash-1' or t['transaction_cost']!=0. or t['transaction_cost_policy']!='explicit_zero_transaction_cost-1':raise ArithmeticError('unsupported secondary cash/conveyance allocation')
    regime=t['regime']
    if regime['framework']!='reviewed_model_policy-1' or regime['initial_credit_policy']!='performing_no_initial_credit_grossup-1' or regime['income_policy']!='dirty_buyer_loan_carry_original_effective_yield-1' or regime['effective_from_ordinal']>at:raise ArithmeticError('unsupported secondary buyer regime')
    if raw.get('performing_draw_tranches') or raw['credit'].get('opening_undrawn_commitment',0.)!=0. or any(p['initial_live_state']!='performing' for p in raw['credit']['paths']):raise ArithmeticError('secondary draws/default/partial histories unsupported')
    if origin['principal_plan']['kind']!='supplied' or not origin['periods'] or origin['periods'][0]['start_ordinal']!=origin['asof_ordinal'] or at-origin['asof_ordinal']>1080:raise ArithmeticError('secondary bounded complete contractual origin required')
    for name in ('path','assumption','schema','model_version','scenario_id','contract','currency','monetary_unit','direction','measure','position_id','tail_end_ordinal'):
        if not exact_json_identity(origin[name],source['original'][name]):raise ArithmeticError('secondary origin provenance differs')
    cut=(at-719163)*86400
    for old,new in zip(source['original']['periods'],source['current']['periods'],strict=True):
        if old['start_ordinal']>=at:continue
        a,b=old['coupon'],deepcopy(new['coupon'])
        if a['kind']=='indexed'and b['kind']=='indexed':
            for original,revised in zip(a['observations'],b['observations'],strict=True):
                if original['interest_start_ordinal']<at:
                    if not exact_json_identity(typed(original),typed(revised)):raise ArithmeticError('secondary served observation changed')
                    data=original['source'];available=data.get('publication_unix_seconds',data.get('information_unix_seconds'))
                    if max(available,original['selection_cutoff_unix_seconds'],a.get('reset_cutoff_unix_seconds')or 0)>cut:raise ArithmeticError('unquantified acquired coupon prefix')
            b['observations']=deepcopy(a['observations'])
        if not exact_json_identity(typed(a),typed(b)):raise ArithmeticError('secondary served coupon convention changed')
    full,_=project(origin,check,raw['factor_policy']);current,segments=project(source['current'],check,raw['factor_policy'])
    original_suffix=[p for p in origin['periods'] if p['payment_ordinal']>=at]
    if not exact_json_identity(typed(original_suffix),typed(source['original']['periods'])):raise ArithmeticError('secondary original coupon conveyance differs')
    principal=origin['opening_principal']-sum(e['components']['principal'] for e in full['schedule']['events'] if e['payment_ordinal']<at)
    compare(float(principal),float(source['current']['opening_principal']),check=check)
    allocated=t['loan_clean_cost']+sum(a['purchased_accrued_cost'] for a in t['coupon_allocations'])+t['transaction_cost']
    compare(float(allocated),float(source['acquisition']['cost']),check=check)
    coupons=[]
    for ai,(audit,allocation) in enumerate(zip(current['audit'],t['coupon_allocations'],strict=True)):
        check();old=next(a for a in full['audit'] if a['period_id']==audit['period_id'])
        compare(float(old['opening_principal']),float(audit['opening_principal']),check=check);compare(float(old['principal_paid']),float(audit['principal_paid']),check=check)
        ss=[s for s in segments if s['native_audit_index']==ai];value=0.
        for segment in ss:
            if at<=segment['interest_start_ordinal']:break
            value=factor(segment,min(at,segment['interest_end_ordinal']),0.)
            if at<segment['interest_end_ordinal']:break
        if observable_policy(raw['factor_policy']):value=observable_factor(ss,source['current'],0.)
        legal=audit['opening_principal']*value
        if allocation['period_id']!=audit['period_id']:raise ArithmeticError('secondary purchased coupon allocation identity differs')
        compare(float(legal),float(allocation['purchased_accrued_cost']),check=check)
        ci=next(i for i,e in enumerate(current['schedule']['events']) if e['event_id']==audit['period_id'])
        coupons.append(dict(period_id=audit['period_id'],native_audit_index=ai,native_cash_index=ci,opening_entitlement_principal=float(audit['opening_principal']),purchased_factor=float(value),purchased_legal_interest=float(legal),allocated_cost=float(allocation['purchased_accrued_cost'])))
    accrued=sum(c['purchased_legal_interest'] for c in coupons);cost=source['acquisition']['cost']
    return dict(contract_origin_ordinal=origin['asof_ordinal'],acquisition_ordinal=at,principal=float(principal),purchased_legal_interest=float(accrued),buyer_cost=float(cost),buyer_book_adjustment=float(cost-principal-accrued),coupons=coupons,full_contractual_source=full,seller_income_inherited=False,accounting_approval=False)


def verify_loan_transfer(raw,actual,check=lambda:False):
    expected=loan_transfer(raw,check)
    if expected is None:
        if 'secondary_acquisition' in actual:raise ArithmeticError('unselected secondary receipt')
        return
    got=deepcopy(actual.get('secondary_acquisition'));full=got.get('full_contractual_source') if isinstance(got,dict)else None
    if not isinstance(full,dict)or not isinstance(full.pop('disclosure',None),str):raise ArithmeticError('secondary full-origin source disclosure absent')
    compare(expected,got,check=check)
