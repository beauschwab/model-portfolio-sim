"""Same-owner fee treasury survives persisted replay and final SQL authority."""
import pytest
from app import model_routes, worker, persistence as db, reporting
from app.model_routes import ModelRunRequest
from test_persistence import durable

SCHEMA = 'commercial-candidate-fee-treasury-1'


def queued():
    raw = model_routes.example(SCHEMA)['request']
    submitted = model_routes.submit(ModelRunRequest(expected_revision=0,schema_name=SCHEMA,request=raw))
    db.WORKER = 'commercial-fee-candidate-treasury-test'
    worker.STOP.clear(); assert db.REPO.acquire(db.WORKER)
    return submitted.id, db.REPO.claim(db.WORKER), raw


def test_real_fee_treasury_worker_reopens_verified_sources_and_original_deadline(durable):
    jid, job, raw = queued()
    deadline = db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix']
    worker.execute(job)
    row = db.REPO.job(jid); assert row['status'] == 'done', row['detail']
    saved = db.CODEC.load(db.result_ref(jid))
    execution = saved['execution']
    assert execution['publication_deadline_unix'] == deadline
    assert execution['source_verification']['verified'] is True
    assert execution['source_verification']['empirical_validation'] is False
    from app.model_tables import restore_sources
    result = restore_sources(saved['model_result'],execution['table_layouts'])
    assert result['raw_request'] == raw
    assert result['funding_paths'] and result['capital'] and result['ftp']
    assert result['source_candidate']['producer_completion'] == dict(complete=True,protocol=7)
    tables = db.CODEC.tables(db.result_ref(jid))
    selected = next(name for name in tables if 'funding_paths' in name)
    assert reporting.table(jid,selected,offset=0,limit=1)['rows']
    db.stop(); db.start(durable)
    assert db.CODEC.load(db.result_ref(jid))['execution'] == execution


@pytest.mark.parametrize('target',['capital','ftp','fee_channel_type','fee_earnings','raw_source','deadline'])
def test_rehashed_fee_treasury_financial_and_source_changes_cannot_publish(durable,monkeypatch,target):
    jid, job, _ = queued(); dump, load = db.CODEC.dump, db.CODEC.load; reached=[]
    def mutate(value):
        ref = dump(value)
        if type(value) is not dict or value.get('execution',{}).get('schema') != SCHEMA:return ref
        reached.append(True); saved = load(ref)
        from app.model_tables import restore_sources,format_sources
        result = restore_sources(saved['model_result'],saved['execution']['table_layouts'])
        if target == 'capital':result['capital']['capital_bridge'][0]['assets'] += 1.
        elif target == 'ftp':result['ftp']['ftp_positions'][0]['external_profit'] += 1.
        elif target == 'fee_channel_type':
            result['funding_paths'][0]['preparation']['raw_request']['states'][0]['channels'][18]['amount'] = True
        elif target == 'fee_earnings':result['funding_paths'][0]['preparation']['days'][0]['external_earnings'] += 1.
        elif target == 'raw_source':result['raw_request']['raw_candidate']['allocation'][0]['face'] += 1.
        else:saved['execution']['publication_deadline_unix'] += 3600.
        saved['model_result'],saved['execution']['table_layouts'] = format_sources(result)
        return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate); worker.execute(job)
    row = db.REPO.job(jid)
    assert reached, row['detail']
    assert row['status'] != 'done' and row['result'] is None


@pytest.mark.parametrize('target',['bytes','cancel','lease','attempt','queued_deadline'])
def test_fee_treasury_sql_commit_fences_exact_objects_and_authority(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid, job, _ = queued(); complete = db.REPO.complete; reached=[]
    def commit(bound,**kwargs):
        if kwargs.get('guard'):
            reached.append(True)
            if target == 'bytes':
                descriptor = next(iter(db.CODEC.tables(kwargs['result']).values()))
                (db.CODEC.objects.root/descriptor['key']).write_bytes(b'changed-after-independent-fee-treasury-replay')
            elif target == 'cancel':worker.STOP.set()
            elif target == 'lease':db.REPO.release(db.WORKER)
            else:
                if target == 'attempt':changes = dict(attempt=bound['attempt']+1)
                else:
                    payload = db.CODEC.load(bound['request'])
                    payload['args'][0]['publication_deadline_unix'] += 3600.
                    changes = dict(request=db.CODEC.dump(payload))
                with db.REPO.engine.begin() as connection:
                    connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(**changes))
        return complete(bound,**kwargs)
    monkeypatch.setattr(db.REPO,'complete',commit)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row = db.REPO.job(jid)
    assert reached, row['detail']
    assert row['status'] != 'done' and row['result'] is None
