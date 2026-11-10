"""A failed stream can never become a valid result or hide its native rejection."""
import pytest
from portfolio_risk.core import product_strategy_native as native


@pytest.mark.parametrize('stdout,stderr,code,message', [
    (b'{"ok":{"partial":', b'{"error":"sequential cut component exceeds8M/32MiB"}', 2, 'exceeds8M/32MiB'),
    (b'{"error":"historical refusal"}', b'', 2, 'historical refusal'),
    (b'{"ok":{"must_not_publish":true}}', b'', 2, 'during streamed output'),
    (b'{"ok":{"partial":', b'not a structured rejection', 2, 'during streamed output'),
])
def test_failed_native_stream_rejects_before_success_decode(monkeypatch, stdout, stderr, code, message):
    class Process:
        returncode = code
        def __init__(self, _args, **files):
            files['stdout'].write(stdout); files['stdout'].flush()
            files['stderr'].write(stderr); files['stderr'].flush()
        def poll(self): return self.returncode
        def wait(self, **_kwargs): return self.returncode
    monkeypatch.setattr(native.subprocess, 'Popen', Process)
    monkeypatch.setattr(native, 'binary_identity', lambda: 'pinned')
    with pytest.raises(ValueError, match=message):
        native._run({}, schema='commercial-cut-mixed-journal-1', command={}, timeout=10.)
