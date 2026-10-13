"""Commercial raw native source, one daily Sim and immutable publication gates."""
from copy import deepcopy
import json
from pathlib import Path
import math
import pytest
import polars as pl
from portfolio_risk.analytics import model_ledger as engine
from portfolio_risk.analytics.commercial_ledger_transport import CommercialWriter,SCHEMAS
from portfolio_risk.analytics.commercial_ledger_reference import verify_preparation,verify_stream

ROOT=Path(__file__).resolve().parents[3]
def raw():return json.loads((ROOT/'examples/models/commercial-credit-ledger-1.json').read_text())


@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    request=raw();path=engine.run_model_ledger(request,tmp_path_factory.mktemp('commercial-native'),partition_rows=7)
    manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    return request,path,manifest,header


def test_full_raw_native_commercial_capture_and_every_report(saved,monkeypatch):
    request,path,manifest,header=saved
    assert manifest['validation']['commercial_source_recomputed'] is True
    assert manifest['validation']['persisted_partitions_replayed'] is True
    assert manifest['validation']['capital_ftp_eligible'] is False
    assert json.loads((path.parent/'producer-completion.json').read_text())==dict(complete=True,protocol=6)
    prepared=verify_preparation(request,header['prepared_request'],header['source_receipt'])
    class Frames:
        completion=dict(complete=True,protocol=6)
        def frames(self,name):
            for part in manifest['tables'][name]['parts']:yield pl.read_parquet(path.parent/part['path'])
        def finish(self):pass
    assert verify_stream(request,prepared,Frames())==manifest['validation']


@pytest.mark.parametrize('target',['source_weight','path_key','year','principal','scalar_bool','protocol_float','reporting_float','work_bool'])
def test_raw_native_source_receipt_mutations_reject(saved,target):
    request,_,_,header=saved;header=deepcopy(header);source=header['source_receipt']['sources'][0]
    if target=='source_weight':header['prepared_request']['participations'][0]['source_weight']+=.01
    elif target=='path_key':source['carry']['credit']['result']['paths'][0]['path_key']+=1
    elif target=='year':source['carry']['carry_points'][1]['year']+=1e-12
    elif target=='principal':source['accounting']['groups'][0]['next']['principal']+=1.
    elif target=='scalar_bool':source['accounting']['groups'][0]['next']['principal']=True
    elif target=='protocol_float':header['source_receipt']['protocol']=6.
    elif target=='reporting_float':header['source_receipt']['reporting_horizon_days']=float(request['spec']['horizon_days'])
    else:header['source_receipt']['admitted_work_upper']=True
    with pytest.raises((ArithmeticError,ValueError)):verify_preparation(request,header['prepared_request'],header['source_receipt'])


@pytest.mark.parametrize('field,value',[('path_key',True),('day',1.),('source_weight',True),('source_weight',float('nan')),('capital_ftp_eligible',0)])
def test_commercial_parquet_transport_refuses_scalar_coercion(tmp_path,field,value):
    writer=CommercialWriter(tmp_path,2,schemas=SCHEMAS)
    row=dict(scenario='baseline',day=0,source_origin_day=0,position_id='loan',source_class='commercial_loan-1',path_key=17,source_weight=1.,allocation_factor=1.,unweighted_book={},capital_ftp_eligible=False);row[field]=value
    with pytest.raises(ValueError):writer.add('commercial_daily',[row])


def test_commercial_path_key_preserves_full_native_unsigned_identity(tmp_path):
    writer=CommercialWriter(tmp_path,2,schemas=SCHEMAS);key=(1<<64)-1
    writer.add('commercial_daily',[dict(scenario='baseline',day=0,source_origin_day=0,position_id='loan',source_class='commercial_loan-1',path_key=key,source_weight=1.,allocation_factor=1.,unweighted_book={},capital_ftp_eligible=False)])
    writer.finish();assert next(writer.frames('commercial_daily'))['path_key'][0]==key


def multiaccount_request():
    request=raw();request['spec']['horizon_days']=30
    other=deepcopy(request['spec']['accounts'][0]);other.update(id='other',entity='other-entity');request['spec']['accounts'].append(other)
    position=deepcopy(request['spec']['positions'][0]);position.update(id='other-commercial',account='other');request['spec']['positions'].append(position)
    request['allocations'].append(dict(position_id='other-commercial',allocated_face=0.))
    sources=[];participations=[]
    for source in request['commercial']:
        for role in ('original','current'):
            terms_source=source['source'][role];asof=terms_source['asof_ordinal'];terms_source.update(reporting_end_ordinal=asof+5,tail_end_ordinal=asof+7)
            terms_source['periods'][0].update(end_ordinal=asof+5,payment_ordinal=asof+7)
        source['source']['acquisition']['cost']=100.+100.*.1*5/365
        source['credit']['max_jumps_per_path']=1;source['credit']['intervals'][0]['end_year']=7/365
        source['credit']['paths'].append(dict(path_id='other-path',path_key=18,initial_live_state='performing'))
        source['credit']['opening_undrawn_commitment']=40.
        terms=source['credit']['intervals'][0];terms['hazards_per_year'][0][3]=10.;terms['credit_conversion_factor']=[.5]*3;terms['loss_given_default']=[.25]*3
        terms['complete_recovery_tails']=[[dict(lag_years=30.25/365,weight=1.)] for _ in range(3)]
        source['credit']['draw_stream']=dict(kind='supplied',stream_id='shared-controls',stream_revision='1',paths=[dict(path_key=key,pairs=[dict(waiting_uniform=math.exp(-10.*day/365),destination_uniform=.5)]) for key,day in ((17,2.5),(18,1000.))])
        clone=deepcopy(source)
        for role in ('original','current'):
            clone['source'][role]['position_id']='other-commercial';clone['source'][role]['contract']['id']='other-commercial'
        sources.extend((source,clone))
        scenario=source['source']['current']['scenario_id']
        for name in ('commercial','other-commercial'):
            participations.extend(dict(scenario=scenario,position_id=name,path_key=key,source_weight=weight) for key,weight in ((17,.25),(18,.75)))
    request['commercial']=sources;request['participations']=participations
    return request


@pytest.mark.parametrize('control',['accounts','paths'])
def test_native_multiple_accounts_or_paths_zero_allocation_and_full_recovery_calendar(tmp_path,control):
    request=multiaccount_request()
    if control=='accounts':
        for source in request['commercial']:
            source['credit']['paths']=source['credit']['paths'][:1]
            source['credit']['draw_stream']['paths']=source['credit']['draw_stream']['paths'][:1]
            source['credit']['intervals'][0]['complete_recovery_tails']=[[dict(lag_years=10.25/365,weight=1.)] for _ in range(3)]
        request['participations']=[row|{'source_weight':1.} for row in request['participations'] if row['path_key']==17]
    else:
        request['spec']['accounts']=request['spec']['accounts'][:1]
        request['spec']['positions']=request['spec']['positions'][:1]
        request['allocations']=request['allocations'][:1]
        request['commercial']=[source for source in request['commercial'] if source['source']['current']['position_id']=='commercial']
        request['participations']=[row for row in request['participations'] if row['position_id']=='commercial']
    path=engine.run_model_ledger(request,tmp_path,partition_rows=11);manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    assert manifest['validation']['persisted_partitions_replayed'] is True
    assert header['source_receipt']['reporting_horizon_days']==30
    assert header['source_receipt']['resolved_specification']['horizon_days']==(30 if control=='accounts' else 38)
    rows=[r for part in manifest['tables']['commercial_closing']['parts'] for r in pl.read_parquet(path.parent/part['path']).to_dicts()]
    assert len(rows)==4;assert {r['scenario'] for r in rows}=={'baseline','stress'}
    if control=='accounts':
        assert {r['position_id'] for r in rows}=={'commercial','other-commercial'}
        assert all(r['allocation_factor']==0. for r in rows if r['position_id']=='other-commercial')
    else:
        assert {r['path_key'] for r in rows}=={17,18}
    defaulted=[json.loads(r['unweighted_book_json']) for r in rows if r['path_key']==17]
    assert all(r['principal']==30. and r['in_default'] for r in defaulted)


def test_native_multiaccount_multipath_all_scenario_work_ceiling_refuses_empty_output(tmp_path):
    with pytest.raises((RuntimeError,ValueError),match='preflight exceeds 8M|daily upper exceeds 8M'):
        engine.run_model_ledger(multiaccount_request(),tmp_path,partition_rows=11)
    assert list(tmp_path.iterdir())==[]
