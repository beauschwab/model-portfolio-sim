"""Bounded compatibility GET; it grants no financial or policy authority."""
from copy import deepcopy
import types
import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient

@pytest.fixture
def client(monkeypatch):
    from app import treasury_routes as route, modeled_product_ftp_source_binding as binding
    raw={'product_owners':[{'position_id':'card-owner','runs':[{'ledger_run_id':'baseline','paths':[{'request':{'kind':'card_v2'}}]}]}]}
    capture={'kind':'card_v2','binding':{'revision':4},'raw':raw,'queued_identity':{'source':'exact'}}
    calls=[]
    def read(*args):
        calls.append(args);return deepcopy(capture)
    monkeypatch.setattr(route.db,'REPO',object());monkeypatch.setattr(route.db,'CODEC',types.SimpleNamespace(load=lambda *a:pytest.fail('full journal hydration forbidden')))
    monkeypatch.setattr(binding,'capture',read)
    web=FastAPI();web.include_router(route.router)
    with TestClient(web)as http:yield http,capture,calls,binding

def test_short_metadata_retains_indices_without_financial_authority(client):
    http,capture,calls,_=client
    r=http.get('/treasury/modeled-product-ftp/sources/source-job?kind=card_v2')
    assert r.status_code==200,r.text
    data=r.json();assert len(calls)==2
    assert data['paths']==[dict(owner_index=0,run_index=0,path_index=0,position_id='card-owner',run_id='baseline')]
    assert data['compatibility_metadata_checked'] is True
    assert all(data[k]is False for k in ['source_journal_verified','capital_ftp_eligible','accounting_standard_approval'])
    assert 'raw'not in data and 'queued_identity'not in data

@pytest.mark.parametrize('change',['row','raw','identity'])
def test_current_object_or_sql_replacement_refuses_metadata(client,monkeypatch,change):
    http,capture,calls,binding=client;n=[0]
    def read(*args):
        n[0]+=1;out=deepcopy(capture)
        if n[0]==2:
            if change=='row':out['binding']['revision']=5
            elif change=='raw':out['raw']['product_owners'][0]['position_id']='replacement'
            else:out['queued_identity']['source']='replacement'
        return out
    monkeypatch.setattr(binding,'capture',read)
    r=http.get('/treasury/modeled-product-ftp/sources/source-job?kind=card_v2')
    assert r.status_code==422 and 'changed'in r.text

@pytest.mark.parametrize('domain',['unknown','kind','pending','too_many','long_id'])
def test_refuse_unknown_uncompleted_or_unbounded_sources(client,monkeypatch,domain):
    http,capture,calls,binding=client;url='/treasury/modeled-product-ftp/sources/source-job?kind=card_v2'
    if domain=='unknown':
        def read(*a):raise KeyError('missing')
        monkeypatch.setattr(binding,'capture',read)
    elif domain=='pending':
        def read(*a):raise ValueError('compatible completed tagged FTP source job required')
        monkeypatch.setattr(binding,'capture',read)
    elif domain=='kind':url=url.replace('card_v2','card_v1')
    elif domain=='too_many':capture['raw']['product_owners'][0]['runs'][0]['paths']*=257
    else:url='/treasury/modeled-product-ftp/sources/'+('x'*101)+'?kind=card_v2'
    r=http.get(url);assert r.status_code==(404 if domain=='unknown'else 422)

def test_deposit_metadata_uses_explicit_raw_runs(client):
    http,capture,calls,_=client
    capture.update(kind='deposit_performing',raw={'position_id':'deposit','runs':[{'run_id':'baseline'},{'run_id':'stress'}]})
    r=http.get('/treasury/modeled-product-ftp/sources/deposit-job?kind=deposit_performing')
    assert r.status_code==200,r.text
    assert [p['run_id']for p in r.json()['paths']]==['baseline','stress']


@pytest.mark.parametrize('change',['boolbytes','extra','format','key','sha'])
def test_actual_codec_object_reference_decoder_refuses_malformed(change):
    from app.modeled_product_ftp_source_binding import Reader
    ref=dict(key='objects/source.bin',sha256='a'*64,bytes=8,format='bin')
    if change=='boolbytes':ref['bytes']=True
    elif change=='extra':ref['permission']=True
    elif change=='format':ref['format']='pickle'
    elif change=='key':ref['key']=False
    else:ref['sha256']='bad'
    with pytest.raises(ValueError):Reader(None,lambda:False).decode(dict(type='object-reference',ref=ref))

def test_actual_codec_object_reference_decoder_is_metadata_only():
    from app.modeled_product_ftp_source_binding import Reader
    ref=dict(key='objects/source.bin',sha256='a'*64,bytes=8,format='bin')
    reader=Reader(types.SimpleNamespace(load=lambda *a:pytest.fail('hydration'),objects=None),lambda:False)
    assert reader.decode(dict(type='object-reference',ref=ref))==ref
    assert not reader.refs and reader.bytes==0


def test_card2_dictionary_expansion_refuses_before_polars(monkeypatch):
    import io,hashlib
    import pyarrow as pa
    import pyarrow.parquet as pq
    import polars as pl
    from app.modeled_product_ftp_source_binding import Reader
    out=io.BytesIO();pq.write_table(pa.table({'opaque':['x'*65536]*1024}),out,use_dictionary=True)
    data=out.getvalue();ref=dict(key='source.parquet',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),format='parquet')
    objects=types.SimpleNamespace(get_bounded=lambda *a:data)
    reader=Reader(types.SimpleNamespace(objects=objects),lambda:False)
    monkeypatch.setattr(pl,'read_parquet',lambda *a,**k:pytest.fail('full Polars decode before logical admission'))
    with pytest.raises(ValueError,match='logical'):
        reader.decode(dict(type='frame',ref=ref,objects={}))


def test_small_card2_frame_retains_logical_bytes_and_cancels_prepass(monkeypatch):
    import io,hashlib
    import pyarrow as pa
    import pyarrow.parquet as pq
    from app.modeled_product_ftp_source_binding import Reader
    out=io.BytesIO();pq.write_table(pa.table({'opaque':['source']*16}),out,use_dictionary=True)
    data=out.getvalue();ref=dict(key='source.parquet',sha256=hashlib.sha256(data).hexdigest(),bytes=len(data),format='parquet')
    reader=Reader(types.SimpleNamespace(objects=types.SimpleNamespace(get_bounded=lambda *a:data)),lambda:False)
    frame=reader.decode(dict(type='frame',ref=ref,objects={}));assert frame.height==16 and reader.logical>0
    reader=Reader(types.SimpleNamespace(objects=types.SimpleNamespace(get_bounded=lambda *a:data)),lambda:False)
    reader.check=lambda:reader.logical>0
    with pytest.raises(InterruptedError):reader.decode(dict(type='frame',ref=ref,objects={}))
