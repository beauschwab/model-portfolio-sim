"""Independent raw-contract replay for the actual moved native commercial Sim.

The producer's cut opening is an output to verify, never an input to this replay.
Full-origin reference projection here is independent verification only.
"""
from collections import defaultdict
import json

from .commercial_candidate_reference import verify_preparation
from .commercial_fee_reference import verify_tables
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

SCHEMA = 'commercial-cut-mixed-journal-1'


def verify_completion(actual, cut):
    expected = dict(complete=True, protocol=8, cut_day=cut,
        native_cut_state_moved=True, original_calendar_retained=True,
        new_sim_started_at_cut=True, prior_publication_verified=False)
    if not exact_json_identity(actual, expected):
        raise ArithmeticError('cut exact producer protocol/metadata differs')


from .commercial_cut_controls_reference import same, raw_prefix, _observed_prefix, _window


def verify(raw, actual, check=lambda: False):
    if actual.get('transport') == 'fixed-sequential-cut-1':
        from .commercial_fixed_capacity_reference import verify as verify_fixed
        return verify_fixed(raw, actual, check)
    names = '''schema raw_source origin_asof_ordinal cut_ordinal cut_day full_end_ordinal
        completed_preparation revised_preparation cut_tables producer_completion
        admitted_work_upper retained_bytes_upper cut_component_work_reservation
        cut_component_bytes_reservation component_output_limit_bytes appended_candidates
        completed_prefix_daily_sim_executed_once resumed_future_daily_sim_executed_once
        complete_origin_source_owners_retained cut_started_mixed_sim_adoption
        original_credit_clock_restarted original_loan_anchor_refitted original_fee_anchor_refitted
        surviving_second_acquisition_posted prior_publication_verified opening_identity_verified
        general_mixed_book_adoption empirical_validation scope'''
    wanted = set(names.split())
    selected = raw.get('preparation_receipt_policy') is not None
    if selected:
        wanted.add('selected_admission')
    if (set(actual) != wanted or raw['schema'] != SCHEMA or actual['schema'] != SCHEMA
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
            ('cut_component_work_reservation', actual['selected_admission']['cut_ledger_census']['work_upper'] if selected else 8_000_000), ('cut_component_bytes_reservation', 32 << 20),
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
    verify_completion(actual['producer_completion'], cut)
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
            if type(prep[key]) is not int or (not selected and prep[key] != actual[key]):
                raise ArithmeticError('cut dual preparation coexistence census differs')
    if selected:
        from .commercial_cut_receipts_reference import verify as verify_receipts
        verify_receipts(raw, actual)
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
    if selected:
        from .commercial_cut_receipts_reference import require_raw_derived_admission
        require_raw_derived_admission()
    return dict(verified=True, raw_graph_units_recomputed=True, raw_observed_prefix_recomputed=True,
        unweighted_surviving_prefix_recomputed=True, cut_opening_recomputed=True,
        complete_original_anchors_retained=True, future_journal_replayed=True,
        gross_closing_replayed=True, persisted_partitions_replayed=True,
        prior_publication_verified=False, opening_identity_verified=False,
        general_mixed_book_adoption=False, empirical_validation=False)
