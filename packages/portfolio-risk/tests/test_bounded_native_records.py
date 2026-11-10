"""Selected stream framing refuses incomplete or unbounded producer output."""
import io
import json

import polars as pl
import pytest

from portfolio_risk.analytics import balance_stream
from portfolio_risk.analytics.bounded_native_records import loads


class Process:
    def __init__(self, output):
        self.stdout = io.BytesIO(output)
    def poll(self): return 0
    def wait(self, timeout=None): return 0
    def kill(self): pass


def run_stream(monkeypatch, tmp_path, records, *, limit=1024, selected=True):
    binary = tmp_path/'native.exe'
    binary.touch()
    encoded = b''.join((json.dumps(row, separators=(',', ':')).encode()+b'\n')
                       if isinstance(row, dict) else row for row in records)
    monkeypatch.setattr(balance_stream.subprocess, 'Popen', lambda *a, **k: Process(encoded))
    writer = balance_stream.PartitionWriter(tmp_path, 2, schemas={'sample': {'id': pl.Int64}})
    header, completion, trailer = {}, {}, {}
    balance_stream._rust({'request': {'schema': 'selected'}}, writer, None, None, 1.,
        executable=binary, workflow=header, completion_protocol=6,
        completion=completion, completion_trailer=trailer if selected else None,
        decode_record=loads, record_bytes=limit, retain_input=True)
    writer.finish()
    return header, completion, trailer, writer


HEADER = {'workflow': {'schema': 'prepared'}}
TABLE = {'table': 'sample', 'rows': [{'id': 1}]}
COMPLETE = {'complete': True, 'protocol': 6}
TRAILER = {'record': 'selected_complete', 'complete': True, 'schema': 'selected'}


def test_selected_trailer_after_real_completion(monkeypatch, tmp_path):
    header, completion, trailer, writer = run_stream(monkeypatch, tmp_path,
        [HEADER, TABLE, COMPLETE, TRAILER])
    assert header == HEADER['workflow'] and completion == COMPLETE and trailer == TRAILER
    assert sum(frame.height for frame in writer.frames('sample')) == 1
    assert (tmp_path/'dated-input.json').is_file()


@pytest.mark.parametrize('records', [
    [HEADER, TABLE, COMPLETE],
    [HEADER, TABLE, TRAILER, COMPLETE],
    [HEADER, TABLE, COMPLETE, TRAILER, TRAILER],
    [HEADER, TABLE, COMPLETE, TABLE],
    [HEADER, TABLE, COMPLETE, COMPLETE],
    [HEADER, TABLE, COMPLETE, {'record': 'x'}, TABLE],
    [TABLE, COMPLETE, TRAILER],
    [HEADER, TABLE, b'{"complete":true,"protocol":6'],
    [HEADER, b'{"table":"sample","table":"sample","rows":[]}\n', COMPLETE, TRAILER],
    [HEADER, b'{"table":"sample","rows":[{"id":NaN}]}\n', COMPLETE, TRAILER],
])
def test_selected_framing_rejects_incomplete_or_corrupt(monkeypatch, tmp_path, records):
    with pytest.raises((ValueError, RuntimeError)):
        run_stream(monkeypatch, tmp_path, records)
    assert not (tmp_path/'dated-input.json').exists()


def test_bounded_record_read(monkeypatch, tmp_path):
    with pytest.raises(ValueError, match='byte bound'):
        run_stream(monkeypatch, tmp_path, [HEADER, TABLE, COMPLETE, TRAILER], limit=32)


def test_historical_completion_rejects_trailer(monkeypatch, tmp_path):
    with pytest.raises(ValueError, match='follows completion'):
        run_stream(monkeypatch, tmp_path, [HEADER, TABLE, COMPLETE, TRAILER], selected=False)


@pytest.mark.parametrize('data', ['[]', '{"x":1,"x":2}', '{"x":Infinity}', '{"x":NaN}'])
def test_strict_records(data):
    with pytest.raises(ValueError): loads(data)
