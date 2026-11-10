"""The process watchdog must include preparation, not restart after hashing."""
import pytest
from contextlib import contextmanager
from pathlib import Path
from portfolio_risk.core import model_native


def forbidden(*args, **kwargs):
    raise AssertionError('expired/cancelled work reached native process creation')


@pytest.mark.parametrize('schema', ['consumer-macro-scenarios-1', 'consumer-macro-products-1', 'consumer-behaviour-spec-1', 'commercial-generated-exercise-1'])
def test_original_deadline_includes_executable_identity_read(monkeypatch, schema):
    now = [0.]
    monkeypatch.setattr(model_native.time, 'monotonic', lambda: now[0])
    def delayed_identity():
        now[0] = .2
        return 'a'*64
    monkeypatch.setattr(model_native, 'binary_identity', delayed_identity)
    monkeypatch.setattr(model_native.subprocess, 'Popen', forbidden)
    with pytest.raises(TimeoutError, match='deadline'):
        model_native.run(schema, {}, timeout=.1)


def test_original_deadline_includes_json_preparation(monkeypatch):
    now = [0.]
    monkeypatch.setattr(model_native.time, 'monotonic', lambda: now[0])
    dumps = model_native.json.dumps
    def delayed_json(*args, **kwargs):
        out = dumps(*args, **kwargs)
        now[0] = .2
        return out
    monkeypatch.setattr(model_native.json, 'dumps', delayed_json)
    monkeypatch.setattr(model_native, 'binary_identity', forbidden)
    monkeypatch.setattr(model_native.subprocess, 'Popen', forbidden)
    with pytest.raises(TimeoutError, match='deadline'):
        model_native.run('consumer-macro-products-1', {}, timeout=.1)


def test_cancelled_entry_does_not_read_executable_or_spawn(monkeypatch):
    monkeypatch.setattr(model_native, 'binary_identity', forbidden)
    monkeypatch.setattr(model_native.subprocess, 'Popen', forbidden)
    with pytest.raises(InterruptedError, match='cancelled'):
        model_native.run('consumer-macro-products-1', {}, cancelled=lambda: True)


def test_original_deadline_includes_input_file_write(monkeypatch, tmp_path):
    now = [0.]
    monkeypatch.setattr(model_native.time, 'monotonic', lambda: now[0])
    monkeypatch.setattr(model_native, 'binary_identity', lambda: 'a'*64)
    monkeypatch.setattr(model_native.subprocess, 'Popen', forbidden)
    @contextmanager
    def scratch(**kwargs):
        yield tmp_path
    monkeypatch.setattr(model_native, 'scratch_directory', scratch)
    original = Path.write_bytes
    def delayed_write(path, data):
        count = original(path, data)
        now[0] = .2
        return count
    monkeypatch.setattr(Path, 'write_bytes', delayed_write)
    with pytest.raises(TimeoutError, match='deadline'):
        model_native.run('consumer-macro-products-1', {}, timeout=.1)
    assert (tmp_path/'input.json').is_file()


@pytest.mark.parametrize('cancel', [False, True])
def test_final_canonical_hash_cannot_return_after_original_control_expires(monkeypatch, tmp_path, cancel):
    now = [0.]
    cancelled = [False]
    monkeypatch.setattr(model_native.time, 'monotonic', lambda: now[0])
    monkeypatch.setattr(model_native, 'binary_identity', lambda: 'a'*64)
    @contextmanager
    def scratch(**kwargs):
        yield tmp_path
    monkeypatch.setattr(model_native, 'scratch_directory', scratch)
    class Completed:
        returncode = 0
        def __init__(self, *args, stdout, **kwargs):
            stdout.write(b'{"ok":true,"result":{}}')
        def poll(self):return 0
        def wait(self, **kwargs):return 0
    monkeypatch.setattr(model_native.subprocess, 'Popen', Completed)
    dumps = model_native.json.dumps
    def delayed_final(value, **kwargs):
        out = dumps(value, **kwargs)
        if 'execution_timeout_ms' not in value:
            now[0] = .2
            cancelled[0] = cancel
        return out
    monkeypatch.setattr(model_native.json, 'dumps', delayed_final)
    with pytest.raises(InterruptedError if cancel else TimeoutError):
        model_native.run('consumer-macro-products-1', {}, timeout=.1, cancelled=lambda: cancelled[0])
