"""Independent complete-origin and prospective-window publication verification."""
from copy import deepcopy
from .commercial_ledger_reference import verify_compiler, identity
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity
from .commercial_dto import typed

SCHEMA = 'commercial-holding-continuation-1'


def raw_identity(a, b):
    if not exact_json_identity(typed(a), typed(b)):
        raise ArithmeticError('commercial completed prefix or original authority changed')


def prefix(raw, check=lambda: False):
    """Check supplied histories without accepting producer opening money."""
    old = typed(raw['completed_source']); new = typed(raw['revised_source'])
    cut = raw['cut_ordinal']; origin = old['source']['current']['asof_ordinal']
    if type(cut) is not int or cut <= origin:
        raise ArithmeticError('holding cut must follow original acquisition')
    year = (cut-origin)/365.
    for field in ('original', 'acquisition', 'policy'):
        raw_identity(old['source'][field], new['source'][field])
    for field in ('factor_policy', 'performing_draw_tranches', 'prepayment_terms', 'maturity_basis_policy', 'reviewed_credit_actions'):
        raw_identity(old.get(field), new.get(field))
    a = deepcopy(old['source']['current']); b = deepcopy(new['source']['current'])
    ap = a.pop('periods'); bp = b.pop('periods'); raw_identity(a, b)
    if len(ap) != len(bp):raise ArithmeticError('holding period coverage changed')
    for p, q in zip(ap, bp, strict=True):
        check()
        for field in ('period_id', 'start_ordinal', 'end_ordinal', 'payment_ordinal', 'day_count'):
            raw_identity(p[field], q[field])
        if p['start_ordinal'] < cut or p['payment_ordinal'] <= cut:raw_identity(p, q)
        observations = lambda row: [o for o in row['coupon'].get('observations', []) if o['fixing_ordinal'] <= cut]
        raw_identity(observations(p), observations(q))
    a = deepcopy(old['credit']); b = deepcopy(new['credit'])
    ai = a.pop('intervals'); bi = b.pop('intervals'); raw_identity(a, b)
    if len(ai) != len(bi):raise ArithmeticError('holding credit interval coverage changed')
    start = 0.
    for index, (p, q) in enumerate(zip(ai, bi, strict=True)):
        check();raw_identity(p['end_year'], q['end_year'])
        if start < year:
            raw_identity(p, q)
            for policy, field in (('accounting', 'recognition'), (None, 'accounting_rates')):
                previous = old[policy][field] if policy else old[field]
                current = new[policy][field] if policy else new[field]
                raw_identity([v for v in previous if v['interval_index'] == index], [v for v in current if v['interval_index'] == index])
        start = p['end_year']
    a = deepcopy(old['accounting']); b = deepcopy(new['accounting'])
    a.pop('recognition'); b.pop('recognition'); raw_identity(a, b)
    raw_identity([v for v in old['source'].get('current_accounting_rate_reviews', []) if v['reset_ordinal'] < cut],
             [v for v in new['source'].get('current_accounting_rate_reviews', []) if v['reset_ordinal'] < cut])
    return year


def verify(raw, result, check=lambda: False):
    """Recompute both complete credit/carry/coupon owners, then select their suffix."""
    check();identity(raw, result['raw_source'])
    if raw['schema'] != SCHEMA or result.get('schema') != SCHEMA:
        raise ArithmeticError('holding continuation schema differs')
    if result.get('complete_origin_replayed') is not True:raise ArithmeticError('complete original replay missing')
    for flag in ('credit_clock_restarted', 'original_anchor_refitted', 'second_acquisition_posted', 'mixed_ledger_adoption',
                 'verified_prior_publication', 'secondary_acquisition_adoption', 'empirical_validation'):
        if result.get(flag) is not False:raise ArithmeticError('holding adoption boundary differs')
    year = prefix(raw, check); compare(year, result['cut_year'], check=check)
    a = verify_compiler(raw['completed_source'], result['completed'], check)
    b = verify_compiler(raw['revised_source'], result['revised'], check)
    for field, limit in (('admitted_work_upper', 8_000_000), ('retained_bytes_upper', 128 << 20)):
        if type(result.get(field)) is not int or not 0 < result[field] <= limit:raise ArithmeticError('holding admission differs')
    if len(result['openings']) != len(raw['completed_source']['credit']['paths']):raise ArithmeticError('holding opening path coverage differs')
    for path, actual in zip(raw['completed_source']['credit']['paths'], result['openings'], strict=True):
        check();key = path['path_key']
        before = [g for g in a['groups'] if g['path_key'] == key and g['year'] <= year]
        revised = [(i, g) for i, g in enumerate(b['groups']) if g['path_key'] == key and g['year'] <= year]
        compare(before, [g for _, g in revised], check=check)
        # Producer prefix records must themselves be byte/scalar identical, even
        # when independent floating-point reductions use the ordinary tolerance.
        native = lambda owner: [g for g in result[owner]['accounting']['groups'] if g['path_key'] == key and g['year'] <= year]
        if not exact_json_identity(native('completed'), native('revised')):raise ArithmeticError('native completed monetary prefix changed')
        if not revised:raise ArithmeticError('opening prefix absent')
        index, group = revised[-1]
        old_carry = [p for p in a['replay']['carry_points'] if p['path_key'] == key and p['year'] <= year]
        new_carry = [p for p in b['replay']['carry_points'] if p['path_key'] == key and p['year'] <= year]
        compare(old_carry, new_carry, check=check)
        if not new_carry or group['year'] != year and not group['next']['in_default']:raise ArithmeticError('cut is not a native earning boundary')
        future = [(i, g) for i, g in enumerate(b['groups']) if g['path_key'] == key and g['year'] > year]
        if any(g['phase'] == 'reviewed_opening_snapshot' for _, g in future):raise ArithmeticError('second holding acquisition')
        expected = dict(path_key=key, through_native_group_index=index, book=group['next'], carry=new_carry[-1],
                        subsequent_native_group_indices=[i for i, _ in future], subsequent_cash=sum(g['cash'] for _, g in future),
                        subsequent_effective_interest=sum(g['effective_interest'] for _, g in future),
                        subsequent_basis_realization=sum(g['basis_realization'] for _, g in future))
        compare(expected, actual, check=check)
    return dict(verified=True, raw_source_recomputed=True, completed_prefix_recomputed=True, original_yield_retained=True,
                subsequent_postings_recomputed=True, credit_clock_restarted=False, mixed_ledger_adoption=False,
                verified_prior_publication=False, secondary_acquisition_adoption=False, empirical_validation=False)
