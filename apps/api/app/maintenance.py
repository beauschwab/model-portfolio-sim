"""Offline reference-aware orphan collection. Published history is retained.

Run a dry-run first. Apply requires all API, worker and catalog publishers to be
stopped; --offline is an operator assertion, not an online distributed GC lock.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import time

import sqlalchemy as sa
from . import repository as r
from .artifacts import Objects
from .storage_config import StorageConfig, ROOT


def scratch_root(config, objects):
    base = Path(os.getenv('WORKBENCH_SCRATCH_DIR', str(ROOT / '.data' / 'scratch'))).resolve()
    return base / objects.prefix.split('/')[-1][:16]


def roots(repo):
    values = []
    with repo.engine.connect() as conn:
        for table, columns in [(r.revisions, ['manifest']), (r.jobs, ['request', 'result']),
                               (r.sessions, ['manifest']), (r.libraries, ['manifest']),
                               (r.research, ['manifest']), (r.publications, ['manifest'])]:
            for row in conn.execute(sa.select(*(table.c[c] for c in columns)).where(repo.scoped(table))):
                values.extend(v for v in row if v is not None)
    return values


def reachable(repo, objects):
    pending, seen = roots(repo), set()
    if repo.head() is None:
        raise ValueError('workspace must exist before collection')
    def visit(node):
        if isinstance(node, dict):
            if {'key', 'sha256', 'bytes', 'format'} <= node.keys():
                pending.append(node)
            else:
                for value in node.values(): visit(value)
        elif isinstance(node, list):
            for value in node: visit(value)
    while pending:
        ref = pending.pop()
        if ref['key'] in seen: continue
        data = objects.get(ref)  # Validate scope, existence and integrity; fail closed.
        seen.add(ref['key'])
        if ref['format'] == 'json': visit(json.loads(data))
    return seen


def inventory(objects):
    pattern = re.compile(re.escape(objects.prefix) + r'/([0-9a-f]{2})/([0-9a-f]{64})\.(json|parquet|bin)$')
    if objects.bucket:
        pages = objects.client.get_paginator('list_objects_v2').paginate(Bucket=objects.bucket, Prefix=objects.prefix + '/')
        entries = ({'key': x['Key'], 'modified': x['LastModified'].timestamp(), 'bytes': x['Size']}
                   for page in pages for x in page.get('Contents', []))
    else:
        base = (objects.root / objects.prefix).resolve()
        entries = ({'key': p.relative_to(objects.root).as_posix(), 'modified': p.stat().st_mtime, 'bytes': p.stat().st_size}
                   for p in base.rglob('*') if p.is_file() and not p.is_symlink() and p.resolve().is_relative_to(base))
    for entry in entries:
        match = pattern.fullmatch(entry['key'])
        if match and match[1] == match[2][:2]: yield entry


def collect(repo, objects, *, grace_seconds=7*86400, apply=False, offline=False, scratch=None):
    if grace_seconds < 3600:
        raise ValueError('orphan grace must be at least one hour')
    if apply:
        if not offline: raise ValueError('apply requires offline assertion; stop API, worker and catalog publishers')
        with repo.engine.connect() as conn:
            lease = conn.execute(sa.select(r.workspaces.c.lease_until).where(repo.scoped(r.workspaces))).scalar_one()
            active = conn.execute(sa.select(sa.func.count()).select_from(r.jobs).where(repo.scoped(r.jobs),
                r.jobs.c.status.in_(['running', 'queued']))).scalar_one()
            if lease > time.time() or active: raise RuntimeError('workspace has a worker lease or active jobs')
    live = reachable(repo, objects)
    cutoff = time.time() - grace_seconds
    candidates = [x for x in inventory(objects) if x['key'] not in live and x['modified'] < cutoff]
    removed = []
    for entry in candidates:
        if not apply: continue
        if objects.bucket:
            head = objects.client.head_object(Bucket=objects.bucket, Key=entry['key'])
            if head['LastModified'].timestamp() >= cutoff: continue
            objects.client.delete_object(Bucket=objects.bucket, Key=entry['key'])
        else:
            path = (objects.root / entry['key']).resolve()
            base = (objects.root / objects.prefix).resolve()
            if not path.is_relative_to(base) or path.is_symlink(): raise ValueError('unsafe object path')
            if path.stat().st_mtime >= cutoff: continue
            path.unlink()
        removed.append(entry['key'])
    scratch_candidates = []
    if scratch and Path(scratch).exists():
        base = Path(scratch).resolve()
        for directory in base.glob('balance-*'):
            if directory.is_symlink() or not directory.is_dir() or directory.resolve().parent != base: continue
            # Root mtime does not track writes inside a long-running child directory.
            newest = max([directory.stat().st_mtime] + [p.stat().st_mtime for p in directory.rglob('*')])
            if newest >= cutoff: continue
            scratch_candidates.append(str(directory))
            if apply: shutil.rmtree(directory)  # Exact verified workspace child, offline only.
    return dict(dry_run=not apply, reachable_objects=len(live), candidates=candidates,
                removed=removed, scratch_candidates=scratch_candidates,
                policy='retain all referenced revisions, requests, results, sessions, libraries and catalog receipts')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--grace-days', type=float, default=7)
    args = parser.parse_args()
    config = StorageConfig.from_env()
    repo = r.Repository(config.database_url, config.tenant, config.workspace)
    repo.migrate()
    objects = Objects(config.artifact_url, config.tenant, config.workspace)
    try:
        print(json.dumps(collect(repo, objects, grace_seconds=args.grace_days*86400,
            apply=args.apply, offline=args.offline, scratch=scratch_root(config, objects)), indent=2))
    finally: repo.engine.dispose()


if __name__ == '__main__': main()
