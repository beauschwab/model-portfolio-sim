"""Independent complete fee proofs and raw admission; no native callbacks."""
from copy import deepcopy
import gzip
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import commercial_fixed_capacity_reference as reference
from portfolio_risk.analytics.commercial_cut_stream_transport import decode
from portfolio_risk.core import model_native, product_strategy_native, quant_native

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture
def inputs(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('native callback during independent fee verification')
    monkeypatch.setattr(model_native, 'run', forbidden)
    monkeypatch.setattr(product_strategy_native, '_run', forbidden)
    monkeypatch.setattr(quant_native, 'call', forbidden)
    monkeypatch.setattr(quant_native, 'term_call', forbidden)
    raw = json.loads((ROOT / 'examples/models/commercial-candidate-holding-journal-1.json').read_text(encoding='utf8'))
    raw['schema'] = 'commercial-cut-mixed-journal-1'
    raw['revised_candidate'] = deepcopy(raw['completed_candidate'])
    packet = json.loads(gzip.decompress((Path(__file__).with_name('fixtures') /
        'commercial-fixed-fee-packet-1.json.gz').read_bytes()))
    return raw, packet


def actual(raw, packet):
    tables, completion = decode(packet['cut_stream'])
    return dict(schema=raw['schema'], transport='fixed-sequential-cut-1',
        raw_source=raw, native_packet=packet, cut_tables=tables, producer_completion=completion)


def test_complete_ordinary_fee_packet_replays_all_eighteen_sources(inputs):
    raw, packet = inputs
    receipt = reference.verify(raw, actual(raw, packet))
    assert receipt['complete_source_proofs'] == 18
    assert receipt['raw_resource_allowance_recomputed'] is True
    assert receipt['gross_closing_replayed'] is True
    assert receipt['prior_publication_verified'] is False
    assert receipt['empirical_validation'] is False


@pytest.mark.parametrize('day', [31, 43])
def test_advanced_fee_prefix_uses_complete_horizon_between_sparse_reports(inputs, day):
    from portfolio_risk.analytics.commercial_cut_prior_reference import prefix
    raw, packet = inputs
    result = actual(raw, packet)
    original = deepcopy(result)
    assert day not in {r['day'] for r in result['cut_tables']['path']}
    retained = prefix(result, raw['completed_candidate']['as_of_ordinal'] + day)
    assert retained['cut_day'] == day
    assert {r['day'] for r in retained['tables']['commercial_fee_daily']} == {day}
    assert 'path' not in retained['tables']
    assert result == original


@pytest.mark.parametrize('day', [45, 46])
def test_advanced_fee_prefix_still_rejects_at_or_beyond_complete_tail(inputs, day):
    from portfolio_risk.analytics.commercial_cut_prior_reference import prefix
    raw, packet = inputs
    with pytest.raises(ValueError, match='complete published tail'):
        prefix(actual(raw, packet), raw['completed_candidate']['as_of_ordinal'] + day)


@pytest.mark.parametrize('target', ['work', 'bytes', 'raw_fee', 'fee_proof', 'cash',
    'original_proof', 'current_proof', 'current_fee_proof', 'persisted_rights'])
def test_forged_fee_bounds_sources_and_persisted_rights_fail(inputs, target):
    raw, packet = inputs
    result = actual(raw, packet)
    original = packet['completed_phase']['original_sources'][0]['proof']
    if target == 'work':
        packet['resource_admission']['completed_preparation_work_upper'] -= 1
    elif target == 'bytes':
        packet['resource_admission']['source_phase_bytes_upper'] -= 1
    elif target == 'raw_fee':
        original['fee_projection']['raw_fee_history']['policy']['fees'][0]['fee_id'] += '-forged'
    elif target == 'fee_proof':
        original['fee_projection'] = None
    elif target == 'cash':
        original['fee_projection']['cash'].clear()
    elif target == 'original_proof':
        packet['completed_phase']['original_sources'].clear()
    elif target == 'current_fee_proof':
        packet['revised_phase']['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources'][0]['fee_projection'] = None
    elif target == 'current_proof':
        packet['revised_phase']['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources'].pop()
    else:
        result['cut_tables']['commercial_fee_daily'][0]['native_fee_snapshot']['fee_receivable'] += 1
    with pytest.raises((ArithmeticError, ValueError, KeyError, TypeError)):
        reference.verify(raw, result)


def test_fee_raw_allowance_cannot_drop_an_unpaired_source(inputs):
    raw, _ = inputs
    p = raw['completed_candidate']['commercial']['units'][0]['graph']['positions'][0]
    p['fees']['histories'][0]['pricing_up'].clear()
    with pytest.raises(ArithmeticError, match='coverage'):
        reference.candidate(raw['completed_candidate'])
