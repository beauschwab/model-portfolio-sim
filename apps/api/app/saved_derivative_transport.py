"""Bounded immutable derivative source/partition importer and replay facade."""
from copy import deepcopy
import hashlib,io
import polars as pl
from .artifacts import PartitionedTable,ObjectReference
from portfolio_risk.analytics.saved_derivative_identity import loads,encoded,exact_identity,probe
from portfolio_risk.analytics.saved_derivative_transport import SCHEMAS,SCHEMA
from portfolio_risk.analytics.saved_derivative_mixed_reference import SCHEMAS_SUPPORTED
from portfolio_risk.analytics.saved_derivative_mixed_reference import verify_preparation,verify_stream
from .deposit_partition_bounds import parquet_metadata
MAX_BYTES=32*1024*1024
MAX_RETAINED=128*1024*1024
MAX_PARTS=65536

def same(a,b,label):
    if not exact_identity(a,b):raise ValueError(label)
def body(objects,ref,check,limit=MAX_BYTES):
    probe(check)
    if type(ref)not in(dict,ObjectReference)or set(ref)!={'key','sha256','bytes','format'}or type(ref['bytes'])is not int or not 0<ref['bytes']<=limit or ref['format']not in {'json','bin','parquet'}:raise ValueError('bounded derivative descriptor')
    data=objects.get_bounded(ref,limit);probe(check);return data

def metadata(codec,ref,names,check):
    manifest=loads(body(codec.objects,ref,check))
    if not exact_identity(manifest.get('schema_version'),1)or manifest.get('value',{}).get('type')!='dict':raise ValueError('derivative Codec metadata shape')
    seen=set();out={}
    def decode(node):
        probe(check)
        if type(node)is not dict:return node
        kind=node.get('type')
        if kind=='object-reference'and set(node)=={'type','ref'}:return ObjectReference(node['ref'])
        if kind in {'dict','list','tuple'}:
            if set(node)!={'type','items'}:raise ValueError('metadata node fields')
            if kind=='dict':
                keys=[pair[0]for pair in node['items']]
                if any(type(k)is not str for k in keys)or len(set(keys))!=len(keys):raise ValueError('metadata duplicate/non-string key')
                return {k:decode(v)for k,v in node['items']}
            out=[decode(v)for v in node['items']];return tuple(out)if kind=='tuple'else out
        raise ValueError('derivative metadata cannot hydrate tables/model objects')
    for key,value in manifest['value']['items']:
        if type(key)is not str or key in seen:raise ValueError('derivative manifest duplicate key')
        seen.add(key)
        if key in names:out[key]=decode(value)
    probe(check);return out

class StoredFrames:
    def __init__(self,codec,output,check):
        self.codec,self.check=codec,check;self.schemas=deepcopy(SCHEMAS);self.parts={};self.refs=[deepcopy(output)];self.bytes=0
        manifest=loads(body(codec.objects,output,check));same(manifest.get('schema_version'),1,'Codec version')
        node=manifest.get('value',{});same(node.get('type'),'dict','derivative result object required')
        keys=[pair[0]for pair in node.get('items',[])]
        if len(set(keys))!=len(keys)or set(keys)!=set(SCHEMAS)|{'execution','revision'}:raise ValueError('derivative persisted table inventory')
        selected=metadata(codec,output,{'execution','revision'},check);self.execution=selected['execution'];self.revision=selected['revision']
        for name,value in node['items']:
            if name not in SCHEMAS:continue
            if set(value)!={'type','value'}or value['type']!='partitioned':raise ValueError('bounded derivative partitions only')
            d=value['value'];schema={k:str(v)for k,v in SCHEMAS[name].items()}
            if set(d)!={'format','schema','rows','parts'}or d['format']!='partitioned-parquet':raise ValueError('derivative partition format')
            same(d['schema'],schema,'derivative partition schema');total=0
            if type(d['parts'])is not list:raise ValueError('derivative partition collection')
            for part in d['parts']:
                if set(part)!={'rows','ref'}or type(part['rows'])is not int or not 0<part['rows']<=self.execution['partition_rows']:raise ValueError('derivative part rows')
                self.refs.append(deepcopy(part['ref']));self.bytes+=part['ref']['bytes'];total+=part['rows']
                if len(self.refs)>MAX_PARTS or self.bytes>MAX_RETAINED:raise ValueError('derivative retained byte/part cap')
            same(d['rows'],total,'derivative partition row metadata');self.parts[name]=deepcopy(d['parts'])
        self._sealed=encoded(dict(parts=self.parts,execution=self.execution,revision=self.revision))
        self._refs=encoded(self.refs)
        self._schema_seal=encoded({k:{n:str(v)for n,v in d.items()}for k,d in self.schemas.items()})
    def _check(self):
        probe(self.check)
        if encoded({k:{n:str(v)for n,v in d.items()}for k,d in self.schemas.items()})!=self._schema_seal:raise ValueError('derivative returned schema rebased')
        if encoded(dict(parts=self.parts,execution=self.execution,revision=self.revision))!=self._sealed or encoded(self.refs)!=self._refs:raise ValueError('derivative returned facade rebased')
    @property
    def completion(self):
        self._check();return loads(body(self.codec.objects,self.execution['producer_completion'],self.check,128))
    def frames(self,name):
        if name not in SCHEMAS:raise ValueError('unknown derivative frame table')
        decoded=logical=0
        for part in self.parts[name]:
            self._check();data=body(self.codec.objects,part['ref'],self.check)
            decoded,logical=parquet_metadata(data,part['rows'],{k:str(v)for k,v in SCHEMAS[name].items()},decoded=decoded,logical=logical,check=lambda:probe(self.check))
            frame=pl.read_parquet(io.BytesIO(data),n_rows=part['rows']+1)
            if frame.height!=part['rows']or frame.schema!=SCHEMAS[name]:raise ValueError('derivative decoded partition schema/rows')
            self._check();yield frame
        self._check()
    def recheck(self,check=None):
        check=check or self.check;self._check()
        for ref in loads(self._refs):body(self.codec.objects,ref,check)
        for key in ('input','modeled_source','producer_completion'):body(self.codec.objects,self.execution[key],check,128 if key=='producer_completion'else MAX_BYTES)
        self._check()

def verify_persisted(codec,output,raw,check=lambda:False):
    facade=StoredFrames(codec,output,check);e=facade.execution
    if raw.get('schema')not in SCHEMAS_SUPPORTED or e['model_version']!=raw['schema'] or e['backend']!='rust' or type(e['partition_rows'])is not int or not 1<=e['partition_rows']<=65536:raise ValueError('persisted derivative runtime identity')
    input=loads(body(codec.objects,e['input'],check));same(input['request'],raw,'persisted original input changed')
    if set(input)!={'request','execution_timeout_ms'}or type(input['execution_timeout_ms'])is not int or not 0<input['execution_timeout_ms']<=1800000:raise ValueError('persisted derivative input envelope')
    header=loads(body(codec.objects,e['modeled_source'],check))
    if set(header)!={'schema','model_schema','prepared_request','source_receipt'}or header['schema']!='model-ledger-preparation-1'or header['model_schema']!=raw['schema']:raise ValueError('persisted derivative workflow')
    control=verify_preparation(raw,header['prepared_request'],header['source_receipt'],check)
    receipt=verify_stream(raw,control,facade,check);same(receipt,e['validation'],'persisted derivative receipt changed')
    facade.recheck();return facade,receipt

def import_manifest(codec,path,check):
    from pathlib import Path
    path=Path(path).resolve();m=loads(path.read_bytes(),1048576);root=path.parent
    if m.get('version')!='saved-derivative-partitions-1'or m.get('model_version')not in SCHEMAS_SUPPORTED or set(m['tables'])!=set(SCHEMAS):raise ValueError('derivative transport manifest')
    result={};total=0;count=0
    def read(d):
        nonlocal total
        probe(check);f=(root/d['path']).resolve()
        if f.parent!=root or type(d['bytes'])is not int or not 0<d['bytes']<=MAX_BYTES or f.stat().st_size!=d['bytes']:raise ValueError('derivative local descriptor bounds')
        total+=d['bytes']
        if total>MAX_RETAINED:raise ValueError('derivative aggregate retained transport bytes')
        data=f.read_bytes()
        if hashlib.sha256(data).hexdigest()!=d['sha256']:raise ValueError('derivative local checksum')
        probe(check);return data
    for name,schema in SCHEMAS.items():
        table=m['tables'][name];same(table['schema'],{k:str(v)for k,v in schema.items()},'native table schema')
        parts=[]
        for d in table['parts']:
            count+=1
            if count>MAX_PARTS or type(d['rows'])is not int or not 0<d['rows']<=m['partition_rows']:raise ValueError('derivative partition count/rows')
            data=read(d);probe(check);ref=codec.objects.put(data,'parquet');probe(check);parts.append(dict(rows=d['rows'],ref=ref))
        result[name]=PartitionedTable(table['schema'],parts)
    e={k:deepcopy(v)for k,v in m.items()if k!='tables'}
    for name in ('input','modeled_source','producer_completion'):
        data=read(m[name]);probe(check);e[name]=ObjectReference(codec.objects.put(data,'bin'));probe(check)
    result['execution']=e;return result
