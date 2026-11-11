"""Lossless direct packet identity and independent full financial controls."""
from copy import deepcopy
import gzip
import json
from pathlib import Path
import pytest

from portfolio_risk.analytics import commercial_sequential_packet_reference as reference
from portfolio_risk.analytics.commercial_cut_capture_reference import strict_loads
from portfolio_risk.analytics.commercial_dto import graph_typed
from portfolio_risk.core import model_native, product_strategy_native, quant_native

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture(scope='module')
def raw():
    return json.loads((ROOT / 'examples/models/commercial-cut-mixed-journal-1.json').read_text())


@pytest.fixture(scope='module')
def packet():
    path = Path(__file__).with_name('fixtures') / 'commercial-sequential-packet-witness-1.json.gz'
    return strict_loads(gzip.decompress(path.read_bytes()))


@pytest.fixture(autouse=True)
def forbid_native_finance(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('native callback in independent direct-packet verification')
    monkeypatch.setattr(model_native, 'run', forbidden)
    monkeypatch.setattr(product_strategy_native, '_run', forbidden)
    monkeypatch.setattr(quant_native, 'call', forbidden)
    monkeypatch.setattr(quant_native, 'term_call', forbidden)


def test_original_request_is_exactly_reconstructed_without_losing_any_of27proofs(raw, packet):
    parts = reference.financial_parts(raw, packet)
    assert parts['raw_request'] == graph_typed(raw)
    assert len(parts['source_records']) == 27
    assert sum(r['scope'] != 'current' for r in parts['source_records']) == 3
    assert sum(r['role'] == 'physical' for r in parts['source_records']) == 6
    assert parts['source_records'][0] is packet['completed_phase']['original_sources'][0]
    assert parts['source_records'][1]['proof'] is packet['completed_phase']['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources'][0]
    assert reference.verify(raw, packet) == dict(financial_replay=True, source_records=27,
        physical_sources_verified=6, pricing_sources_verified=21, complete_resource_proof=False,
        independent_persisted_verification=False, publication_eligible=False)


@pytest.mark.parametrize('mutation', ['header-date', 'header-type', 'candidate-identity',
    'missing-original', 'extra-original', 'original-order', 'missing-current',
    'extra-current', 'current-order', 'source-money', 'journal-money', 'lineage-clock',
    'completion-authority', 'resource-authority', 'publication-authority', 'unknown-field',
    'unknown-phase-field', 'unknown-schema'])
def test_no_unverified_reference_amount_source_or_authority_can_enter_the_packet(raw, packet, mutation):
    changed = deepcopy(packet)
    phase = changed['revised_phase']
    sources = phase['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources']
    if mutation == 'header-date':
        changed['raw_request_header']['cut_ordinal'] += 1
    elif mutation == 'header-type':
        changed['raw_request_header']['cut_ordinal'] = float(changed['raw_request_header']['cut_ordinal'])
    elif mutation == 'candidate-identity':
        phase['preparation']['raw_request']['allocation'][0]['face'] += 1.
    elif mutation == 'missing-original':
        phase['original_sources'].pop()
    elif mutation == 'extra-original':
        phase['original_sources'].append(deepcopy(phase['original_sources'][0]))
    elif mutation == 'original-order':
        phase['original_sources'].reverse()
    elif mutation == 'missing-current':
        sources.pop()
    elif mutation == 'extra-current':
        sources.append(deepcopy(sources[0]))
    elif mutation == 'current-order':
        sources[0], sources[1] = sources[1], sources[0]
    elif mutation == 'source-money':
        sources[0]['cash'][0]['principal'] += 1.
    elif mutation == 'journal-money':
        changed['cut_tables']['journal'][0]['value'] += 1.
    elif mutation == 'lineage-clock':
        changed['cut_tables']['commercial_source_lineage'][0]['native_credit_year'] += .01
    elif mutation == 'completion-authority':
        changed['producer_completion']['prior_publication_verified'] = True
    elif mutation == 'resource-authority':
        changed['joined_raw_admission_proved'] = True
    elif mutation == 'publication-authority':
        changed['publication_eligible'] = True
    elif mutation == 'unknown-field':
        changed['native_admission_granted'] = True
    elif mutation == 'unknown-phase-field':
        phase['prepared_money_override'] = 0.
    else:
        changed['schema'] = 'caller-packet-policy-1'
    with pytest.raises(ArithmeticError):
        reference.verify(raw, changed)


def test_cancelled_packet_verification_cannot_return_financial_acceptance(raw, packet):
    def cancelled():
        raise TimeoutError('cancelled direct packet verification')
    with pytest.raises(TimeoutError):
        reference.verify(raw, packet, cancelled)


def test_forged_packet_authority_refuses_before_any_financial_projection(raw, packet, monkeypatch):
    changed = deepcopy(packet)
    changed['joined_raw_admission_proved'] = True
    def forbidden(*args, **kwargs):
        raise AssertionError('financial projection before packet authority check')
    monkeypatch.setattr(reference, 'verify_parts', forbidden)
    with pytest.raises(ArithmeticError):
        reference.verify(raw, changed)
