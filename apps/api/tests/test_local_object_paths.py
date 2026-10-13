"""Real long-path immutable object operations, including publication and reads."""
import hashlib
import os

import polars as pl
import pytest

from app.artifacts import Codec, Objects


def test_long_local_object_paths_preserve_publication_and_identity(tmp_path, monkeypatch):
    # Each directory component fits normal filesystem limits; the final keyed
    # object exceeds Win32 MAX_PATH. No registry changes or shorter path fallback.
    root = tmp_path / ('nested-' + 'a' * 120) / ('nested-' + 'b' * 90)
    objects = Objects(str(root), 'long-path-tenant', 'long-path-workspace')
    data = b'complete immutable artifact\n'
    ref = objects.put(data, 'bin')
    assert len(str(objects.root / ref['key'])) > 260
    expected_prefix = 'workbench/' + hashlib.sha256(
        b'long-path-tenant\0long-path-workspace').hexdigest()
    assert objects.prefix == expected_prefix
    assert ref == dict(key=f'{expected_prefix}/{ref["sha256"][:2]}/{ref["sha256"]}.bin',
                       sha256=hashlib.sha256(data).hexdigest(), bytes=len(data), format='bin')
    assert objects.get(ref) == objects.get_bounded(ref, len(data)) == data
    # A previously published object is reused without opening a new temporary.
    with monkeypatch.context() as patch:
        patch.setattr('app.artifacts.tempfile.mkstemp',
                      lambda **kwargs: pytest.fail('existing immutable object rewritten'))
        assert objects.put(data, 'bin') == ref
    assert list(objects.root.rglob('tmp*')) == []
    with pytest.raises(ValueError, match='workspace|identity'):
        Objects(str(root), 'different-tenant', 'long-path-workspace').get(ref)
    with pytest.raises(ValueError, match='descriptor'):
        objects.get_bounded(ref, len(data) - 1)
    (objects.root / ref['key']).write_bytes(b'x' * len(data))
    with pytest.raises(ValueError, match='checksum'):
        objects.get(ref)
    with pytest.raises(ValueError, match='checksum'):
        objects.get_bounded(ref, len(data))
    if os.name == 'nt':
        assert str(objects.root).startswith('\\\\?\\')
        assert Objects(str(objects.root), 'same', 'root').root == objects.root


def test_long_local_codec_parquet_and_bounded_object_roundtrip(tmp_path):
    root = tmp_path / ('parquet-' + 'a' * 115) / ('parquet-' + 'b' * 95)
    objects = Objects(str(root), 'tenant', 'codec')
    codec = Codec(objects)
    frame = pl.DataFrame({'principal': [1.0, 2.0], 'origin': ['first', 'second']})
    ref = codec.dump({'reviewed': True, 'flows': frame})
    restored = codec.load(ref)
    assert restored['reviewed'] is True
    assert restored['flows'].equals(frame)
    tables = codec.tables(ref)
    assert set(tables) == {'/flows'}
    table = tables['/flows']
    data = objects.get_bounded(table, table['bytes'])
    assert data[:4] == data[-4:] == b'PAR1'
    assert len(str(objects.root / table['key'])) > 260
    assert list(objects.root.rglob('tmp*')) == []
