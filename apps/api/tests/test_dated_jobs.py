"""Actual native dated execution through durable storage and publication fences."""
import json
from pathlib import Path

import pytest
from fastapi import HTTPException
from app import main, store, worker, persistence as db, maintenance, reporting
from app.schemas import DatedBalanceStressRequest
from test_persistence import durable


def specification():
    return json.loads((Path(__file__).resolve().parents[3] / 'examples/models/dated-ledger-1.json').read_text(encoding='utf-8'))


def queued():
    raw = specification()
    request = dict(specification=raw, large_book=False,
        binary_sha256=db.identity()['balance_native_sha256'],
        validation_library_sha256=db.identity()['ledger_validation_native_sha256'])
    jid = store.submit('dated_balance_stress', store.run_dated_balance_stress,
                       request, state=store.snapshot())
    db.WORKER = 'dated-test'
    worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    return jid, db.REPO.claim(db.WORKER)


def test_dated_job_publishes_verified_sources_and_scoped_partitions(durable, monkeypatch):
    from portfolio_risk.analytics import balance_stress, balance_stream
    def forbidden(*args, **kwargs): raise AssertionError('Python financial execution')
    monkeypatch.setattr(balance_stress, 'validate', forbidden)
    monkeypatch.setattr(balance_stress, '_simulate', forbidden)
    monkeypatch.setattr(balance_stream, '_python', forbidden)
    body = DatedBalanceStressRequest(expected_revision=0, specification=specification())
    assert main.dated_balance_stress_example()['specification'] == specification()
    submitted = main.dated_balance_stress(body)
    db.WORKER = 'dated-endpoint-test'; worker.STOP.clear()
    assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER))
    jid = submitted['id']
    assert db.REPO.job(jid)['status'] == 'done', db.REPO.job(jid)['detail']
    result = db.CODEC.load(db.result_ref(jid)); execution = result['execution']
    assert execution['validation']['journal_replayed']
    assert execution['validation']['dated_source_reconciled']
    assert execution['version'] == 'dated-balance-partitions-1'
    assert 'specification' not in execution
    assert json.loads(db.CODEC.objects.get(execution['input'])) == specification()
    assert execution['input']['key'] in maintenance.reachable(db.REPO, db.CODEC.objects)
    manifest = main.run_manifest(jid)
    assert manifest['execution']['validation']['dated_source_reconciled']
    assert '/event_lineage' in manifest['tables'] and '/dated_rights' in manifest['tables']
    get = db.CODEC.objects.get
    def no_input_on_page(ref):
        assert ref not in (execution['input'], execution['request_manifest'])
        return get(ref)
    monkeypatch.setattr(db.CODEC.objects, 'get', no_input_on_page)
    page = reporting.table(jid, '/event_lineage', offset=0, limit=2)
    assert len(page['rows']) == 2
    before = store.job_result(jid)
    db.stop(); db.start(durable)
    assert store.job_result(jid) == before


@pytest.mark.parametrize('failure', ['binary', 'validator', 'upload', 'cancel', 'lease'])
def test_dated_failed_attempt_never_publishes(durable, monkeypatch, failure):
    jid, job = queued()
    if failure in {'binary', 'validator'}:
        from portfolio_risk.analytics import balance_stream, ledger_native
        path = (balance_stream.binary_path if failure == 'binary' else ledger_native.library_path)()
        module = balance_stream if failure == 'binary' else ledger_native
        monkeypatch.setattr(module, 'binary_path' if failure == 'binary' else 'library_path', lambda: path.with_name('missing-native'))
    elif failure == 'upload':
        put = db.CODEC.objects.put
        def failed(data, suffix):
            if suffix == 'parquet': raise OSError('injected dated upload failure')
            return put(data, suffix)
        monkeypatch.setattr(db.CODEC.objects, 'put', failed)
    else:
        original = db.CODEC.import_dated_partitions
        def interrupted(path, cancelled):
            if failure == 'cancel': db.REPO.cancel(jid)
            else: db.REPO.release(db.WORKER)
            return original(path, cancelled)
        monkeypatch.setattr(db.CODEC, 'import_dated_partitions', interrupted)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert row['status'] != 'done' and row['result'] is None


def test_dated_admission_revision_capacity_and_validator_identity(durable, monkeypatch):
    body = DatedBalanceStressRequest(expected_revision=0, specification=specification())
    for edit, status in [({'expected_revision': 1}, 409), ({'large_book': True}, 422)]:
        with pytest.raises(HTTPException) as exc: main.dated_balance_stress(body.model_copy(update=edit))
        assert exc.value.status_code == status
    monkeypatch.setitem(db._IDENTITY, 'ledger_validation_native_sha256', '0' * 64)
    with pytest.raises(HTTPException) as exc: main.dated_balance_stress(body)
    assert exc.value.status_code == 409
    with pytest.raises(ValueError):
        DatedBalanceStressRequest(expected_revision=0, specification=specification(), backend='python')
