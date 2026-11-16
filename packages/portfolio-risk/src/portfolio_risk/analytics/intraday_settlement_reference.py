"""Independent reference for `intraday-settlement-1`.

Written from docs/models/intraday-settlement.md, not from the Rust source. It keeps
each lot, payment and facility as a small record and walks a priority queue, where
the native replay uses index vectors and a merged work list.
"""
import heapq

DAY = 1440
ARRIVAL, DRAW_CASH, RECEIPT, PAYMENT, MOVE, DRAW, CYCLE = range(7)


def replay(request):
    if request.get('schema') != 'intraday-settlement-1':
        raise ValueError('intraday settlement must select intraday-settlement-1')
    stop = request.get('stop_minute')
    end = stop if stop is not None else request['horizon_minute'] + 1
    systems = {s['id']: s for s in request['systems']}
    facilities = {f['id']: dict(f, used=0.) for f in request.get('facilities', [])}
    lots = {l['id']: dict(l, pledged=False) for l in request.get('lots', [])}
    payments = [dict(p, left=p['amount'], arrived=False, first=None, done=None, late=False)
                for p in request.get('payments', [])]
    receipts = [dict(x, settled=False) for x in request.get('receipts', [])]
    moves, draws = request.get('moves', []), request.get('draws', [])
    legal = request.get('netting_sets', [])
    state = dict(cash=request['opening_cash'], paid=0., received=0., drawn=0., unsettled=0., blocked=0,
                 first_shortfall=None, shortfall_cycles=0, peak=0., peak_minute=None)
    events, heap = [], []

    def push(minute, kind, index, extra=None):
        heapq.heappush(heap, (minute, kind, index, len(heap), extra))

    for i, x in enumerate(receipts):
        if x.get('netting_set') is None:
            push(x['minute'], RECEIPT, i)
    for i, p in enumerate(payments):
        push(p['minute'], PAYMENT, i)
    for i, m in enumerate(moves):
        push(m['minute'], MOVE, i, 'instruction')
    for i, d in enumerate(draws):
        push(d['minute'], DRAW, i)
    for i, s in enumerate(request['systems']):
        for day in range(request['horizon_minute']//DAY + 1):
            if day in s.get('closed_days', []):
                continue
            for t in range(s['open'], s['close'] + 1, s['cycle_minutes']):
                push(day*DAY + t, CYCLE, i)

    def queued():
        return sum(p['left'] for p in payments if p['arrived'])

    def emit(minute, event, ident, amount=0., detail=''):
        total = queued()
        if total > state['peak']:
            state['peak'], state['peak_minute'] = total, minute
        events.append(dict(seq=len(events), minute=minute, event=event, id=ident, amount=amount,
                           cash=state['cash'], queued=total, detail=detail))

    def pay(minute, p, amount, detail):
        p['left'] -= amount
        state['paid'] += amount
        if p['first'] is None:
            p['first'] = minute
        if p['left'] <= 0:
            p['left'], p['done'] = 0., minute
        emit(minute, 'payment_paid', p['id'], amount, detail)

    def blocked_reason(minute, draw, facility):
        threshold = draw.get('only_if_cash_below')
        if threshold is not None and state['cash'] >= threshold:
            return 'not_needed'
        if any(a <= minute < b for a, b in facility.get('outages', [])):
            return 'outage'
        if not facility['open'] <= minute % DAY <= facility['close']:
            return 'facility_closed'
        for ident in draw['lots']:
            lot = lots[ident]
            if lot['pledged']:
                return 'already_pledged'
            if lot['location'] == 'in_transit':
                return 'collateral_in_transit'
            if lot['location'] != facility['location']:
                return 'collateral_not_at_facility'
            if facility['id'] not in lot.get('haircuts', {}):
                return 'ineligible'
        return ''

    def settle_cycle(minute, system):
        capacity = system.get('capacity_per_cycle')
        capacity = float('inf') if capacity is None else capacity
        waiting = lambda: [p for p in payments if p['arrived'] and p['left'] > 0 and p['system'] == system['id']]
        held, short = set(), False
        for name in legal:
            due = [x for x in receipts if not x['settled'] and x['minute'] <= minute
                   and x.get('netting_set') == name and x.get('system') == system['id']]
            if not due:
                continue
            owed = [p for p in waiting() if p.get('netting_set') == name]
            need = sum(p['left'] for p in owed) - sum(x['amount'] for x in due)
            if need > 0 and (need > state['cash'] or need > capacity):
                # The set waits whole: its payments are not paid gross this cycle.
                short = short or need > state['cash']
                held.update(p['id'] for p in owed)
                emit(minute, 'netting_waiting', name, need, 'cash' if need > state['cash'] else 'capacity')
                continue
            if need > 0:
                capacity -= need
            for x in due:
                x['settled'] = True
                state['cash'] += x['amount']
                state['received'] += x['amount']
                emit(minute, 'receipt', x['id'], x['amount'], name)
            for p in owed:
                amount = p['left']
                state['cash'] -= amount
                pay(minute, p, amount, name)
        for p in sorted((p for p in waiting() if p['id'] not in held), key=lambda p: p['minute']):   # stable sort
            amount = min(p['left'], state['cash'], capacity)
            if amount > 0:
                state['cash'] -= amount
                capacity -= amount
                pay(minute, p, amount, system['id'])
            if p['left'] > 0:
                unmet = sum(q['left'] for q in waiting())
                if capacity <= 0 and state['cash'] > 0:
                    emit(minute, 'capacity_limited', system['id'], unmet)
                else:
                    short = True
                    emit(minute, 'shortfall', system['id'], unmet)
                break
        if short:
            if state['first_shortfall'] is None:
                state['first_shortfall'] = minute
            state['shortfall_cycles'] += 1

    while heap:
        minute, kind, index, _, extra = heapq.heappop(heap)
        if minute >= end:
            break
        if kind == ARRIVAL:
            move = moves[index]
            lots[move['lot']]['location'] = move['to']
            emit(minute, 'move_completed', move['id'], 0., move['to'])
        elif kind == DRAW_CASH:
            state['cash'] += extra
            state['drawn'] += extra
            state['unsettled'] -= extra
            emit(minute, 'draw_settled', draws[index]['id'], extra)
        elif kind == RECEIPT:
            x = receipts[index]
            state['cash'] += x['amount']
            state['received'] += x['amount']
            emit(minute, 'receipt', x['id'], x['amount'])
        elif kind == PAYMENT:
            p = payments[index]
            s = systems[p['system']]
            last = s['open'] + (s['close'] - s['open'])//s['cycle_minutes']*s['cycle_minutes']
            p['late'] = minute//DAY in s.get('closed_days', []) or minute % DAY > last
            p['arrived'] = True
            emit(minute, 'payment_requested', p['id'], p['amount'], 'after_cutoff' if p['late'] else '')
        elif kind == MOVE:
            move = moves[index]
            lot = lots[move['lot']]
            of_day = minute % DAY
            if lot['pledged'] or lot['location'] == 'in_transit':
                emit(minute, 'move_blocked', move['id'], 0.,
                     'already_pledged' if lot['pledged'] else 'collateral_in_transit')
            elif extra == 'instruction' and not move['window_open'] <= of_day <= move['window_close']:
                opening = minute - of_day + move['window_open'] + (DAY if of_day > move['window_close'] else 0)
                push(opening, MOVE, index, 'start')
                emit(minute, 'move_waiting', move['id'], 0., 'custodian_window_closed')
            else:
                lot['location'] = 'in_transit'
                push(minute + move['transfer_minutes'], ARRIVAL, index)
                emit(minute, 'move_started', move['id'], 0., move['to'])
        elif kind == DRAW:
            draw = draws[index]
            facility = facilities[draw['facility']]
            reason = blocked_reason(minute, draw, facility)
            amount = 0.
            if not reason:
                value = 0.
                for ident in draw['lots']:
                    value += lots[ident]['value']*(1. - lots[ident]['haircuts'][facility['id']])
                amount = min(value, facility['capacity'] - facility['used'])
                if draw.get('amount') is not None:
                    amount = min(amount, draw['amount'])
                if amount <= 0:
                    reason = 'no_capacity'
            if reason:
                state['blocked'] += 1
                emit(minute, 'draw_blocked', draw['id'], 0., reason)
                continue
            for ident in draw['lots']:
                lots[ident]['pledged'] = True
            facility['used'] += amount
            state['unsettled'] += amount
            push(minute + facility['settlement_lag_minutes'], DRAW_CASH, index, amount)
            emit(minute, 'draw_requested', draw['id'], amount, facility['id'])
        else:
            settle_cycle(minute, request['systems'][index])
    if stop is not None:
        emit(stop, 'stopped', '')
    requested = sum(p['amount'] for p in payments if p['arrived'])
    unpaid = queued()
    error = request['opening_cash'] + state['received'] + state['drawn'] - state['paid'] - state['cash']
    if abs(error) > 1e-9*max(1., request['opening_cash'], state['paid'], state['received']) or state['cash'] < -1e-9 \
            or abs(requested - state['paid'] - unpaid) > 1e-9*max(1., requested):
        raise ArithmeticError('intraday settlement failed to reconcile')
    return dict(schema='intraday-settlement-1', events=events,
                payments=[dict(id=p['id'], system=p['system'], minute=p['minute'], amount=p['amount'],
                               requested=p['arrived'], paid=p['amount'] - p['left'] if p['arrived'] else 0.,
                               unpaid=p['left'] if p['arrived'] else 0., first_paid_minute=p['first'],
                               settled_minute=p['done'], after_cutoff=p['late']) for p in payments],
                summary=dict(opening_cash=request['opening_cash'], closing_cash=state['cash'], requested=requested,
                             paid=state['paid'], unpaid=unpaid, received=state['received'], drawn=state['drawn'],
                             draws_unsettled=state['unsettled'], draws_blocked=state['blocked'],
                             first_shortfall_minute=state['first_shortfall'], shortfall_cycles=state['shortfall_cycles'],
                             peak_queue=state['peak'], peak_queue_minute=state['peak_minute'], stop_minute=stop,
                             reconciliation_error=error))
