"""Explicit independent financial replay of an unpublished sequential witness.

This module is not registered with a model worker/publication route. It does not
prove joined admission, allocator layout, persisted publication or a predecessor.
Expected money always comes from raw independent sources, never moved native state.
"""
from collections import defaultdict
import json

from .commercial_cut_capture_reference import (
    PROOF_KEYS, RECORD_KEYS, _finite_tree, finite_source_order,
)
from .commercial_cut_mixed_reference import raw_prefix, _observed_prefix, _window
from .commercial_candidate_reference import verify_preparation
from .commercial_dto import graph_identity
from .commercial_fee_reference import verify_tables
from .commercial_graph_reference import _economic
from .commercial_ledger_reference import verify_compiler
from .commercial_reference_math import _cash
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity


def _require(ok, message):
    if not ok:
        raise ArithmeticError(message)


_PART_KEYS = frozenset('''raw_request completed_preparation revised_preparation cut_tables
    producer_completion component_receipt source_records joined_raw_admission_proved
    publication_eligible'''.split())


def verify(raw, actual, check=lambda: False):
    """Verify all financial rows/proofs while explicitly denying publication."""
    _require(type(actual) is dict, 'sequential witness object required')
    _finite_tree(actual)
    expected_keys = set('''schema raw_request completed_preparation revised_preparation
        cut_tables producer_completion component_receipt source_records source_record_bytes
        source_preparation_work_upper_reported source_preparation_sum_bytes_upper_reported
        source_preparation_phase_bytes_upper_reported source_capture_work
        source_traversal_work joined_raw_admission_proved publication_eligible'''.split())
    if actual.get('schema') == 'commercial-sequential-cut-witness-2':
        expected_keys.add('construction_census')
        _require(actual.get('construction_census') in ('immutable-index-1', 'historical-1'),
                 'sequential construction census differs')
    _require(type(actual) is dict and set(actual) == expected_keys
             and actual['schema'] in ('commercial-sequential-cut-witness-1', 'commercial-sequential-cut-witness-2')
             and exact_json_identity(raw, actual['raw_request']), 'sequential witness identity differs')
    _require(actual['joined_raw_admission_proved'] is False
             and actual['publication_eligible'] is False, 'unpublished sequential boundary differs')
    # These bounds remain reported metadata. Agreement is not a raw resource proof.
    for key in ('source_record_bytes', 'source_preparation_work_upper_reported',
                'source_preparation_sum_bytes_upper_reported',
                'source_preparation_phase_bytes_upper_reported', 'source_capture_work',
                'source_traversal_work'):
        _require(type(actual[key]) is int and actual[key] > 0, 'sequential reported counter differs')
    return verify_parts(raw, {k: actual[k] for k in _PART_KEYS}, check)


def verify_parts(raw, actual, check=lambda: False):
    """Replay complete financial parts without inventing resource metadata."""
    _require(type(actual) is dict and set(actual) == _PART_KEYS,
             'sequential complete financial parts required')
    _finite_tree(actual)
    _require(exact_json_identity(raw, actual['raw_request'])
             and actual['joined_raw_admission_proved'] is False
             and actual['publication_eligible'] is False,
             'sequential financial identity/evidence boundary differs')
    raw_prefix(raw, check)
    old = raw['completed_candidate']; new = raw['revised_candidate']
    cut = raw['cut_ordinal'] - old['as_of_ordinal']
    expected_completion = dict(complete=True, protocol=9, cut_day=cut,
        native_cut_state_moved=True, original_calendar_retained=True,
        new_sim_started_at_cut=True, prior_publication_verified=False,
        sequential_source_owners=True)
    _require(exact_json_identity(actual['producer_completion'], expected_completion),
             'sequential exact completion differs')
    receipt = actual['component_receipt']
    _require(type(receipt) is dict and set(receipt) == set('''protocol component_work_upper
        component_retained_bytes_upper complete_observed_prefix_bytes runs_moved
        completed_source_borrow_retained raw_coordinator_admission_granted
        independent_persisted_verification'''.split()), 'sequential component receipt fields differ')
    _require(type(receipt['protocol']) is int and receipt['protocol'] == 9
             and type(receipt['runs_moved']) is int and receipt['runs_moved'] == len(old['specification'].get('scenarios', [])) + 1
             and all(receipt[k] is False for k in ('completed_source_borrow_retained',
                 'raw_coordinator_admission_granted', 'independent_persisted_verification')),
             'sequential component ownership/evidence boundary differs')
    for key, limit in [('component_work_upper', 8_000_000),
                       ('component_retained_bytes_upper', 32 << 20),
                       ('complete_observed_prefix_bytes', 4 << 20)]:
        _require(type(receipt[key]) is int and 0 < receipt[key] <= limit,
                 'sequential reported component cap differs')
    expected_records = finite_source_order(raw)
    records = actual['source_records']
    _require(type(records) is list and len(records) == len(expected_records),
             'sequential complete source coverage differs')
    a, ad, af = verify_preparation(old, actual['completed_preparation'], check)
    b, bd, bf = verify_preparation(new, actual['revised_preparation'], check)
    _require(a['horizon'] == b['horizon'] and 0 < cut < a['horizon'], 'sequential calendar differs')
    physical = pricing = 0
    for item, record in zip(expected_records, records, strict=True):
        check()
        _require(type(record) is dict and set(record) == RECORD_KEYS
                 and exact_json_identity(item.metadata,
                     {k: v for k, v in record.items() if k != 'proof'}),
                 'sequential exact source order/metadata differs')
        proof = record['proof']
        wanted = PROOF_KEYS | ({'fee_projection'} if item.position.get('fees') else set())
        _require(type(proof) is dict and set(proof) == wanted
                 and exact_json_identity(item.proof_metadata,
                     {k: proof[k] for k in item.proof_metadata}),
                 'sequential exact complete proof fields differ')
        graph_identity(item.history['source'], proof['source_request'])
        if item.metadata['role'] == 'physical':
            reference = verify_compiler(item.history['source'], proof['projection'], check)['replay']
            physical += 1
        else:
            reference = _economic(item.history['source'], proof['projection'], check)
            pricing += 1
        compare(_cash(reference['credit']), proof['cash'], '$.sequential.source_cash', check)
        if item.position.get('fees'):
            from . import commercial_fee_adoption_reference as fees
            fees.validate(item.position['fees'], item.graph, item.position, check)
            fees.project(item.position['fees'], item.proof_metadata['scenario_id'],
                item.metadata['role'], item.history, reference, proof['fee_projection'], check,
                original=item.original)
    full_a = verify_tables(a, af, ad, None, check)
    full_b = verify_tables(b, bf, bd, None, check)
    _observed_prefix(full_a, full_b, cut, {o['position_id'] for o in old['owners']}, check)
    expected = _window(full_b, actual['revised_preparation']['resolved_specification'], cut, check)
    _require(type(actual['cut_tables']) is dict and not set(actual['cut_tables']) - set(expected),
             'sequential table ownership differs')
    for name, rows in expected.items():
        check()
        compare(rows, actual['cut_tables'].get(name, []), '$.sequential.' + name, check)
    closing = defaultdict(float)
    for row in actual['cut_tables'].get('journal', []):
        closing[tuple(row[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id'))] += row['value']
    trial = {tuple(r[k] for k in ('scenario', 'account', 'gl_account', 'instrument_id')): r['balance']
             for r in actual['cut_tables'].get('trial_balance', [])}
    _require(set(closing) == set(trial), 'sequential closing lineage differs')
    for key, value in closing.items():
        compare(value, trial[key], check=check)
    for component in (actual['completed_preparation'], actual['revised_preparation'],
                      actual['cut_tables'], records):
        _require(len(json.dumps(component, allow_nan=False, separators=(',', ':')).encode()) <= 4 << 20,
                 'sequential witness component exceeds4MiB')
    return dict(financial_replay=True, source_records=len(records),
        physical_sources_verified=physical, pricing_sources_verified=pricing,
        complete_resource_proof=False, independent_persisted_verification=False,
        publication_eligible=False)
