"""Independent replay of a frozen full-request native component witness.

The compressed fixture contains all27 source proofs, both complete preparations
and every visible table. It is implementation evidence, not observed loan data
or a resource/publication permit. The native companion regenerates the same full
request in sequential scopes and compares every row with the historical owner.
"""
from copy import deepcopy
import gzip
import hashlib
from pathlib import Path

import pytest

from portfolio_risk.analytics import commercial_sequential_cut_reference as oracle
from portfolio_risk.analytics.commercial_cut_capture_reference import strict_loads
from portfolio_risk.core import model_native, product_strategy_native, quant_native


FIXTURE = Path(__file__).with_name('fixtures') / 'commercial-sequential-cut-witness-2.json.gz'


@pytest.fixture(scope='module')
def full_witness():
    data = gzip.decompress(FIXTURE.read_bytes())
    return strict_loads(data)


@pytest.fixture(autouse=True)
def forbid_native_finance(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('native financial callback in independent sequential replay')
    monkeypatch.setattr(model_native, 'run', forbidden)
    monkeypatch.setattr(product_strategy_native, '_run', forbidden)
    monkeypatch.setattr(quant_native, 'call', forbidden)
    monkeypatch.setattr(quant_native, 'term_call', forbidden)


def test_full_original_request_all_sources_and_rows_pass_independent_replay(full_witness):
    root = Path(__file__).resolve().parents[3]
    source = root / 'examples/models/commercial-cut-mixed-journal-1.json'
    assert hashlib.sha256(source.read_bytes()).hexdigest() == '0280c5084e5cd52267d2c0f5fb07cbeed10f22a02e00243267997008991c38ce'
    raw = full_witness['raw_request']
    assert len(raw['completed_candidate']['commercial']['units']) == 1
    assert len(raw['revised_candidate']['commercial']['units']) == 2
    assert oracle.verify(raw, full_witness) == dict(financial_replay=True,
        source_records=27, physical_sources_verified=6, pricing_sources_verified=21,
        complete_resource_proof=False, independent_persisted_verification=False,
        publication_eligible=False)


@pytest.mark.parametrize('family', ['completion', 'receipt', 'resource', 'publication'])
def test_forged_authority_is_rejected_before_financial_projection(full_witness, monkeypatch, family):
    changed = deepcopy(full_witness)
    if family == 'completion':
        changed['producer_completion']['prior_publication_verified'] = True
    elif family == 'receipt':
        changed['component_receipt']['raw_coordinator_admission_granted'] = True
    elif family == 'resource':
        changed['joined_raw_admission_proved'] = True
    else:
        changed['publication_eligible'] = True
    def forbidden(*args, **kwargs):
        raise AssertionError('financial projection began before authority validation')
    monkeypatch.setattr(oracle, 'verify_preparation', forbidden)
    with pytest.raises(ArithmeticError):
        oracle.verify(changed['raw_request'], changed)


@pytest.mark.parametrize('mutation', ['missing-source', 'duplicate-source', 'reordered-source',
    'source-money', 'source-path', 'raw-history', 'graph-money', 'cut-journal',
    'cut-lineage', 'cut-closing', 'table-injection', 'integer-completion',
    'extra-completion', 'counter-type', 'unknown-census', 'extra-field'])
def test_complete_source_proofs_metadata_and_persistable_rows_cannot_be_tampered(full_witness, mutation):
    changed = deepcopy(full_witness)
    if mutation == 'missing-source':
        changed['source_records'].pop()
    elif mutation == 'duplicate-source':
        changed['source_records'][-1] = deepcopy(changed['source_records'][0])
    elif mutation == 'reordered-source':
        changed['source_records'][0], changed['source_records'][1] = changed['source_records'][1], changed['source_records'][0]
    elif mutation == 'source-money':
        changed['source_records'][0]['proof']['cash'][0]['principal'] += 1.
    elif mutation == 'source-path':
        changed['source_records'][0]['proof']['path_key'] += 1
    elif mutation == 'raw-history':
        changed['source_records'][0]['proof']['source_request']['credit']['paths'][0]['path_key'] += 1
    elif mutation == 'graph-money':
        changed['revised_preparation']['native_preparation']['source']['values'][0] += 1.
    elif mutation == 'cut-journal':
        changed['cut_tables']['journal'][0]['value'] += 1.
    elif mutation == 'cut-lineage':
        changed['cut_tables']['commercial_source_lineage'][0]['native_credit_year'] += .01
    elif mutation == 'cut-closing':
        changed['cut_tables']['closing_statements'][0]['assets'] += 1.
    elif mutation == 'table-injection':
        changed['cut_tables']['unreviewed_money'] = [{'cash': 1.}]
    elif mutation == 'integer-completion':
        changed['producer_completion']['complete'] = 1
    elif mutation == 'extra-completion':
        changed['producer_completion']['reviewed'] = True
    elif mutation == 'counter-type':
        changed['component_receipt']['runs_moved'] = 2.
    elif mutation == 'unknown-census':
        changed['construction_census'] = 'caller-work-discount'
    else:
        changed['complete_resource_proof'] = True
    with pytest.raises(ArithmeticError):
        oracle.verify(changed['raw_request'], changed)


def test_historical_component_schema_still_reads_without_gaining_authority(full_witness):
    changed = deepcopy(full_witness)
    changed['schema'] = 'commercial-sequential-cut-witness-1'
    changed.pop('construction_census')
    assert oracle.verify(changed['raw_request'], changed)['publication_eligible'] is False


def test_original_verification_cancellation_aborts_without_acceptance(full_witness):
    def cancelled():
        raise TimeoutError('cancelled independent sequential replay')
    with pytest.raises(TimeoutError):
        oracle.verify(full_witness['raw_request'], full_witness, cancelled)


@pytest.mark.parametrize('value', [None, [], 1, True, 'witness'])
def test_nonobject_witness_refuses_with_validation_error(value):
    with pytest.raises(ArithmeticError, match='object required'):
        oracle.verify({}, value)
