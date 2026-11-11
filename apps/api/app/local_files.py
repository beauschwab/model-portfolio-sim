"""Platform filesystem spelling; never changes immutable object keys or contents."""
import os
from pathlib import Path


def local_root(path):
    root = Path(path).resolve()
    # Win32 file/rename operations can still enforce MAX_PATH without this prefix,
    # independently of Python's support for long directory names.
    if os.name == 'nt':
        absolute = str(root)
        if not absolute.startswith('\\\\?\\'):
            absolute = ('\\\\?\\UNC\\' + absolute[2:] if absolute.startswith('\\\\')
                        else '\\\\?\\' + absolute)
        root = Path(absolute)
    return root
