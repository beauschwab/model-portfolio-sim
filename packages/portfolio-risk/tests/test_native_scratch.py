"""Process scratch cleanup must preserve deadline failures and expose leaks."""
from pathlib import Path
import sys
import threading

import pytest

from portfolio_risk.core import native_scratch


def test_cleanup_preserves_execution_failure_and_removes_files():
    with pytest.raises(TimeoutError, match='original execution deadline'):
        with native_scratch.scratch_directory(prefix='portfolio-test-') as scratch:
            path = Path(scratch)
            (path / 'error.txt').write_text('private child output')
            raise TimeoutError('original execution deadline')
    assert not path.exists()


@pytest.mark.skipif(sys.platform != 'win32', reason='Windows inherited-file-lock semantics')
def test_transient_file_lock_does_not_replace_execution_deadline():
    lock = None
    timer = None
    try:
        with pytest.raises(TimeoutError, match='original execution deadline'):
            with native_scratch.scratch_directory(prefix='portfolio-test-') as scratch:
                path = Path(scratch)
                lock = (path / 'error.txt').open('wb')
                timer = threading.Timer(.05, lock.close)
                timer.start()
                raise TimeoutError('original execution deadline')
        assert not path.exists()
    finally:
        if timer is not None:
            timer.join()
        if lock is not None:
            lock.close()


def test_permanent_permission_failure_is_not_silently_ignored(monkeypatch):
    class LockedDirectory:
        name = 'unused-private-directory'
        attempts = 0

        def cleanup(self):
            self.attempts += 1
            raise PermissionError('permanent file lock')

    locked = LockedDirectory()
    monkeypatch.setattr(native_scratch.tempfile, 'TemporaryDirectory', lambda **kwargs: locked)
    delays = []
    monkeypatch.setattr(native_scratch.time, 'sleep', delays.append)
    with pytest.raises(PermissionError, match='permanent file lock'):
        with native_scratch.scratch_directory(prefix='portfolio-test-'):
            pass
    assert locked.attempts == 8
    assert sum(delays) <= .56 + 1e-15


def test_non_permission_cleanup_error_is_not_retried(monkeypatch):
    class BrokenDirectory:
        name = 'unused-private-directory'

        def cleanup(self):
            raise OSError('different cleanup failure')

    monkeypatch.setattr(native_scratch.tempfile, 'TemporaryDirectory', lambda **kwargs: BrokenDirectory())
    monkeypatch.setattr(native_scratch.time, 'sleep', lambda delay: pytest.fail('unrelated failure retried'))
    with pytest.raises(OSError, match='different cleanup failure'):
        with native_scratch.scratch_directory(prefix='portfolio-test-'):
            pass
