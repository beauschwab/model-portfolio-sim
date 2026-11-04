"""Columnar transport controls; every financial expectation is independently replayed."""
from copy import deepcopy
import gzip
import json
from pathlib import Path
import struct

import pytest

from portfolio_risk.analytics import commercial_cut_stream_transport as stream
from portfolio_risk.analytics import commercial_sequential_packet_reference as reference
from portfolio_risk.analytics.commercial_cut_capture_reference import strict_loads
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
from portfolio_risk.core import model_native, product_strategy_native, quant_native

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture(scope='module')
def packet():
    path = Path(__file__).with_name('fixtures') / 'commercial-sequential-packet-witness-2.json.gz'
    return strict_loads(gzip.decompress(path.read_bytes()))


@pytest.fixture(autouse=True)
def forbid_native_finance(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError('native callback in independent stream verification')
    monkeypatch.setattr(model_native, 'run', forbidden)
    monkeypatch.setattr(product_strategy_native, '_run', forbidden)
    monkeypatch.setattr(quant_native, 'call', forbidden)
    monkeypatch.setattr(quant_native, 'term_call', forbidden)


def frame(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')) + '\n'


def journal(labels=None):
    return dict(table='journal', dictionary=['stress雪', 'a', 'e', 'g', 'p'] if labels is None else labels,
                columns=dict(scenario=[0], account=[1], event=[2], gl_account=[3], instrument_id=[4],
                             day=[2], transaction_number=[9], value=[-0.0]))


COMPLETE = dict(complete=True, protocol=9)


def test_full_original_stream_replays_all27sources_and_every_historical_table(packet):
    raw = json.loads((ROOT / 'examples/models/commercial-cut-mixed-journal-1.json').read_text())
    old = strict_loads(gzip.decompress((Path(__file__).with_name('fixtures') /
                        'commercial-sequential-packet-witness-1.json.gz').read_bytes()))
    tables, completion = stream.decode(packet['cut_stream'])
    assert exact_json_identity(tables, old['cut_tables'])
    assert exact_json_identity(completion, old['producer_completion'])
    assert reference.verify(raw, packet) == dict(financial_replay=True, source_records=27,
        physical_sources_verified=6, pricing_sources_verified=21, complete_resource_proof=False,
        independent_persisted_verification=False, publication_eligible=False)


def test_dictionary_resends_rows_numbers_and_unicode_are_preserved():
    a, b = journal(), journal([])
    b['columns']['value'] = [3.25]
    wire = frame(a) + frame(dict(table='path', rows=[dict(name='雪😀', number=-0.0, integer=0)]))
    wire += frame(b) + frame(COMPLETE)
    tables, completion = stream.decode(wire)
    assert completion == COMPLETE
    first, second = tables['journal']
    assert first['scenario'] == 'stress雪'
    assert first['scenario'] is second['scenario']
    assert struct.pack('>d', first['value']) == struct.pack('>d', -0.0)
    assert second['value'] == 3.25
    assert type(tables['path'][0]['integer']) is int
    assert type(tables['path'][0]['number']) is float


@pytest.mark.parametrize('change', ['duplicate-key', 'truncated', 'empty-frame', 'extra-completion',
    'post-completion', 'no-completion', 'protocol-float', 'wrong-protocol', 'nonfinite', 'integer-amount',
    'negative-index', 'bool-index', 'float-index', 'missing-label', 'duplicate-label', 'bad-label',
    'shape', 'extra-column', 'missing-column', 'unknown-table', 'extra-frame-field', 'scalar-row',
    'scalar-frame', 'missing-newline', 'surrogate'])
def test_malformed_or_ambiguous_streams_cannot_reach_financial_replay(change):
    a, end = journal(), deepcopy(COMPLETE)
    suffix = ''
    if change == 'duplicate-key':
        wire = '{"complete":true,"protocol":9,"protocol":9}\n'
    elif change == 'truncated':
        wire = frame(a) + '{"complete":\n'
    elif change == 'empty-frame':
        wire = frame(a) + '\n' + frame(end)
    elif change == 'extra-completion':
        wire = frame(a) + frame(end) + frame(end)
    elif change == 'post-completion':
        wire = frame(end) + frame(a)
    elif change == 'no-completion':
        wire = frame(a)
    elif change == 'scalar-frame':
        wire = '[]\n'
    elif change == 'missing-newline':
        wire = frame(end).rstrip('\n')
    elif change == 'surrogate':
        wire = '\ud800\n'
    else:
        if change == 'protocol-float': end['protocol'] = 9.0
        elif change == 'wrong-protocol': end['protocol'] = 8
        elif change == 'nonfinite': a['columns']['value'] = [float('nan')]
        elif change == 'integer-amount': a['columns']['value'] = [0]
        elif change == 'negative-index': a['columns']['account'] = [-1]
        elif change == 'bool-index': a['columns']['account'] = [True]
        elif change == 'float-index': a['columns']['account'] = [1.0]
        elif change == 'missing-label': a['dictionary'].pop()
        elif change == 'duplicate-label': a['dictionary'].append('a')
        elif change == 'bad-label': a['dictionary'][0] = 4
        elif change == 'shape': a['columns']['value'].append(1.0)
        elif change == 'extra-column': a['columns']['override'] = [0]
        elif change == 'missing-column': a['columns'].pop('value')
        elif change == 'unknown-table': a['table'] = 'override'
        elif change == 'extra-frame-field': a['override'] = 1
        elif change == 'scalar-row': a = dict(table='path', rows=[0])
        wire = frame(a) + frame(end) + suffix
    with pytest.raises(ArithmeticError):
        stream.decode(wire)


def test_stream_byte_limit_counts_utf8_before_any_parse(monkeypatch):
    def forbidden(*args):
        raise AssertionError('parse before byte admission')
    monkeypatch.setattr(stream, 'strict_loads', forbidden)
    with pytest.raises(ArithmeticError, match='exceeds4MiB'):
        stream.decode('😀' * ((1 << 20) + 1) + '\n')


def test_cancel_during_large_frame_utf8_scan_before_parse(monkeypatch):
    calls = 0
    def check():
        nonlocal calls
        calls += 1
        if calls == 3:
            raise TimeoutError('original deadline')
    def forbidden(*args):
        raise AssertionError('parse after cancellation')
    monkeypatch.setattr(stream, 'strict_loads', forbidden)
    with pytest.raises(TimeoutError, match='original deadline'):
        stream.decode(' ' * 100000 + '\n', check)


def test_stream_packet_rejects_claimed_authority_before_decode(packet, monkeypatch):
    changed = dict(packet, joined_raw_admission_proved=True)
    def forbidden(*args):
        raise AssertionError('decode before authority check')
    monkeypatch.setattr(stream, 'decode', forbidden)
    with pytest.raises(ArithmeticError):
        reference.financial_parts({}, changed)


@pytest.mark.parametrize('mutation', ['journal-amount', 'native-clock', 'completion-authority',
                                    'remove-lineage', 'repeat-journal'])
def test_validly_encoded_stream_tampering_fails_independent_financial_replay(packet, mutation):
    raw = json.loads((ROOT / 'examples/models/commercial-cut-mixed-journal-1.json').read_text())
    changed = dict(packet)
    frames = [strict_loads(line) for line in packet['cut_stream'].splitlines()]
    if mutation == 'journal-amount':
        next(r for r in frames if r.get('table') == 'journal')['columns']['value'][0] += 1.0
    elif mutation == 'native-clock':
        next(r for r in frames if r.get('table') == 'commercial_source_lineage')['rows'][0]['native_credit_year'] += .01
    elif mutation == 'completion-authority':
        frames[-1]['prior_publication_verified'] = True
    elif mutation == 'remove-lineage':
        next(r for r in frames if r.get('table') == 'commercial_source_lineage')['rows'].pop()
    else:
        repeat = deepcopy(next(r for r in frames if r.get('table') == 'journal'))
        repeat['dictionary'] = []
        frames.insert(-1, repeat)
    changed['cut_stream'] = ''.join(map(frame, frames))
    with pytest.raises(ArithmeticError):
        reference.verify(raw, changed)
