"""Portable metadata controls; these deliberately do not fabricate finance proof."""
from copy import deepcopy
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics import commercial_cut_capture_reference as reference
from portfolio_risk.analytics.commercial_dto import graph_typed

ROOT = Path(__file__).resolve().parents[3]


def encode(rows):
    return b''.join(json.dumps(row, allow_nan=False, separators=(',', ':')).encode() + b'\n'
                    for row in rows)


def receipt_for(data, rows):
    return dict(schema='commercial-cut-source-capture-receipt-1', typed_request_bytes=1000,
        source_preparation_work_upper=1, raw_prefix_work_upper=8000,
        capture_work=2 * len(data) + 16 * (len(rows) - 1) + 2048,
        admitted_work_upper=8001 + 2 * len(data) + 16 * (len(rows) - 1) + 2048,
        reported_retained_bytes_upper=1, encoded_bytes=len(data), source_records=len(rows)-1,
        counts=deepcopy(rows[-1]['counts']), all_selected_source_records_captured=True,
        retains_typed_original_economic_owners=False, original_quote_or_anchor_refitted=False,
        cut_sim_executed=False, cut_admission_granted=False, complete_source_sim_codec_bounds=False,
        independent_persisted_verification=False, publication_eligible=False)


@pytest.fixture
def control():
    raw = json.loads((ROOT / 'packages/portfolio-risk/tests/fixtures/'
                     'commercial-short-secondary-cut-refusal.json').read_text())
    raw['preparation_receipt_policy'] = 'encode_native_preparations_before_value_decode-1'
    rows = []
    counts = dict(original_pricing=0, original_physical=0, physical=0,
                  pricing=0, pricing_down=0, pricing_up=0)
    # Independent fixture order, not reference.expected_sources(). Projection and
    # fee dictionaries intentionally contain no financial proof or calculated cash.
    for side in ('completed', 'revised'):
        for u, unit in enumerate(raw[side + '_candidate']['commercial']['units']):
            graph = unit['graph']
            for p, position in enumerate(graph['positions']):
                histories = [(None, 'original_pricing', 'pricing', h, history,
                              graph['original_scenario'])
                             for h, history in enumerate(position['original_pricing'])]
                for s, scenario in enumerate(graph['scenarios']):
                    current = next(h for h in position['histories']
                                   if h['scenario_id'] == scenario['scenario_id'])
                    index = 0
                    for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
                        for history in current[role]:
                            histories.append((s, 'current', role, index, history, scenario))
                            index += 1
                for s, scope, role, h, history, scenario in histories:
                    proof = dict(position_id=position['position_id'],
                        scenario_id=scenario['scenario_id'], run_id=scenario['run_id'],
                        path_id=history['path_id'], path_key=history['source']['credit']['paths'][0]['path_key'],
                        weight=float(history['weight']), role=role,
                        source_request=graph_typed(history['source']), cash=[], projection={})
                    if position.get('fees'):
                        proof['fee_projection'] = {}
                    rows.append(dict(schema='commercial-cut-source-record-1', side=side,
                        unit_index=u, unit_id=unit['unit_id'], scope=scope,
                        position_index=p, scenario_index=s, history_index=h, role=role, proof=proof))
                    counts[scope if scope == 'original_pricing' else role] += 1
    rows.append(dict(schema='commercial-cut-source-capture-complete-1',
        source_records=len(rows), counts=counts, capture_complete=True,
        cut_sim_executed=False, publication_eligible=False))
    data = encode(rows)
    return raw, rows, data, receipt_for(data, rows)


def test_portable_layout_covers_all_raw_sources_without_claiming_financial_replay(control):
    raw, rows, data, receipt = control
    result = reference.validate_layout(raw, data, receipt)
    assert result['source_records'] == 27
    assert result['counts'] == dict(original_pricing=3, original_physical=0,
        physical=6, pricing=6, pricing_down=6, pricing_up=6)
    assert result['layout_verified'] is True
    assert result['financial_replay'] is False
    assert result['complete_resource_proof'] is False
    assert result['publication_eligible'] is False


@pytest.mark.parametrize('field,value', [
    ('scope', 'original_physical'), ('side', 'revised'), ('unit_id', 'foreign'),
    ('unit_index', True), ('position_index', 1), ('scenario_index', 0),
    ('history_index', 1), ('role', 'physical'), ('schema', 'future-schema'),
])
def test_layout_rejects_original_source_role_order_and_typed_index_changes(control, field, value):
    raw, rows, _, _ = control
    rows[0][field] = value
    data = encode(rows)
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt_for(data, rows))


@pytest.mark.parametrize('target', ['last', 'first', 'reorder', 'footer', 'extra', 'proof_extra',
                                  'missing_projection', 'source', 'weight', 'path', 'run'])
def test_layout_requires_complete_raw_sources_and_full_records(control, target):
    raw, rows, _, _ = control
    if target == 'last':
        del rows[-2]
    elif target == 'first':
        del rows[0]
    elif target == 'reorder':
        rows[1], rows[2] = rows[2], rows[1]
    elif target == 'footer':
        rows[-1]['publication_eligible'] = True
    elif target == 'extra':
        rows[0]['cut_admission_granted'] = True
    elif target == 'proof_extra':
        rows[0]['proof']['caller_money_verified'] = True
    elif target == 'missing_projection':
        del rows[0]['proof']['projection']
    elif target == 'source':
        rows[-2]['proof']['source_request']['source']['acquisition']['cost'] += 1.
    elif target == 'weight':
        rows[0]['proof']['weight'] = 0.
    elif target == 'path':
        rows[0]['proof']['path_key'] += 1
    else:
        rows[0]['proof']['run_id'] += '-other'
    data = encode(rows)
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt_for(data, rows))


@pytest.mark.parametrize('key', [
    'retains_typed_original_economic_owners', 'original_quote_or_anchor_refitted',
    'cut_sim_executed', 'cut_admission_granted', 'complete_source_sim_codec_bounds',
    'independent_persisted_verification', 'publication_eligible',
])
def test_capture_cannot_promote_component_receipt_to_publication(control, key):
    raw, _, data, receipt = control
    receipt[key] = True
    with pytest.raises(ArithmeticError, match='eligibility'):
        reference.validate_layout(raw, data, receipt)


@pytest.mark.parametrize('key,value', [
    ('capture_work', 0), ('source_records', 26), ('encoded_bytes', 1),
    ('raw_prefix_work_upper', 7999), ('admitted_work_upper', 1),
    ('typed_request_bytes', True), ('reported_retained_bytes_upper', 129 << 20),
    ('source_preparation_work_upper', -1), ('typed_request_bytes', 3 << 20),
    ('admitted_work_upper', 1 << 64),
])
def test_exact_counter_types_formulas_and_component_caps(control, key, value):
    raw, _, data, receipt = control
    receipt[key] = value
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt)


@pytest.mark.parametrize('data', [
    b'{"schema":"x","schema":"y"}\n{}\n',
    b'{"n":NaN}\n{}\n', b'{"n":Infinity}\n{}\n', b'{"n":1e999}\n{}\n',
    b'{"n":18446744073709551616}\n{}\n', b'{}\n\n', b'{}\n{}', b'\xff\n{}\n',
    b'[]\n{}\n', b'\xef\xbb\xbf{}\n{}\n',
    b'{"n":"\\ud800"}\n{}\n', b'{"\\ud800":0}\n{}\n',
    b'[' * 129 + b'0' + b']' * 129 + b'\n{}\n', b'x' * ((4 << 20) + 1),
], ids=['duplicate-key', 'nan', 'infinity', 'float-overflow', 'uint-overflow',
        'empty-line', 'missing-newline', 'invalid-utf8', 'nonobject', 'utf8-bom',
        'surrogate-value', 'surrogate-key', 'depth-limit', 'byte-cap'])
def test_jsonl_rejects_duplicates_nonfinite_depth_truncation_and_capacity(control, data):
    raw, _, _, receipt = control
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt)


@pytest.mark.parametrize('name', ['exercise', 'generated_policy', 'original_physical'])
def test_capture_rejects_unsupported_proof_families(control, name):
    raw, _, data, receipt = control
    position = raw['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]
    position[name] = [deepcopy(position['original_pricing'][0])] if name == 'original_physical' else {'unsupported': True}
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt)


def test_zero_weight_paths_are_covered_and_do_not_grant_raw_source_replacement(control):
    raw, rows, _, _ = control
    for side in ('completed', 'revised'):
        for unit in raw[side + '_candidate']['commercial']['units']:
            for position in unit['graph']['positions']:
                for history in position['original_pricing']:
                    history['weight'] = 0.
                for scenario in position['histories']:
                    for role in ('physical', 'pricing', 'pricing_down', 'pricing_up'):
                        for history in scenario[role]:
                            history['weight'] = 0.
    for row in rows[:-1]:
        row['proof']['weight'] = 0.
    data = encode(rows)
    assert reference.validate_layout(raw, data, receipt_for(data, rows))['source_records'] == 27
    rows[-2]['proof']['source_request']['credit']['paths'][0]['path_key'] += 1
    data = encode(rows)
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt_for(data, rows))


def test_original_scope_count_types_and_final_completion_are_exact(control):
    raw, rows, _, _ = control
    rows[-1]['counts']['original_pricing'] = 3.
    data = encode(rows)
    receipt = receipt_for(data, rows)
    with pytest.raises(ArithmeticError):
        reference.validate_layout(raw, data, receipt)


def test_cooperative_cancellation_does_not_validate_a_partial_capture(control):
    raw, _, data, receipt = control
    def cancelled():
        raise TimeoutError('cancelled verification')
    with pytest.raises(TimeoutError):
        reference.validate_layout(raw, data, receipt, cancelled)
