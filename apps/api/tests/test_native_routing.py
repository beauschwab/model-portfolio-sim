"""Legacy-side guarantees for the Rust workbench service (issue #47).

The Rust service queues native jobs in the shared ``jobs`` table under ``native_*`` statuses and
routes a cut-over workspace through ``worker_routes``. The legacy oldest-job claim must never
consume or requeue a native job, and the legacy worker must not take the lease of a workspace
routed to Rust. When the Rust binary is built, a database it created is read here unchanged.
"""
import json
import os
import subprocess
import time
import uuid
from pathlib import Path

import pytest
import sqlalchemy as sa

from app.repository import Repository, jobs

ROOT = Path(__file__).resolve().parents[3]
NATIVE = Path(os.getenv('WORKBENCH_NATIVE_EXE', ROOT / 'apps/api-rust/target/debug' /
                        ('workbench-native.exe' if os.name == 'nt' else 'workbench-native')))
ROUTES_DDL = ('CREATE TABLE worker_routes (tenant VARCHAR(128) NOT NULL, workspace VARCHAR(128) NOT NULL, '
              'engine VARCHAR(16) NOT NULL, epoch INTEGER NOT NULL, actor VARCHAR(200) NOT NULL, '
              'changed_at FLOAT NOT NULL, PRIMARY KEY (tenant, workspace))')


def repository(tmp_path):
    repo = Repository(f"sqlite:///{(tmp_path / 'workbench.db').as_posix()}", 'tests', uuid.uuid4().hex)
    repo.migrate()
    repo.initialize({'key': 'k', 'sha256': '0' * 64, 'bytes': 1, 'format': 'json'})
    return repo


def native_job(repo, status, owner=None):
    with repo.engine.begin() as conn:
        conn.execute(jobs.insert().values(**repo.identity, id=uuid.uuid4().hex, kind='liquidity-lifecycle-1',
                                          revision=0, status=status, request={'schema': 'liquidity-lifecycle-1'},
                                          created_at=time.time() - 60, attempt=0, owner=owner))


def test_legacy_claim_never_consumes_or_requeues_native_jobs(tmp_path):
    repo = repository(tmp_path)
    native_job(repo, 'native_queued')
    native_job(repo, 'native_running', owner='rust:other')
    assert repo.acquire('legacy-worker', 30)
    assert repo.claim('legacy-worker') is None
    with repo.engine.connect() as conn:
        statuses = sorted(conn.execute(sa.select(jobs.c.status).where(repo.scoped(jobs))).scalars())
    assert statuses == ['native_queued', 'native_running']


def test_the_legacy_worker_refuses_a_workspace_routed_to_rust(tmp_path):
    repo = repository(tmp_path)
    assert repo.acquire('legacy-worker', 1)
    repo.release('legacy-worker')
    with repo.engine.begin() as conn:
        conn.exec_driver_sql(ROUTES_DDL)
        conn.execute(sa.text("INSERT INTO worker_routes VALUES (:t, :w, 'rust', 1, 'test', 0)"),
                     {'t': repo.identity['tenant'], 'w': repo.identity['workspace']})
    assert not repo.acquire('legacy-worker', 30)
    with repo.engine.begin() as conn:
        conn.execute(sa.text("UPDATE worker_routes SET engine = 'legacy', epoch = 2"))
    assert repo.acquire('legacy-worker', 30)


@pytest.mark.skipif(not NATIVE.exists(), reason='build apps/api-rust (cargo build) to run the cross-engine check')
def test_a_database_created_by_the_rust_service_is_read_unchanged(tmp_path):
    url = f"sqlite:///{(tmp_path / 'native.db').as_posix()}"
    env = {'DATABASE_URL': url, 'ARTIFACT_URL': str(tmp_path / 'artifacts'), 'WORKBENCH_TENANT_ID': 'tests',
           'WORKBENCH_WORKSPACE_ID': 'native', 'SystemRoot': os.environ.get('SystemRoot', '')}
    for command in (['migrate'], ['init'], ['cutover', '--to', 'rust']):
        subprocess.run([str(NATIVE), 'admin', *command], env=env, check=True, capture_output=True)
    repo = Repository(url, 'tests', 'native')
    repo.migrate()  # The shared schema version and tables are the ones SQLAlchemy expects.
    assert repo.head() == 0
    manifest = repo.revision(0)['manifest']
    assert set(manifest) == {'key', 'sha256', 'bytes', 'format'}
    assert not repo.acquire('legacy-worker', 30)
    status = json.loads(subprocess.run([str(NATIVE), 'admin', 'status'], env=env, check=True,
                                       capture_output=True).stdout)
    assert status['engine'] == 'rust' and status['revision'] == 0
