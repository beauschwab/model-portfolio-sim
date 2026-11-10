"""Actual JS request emission, native execution and durable raw-identity gates."""
from copy import deepcopy
import json
from pathlib import Path
import subprocess
import pytest
from app import main,model_routes,persistence as db,worker
from app.model_tables import restore_sources,format_sources
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact
from test_persistence import durable
from test_commercial_prior_opening import execute

ROOT=Path(__file__).resolve().parents[3]


def queued(schema):
    from fastapi.testclient import TestClient
    raw=json.loads((ROOT/'examples/models'/f'{schema}.json').read_text())
    ordinary=schema in('commercial-credit-ledger-1','commercial-generated-candidate-journal-1')
    value=dict(expected_revision=0,specification=raw)if ordinary else dict(expected_revision=0,schema_name=schema,request=raw)
    body=subprocess.check_output(['node','-e','process.stdout.write(JSON.stringify(JSON.parse(require("fs").readFileSync(0,"utf8"))))'],
        input=json.dumps(value,allow_nan=False,separators=(',',':')).encode())
    submitted=json.loads(body);raw=submitted['specification'if ordinary else 'request']
    # The durable fixture owns startup; exercise real HTTP parsing/dependencies
    # without starting a second application lifespan over its temporary database.
    client=TestClient(main.app)
    try:response=client.post('/balance-stress/modeled'if ordinary else '/models/runs',content=body,headers={'content-type':'application/json'})
    finally:client.close()
    assert response.status_code==200,response.text
    return response.json()['id'],raw


@pytest.mark.parametrize('schema',['commercial-credit-ledger-1','commercial-fee-journal-1','commercial-holding-journal-1',
    'commercial-candidate-holding-journal-1','commercial-candidate-fee-treasury-1','commercial-generated-candidate-journal-1'])
def test_js_browser_emitted_commercial_requests_publish_and_reopen_exact_raw(durable,schema):
    jid,raw=queued(schema);row=execute(jid,'browser-dto-worker')
    assert row['status']=='done',row['detail']
    saved=db.CODEC.load(row['result']);e=saved['execution']
    if schema in('commercial-credit-ledger-1','commercial-generated-candidate-journal-1'):
        assert exact(json.loads(db.CODEC.objects.get(e['input']))['request'],raw)
        assert e['validation']['ordered_same_sim_journal_replayed'if schema=='commercial-credit-ledger-1'else 'journal_replayed']is True
        assert e['validation']['persisted_partitions_replayed']is True
    else:
        native=restore_sources(saved['model_result'],e['table_layouts'])
        assert exact(native.get('raw_source',native.get('raw_request')),raw)
        assert e['source_verification']['verified']is True
        if schema=='commercial-fee-journal-1':
            spec=native['loan_receipt']['original_request']['spec']
            assert type(spec['accounts'][0]['cash'])is int and type(spec['positions'][0]['rate'])is int
    db.stop();db.start(durable)
    reopened=db.CODEC.load(db.result_ref(jid))
    assert reopened['execution']==e
    if schema in('commercial-credit-ledger-1','commercial-generated-candidate-journal-1'):assert reopened==saved
    else:assert exact(restore_sources(reopened['model_result'],reopened['execution']['table_layouts']),native)


@pytest.mark.parametrize('schema',['commercial-credit-ledger-1','commercial-fee-journal-1'])
@pytest.mark.parametrize('change',['money','int_to_float','boolean'])
def test_browser_money_raw_receipt_retyping_or_rehash_cannot_publish(durable,monkeypatch,schema,change):
    jid,_=queued(schema);dump=db.CODEC.dump;load=db.CODEC.load;changed=[]
    def mutate(value):
        ref=dump(value)
        if type(value)is not dict:return ref
        e=value.get('execution',{})
        if schema=='commercial-credit-ledger-1':
            if e.get('model_version')!=schema:return ref
            from app.artifacts import ObjectReference
            saved=load(ref);e=saved['execution'];header=json.loads(db.CODEC.objects.get(e['modeled_source']))
            spec=header['source_receipt']['original_request']['spec']
            if change=='money':spec['accounts'][0]['cash']+=1
            else:spec['positions'][0]['rate']=0. if change=='int_to_float'else False
            e['modeled_source']=ObjectReference(db.CODEC.objects.put(json.dumps(header).encode(),'bin'))
        else:
            if e.get('schema')!=schema:return ref
            saved=load(ref);native=restore_sources(saved['model_result'],saved['execution']['table_layouts'])
            spec=native['loan_receipt']['original_request']['spec']
            if change=='money':spec['accounts'][0]['cash']+=1
            else:spec['positions'][0]['rate']=0. if change=='int_to_float'else False
            saved['model_result'],saved['execution']['table_layouts']=format_sources(native)
        changed.append(True);return dump(saved)
    monkeypatch.setattr(db.CODEC,'dump',mutate)
    row=execute(jid,'browser-dto-tamper')
    assert changed and row['status']!='done'and row['result']is None
