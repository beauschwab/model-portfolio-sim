"""Independent raw-contract replay for the actual moved native commercial Sim.

The producer's cut opening is an output to verify, never an input to this replay.
Full-origin reference projection here is independent verification only.
"""
from collections import defaultdict
from copy import deepcopy
import json
import math

from .commercial_candidate_reference import verify_preparation
from .commercial_candidate_holding_reference import raw_prefix as survivor_prefix
from .commercial_dto import graph_typed
from .commercial_fee_reference import verify_tables
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

SCHEMA = 'commercial-cut-mixed-journal-1'


def same(a, b):
    if not exact_json_identity(graph_typed(a), graph_typed(b)):
        raise ArithmeticError('cut immutable owner or observed raw prefix differs')


def raw_prefix(raw, check=lambda: False):
    """Validate surviving complete origins, then separately validate new dates."""
    old = raw['completed_candidate']; new = deepcopy(raw['revised_candidate'])
    n = len(old['commercial']['units']); cut = raw['cut_ordinal']
    if min(len(new['commercial']['units']), len(new['owners']), len(new['allocation']),
           len(new['specification']['positions'])) < n:
        raise ArithmeticError('cut cannot remove surviving commercial owners')
    same(old['as_of_ordinal'], new['as_of_ordinal'])
    for a, b in [(old['commercial'], new['commercial'])] + [
            (a['graph'], b['graph']) for a, b in zip(
                old['commercial']['units'], new['commercial']['units'][:n], strict=True)]:
        check()
        same([d for d in a['reporting_ordinals'] if d <= cut],
             [d for d in b['reporting_ordinals'] if d <= cut])
        b['reporting_ordinals'] = deepcopy(a['reporting_ordinals'])
    appended = new['commercial']['units'][n:]
    if any(u['graph']['as_of_ordinal'] <= cut for u in appended):
        raise ArithmeticError('cut appended actual acquisition must follow cut')
    new['commercial']['units'] = new['commercial']['units'][:n]
    for key in ('owners', 'allocation'):
        new[key] = new[key][:n]
    new['specification']['positions'] = new['specification']['positions'][:n]
    new['reporting_end_ordinal'] = old['reporting_end_ordinal']
    if 'horizon_days' in old['specification']:
        new['specification']['horizon_days'] = old['specification']['horizon_days']
    survivor_prefix(dict(completed_candidate=old, revised_candidate=new, cut_ordinal=cut), check)
    return len(appended)


def _observed_prefix(old, new, cut, survivor_ids, check):
    """Compare unweighted native endpoints, even when allocation is zero."""
    for name in set(old) | set(new):
        if name in ('summary', 'trial_balance', 'closing_statements', 'consolidated', 'attribution'):
            continue
        def rows(tables):
            result = []
            for row in tables.get(name, []):
                if type(row.get('day')) is not int or row['day'] > cut:
                    continue
                position = row.get('position_id', row.get('position', row.get('instrument_id', '')))
                if position and position not in survivor_ids:
                    # Future candidates' generic zero exposure placeholders are
                    # not surviving financial owners or money at the cut.
                    if name != 'exposures' or any(row.get(k, 0.) != 0. for k in (
                            'balance', 'allowance', 'commitment', 'accrued_interest')):
                        raise ArithmeticError('future candidate has observed money/source groups')
                    continue
                result.append(row)
            return result
        check()
        if not exact_json_identity(rows(old), rows(new)):
            raise ArithmeticError('cut exact ordered unweighted observed monetary prefix differs/' + name)


def _window(full, specification, cut, check):
    """Retain actual full-origin counters, create signed opening, filter future.

    P&L stock remains in its existing GL, and original transaction numbers
    survive. The resumed Sim's accounting history is not a new acquisition.
    """
    balances = defaultdict(float)
    for row in full['journal']:
        check()
        if type(row.get('value')) is not float or not math.isfinite(row['value']):
            raise ArithmeticError('cut raw journal scalar differs')
        if row['day'] <= cut:
            balances[tuple(row[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id'))] += row['value']
    result = {name: [] for name in full}
    scenarios = ['baseline'] + [s['name'] for s in specification['scenarios']]
    for scenario in scenarios:
        opening = [(key, value) for key, value in sorted(balances.items()) if key[0] == scenario]
        for (run, account, gl, instrument), value in opening:
            result['journal'].append(dict(transaction_number=0, scenario=run, day=cut,
                account=account, event='native_verified_cut_opening', gl_account=gl,
                instrument_id=instrument, value=value))
        result['journal'].extend(deepcopy(r) for r in full['journal']
                                 if r['scenario'] == scenario and r['day'] > cut)
    snapshots = {'path', 'exposures', 'commercial_daily', 'commercial_closing',
                 'commercial_fee_daily', 'commercial_fee_closing'}
    terminal = {'summary', 'trial_balance', 'closing_statements', 'consolidated', 'reverse_grid'}
    for name, rows in full.items():
        if name in terminal:
            result[name] = deepcopy(rows)
        elif name in snapshots:
            result[name] = [deepcopy(r) for r in rows if r['day'] >= cut]
        elif name not in ('journal', 'attribution'):
            result[name] = [deepcopy(r) for r in rows if type(r.get('day')) is int and r['day'] > cut]
    totals = defaultdict(lambda: [0., 0., 0.])
    for row in result['ledger']:
        check()
        for i, k in enumerate(('cash', 'earnings', 'aoci')):
            totals[row['scenario'], row['account'], row['event']][i] += row[k]
    keys = sorted({(a, e) for _, a, e in totals})
    for scenario in scenarios[1:]:
        for account, event in keys:
            base = totals['baseline', account, event]; other = totals[scenario, account, event]
            result['attribution'].append(dict(scenario=scenario, account=account, event=event,
                cash_delta=other[0]-base[0], earnings_delta=other[1]-base[1], aoci_delta=other[2]-base[2]))
    return result


def verify(raw, actual, check=lambda: False):
    names = '''schema raw_source origin_asof_ordinal cut_ordinal cut_day full_end_ordinal
        completed_preparation revised_preparation cut_tables producer_completion
        admitted_work_upper retained_bytes_upper cut_component_work_reservation
        cut_component_bytes_reservation component_output_limit_bytes appended_candidates
        completed_prefix_daily_sim_executed_once resumed_future_daily_sim_executed_once
        complete_origin_source_owners_retained cut_started_mixed_sim_adoption
        original_credit_clock_restarted original_loan_anchor_refitted original_fee_anchor_refitted
        surviving_second_acquisition_posted prior_publication_verified opening_identity_verified
        general_mixed_book_adoption empirical_validation scope'''
    if (set(actual) != set(names.split()) or raw['schema'] != SCHEMA or actual['schema'] != SCHEMA
            or not exact_json_identity(raw, actual['raw_source'])):
        raise ArithmeticError('cut exact raw/result identity differs')
    check(); appended = raw_prefix(raw, check)
    old = raw['completed_candidate']; new = raw['revised_candidate']
    a, ad, af = verify_preparation(old, actual['completed_preparation'], check)
    b, bd, bf = verify_preparation(new, actual['revised_preparation'], check)
    origin = old['as_of_ordinal']; cut = raw['cut_ordinal'] - origin
    if a['horizon'] != b['horizon'] or not 0 < cut < a['horizon']:
        raise ArithmeticError('cut first tier requires same complete native horizon')
    for key, value in (('origin_asof_ordinal', origin), ('cut_ordinal', raw['cut_ordinal']),
            ('cut_day', cut), ('full_end_ordinal', origin + b['horizon']), ('appended_candidates', appended),
            ('cut_component_work_reservation', 8_000_000), ('cut_component_bytes_reservation', 32 << 20),
            ('component_output_limit_bytes', 4 << 20)):
        if type(actual[key]) is not int or actual[key] != value:
            raise ArithmeticError('cut exact calendar/capacity differs/' + key)
    for key in ('completed_prefix_daily_sim_executed_once', 'resumed_future_daily_sim_executed_once',
                'complete_origin_source_owners_retained', 'cut_started_mixed_sim_adoption'):
        if actual[key] is not True:
            raise ArithmeticError('cut actual native capability evidence absent')
    for key in ('original_credit_clock_restarted', 'original_loan_anchor_refitted', 'original_fee_anchor_refitted',
                'surviving_second_acquisition_posted', 'prior_publication_verified', 'opening_identity_verified',
                'general_mixed_book_adoption', 'empirical_validation'):
        if actual[key] is not False:
            raise ArithmeticError('cut adoption/evidence boundary differs')
    if not exact_json_identity(actual['producer_completion'], dict(complete=True, protocol=8)):
        raise ArithmeticError('cut producer protocol differs')
    for key, limit in (('admitted_work_upper', 32_000_000), ('retained_bytes_upper', 128 << 20)):
        if type(actual[key]) is not int or not 0 < actual[key] <= limit:
            raise ArithmeticError('cut joined native bound differs')
    if type(actual['scope']) is not str or not actual['scope']:
        raise ArithmeticError('cut disclosure absent')
    for component in (actual['completed_preparation'], actual['revised_preparation'], actual['cut_tables']):
        if len(json.dumps(component, allow_nan=False, separators=(',', ':')).encode()) > 4 << 20:
            raise ArithmeticError('cut component exceeds4MiB')
    for prep in (actual['completed_preparation'], actual['revised_preparation']):
        for key in ('admitted_work_upper', 'retained_bytes_upper'):
            if type(prep[key]) is not int or prep[key] != actual[key]:
                raise ArithmeticError('cut dual preparation coexistence census differs')
    # Rebuild both entire monetary histories independently. This verifier does
    # not invoke a native pricer or consume any producer monetary checkpoint.
    full_a = verify_tables(a, af, ad, None, check)
    full_b = verify_tables(b, bf, bd, None, check)
    _observed_prefix(full_a, full_b, cut, {o['position_id'] for o in old['owners']}, check)
    expected = _window(full_b, actual['revised_preparation']['resolved_specification'], cut, check)
    allowed = set(expected)
    if set(actual['cut_tables']) - allowed:
        raise ArithmeticError('cut table ownership differs')
    for name, rows in expected.items():
        compare(rows, actual['cut_tables'].get(name, []), 'cut.' + name, check)
    closing = defaultdict(float)
    for row in actual['cut_tables'].get('journal', []):
        closing[tuple(row[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id'))] += row['value']
    trial = {tuple(r[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id')): r['balance']
             for r in actual['cut_tables'].get('trial_balance', [])}
    if set(closing) != set(trial):
        raise ArithmeticError('cut opening/future closing GL lineage differs')
    for key, value in closing.items():
        compare(value, trial[key], check=check)
    return dict(verified=True, raw_graph_units_recomputed=True, raw_observed_prefix_recomputed=True,
        unweighted_surviving_prefix_recomputed=True, cut_opening_recomputed=True,
        complete_original_anchors_retained=True, future_journal_replayed=True,
        gross_closing_replayed=True, persisted_partitions_replayed=True,
        prior_publication_verified=False, opening_identity_verified=False,
        general_mixed_book_adoption=False, empirical_validation=False)
