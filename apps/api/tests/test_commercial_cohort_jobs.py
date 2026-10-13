"""Commercial cohort raw intake, persisted economics and final SQL authority."""
from copy import deepcopy
import json
import pytest
from app import model_routes,persistence as db,worker
from test_persistence import durable

def raw_cohort():
    def row(name,multiplier):
        graph=deepcopy(model_routes.example('commercial-product-graph-1')['request']);p=graph['positions'][0];p['position_id']=name
        sources=list(p['original_pricing'])
        for history in p['histories']:
            for role in ('physical','pricing','pricing_down','pricing_up'):sources+=history[role]
        for h in sources:
            for role in ('original','current'):h['source']['source'][role]['position_id']=name
        return dict(row_id=name,amount_multiplier=multiplier,graph=dict(kind='commercial',request=graph))
    metrics=['date/738886/pv','date/738886/dv01','date/738901/pv','date/738901/dv01','source_full_cash/principal','source_full_cash/interest','source_full_cash/fees','source_full_gross_interest','source_full_effective_interest','source_full_default/principal','source_full_default/principal_loss']
    return dict(schema='graph-cohort-audit-1',source_revision='explicit-commercial-1',monetary_unit='currency_units',cohorts=[dict(cohort_id='commercial',representative=row('representative',2.),members=[row('member',2.)],tolerances=[dict(metric=m,absolute_amount=1e-9,relative=1e-12)for m in metrics])])

def queued():
    jid=model_routes.submit(model_routes.ModelRunRequest(schema_name='graph-cohort-audit-1',request=raw_cohort(),expected_revision=0)).id
    db.WORKER='commercial-cohort-test';worker.STOP.clear();assert db.REPO.acquire(db.WORKER)
    return jid,db.REPO.claim(db.WORKER)

def test_durable_commercial_cohort_recomputes_each_source_and_retains_original_budget(durable):
    jid,job=queued();deadline=db.CODEC.load(job['request'])['args'][0]['publication_deadline_unix'];worker.execute(job)
    row=db.REPO.job(jid);assert row['status']=='done',row['detail']
    result=db.CODEC.load(db.result_ref(jid));assert result['execution']['publication_deadline_unix']==deadline
    gate=result['execution']['source_verification'];assert gate['verified']is True and gate['representative_member_amounts_recomputed']is True
    assert gate['empirical_validation']is False

@pytest.mark.parametrize('target',['row_amount','error','receipt'])
def test_rehashed_persisted_cohort_does_not_replace_independent_economics(durable,monkeypatch,target):
    import polars as pl
    jid,job=queued();dump=db.CODEC.dump;changed=[False]
    def corrupt(value):
        if isinstance(value,dict)and'model_result'in value and'execution'in value:
            value=deepcopy(value);changed[0]=True
            if target=='receipt':value['execution']['source_verification']['verified']=1
            elif target=='error':value['model_result']['errors']=value['model_result']['errors'].with_columns((pl.col('allowed_error')+1.).alias('allowed_error'))
            else:
                rows=value['model_result']['rows'].to_dicts();amounts=json.loads(rows[0]['amounts']);amounts[0]['amount']+=1.;rows[0]['amounts']=json.dumps(amounts,separators=(',',':'));value['model_result']['rows']=pl.DataFrame(rows)
        return dump(value)
    monkeypatch.setattr(db.CODEC,'dump',corrupt);worker.execute(job)
    row=db.REPO.job(jid);assert changed[0]and row['status']!='done'and row['result']is None

@pytest.mark.parametrize('target',['source','cancel','deadline'])
def test_cohort_final_sql_guard_retains_queued_source_and_deadline(durable,monkeypatch,target):
    import sqlalchemy as sa
    from app.repository import jobs
    jid,job=queued();complete=db.REPO.complete;changed=[False]
    def corrupt(bound,**kw):
        if kw.get('guard'):
            changed[0]=True
            if target=='cancel':worker.STOP.set()
            elif target=='source':
                from portfolio_risk.analytics import balance_stream
                monkeypatch.setattr(balance_stream,'source_identity',lambda:'0'*64)
            else:
                payload=db.CODEC.load(bound['request']);payload['args'][0]['publication_deadline_unix']+=3600.
                with db.REPO.engine.begin()as connection:connection.execute(sa.update(jobs).where(db.REPO.scoped(jobs),jobs.c.id==jid).values(request=db.CODEC.dump(payload)))
        return complete(bound,**kw)
    monkeypatch.setattr(db.REPO,'complete',corrupt)
    try:worker.execute(job)
    finally:worker.STOP.clear()
    row=db.REPO.job(jid);assert changed[0]and row['status']!='done'and row['result']is None

@pytest.mark.parametrize('target',['amount','overwritten_kind','overwritten_schema'])
def test_commercial_cohort_http_intake_keeps_original_duplicate_fields(durable,target):
    from fastapi.testclient import TestClient
    from app import main
    body=json.dumps(dict(schema_name='graph-cohort-audit-1',request=raw_cohort(),expected_revision=0),separators=(',',':'))
    if target=='amount':body=body.replace('"amount_multiplier":2.0','"amount_multiplier":2.0,"amount_multiplier":3.0',1)
    elif target=='overwritten_kind':body=body.replace('"kind":"commercial"','"kind":"commercial","kind":"deposit"')
    else:body=body.replace('"schema_name":"graph-cohort-audit-1"','"schema_name":"graph-cohort-audit-1","schema_name":"dated-term-1"')
    with TestClient(main.app)as client:response=client.post('/models/runs',content=body,headers={'content-type':'application/json'})
    assert response.status_code==422 and'duplicate'in response.text
