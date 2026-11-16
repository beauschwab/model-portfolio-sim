"""Independent reference for the `rate-shock-1` scenario set and limit status.

Written from the contract text in docs/models/rate-shocks.md, not from the Rust
source. Used only under the explicit Python reference context and by tests.
"""
import math

METRICS = ('eve', 'nii_12m', 'nii_24m')


def _label(value):
    return str(int(value)) if float(value).is_integer() else str(value).replace('.', 'p')


def _instantaneous(policy, tenors):
    out = []
    for bp in policy['parallel_bp']:
        out.append((f'parallel_up_{_label(bp)}', 'parallel', [bp for _ in tenors]))
        out.append((f'parallel_down_{_label(bp)}', 'parallel', [-bp for _ in tenors]))
    decay = [math.exp(-t / policy['decay_years']) for t in tenors]
    short = [policy['short_bp'] * d for d in decay]
    long = [policy['long_bp'] * (1 - d) for d in decay]
    out.append(('short_up', 'short', short))
    out.append(('short_down', 'short', [-s for s in short]))
    for name in ('steepener', 'flattener'):
        a, b = policy[name]
        out.append((name, name, [a * s + b * l for s, l in zip(short, long)]))
    return out


def _names(policy):
    names = [name for name, _, _ in _instantaneous(policy, [1.])]
    for bp in policy['ramp_bp']:
        names += [f'ramp_up_{_label(bp)}', f'ramp_down_{_label(bp)}']
    return names


def _floor(base, shift, floor):
    shocked = base + shift
    if floor is None or shift >= 0:
        return shocked
    return max(shocked, min(floor, base))


def _check(policy):
    if policy.get('schema') != 'rate-shock-1':
        raise ValueError('rate shock policy must select rate-shock-1')
    names = set(_names(policy))
    seen = set()
    for limit in policy.get('limits', []):
        key = (limit['metric'], limit['scenario'])
        valid = (limit['applicability'] == 'internal_limit' and (limit.get('max_decline_pct') or 0) > 0) or (
            limit['applicability'] == 'not_applicable' and limit.get('max_decline_pct') is None)
        if limit['metric'] not in METRICS or limit['scenario'] not in names or key in seen or not valid:
            raise ValueError('invalid rate shock limit')
        seen.add(key)


def rate_shock_set(policy, tenors, swap_rates):
    _check(policy)
    scenarios = []
    for name, family, shift in _instantaneous(policy, tenors):
        rates = [_floor(r, s * 1e-4, policy['floor']) for r, s in zip(swap_rates, shift)]
        scenarios.append(dict(name=name, family=family, timing='instantaneous', swap_rates=rates,
                              requested_shift_bp=shift, applied_shift_bp=[(a - b) * 1e4 for a, b in zip(rates, swap_rates)],
                              floor_applied=any(a != b + s * 1e-4 for a, b, s in zip(rates, swap_rates, shift))))
    for bp in policy['ramp_bp']:
        for side, sign in (('up', 1.), ('down', -1.)):
            requested = sign * bp * 1e-4
            level = _floor(swap_rates[0], requested, policy['floor'])
            bound = level != swap_rates[0] + requested
            shift = level - swap_rates[0] if bound else requested
            scenarios.append(dict(name=f'ramp_{side}_{_label(bp)}', family='ramp', timing='ramp',
                                  ramp=dict(shift=shift, months=policy['ramp_months']), requested_shift_bp=sign * bp,
                                  applied_shift_bp=shift * 1e4, floor_applied=bound))
    return dict(schema='rate-shock-1', scenarios=scenarios, conventions={}, tenors=list(tenors))


def rate_shock_report(policy, base, outcomes):
    _check(policy)
    if [o['name'] for o in outcomes] != _names(policy):
        raise ValueError('rate shock outcomes must cover the policy set in order')

    def values(outcome):
        if outcome.get('unavailable'):
            if outcome['eve'] is not None or outcome['nii']:
                raise ValueError('an unevaluated scenario carries a reason and no values')
            return [None, None, None]
        nii = outcome['nii']
        return [outcome['eve'], sum(nii[:12]) if len(nii) >= 12 else None, sum(nii[:24]) if len(nii) >= 24 else None]

    limits = {(l['metric'], l['scenario']): l for l in policy.get('limits', [])}
    reference = values(base)
    rows, counts = [], {}
    for outcome in outcomes:
        for metric, b, v in zip(METRICS, reference, values(outcome)):
            delta = None if b is None or v is None else v - b
            pct = None if delta is None or b <= 0 else delta / b * 100
            limit = limits.get((metric, outcome['name']))
            headroom = None
            if limit is None:
                status = 'unconfigured'
            elif limit['applicability'] != 'internal_limit':
                status = 'not_applicable'
            elif pct is None:
                status = 'unavailable'
            else:
                headroom = limit['max_decline_pct'] + pct
                status = 'breach' if -pct > limit['max_decline_pct'] else 'within_limit'
            counts[status] = counts.get(status, 0) + 1
            rows.append(dict(scenario=outcome['name'], metric=metric, base=b, value=v, delta=delta, delta_pct=pct,
                             applicability=limit['applicability'] if limit else 'unconfigured',
                             max_decline_pct=limit.get('max_decline_pct') if limit else None, headroom_pct=headroom,
                             status=status, unavailable_reason=None))
    overall = next((s for s in ('breach', 'unavailable', 'within_limit') if counts.get(s)), 'unconfigured')
    return dict(schema='rate-shock-1', rows=rows, status=overall, counts=counts)
