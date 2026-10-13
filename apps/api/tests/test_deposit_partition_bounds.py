"""Selected decoded admission before read; no native financial process."""
import io,hashlib
from types import SimpleNamespace
import polars as pl
import pytest

@pytest.fixture
def selected(tmp_path):
    from app import deposit_partition_bounds as helper,artifacts
    codec=artifacts.Codec(artifacts.Objects(str(tmp_path/'objects'),'tests','bounded'))
    data=io.BytesIO();pl.DataFrame({'memo':['owned']}).write_parquet(data)
    value=data.getvalue();part=tmp_path/'part.parquet';part.write_bytes(value)
    manifest={'tables':{'test':{'schema':{'memo':'String'},'parts':[{'path':part.name,'rows':1,'bytes':len(value),'sha256':hashlib.sha256(value).hexdigest()}]}}}
    limits={'bytes':0,'count':0,'partition_rows':7,'schemas':{'test':{'memo':pl.String}},'decoded':0,'logical':0}
    return helper,artifacts,codec,tmp_path/'manifest.json',manifest,limits,value

@pytest.mark.parametrize('kind',['rows_bool','rows_excess','bytes_bool','bytes_excess','wrong_schema','serialized_aggregate','partition_count','actual_file_size'])
def test_selected_descriptor_refuses_before_decode(selected,monkeypatch,kind):
    _,artifacts,codec,path,manifest,limits,_=selected
    part=manifest['tables']['test']['parts'][0]
    if kind=='rows_bool':part['rows']=True
    elif kind=='rows_excess':part['rows']=8
    elif kind=='bytes_bool':part['bytes']=True
    elif kind=='bytes_excess':part['bytes']=32*1024*1024+1
    elif kind=='wrong_schema':manifest['tables']['test']['schema']['memo']='Float64'
    elif kind=='serialized_aggregate':limits['bytes']=32*1024*1024
    elif kind=='partition_count':limits['count']=65536
    else:(path.parent/part['path']).write_bytes(b'changed')
    monkeypatch.setattr(artifacts.pl,'read_parquet',lambda *a,**k:pytest.fail('decode before selected admission'))
    with pytest.raises(ValueError):codec._import_partitions(path,manifest,lambda:False,limits=limits)

@pytest.mark.parametrize('kind',['chunk','aggregate','row_count','negative_chunk','cancel'])
def test_compressed_footer_bounds_before_decode(selected,monkeypatch,kind):
    helper,artifacts,codec,path,manifest,limits,_=selected
    import pyarrow.parquet as pq
    size=helper.LIMIT+1 if kind=='chunk' else -1 if kind=='negative_chunk' else helper.LIMIT
    if kind=='aggregate':limits['decoded']=1
    metadata=SimpleNamespace(num_rows=2 if kind=='row_count' else 1,num_row_groups=1,row_group=lambda i:SimpleNamespace(num_columns=1,column=lambda j:SimpleNamespace(total_uncompressed_size=size)))
    monkeypatch.setattr(pq,'read_metadata',lambda *a:metadata)
    monkeypatch.setattr(artifacts.pl,'read_parquet',lambda *a,**k:pytest.fail('decode before footer admission'))
    with pytest.raises((ValueError,InterruptedError)):
        codec._import_partitions(path,manifest,lambda:kind=='cancel',limits=limits)


def test_valid_partition_metadata_then_bounded_decode(selected):
    _,_,codec,path,manifest,limits,_=selected
    result=codec._import_partitions(path,manifest,lambda:False,limits=limits)
    assert result['test'].descriptor()['rows']==1 and 0<limits['decoded']<=32*1024*1024

@pytest.mark.parametrize('kind',['bool_bytes','oversized_descriptor','oversized_file','checksum','s3_content_length'])
def test_bounded_object_read_refuses_before_unbounded_get(selected,monkeypatch,kind):
    _,_,codec,_,_,_,data=selected
    ref=codec.objects.put(data,'parquet')
    if kind=='bool_bytes':ref['bytes']=True
    elif kind=='oversized_descriptor':ref['bytes']=32*1024*1024+1
    elif kind=='oversized_file':(codec.objects.root/ref['key']).write_bytes(data+b'changed')
    elif kind=='checksum':(codec.objects.root/ref['key']).write_bytes(b'x'*len(data))
    else:
        codec.objects.bucket='tests'
        class Client:
            def get_object(self,**k):return {'ContentLength':ref['bytes']+1,'Body':io.BytesIO(data)}
        codec.objects.client=Client()
    monkeypatch.setattr(codec.objects,'get',lambda *a:pytest.fail('historical unbounded get called'))
    with pytest.raises(ValueError):codec.objects.get_bounded(ref,32*1024*1024)


def test_dictionary_logical_expansion_refuses_before_polars_decode(selected,monkeypatch):
    _,artifacts,codec,path,manifest,limits,_=selected
    import pyarrow as pa
    import pyarrow.parquet as pq
    buf=io.BytesIO();pq.write_table(pa.table({'memo':['x'*65536]*1024}),buf,use_dictionary=True,compression='zstd')
    data=buf.getvalue();part=manifest['tables']['test']['parts'][0]
    part.update(rows=1024,bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
    (path.parent/part['path']).write_bytes(data);limits['partition_rows']=65536
    metadata=pq.read_metadata(io.BytesIO(data))
    assert sum(metadata.row_group(i).column(j).total_uncompressed_size for i in range(metadata.num_row_groups) for j in range(metadata.row_group(i).num_columns))<32*1024*1024
    monkeypatch.setattr(artifacts.pl,'read_parquet',lambda *a,**k:pytest.fail('full decode before logical admission'))
    with pytest.raises(ValueError,match='logical decoded'):codec._import_partitions(path,manifest,lambda:False,limits=limits)


def test_logical_aggregate_refuses_before_polars_decode(selected,monkeypatch):
    _,artifacts,codec,path,manifest,limits,_=selected;limits['logical']=32*1024*1024
    monkeypatch.setattr(artifacts.pl,'read_parquet',lambda *a,**k:pytest.fail('full decode before logical admission'))
    with pytest.raises(ValueError,match='logical decoded'):codec._import_partitions(path,manifest,lambda:False,limits=limits)


@pytest.mark.parametrize('mode',['dictionary','cancel'])
def test_standalone_selected_writer_guards_before_full_decode(selected,monkeypatch,mode):
    _,artifacts,_,path,_,_,_=selected
    import pyarrow as pa
    import pyarrow.parquet as pq
    from portfolio_risk.analytics.deposit_continuation_transport import ContinuationDepositWriter
    buf=io.BytesIO();pq.write_table(pa.table({'memo':['x'*65536]*1024}),buf,use_dictionary=True,compression='zstd')
    data=buf.getvalue();part=path.parent/'owned.parquet';part.write_bytes(data)
    writer=ContinuationDepositWriter(path.parent,65536,schemas={'test':{'memo':pl.String}})
    writer.parts['test'].append(dict(path=part.name,rows=1024,bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
    if mode=='cancel':
        def stopped():raise InterruptedError('late standalone stop')
        writer.check=stopped
    monkeypatch.setattr(artifacts.pl,'read_parquet',lambda *a,**k:pytest.fail('standalone full decode before admission'))
    with pytest.raises((ValueError,InterruptedError)):list(writer.frames('test'))
