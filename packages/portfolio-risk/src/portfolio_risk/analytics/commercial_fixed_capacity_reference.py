"""Independent raw-dimensional allowance for ordinary fixed commercial cuts.

The native exhaustive allocation inventory must fit this allowance before graph
construction. This module never uses producer allocation counts as dimensions.
Financial replay, complete journal admission and persisted publication remain
separate checks. An allowance is requested heap capacity, not process RSS.
"""
import math

from .commercial_fixed_admission_reference import (
    _candidate_dto, _guard, _require, candidate as indexed_candidate,
    graph as indexed_graph,
)
from .commercial_credit_reference import full_end
from ._deposit_continuation_income_reference import native_json_length as length
from .modeled_json_identity import exact_json_identity


def reservation(raw, check=lambda: False):
    _guard(raw, check)
    raw = _candidate_dto(raw)
    total = length(raw) * 128
    sources = 0
    for unit in raw['commercial']['units']:
        for position in unit['graph']['positions']:
            _require(not any(position.get(k) for k in
                ('exercise', 'generated_policy', 'original_physical')),
                'fixed allowance needs a separate advanced-owner inventory')
            from .commercial_fixed_fee_capacity_reference import reservation as fee_reservation
            total += fee_reservation(position)
            histories = [(h, False) for h in position['original_pricing']]
            for scene in position['histories']:
                histories.extend((h, role == 'physical') for role in
                    ('physical', 'pricing', 'pricing_down', 'pricing_up') for h in scene[role])
            for history, physical in histories:
                if check():
                    raise InterruptedError('fixed capacity census cancelled')
                source = history['source']
                _require(not any(source.get(k) for k in ('prepayment_terms',
                    'maturity_basis_policy', 'performing_draw_tranches', 'reviewed_credit_actions'))
                    and not source['source'].get('transfer')
                    and not source['factor_policy'].get('terminal_obligations'),
                    'fixed source allowance needs separate advanced ownership')
                for side in ('original', 'current'):
                    contract = source['source'][side]
                    _require(contract['principal_plan']['kind'] == 'supplied'
                        and all(p['coupon']['kind'] == 'fixed' for p in contract['periods']),
                        'fixed allowance requires complete supplied fixed contracts')
                periods = len(source['source']['current']['periods'])
                credit = source['credit']
                days = math.ceil(full_end(source) * 365.)
                _require(0 < days <= 4096 and 0 < len(credit['intervals']) <= 4096,
                         'fixed complete daily interval domain differs')
                intervals = min(4096, len(credit['intervals']) + days + 1)
                points = len(credit['paths']) * (
                    intervals + 2 * periods + 2 * credit['max_jumps_per_path'] + 69)
                total += points * (periods + 1) * (16384 if physical else 4096) + 65536
                sources += 1
    _require(0 < total <= 128 << 20, 'fixed raw allowance exceeds128MiB')
    return total, sources


def candidate(raw, check=lambda: False):
    expected, specification = indexed_candidate(raw, check, ordinary_fees=True)
    typed = _candidate_dto(raw)
    portable, sources = reservation(raw, check)
    old_graphs = sum(indexed_graph(u['graph'], check, ordinary_fees=True)[1] for u in typed['commercial']['units'])
    expected['retained_bytes_upper'] += portable - old_graphs
    _require(0 < expected['retained_bytes_upper'] <= 128 << 20,
             'fixed candidate allowance exceeds128MiB')
    return expected, specification, dict(portable_bytes=portable, source_count=sources)


def intake(raw):
    """Count the original envelope independently of the typed/native receipt."""
    shape = dict(objects=0, arrays=0, strings=0, primitives=0,
                 string_bytes=0, array_minimum_bytes=0)

    def visit(value):
        if type(value) is dict:
            shape['objects'] += 1
            for key, child in value.items():
                shape['string_bytes'] += len(key.encode('utf-8'))
                visit(child)
        elif type(value) is list:
            shape['arrays'] += 1
            if value:
                shape['array_minimum_bytes'] += 8 * (
                    8192 if any(type(v) is dict for v in value) else 32)
            for child in value:
                visit(child)
        elif type(value) is str:
            shape['strings'] += 1
            shape['string_bytes'] += len(value.encode('utf-8'))
        elif value is None or type(value) in (bool, int, float):
            shape['primitives'] += 1
        else:
            raise ArithmeticError('fixed intake JSON type differs')

    # Timeout values have identical typed shape. The original raw request,
    # including omitted/default fields, supplies every variable dimension.
    visit(dict(op='analyze_commercial_cut_mixed_journal',
               schema='commercial-cut-mixed-journal-1', execution_timeout_ms=1,
               request=raw))
    upper = (shape['objects'] * (3 * 8192 + 512)
             + (shape['arrays'] + shape['strings'] + shape['primitives']) * 3 * 32
             + shape['string_bytes'] * 2 + shape['array_minimum_bytes'] + 8 * 8192)
    return shape, upper


def verify_packet_resources(raw, packet, check=lambda: False):
    """Verify the complete raw allowance; telemetry cannot enlarge its ceiling."""
    a, _, _ = candidate(raw['completed_candidate'], check)
    b, _, _ = candidate(raw['revised_candidate'], check)
    _, typed = intake(raw)
    canonical = dict(schema=raw['schema'], cut_ordinal=raw['cut_ordinal'],
        completed_candidate=_candidate_dto(raw['completed_candidate']),
        revised_candidate=_candidate_dto(raw['revised_candidate']))
    source_work = a['work_upper'] + b['work_upper']
    prefix_work = length(canonical) * 8
    remaining = 32_000_000 - source_work - prefix_work
    phase = max(a['retained_bytes_upper'], b['retained_bytes_upper']) + (32 << 20) + (4 << 20) + typed + (64 << 10)
    _require(remaining > 0 and phase <= 128 << 20,
             'fixed joint raw allowance exceeds existing limits')
    expected = dict(completed_preparation_work_upper=a['work_upper'],
        revised_preparation_work_upper=b['work_upper'],
        completed_preparation_bytes_upper=a['retained_bytes_upper'],
        revised_preparation_bytes_upper=b['retained_bytes_upper'],
        typed_input_bytes_upper=typed, source_phase_bytes_upper=phase,
        source_preparation_work_upper=source_work,
        raw_prefix_work_reservation=prefix_work, controlled_work_budget=remaining,
        maximum_work=32_000_000, maximum_bytes=128 << 20)
    _require(exact_json_identity(expected, packet.get('resource_admission')),
             'fixed raw-derived joint resource admission differs')
    for side, admission in [('completed', a), ('revised', b)]:
        preparation = packet[side + '_phase']['preparation']
        _require(exact_json_identity(admission['work_upper'], preparation['admitted_work_upper'])
            and exact_json_identity(admission['retained_bytes_upper'], preparation['retained_bytes_upper']),
            'fixed preparation and joint source allowance differ')
    return expected


def financial_parts(raw, packet, check=lambda: False):
    """Adapt framing only; the delegated financial reader grants no authority."""
    from .commercial_sequential_packet_reference import financial_parts as read_parts
    _require(packet.get('schema') == 'commercial-fixed-sequential-cut-1',
             'fixed sequential packet schema differs')
    _require(packet.get('joined_raw_admission_proved') is True
             and packet.get('publication_eligible') is False,
             'fixed native admission/publication boundary differs')
    verify_packet_resources(raw, packet, check)
    # Resource authority is checked above, independently of this financial-only
    # view. Historical witness readers and their false authority flags remain.
    financial = {k: v for k, v in packet.items() if k != 'resource_admission'}
    financial['schema'] = 'commercial-sequential-packet-witness-2'
    financial['joined_raw_admission_proved'] = False
    return read_parts(raw, financial, check)


def verify(raw, actual, check=lambda: False):
    """Replay the raw allowance and financial owners before/after persistence."""
    from .commercial_sequential_cut_reference import verify_parts
    _require(type(actual) is dict and set(actual) == {
        'schema', 'transport', 'raw_source', 'native_packet', 'cut_tables', 'producer_completion'},
        'fixed cut persisted result coverage differs')
    _require(actual['schema'] == 'commercial-cut-mixed-journal-1'
        and actual['transport'] == 'fixed-sequential-cut-1'
        and exact_json_identity(raw, actual['raw_source']),
        'fixed cut exact original input differs')
    parts = financial_parts(raw, actual['native_packet'], check)
    replay = verify_parts(parts['raw_request'], parts, check)
    _require(exact_json_identity(parts['cut_tables'], actual['cut_tables'])
        and exact_json_identity(parts['producer_completion'], actual['producer_completion']),
        'fixed persisted cut tables differ from verified native stream')
    _require(replay['financial_replay'] is True, 'fixed cut financial replay absent')
    return dict(verified=True, raw_graph_units_recomputed=True,
        raw_observed_prefix_recomputed=True, unweighted_surviving_prefix_recomputed=True,
        cut_opening_recomputed=True, complete_original_anchors_retained=True,
        future_journal_replayed=True, gross_closing_replayed=True,
        persisted_partitions_replayed=True, raw_resource_allowance_recomputed=True,
        complete_source_proofs=replay['source_records'],
        prior_publication_verified=False, opening_identity_verified=False,
        general_mixed_book_adoption=False, empirical_validation=False)
