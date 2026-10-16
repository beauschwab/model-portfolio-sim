"""Real generated P paths, persisted partitions and independent journal controls."""
from copy import deepcopy
import json
import os
from pathlib import Path
import polars as pl
import pytest

from portfolio_risk.analytics.balance_stream import _rust
from portfolio_risk.analytics.bounded_native_records import loads
from portfolio_risk.analytics.commercial_generated_candidate_reference import verify_preparation, verify_stream
from portfolio_risk.analytics.commercial_generated_transport import GeneratedWriter, VerificationFrames
from portfolio_risk.analytics.model_ledger import binary_path

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture(scope='module')
def generated(tmp_path_factory):
    raw = json.loads((ROOT/'examples/models/commercial-generated-candidate-journal-1.json').read_text())
    return run_generated(raw, tmp_path_factory.mktemp('generated-candidate'))


def run_generated(raw, directory):
    output = GeneratedWriter(directory, 65536, check=lambda: False)
    header, completion, trailer = {}, {}, {}
    executable = Path(os.environ.get('PORTFOLIO_GENERATED_TEST_BINARY', binary_path()))
    _rust(dict(op='analyze_commercial_generated_candidate_journal', schema=raw['schema'], request=raw,
        execution_timeout_ms=120000), output, None, lambda: False, 120., executable=executable,
        workflow=header, completion_protocol=6, completion=completion, completion_trailer=trailer,
        decode_record=loads, record_bytes=32 << 20)
    output.finish()
    output.admit_persisted()
    source = verify_preparation(raw, header['prepared_request'], header['source_receipt'])
    frames = VerificationFrames(output, lambda: completion, lambda: trailer)
    return raw, source, frames


def test_later_outer_calendar_replays_real_persisted_partitions_beyond_old_buffer(tmp_path):
    raw = json.loads((ROOT/'examples/models/commercial-generated-candidate-journal-1.json').read_text())
    # Native counting-sink acceptance proves this exact unchanged financial law
    # emits 33,825,744 NDJSON bytes, above the historical 32 MiB result bound.
    raw['specification']['horizon_days'] = 100
    raw, source, frames = run_generated(raw, tmp_path)
    result = verify_stream(raw, source, frames)
    assert result['verified'] and result['persisted_partitions_replayed']
    assert source['receipt']['physical_paths'] == 256
    assert source['receipt']['full_end_ordinal'] - raw['as_of_ordinal'] == 100
    assert frames._frames.object_count > 1
    # Mature sources remain in gross daily ownership throughout the outer
    # calendar. Independent replay checks every unchanged absolute endpoint.
    assert sum(frame.height for frame in frames.frames('commercial_daily')) == 256 * 101


def test_genuine_generated_sources_replay_ordered_persisted_journal_and_reports(generated):
    raw, source, frames = generated
    result = verify_stream(raw, source, frames)
    assert result['verified'] and result['journal_replayed'] and result['persisted_partitions_replayed']
    assert result['maximum_live_full_physical_owners'] == 1
    assert result['compact_absolute_endpoints_replayed']
    assert source['receipt']['physical_paths'] == 256
    assert not result['opening_identity_verified'] and not result['mixed_ledger_adoption']


class Changed:
    def __init__(self, frames, table, change):
        self.parent, self.table, self.change = frames, table, change
        self.schemas, self.completion, self.trailer = frames.schemas, frames.completion, frames.trailer
    def frames(self, table):
        changed = False
        for frame in self.parent.frames(table):
            if table != self.table or changed:
                yield frame
            else:
                changed = True
                yield self.change(frame)


@pytest.mark.parametrize('target', ['missing_source', 'duplicate_source', 'source_order', 'weight', 'group', 'journal', 'snapshot'])
def test_changed_persisted_source_or_journal_is_rejected_even_without_checksum_authority(generated, target):
    raw, source, frames = generated
    table = 'commercial_candidate_native_sources'
    def changed(frame):
        if target == 'missing_source': return frame.clear()
        if target == 'duplicate_source': return pl.concat([frame, frame])
        rows = frame.to_dicts()
        if target == 'source_order': rows[0]['source_index'] += 1
        elif target == 'weight':
            cell = json.loads(rows[0]['native_json']); cell['weight'] += .01
            rows[0]['native_json'] = json.dumps(cell, separators=(',', ':'))
        elif target == 'group':
            cell = json.loads(rows[0]['native_atomic_group_json']); cell['next']['principal'] += 1.
            rows[0]['native_atomic_group_json'] = json.dumps(cell, separators=(',', ':'))
        elif target == 'journal': rows[0]['debit'] += 1.
        elif target == 'snapshot':
            cell = json.loads(rows[0]['unweighted_book_json']); cell['principal'] += 1.
            rows[0]['unweighted_book_json'] = json.dumps(cell, separators=(',', ':'))
        return pl.DataFrame(rows, schema=frame.schema)
    if target == 'group': table = 'commercial_source_lineage'
    if target == 'journal': table = 'journal'
    if target == 'snapshot': table = 'commercial_daily'
    with pytest.raises((ArithmeticError, ValueError)):
        verify_stream(raw, source, Changed(frames, table, changed))


def test_missing_or_forged_native_trailer_cannot_publish(generated):
    raw, source, frames = generated
    changed = Changed(frames, '', lambda frame: frame)
    changed.trailer = deepcopy(frames.trailer)
    changed.trailer['independent_persisted_verification'] = True
    with pytest.raises(ValueError, match='cannot grant'):
        verify_stream(raw, source, changed)


def test_cancellation_reaches_independent_materialization_and_closes_temporary_store(generated):
    raw, source, frames = generated
    calls = 0
    def check():
        nonlocal calls
        calls += 1
        if calls > 2000:
            raise InterruptedError('cancel independent generated replay')
    with pytest.raises(InterruptedError):
        verify_stream(raw, source, frames, check)
