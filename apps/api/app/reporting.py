"""Bounded result-table queries. No arbitrary SQL or client-controlled URLs."""
import tempfile
from pathlib import Path

import duckdb
import io
import polars as pl

from . import persistence as db


def table(jid, path, *, columns=None, offset=0, limit=100):
    if not 0 <= offset <= 1_000_000_000 or not 1 <= limit <= 1000:
        raise ValueError('offset or limit outside supported range')
    ref = db.CODEC.tables(db.result_ref(jid)).get(path)
    if ref is None:
        raise KeyError('unknown result table')
    if ref.get('format') == 'partitioned-parquet':
        names = list(ref['schema']) if columns is None else columns
        if not names or len(names) != len(set(names)) or set(names) - set(ref['schema']):
            raise ValueError('select at least one valid unique column')
        skip, remaining, rows = offset, limit, []
        for part in ref['parts']:
            if skip >= part['rows']:
                skip -= part['rows']
                continue
            frame = pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])))
            if frame.height != part['rows'] or {k: str(v) for k,v in frame.schema.items()} != ref['schema']:
                raise ValueError('partition schema or row count mismatch')
            selected = frame.select(names).slice(skip, remaining)
            rows.extend(selected.rows())
            remaining -= selected.height
            skip = 0
            if not remaining:
                break
        return {'columns': names, 'rows': rows, 'offset': offset, 'limit': limit, 'total': ref['rows']}
    # Fetch only the selected Parquet artifact. Local/S3 share identical checksum
    # and tenant checks; direct remote scans can be added behind this boundary.
    raw = db.CODEC.objects.get(ref)
    with tempfile.TemporaryDirectory(prefix='workbench-query-') as directory:
        local = Path(directory) / 'result.parquet'
        local.write_bytes(raw)
        with duckdb.connect(config={'threads': 1, 'memory_limit': '256MB', 'autoinstall_known_extensions': False,
                                   'autoload_known_extensions': False}) as conn:
            relation = conn.read_parquet(str(local))
            names = relation.columns if columns is None else columns
            if not names or set(names) - set(relation.columns):
                raise ValueError('select at least one column')
            selected = relation.project(', '.join('"' + name.replace('"', '""') + '"' for name in names))
            rows = selected.limit(limit, offset=offset).fetchall()
            return {'columns': names, 'rows': rows, 'offset': offset, 'limit': limit,
                    'total': relation.count('*').fetchone()[0]}
