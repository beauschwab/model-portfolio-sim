"""Bounded cleanup for private native-process transport files."""
from contextlib import contextmanager
import tempfile
import time


@contextmanager
def scratch_directory(*, prefix):
    """Remove owned scratch files after callers close and reap their child.

    Windows may briefly retain an inherited file lock after process termination.
    Retry only permission failures, for at most 0.56 seconds; never silently
    discard an unrecoverable cleanup failure or alter a calculation deadline.
    """
    directory = tempfile.TemporaryDirectory(prefix=prefix)
    try:
        yield directory.name
    finally:
        for attempt in range(8):
            try:
                directory.cleanup()
                break
            except PermissionError:
                if attempt == 7:
                    raise
                time.sleep(.02 * (attempt + 1))
