"""Independent fee source/ordered intent replay; never a production calculator.

This reference accepts explicit reviewed service/settlement/disposition tapes.
It does not infer native credit events, approval or empirical accuracy.
"""
from copy import deepcopy
import math
from .modeled_json_identity import exact_json_identity

SCHEMA = 'nonintegral-fee-accounting-1'
FIELDS = ('earned', 'cancelled_unearned', 'waived_earned', 'recovered_waived',
          'net_service_cash', 'fee_receivable', 'fee_payable',
          'deferred_fee_income', 'prepaid_fee_expense', 'unearned_entitlement')


def _typed_money(raw):
    """Mirror only the DTO's f64 fields, retaining strict original clock categories."""
    source=deepcopy(raw)
    for fee in source['fees']:
        if fee.get('earned_refunds_allowed') is False:
            fee.pop('earned_refunds_allowed')
    for rows,key in ((source['fees'],'entitlement'),(source['events'],'amount')):
        for row in rows:
            if type(row[key])not in(int,float) or not math.isfinite(row[key]) or abs(row[key])>1e15:
                raise ValueError('finite typed fee money required')
            row[key]=float(row[key])
    return source


def _compare(a, b):
    if type(a) is dict:
        if type(b) is not dict or set(a) != set(b): raise ValueError('fee result fields differ')
        for k in a: _compare(a[k], b[k])
    elif type(a) is list:
        if type(b) is not list or len(a) != len(b): raise ValueError('fee result rows differ')
        for x, y in zip(a, b, strict=True): _compare(x, y)
    elif type(a) is float:
        if type(b) not in (int, float) or not math.isfinite(b) or not math.isclose(a, b, rel_tol=3e-12, abs_tol=3e-12):
            raise ValueError('fee money differs from independent replay')
    elif not exact_json_identity(a, b): raise ValueError('fee source/clock/adoption identity differs')


def replay(raw, check=lambda: False, *, transfer=None):
    check()
    if raw['schema'] != SCHEMA or raw['measure'] != 'physical': raise ValueError('physical fee selection required')
    raw=_typed_money(raw)
    states = {f['fee_id']: dict.fromkeys(FIELDS, 0.) for f in raw['fees']}
    fees = {f['fee_id']: f for f in raw['fees']}
    if len(fees) != len(raw['fees']) or not 1 <= len(fees) <= 2048 or len(raw['events']) > 8192:
        raise ValueError('fee identity/cardinality differs')
    for f in raw['fees']: states[f['fee_id']]['unearned_entitlement'] = float(f['entitlement'])
    groups=[];native_transfer=None
    if transfer is not None:
        if any(e['event_id'].startswith('native-fee-transfer:')for e in raw['events']):raise ValueError('reserved native transferred fee event identity')
        complete=deepcopy(transfer['origin_service']);complete['measure']='physical';replay(complete,check)
        origin=deepcopy(transfer['origin_service']);origin['measure']='physical';origin['events']=[e for e in origin['events']if e['ordinal']<raw['asof_ordinal']];origin['reporting_end_ordinal']=raw['asof_ordinal']
        prefix=replay(origin,check)
        states={s['fee_id']:deepcopy(s['state'])for s in prefix['closing']}
        for state in states.values():
            net=state['fee_receivable']+state['fee_payable']+state['deferred_fee_income']+state['prepaid_fee_expense']
            state['acquired_stock_selected']=True
            if net!=0.:state['transferred_net_stock']=net
        native_transfer=dict(origin=_typed_money(transfer['origin_service']),acquisition_ordinal=raw['asof_ordinal'],review=deepcopy(transfer['review']),states=list(deepcopy(states).values()))
        for i,fee in enumerate(raw['fees']):
            state=states[fee['fee_id']];net=state.get('transferred_net_stock',0.);postings=[]
            for account,value in [('cash',-net)]+[(k,state[k])for k in FIELDS[5:9]]:
                if value!=0.:postings.append(dict(account=account,amount=float(value)))
            event=dict(event_id=f"native-fee-transfer:{transfer['review']['id']}:{i}",fee_id=fee['fee_id'],source=deepcopy(fee['source']),review=deepcopy(transfer['review']),ordinal=raw['asof_ordinal'],sequence=i,action='acquire_transferred_stock',reason='other_reviewed',amount=float(net))
            groups.append(dict(event=event,policy=deepcopy(raw['policy']),postings=postings,next=deepcopy(state)))
        for i,event in enumerate(raw['events']):event['sequence']=len(raw['fees'])+i
    reporting = deepcopy(states); ids = set(); previous = None
    for e in raw['events']:
        check(); clock = (e['ordinal'], e['sequence'])
        if e['event_id'] in ids or previous is not None and clock <= previous: raise ValueError('fee source order differs')
        if not raw['asof_ordinal'] <= e['ordinal'] <= raw['complete_end_ordinal']: raise ValueError('fee source date differs')
        ids.add(e['event_id']); previous = clock
        f = fees[e['fee_id']]; s = states[e['fee_id']]; old = deepcopy(s)
        refunded=s.get('refunded_earned',0.)
        sign = 1. if f['entitlement'] > 0. else -1.; amount = float(e['amount']); magnitude = sign * amount
        acquired=s.get('acquired_stock_selected',False)and e['action']in('acquire','release_acquired_cost')
        if acquired:magnitude=abs(amount)
        if magnitude <= 0. or not exact_json_identity(f['source'], e['source']): raise ValueError('fee sign/source differs')
        action = e['action']; capacity = {
            'acquire': 1e15,
            'release_acquired_cost': (s.get('acquired_cost',0.)-s.get('released_acquired_cost',0.))*sign,
            'earn': s['unearned_entitlement'], 'cancel_unearned': s['unearned_entitlement'],
            'cash': f['entitlement']-s['cancelled_unearned']-refunded-s['waived_earned']-s['net_service_cash'],
            'refund_prepaid': s['net_service_cash']-s['earned']+refunded+s['waived_earned'],
            'refund_earned': min((s['earned']-refunded-s['waived_earned'])*sign,s['net_service_cash']*sign)*sign,
            'waive_unpaid_earned': s['earned']-refunded-s['waived_earned']-s['net_service_cash'],
            'recover_waived': s['waived_earned']-s['recovered_waived'],
        }[action] * (1. if action in ('acquire','release_acquired_cost')else sign)
        if acquired and action=='release_acquired_cost':
            remaining=s.get('acquired_cost',0.)-s.get('released_acquired_cost',0.)
            if amount*remaining<=0.:raise ValueError('acquired residual valuation sign differs')
            capacity=abs(remaining)
        if action=='acquire' and(e['ordinal']!=raw['asof_ordinal']or s.get('acquired_cost',0.)!=0. or not s.get('acquired_stock_selected',False)and(s['earned']!=0. or s['net_service_cash']!=0.)):raise ValueError('original fee acquisition must precede service/cash')
        if magnitude > capacity: raise ValueError('fee event exceeds reviewed rights')
        if action == 'earn' and e['reason'] != 'service' or action in ('cash', 'refund_prepaid') and e['reason'] != 'settlement':
            raise ValueError('fee service/settlement reason differs')
        if action in ('waive_unpaid_earned', 'cancel_unearned', 'recover_waived', 'refund_earned','acquire','release_acquired_cost') and e['reason'] not in ('default', 'exercise', 'maturity', 'other_reviewed'):
            raise ValueError('fee disposition review differs')
        postings = []
        def post(account, value):
            if value != 0.: postings.append(dict(account=account, amount=float(value)))
        if action=='acquire':s['acquired_cost']=amount;post('cash',-amount)
        elif action=='release_acquired_cost':s['released_acquired_cost']=s.get('released_acquired_cost',0.)+amount;post('fee_acquisition_expense',amount)
        elif action == 'earn': s['earned'] += amount; post('fee_income', -amount)
        elif action == 'cash': s['net_service_cash'] += amount; post('cash', amount)
        elif action == 'refund_prepaid':
            if f['prepaid_refunds_allowed'] is not True: raise ValueError('fee refund terms differ')
            s['net_service_cash'] -= amount; post('cash', -amount)
        elif action == 'refund_earned':
            if f.get('earned_refunds_allowed') is not True: raise ValueError('earned fee refund terms differ')
            s['refunded_earned']=refunded+amount
            s['net_service_cash']-=amount;post('cash',-amount);post('fee_income',amount)
        elif action == 'waive_unpaid_earned': s['waived_earned'] += amount; post('fee_writeoff_expense', amount)
        elif action == 'cancel_unearned': s['cancelled_unearned'] += amount
        else: s['recovered_waived'] += amount; post('cash', amount); post('fee_recovery_income', -amount)
        net = sign * (s['earned']-s.get('refunded_earned',0.)-s['waived_earned']-s['net_service_cash'])
        s['fee_receivable'] = max(net, 0.) if sign > 0. else 0.
        s['fee_payable'] = -max(net, 0.) if sign < 0. else 0.
        s['deferred_fee_income'] = min(net, 0.) if sign > 0. else 0.
        s['prepaid_fee_expense'] = -min(net, 0.) if sign < 0. else 0.
        s['unearned_entitlement'] = f['entitlement']-s['earned']-s['cancelled_unearned']
        basis=s.get('acquired_cost',0.)-s.get('released_acquired_cost',0.)
        asset_contra=min(basis,0.)if s.get('acquired_stock_selected')and s.get('transferred_net_stock',0.)>0. else 0.
        liability_contra=max(basis,0.)if s.get('acquired_stock_selected')and s.get('transferred_net_stock',0.)<0. else 0.
        for key,value in (('acquired_fee_asset_valuation_adjustment',asset_contra),('acquired_fee_liability_valuation_adjustment',liability_contra),('acquired_fee_asset',0. if liability_contra else max(basis,0.)),('acquired_fee_liability',0. if asset_contra else min(basis,0.))):
            if value!=0.:s[key]=value
            else:s.pop(key,None)
            post(key,value-old.get(key,0.))
        for account in FIELDS[5:9]: post(account, s[account]-old[account])
        if abs(sum(p['amount'] for p in postings)) > max(1e-9, 64*math.ulp(1.)*magnitude): raise ValueError('fee intent imbalance')
        groups.append(dict(event=deepcopy(e), policy=deepcopy(raw['policy']), postings=postings, next=deepcopy(s)))
        if e['ordinal'] <= raw['reporting_end_ordinal']: reporting[e['fee_id']] = deepcopy(s)
    def rows(values):
        return [dict(fee_id=f['fee_id'], source=deepcopy(f['source']), position_id=f['position_id'],
                     tranche_id=f['tranche_id'], state=values[f['fee_id']]) for f in raw['fees']]
    expected = dict(schema=SCHEMA, raw_request=raw, groups=groups, reporting=rows(reporting), closing=rows(states),
                    full_source_tail_replayed=True, loan_carrying_value_changed=False, native_credit_event_binding=False,
                    mixed_ledger_adoption=False, pricing_unit_ftp_adoption=False, empirical_validation=False, accounting_approval=False)
    if native_transfer is not None:expected['native_transfer']=native_transfer
    check(); return expected


def verify(raw, result, check=lambda: False, *, transfer=None):
    expected=replay(raw,check,transfer=transfer)
    _compare(expected,result);check()
    return dict(verified=True, raw_source_recomputed=True, ordered_fee_intents_replayed=True,
                reporting_and_full_tail_replayed=True, loan_carrying_value_changed=False,
                native_credit_event_binding=False, mixed_ledger_adoption=False, pricing_unit_ftp_adoption=False,
                empirical_validation=False, accounting_approval=False)
