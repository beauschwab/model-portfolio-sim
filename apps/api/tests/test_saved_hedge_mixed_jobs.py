"""Durable saved-hedge-mixed-1 jobs: actual worker, real SQLite/Codec/Parquet, independent reopen.

The native model-ledger subprocess is the financial owner. The independent hedge and mixed
references reconstruct the formal journal and the shared daily GL before and after persistence.
"""
from copy import deepcopy
import io
from pathlib import Path
import pytest
import polars as pl
from app import worker, store, persistence as db, saved_derivative_jobs as candidate
from app.artifacts import Codec, Objects, PartitionedTable, ObjectReference
from app.repository import Repository
from app.storage_config import StorageConfig
from app.saved_derivative_transport import verify_persisted, StoredFrames
from portfolio_risk.analytics.saved_derivative_identity import loads, encoded

ROOT = Path(__file__).resolve().parents[3]
FIXTURE = ROOT / 'packages/portfolio-risk/tests/fixtures/saved_hedge_mixed/base.json'


def raw():
    return loads(FIXTURE.read_bytes())


@pytest.fixture
def durable(tmp_path, monkeypatch):
    config = StorageConfig('sqlite:///' + str(tmp_path / 'worker.db'), str(tmp_path / 'objects'), 'isolated', 'hedge')
    repo = Repository(config.database_url, config.tenant, config.workspace)
    repo.migrate()
    objects = Objects(config.artifact_url, config.tenant, config.workspace)
    codec = Codec(objects)
    identity = db.current_identity()
    monkeypatch.setattr(db, '_IDENTITY', deepcopy(identity))
    monkeypatch.setattr(db, 'REPO', repo)
    monkeypatch.setattr(db, 'CODEC', codec)
    monkeypatch.setattr(db, 'CONFIG', config)
    worker.STOP.clear()
    store.SETTINGS = store.SETTINGS.model_copy(update={'n_threads': 1, 'compute_backend': 'rust'})
    state = store.snapshot_local()
    state['revision'] = 0
    store.STATE_META['revision'] = 0
    repo.initialize(codec.dump(state))
    from portfolio_risk.core import native as product_native
    from portfolio_risk.core import quant_native as risk_native
    from portfolio_risk.analytics import ledger_native

    def forbidden(*a, **kw):
        raise AssertionError('Python financial/native DLL callback forbidden')
    for mod, attribute in [(risk_native, '_load'), (risk_native, 'call'), (risk_native, 'term_call'),
                           (product_native, 'discount_backend'), (ledger_native, 'validate_spec')]:
        if hasattr(mod, attribute):
            monkeypatch.setattr(mod, attribute, forbidden)

    def start(r=None, timeout=300., execute=True):
        q = candidate.queue_input(raw() if r is None else r, timeout=timeout)
        payload = dict(operation=candidate.OPERATION, args=[q], state=store.snapshot_local(), identity=deepcopy(identity))
        ref = codec.dump(payload)
        jid = repo.enqueue('saved_derivative_mixed', 0, ref, {})
        assert repo.acquire('hedge-worker', 300.)
        job = repo.claim('hedge-worker')
        assert job['id'] == jid
        if execute:
            worker.execute(job)
        return q, job
    yield dict(repo=repo, objects=objects, codec=codec, start=start)
    worker.STOP.clear()
    repo.engine.dispose()


def test_hedge_mixed_job_publishes_and_reopens(durable):
    c = durable
    q, job = c['start']()
    row = c['repo'].job(job['id'])
    assert row['status'] == 'done', row['detail']
    execution = c['codec'].metadata(row['result'], {'execution', 'revision'})['execution']
    receipt = execution['validation']
    assert receipt['schema'] == 'saved-hedge-mixed-verification-1'
    assert receipt['formal_hedge_relationships_replayed'] is True and receipt['complete_hedge_basis_oci_tail_verified'] is True
    assert receipt['accounting_standard_approval'] is False and receipt['capital_ftp_eligible'] is False
    assert execution['model_version'] == 'saved-hedge-mixed-1'
    reopened = Codec(Objects(str(c['objects'].root), 'isolated', 'hedge'))
    new = Repository(c['repo'].engine.url, 'isolated', 'hedge')
    try:
        _, again = verify_persisted(reopened, new.job(job['id'])['result'], q['specification'])
        assert again == receipt
    finally:
        new.engine.dispose()


@pytest.mark.parametrize('target', ['native_hedge', 'journal', 'lineage', 'receipt'])
def test_rehashed_hedge_persistence_fails_independent_reopen(durable, target):
    c = durable
    q, job = c['start']()
    row = c['repo'].job(job['id'])
    assert row['status'] == 'done', row['detail']
    facade = StoredFrames(c['codec'], row['result'], lambda: False)
    result = {name: PartitionedTable({k: str(v) for k, v in facade.schemas[name].items()}, deepcopy(parts))
              for name, parts in facade.parts.items()}
    result['execution'] = deepcopy(facade.execution)
    result['revision'] = facade.revision
    e = result['execution']
    if target == 'native_hedge':
        h = loads(c['objects'].get(e['modeled_source']))
        intent = next(i for i in h['source_receipt']['native_hedges'][0]['intents'] if i['event_id'].startswith('hedged_item_basis'))
        intent['postings'][0]['debit_amount'] += .01
        intent['postings'][1]['debit_amount'] -= .01
        e['modeled_source'] = ObjectReference(c['objects'].put(encoded(h), 'bin'))
    elif target == 'receipt':
        e['validation']['complete_hedge_basis_oci_tail_verified'] = 1
    else:
        table = {'journal': 'journal', 'lineage': 'derivative_journal_lineage'}[target]
        column = {'journal': 'debit', 'lineage': 'debit_amount'}[target]
        parts = deepcopy(facade.parts[table])
        part = parts[-1]
        frame = pl.read_parquet(io.BytesIO(c['objects'].get(part['ref'])))
        frame = frame.with_columns(pl.when(pl.int_range(pl.len()) == 0).then(pl.col(column) + .01).otherwise(pl.col(column)).alias(column))
        out = io.BytesIO()
        frame.write_parquet(out)
        part['ref'] = c['objects'].put(out.getvalue(), 'parquet')
        result[table] = PartitionedTable({k: str(v) for k, v in facade.schemas[table].items()}, parts)
    replacement = c['codec'].dump(result)
    with pytest.raises((ValueError, ArithmeticError, KeyError)):
        verify_persisted(c['codec'], replacement, q['specification'])


def test_hedge_overlay_drift_never_publishes(durable):
    c = durable
    r = raw()
    second = deepcopy(r['owners'][0]['runs'][0])
    second['hedge']['run']['revision'] = 'r2'
    r['owners'][0]['runs'].append(second)
    _, job = c['start'](r)
    row = c['repo'].job(job['id'])
    assert row['status'] != 'done' and row['result'] is None


def test_hedge_mixed_example_route():
    from app import saved_derivative_routes
    example = saved_derivative_routes.hedge_example()
    assert example['schema'] == 'saved-hedge-mixed-1' and example == raw()
