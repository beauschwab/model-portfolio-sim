"""Authored selected HTTP/raw/Parquet acceptance; default process pending rebuild."""
from pathlib import Path
from copy import deepcopy
import json,asyncio,io,time
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from starlette.requests import Request
from app import model_routes,cd_sequence_jobs,worker,persistence as db
from app.artifacts import Codec
from portfolio_risk.core import model_native
from test_persistence import durable
ROOT=Path(__file__).resolve().parents[3]
CASES=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/cd-sequence-graph2/cases.json').read_bytes())

def client():
    app=FastAPI();app.include_router(model_routes.router);return TestClient(app)

def browser_json(value):
    if type(value)is float and value.is_integer():return int(value)
    if type(value)is dict:return {k:browser_json(v)for k,v in value.items()}
    if type(value)is list:return [browser_json(v)for v in value]
    return value

def test_real_http_browser_integral_money_then_full_worker_source(durable):
    c=CASES[26];raw=browser_json(c['raw'])
    response=client().post('/models/runs',json=dict(schema_name=c['schema'],request=raw,expected_revision=0,timeout=60.))
    assert response.status_code==200,response.text
    jid=response.json()['id'];payload=db.CODEC.metadata(db.REPO.job(jid)['request'],{'args'})
    assert type(payload['args'][0]['request']['positions'][0]['known']['contract']['opening']['principal'])is int
    db.WORKER='browser-cd';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    worker.execute(db.REPO.claim(db.WORKER));row=db.REPO.job(jid)
    assert row['status']=='done',row['detail']
    output=db.CODEC.load(db.result_ref(jid))
    assert output['execution']['source_verification']['complete_preview_tails_verified']is True

@pytest.mark.parametrize('replacement',[('"schema":"term-product-graph-2"','"schema":"term-product-graph-2","schema":"term-product-graph-2"'),('"principal":100','"principal":100,"principal":100')])
def test_actual_http_nested_duplicate_before_queue(durable,replacement):
    c=CASES[26];body=json.dumps(dict(schema_name=c['schema'],request=c['raw'],expected_revision=0),separators=(',',':'))
    assert replacement[0]in body;body=body.replace(*replacement,1)
    response=client().post('/models/runs',content=body,headers={'Content-Type':'application/json'})
    assert response.status_code==422 and'duplicate field'in response.text
    
    import sqlalchemy as sa
    from app.repository import jobs
    with db.REPO.engine.connect()as conn:assert conn.execute(sa.select(sa.func.count()).select_from(jobs).where(db.REPO.scoped(jobs))).scalar_one()==0

def test_http_entry_clock_captured_before_body_formatting(durable,monkeypatch):
    c=CASES[26];body=json.dumps(dict(schema_name=c['schema'],request=c['raw'],expected_revision=0,timeout=1.))
    started=[time.time()]
    async def receive():
        await asyncio.sleep(.02)
        return dict(type='http.request',body=body.encode(),more_body=False)
    request=Request(dict(type='http',method='POST',path='/models/runs',headers=[]),receive)
    raw,entry=asyncio.run(model_routes._selected_cd_body(request))
    assert entry<=time.time()-.015
    captured=[]
    original=model_routes.enqueue
    def enqueue(*a,**kw):captured.append(kw['accepted_start']);return original(*a,**kw)
    monkeypatch.setattr(model_routes,'enqueue',enqueue)
    model_routes.submit(model_routes.ModelRunRequest(schema_name=c['schema'],request=raw,expected_revision=0,timeout=1.),(raw,entry))
    assert captured==[entry]

def test_dictionary_expansion_rejects_before_full_codec_decode(durable,monkeypatch):
    import pyarrow as pa,pyarrow.parquet as pq
    from portfolio_risk.core.indexed_book_job_native import JobBudget
    table=pa.table({'source_json':['x'*65536]*1024})
    data=io.BytesIO();pq.write_table(table,data,use_dictionary=True,compression='zstd')
    ref=db.CODEC.objects.put(data.getvalue(),'parquet')
    manifest=dict(schema_version=1,value=dict(type='frame',ref=ref,objects={}))
    outer=db.CODEC.objects.put(json.dumps(manifest).encode(),'json')
    def forbidden(*a,**k):raise AssertionError('logical over-cap Parquet reached full decode')
    monkeypatch.setattr(Codec,'decode',forbidden)
    with pytest.raises(ValueError,match='logical decoded component byte cap'):
        cd_sequence_jobs.load_bounded(outer,JobBudget(30.,lambda:False))

@pytest.mark.parametrize('field,value',[('schema_version',1.0),('schema_version',True)])
def test_manifest_schema_exact_type_before_decode(durable,monkeypatch,field,value):
    from portfolio_risk.core.indexed_book_job_native import JobBudget
    ref=db.CODEC.objects.put(json.dumps({field:value,'value':None}).encode(),'json')
    monkeypatch.setattr(Codec,'decode',lambda *a:(_ for _ in()).throw(AssertionError('untyped manifest decoded')))
    with pytest.raises(ValueError,match='typed CD persisted manifest'):cd_sequence_jobs.load_bounded(ref,JobBudget(30.,lambda:False))

@pytest.mark.parametrize('body',[[],{'schema_name':'term-product-graph-2','expected_revision':0},{'schema_name':'term-product-graph-2','request':None,'expected_revision':0}])
def test_invalid_http_envelope_has_typed_validation_error_without_queue(durable,body):
    response=client().post('/models/runs',json=body)
    assert response.status_code==422,response.text
