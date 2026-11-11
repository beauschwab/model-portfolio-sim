"""Independent raw fee dimensions for the ordinary fixed cut allowance."""


def events(policy):
    if policy.get('transfer') is not None:
        raise ArithmeticError('fixed fees need separate transferred-right inventory')
    exits = [policy[k] for k in ('default', 'prepayment', 'exercise', 'maturity')]
    recovery = max(len(p['recoveries']) for p in exits)
    refunded = int(any(p.get('refund_earned_fraction', 0.) > 0. for p in exits))
    return len(policy['events']) + len(policy['fees']) * (3 + recovery + refunded)


def histories(position):
    fees = position.get('fees')
    if fees is None:
        return
    if fees.get('original_physical'):
        raise ArithmeticError('fixed fees need separate original Physical inventory')
    def pairs(loans, services, physical):
        index = {h['path_id']: h for h in services}
        if len(index) != len(services) or set(index) != {h['path_id'] for h in loans}:
            raise ArithmeticError('complete fee history identity coverage differs')
        for loan in loans:
            h = index[loan['path_id']]
            extra = len(fees['allocations']) + len(h.get('acquisition_releases', []))
            yield h, loan, physical, extra
    yield from pairs(position['original_pricing'], fees['original_pricing'], False)
    scenes = {s['scenario_id']: s for s in fees['histories']}
    if len(scenes) != len(fees['histories']) or set(scenes) != {s['scenario_id'] for s in position['histories']}:
        raise ArithmeticError('complete fee scenario coverage differs')
    for scene in position['histories']:
        for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
            yield from pairs(scene[role], scenes[scene['scenario_id']][role], role == 'physical')


def graph_allowance(position):
    work = capacity = 0
    for history, loan, physical, extra in histories(position):
        cells = events(history['policy']) * len(loan['source']['credit']['paths'])
        work += cells * 128 * 2 + (extra * 128 if physical else 0)
        capacity += cells * 4096 * 2 + (extra * 4096 if physical else 0)
    return work, capacity


def reservation(position):
    return sum((events(h['policy']) + extra + len(h['policy']['fees']) + 1) * (256 << 10)
        for h, _, _, extra in histories(position))
