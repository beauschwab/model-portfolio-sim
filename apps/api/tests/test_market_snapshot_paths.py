"""Actual immutable snapshot IO beyond the Win32 path limit, without registry edits."""
import hashlib
import os
from pathlib import Path

import pytest

from app import market_data as md, persistence
from app.local_files import local_root


@pytest.fixture
def long_root(tmp_path, monkeypatch):
    root = tmp_path / ('research-' + 'a' * 110) / ('snapshot-' + 'b' * 90)
    monkeypatch.setenv('MARKET_DATA_DIR', str(root))
    monkeypatch.setattr(persistence, 'REPO', None)
    return root


def test_snapshot_raw_hash_read_list_and_integrity_survive_long_paths(long_root):
    raw = b'complete retained raw observation\n'
    payload = dict(schema_version=md.VERSION, dataset='pmms', as_of='2026-09-25',
                   fetched_at='2026-09-25T12:00:00Z', observation_count=0,
                   warnings=[], curve=None, sources=[], observations=[])
    saved = md.save_snapshot(payload, [raw])
    root = md._root()
    destination = root / 'snapshots' / (saved['id'] + '.json')
    assert len(str(destination)) > 260
    assert md.get_snapshot(saved['id']) == saved
    assert md.list_snapshots()[0]['id'] == saved['id']
    assert (root / 'raw' / (hashlib.sha256(raw).hexdigest() + '.bin')).read_bytes() == raw
    assert not list(root.rglob('*.tmp'))
    assert local_root(str(root)) == root
    if os.name == 'nt':
        assert str(root).startswith('\\\\?\\')
    destination.write_bytes(b'{}')
    with pytest.raises(md.DataError, match='integrity'):
        md.get_snapshot(saved['id'])


def test_failed_long_path_replace_preserves_old_bytes_and_removes_only_temp(long_root, monkeypatch):
    destination = md._root() / 'snapshots' / ('c' * 64 + '.json')
    md._atomic(destination, b'original')
    original_replace = Path.replace
    def refuse_snapshot_replace(path, target):
        if target == destination:
            raise OSError('injected replace refusal')
        return original_replace(path, target)
    monkeypatch.setattr(Path, 'replace', refuse_snapshot_replace)
    with pytest.raises(OSError, match='injected'):
        md._atomic(destination, b'replacement')
    assert destination.read_bytes() == b'original'
    assert not list(destination.parent.glob('*.tmp'))
