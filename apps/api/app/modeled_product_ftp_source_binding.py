"""Explicit completed Card2/deposit source transport. No finance or native calls.

The saved receipt is evidence to reproduce, never a permission grant. Current
independent raw-source/journal verification is mandatory under the NEW job check.
"""
from copy import deepcopy
from dataclasses import dataclass
import hashlib,io,json,math,time
import polars as pl
from .completed_source_reader import Reader,probe,exact,encoded,loads,descriptor,restore_tables as _restore
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
from portfolio_risk.analytics.modeled_product_ftp_identity import mixed_clones

KINDS={'card_v2':('model_analysis','store.run_model_analysis','product-mixed-product-credit-1'),
       'deposit_performing':('modeled_balance_stress','store.run_modeled_balance_stress','deposit-performing-income-ledger-1'),
       'deposit_portfolio':('modeled_balance_stress','store.run_modeled_balance_stress','deposit-performing-portfolio-ledger-2')}
LIMIT=32*1024*1024
MAX_RAW=1024*1024
MAX_PARTS=4096
_SEAL=object()

def _row(row,kind):
    if kind not in KINDS or row['status']!='done' or row['kind']!=KINDS[kind][0] or type(row['revision'])is not int or row['revision']<0:raise ValueError('compatible completed tagged FTP source job required')
    if type(row.get('attempt'))is not int or row['attempt']<1 or any(type(row.get(k))is not str or not row[k]for k in ('owner','token')):raise ValueError('completed source durable attempt/owner identity required')
    out={k:deepcopy(row[k])for k in ('id','kind','status','revision','attempt','owner','token','request','result')}
    for k in ('request','result'):descriptor(out[k],'json')
    return out

def _queued(reader,binding,kind):
    queued=reader.select(reader.manifest(binding['request']),{'operation','args','identity'})
    if queued.get('operation')!=KINDS[kind][1]or type(queued.get('args'))not in(list,tuple)or len(queued['args'])!=1 or type(queued['args'][0])is not dict or type(queued.get('identity'))is not dict:raise ValueError('completed source queued operation mismatch')
    q=queued['args'][0];raw=q.get('request')if kind=='card_v2'else q.get('specification')
    if kind=='card_v2':
        if type(q.get('timeout'))not in(int,float)or not math.isfinite(q['timeout'])or not .001<=q['timeout']<=1800.:raise ValueError('source original bounded timeout domain')
    else:
        d=q.get('publication_deadline_unix')
        if type(d)is not float or not math.isfinite(d):raise ValueError('source original accepted deadline provenance domain')
        if type(q.get('large_book'))is not bool:raise ValueError('source large-book identity domain')
    if type(raw)is not dict or raw.get('schema')!=KINDS[kind][2] or len(encoded(raw))>MAX_RAW:raise ValueError('compatible explicit raw source schema required')
    if kind=='card_v2':
        paths=[p for o in raw.get('product_owners',[])for r in o['runs']for p in r['paths']]
        if not paths or any(p['request'].get('kind')!='card_v2'for p in paths):raise ValueError('Card2 selection requires every explicit product owner path card_v2')
    return queued,q,raw


def capture(repo,codec,source_job,kind,check):
    """Server-only bounded metadata capture; journal replay occurs in the worker."""
    probe(check);binding=_row(repo.job(source_job),kind);probe(check)
    reader=Reader(codec,check);queued,q,raw=_queued(reader,binding,kind)
    metadata=reader.select(reader.manifest(binding['result']),{'execution','revision'})
    exact(metadata.get('revision'),binding['revision'],'completed source revision differs')
    e=metadata.get('execution',{});model=e.get('schema')if kind=='card_v2'else e.get('model_version')
    if model!=KINDS[kind][2]or e.get('backend')!='rust':raise ValueError('completed source executed model incompatible')
    probe(check)
    return dict(kind=kind,binding=binding,raw=deepcopy(raw),queued_identity=deepcopy(queued['identity']))

class StoredFrames:
    def __init__(self,reader,tables,completion,bound,kind='deposit_performing'):
        if kind=='deposit_portfolio':
            from portfolio_risk.analytics.deposit_portfolio_transport import SCHEMAS
        else:
            from portfolio_risk.analytics.deposit_continuation_transport import SCHEMAS
        self.reader=reader;self.schemas={k:{n:str(t)for n,t in v.items()}for k,v in SCHEMAS.items()};self.parts={};self._completion=completion
        if set(tables)!=set(SCHEMAS)or type(bound)is not int or not 1<=bound<=65536:raise ValueError('performing deposit table coverage/bound')
        count=size=0
        for name,schema in self.schemas.items():
            table=tables[name]
            if type(table)is not dict or table.get('format')!='partitioned-parquet' or not exact_json_identity(table.get('schema'),schema)or type(table.get('rows'))is not int or table['rows']<0 or type(table.get('parts'))is not list:raise ValueError('source partition descriptor')
            rows=0
            for p in table['parts']:
                if type(p.get('rows'))is not int or not 1<=p['rows']<=bound:raise ValueError('source partition row bound')
                descriptor(p.get('ref'),'parquet');count+=1;size+=p['ref']['bytes'];rows+=p['rows']
                if count>MAX_PARTS or size>LIMIT:raise ValueError('source retained partition admission')
            if rows!=table['rows']:raise ValueError('source partition coverage')
            self.parts[name]=deepcopy(table['parts'])
        self.decoded=self.logical=0
    @property
    def completion(self):return loads(self.reader.body(self._completion,limit=128))
    def frames(self,name):
        from portfolio_risk.analytics._deposit_continuation_partition_bounds import parquet_metadata
        for part in self.parts[name]:
            data=self.reader.body(part['ref'],'parquet')
            self.decoded,self.logical=parquet_metadata(data,part['rows'],self.schemas[name],decoded=self.decoded,logical=self.logical,check=self.reader.check)
            frame=pl.read_parquet(io.BytesIO(data),n_rows=part['rows']+1)
            if frame.height!=part['rows']or {k:str(v)for k,v in frame.schema.items()}!=self.schemas[name]:raise ValueError('source partition cells/schema')
            yield frame

@dataclass(frozen=True)
class VerifiedSource:
    raw:dict
    provenance:dict
    verification:dict
    native:dict
    _seal:object
    _recheck:object
    def recheck(self,check=None,conn=None):
        if self._seal is not _SEAL:raise ValueError('private completed source required')
        self._recheck(check,conn)
    def match(self,ftp_output,source_id,check):
        self.recheck(check)
        records=[s for s in ftp_output['result']['source_preparation']['sources']if s['source_id']==source_id]
        if len(records)!=1:raise ValueError('native selected source coverage differs')
        actual=records[0]['raw_native_result']
        if self.provenance['kind']=='card_v2':
            mixed_clones(self.raw,actual)
            from portfolio_risk.analytics.product_rights_reference import _check_equal
            _check_equal(actual,self.native,'completed-source',check)
        else:
            # Both native source/ordered financial streams are independently checked.
            from portfolio_risk.analytics._deposit_continuation_income_reference import equal
            equal(actual['prepared_request'],self.native['prepared_request'],'source-prepared')
            from portfolio_risk.analytics._modeled_product_dto_identity import deposit_owner,deposit_portfolio
            normalize=deposit_portfolio if self.provenance['kind']=='deposit_portfolio'else deposit_owner
            expected=deepcopy(self.native['source_receipt'])
            # Selected standalone header retained the original Value, while the
            # nested native source compiler serializes its declared source1 DTO.
            # Each clone is exact against its separate raw/typed authority.
            exact(expected['raw_request'],self.raw,'standalone retained original deposit Value differs')
            expected['raw_request']=normalize(self.raw)
            exact(actual['native']['raw_request'],expected['raw_request'],'nested deposit DTO clone differs')
            equal(actual['native'],expected,'source-receipt')
            # The FTP coordinator supplies its independently declared typed DTO
            # to the nested compiler; the standalone completed job retained its
            # original Value. Their original-vs-DTO raw clones stay separately
            # exact, while computed output equality uses the unchanged gate.
            from portfolio_risk.analytics.modeled_product_ftp_source_reference import checked_source
            checked_source(dict(input=dict(kind='deposit_portfolio'if self.provenance['kind']=='deposit_portfolio'else 'deposit_continuation',request=self.raw)),actual,check)
        self.recheck(check)

def reopen(repo,codec,captured,check):
    """No old source deadline resets: every read/replay uses the new job check."""
    captured=deepcopy(captured);kind=captured['kind'];binding=captured['binding'];last=[-math.inf]
    def current(force=False,callback=check,conn=None):
        probe(callback)
        if force or time.monotonic()-last[0]>=.1:
            if conn is None:row=repo.job(binding['id'])
            else:
                import sqlalchemy as sa
                from .repository import jobs
                row=conn.execute(sa.select(jobs).where(repo.scoped(jobs),jobs.c.id==binding['id'])).mappings().one()
            exact(_row(row,kind),binding,'completed source job/ref changed');last[0]=time.monotonic()
        probe(callback);return False
    def forced():return current(True)
    current(True);reader=Reader(codec,forced);queued,q,raw=_queued(reader,binding,kind)
    exact(raw,captured['raw'],'selected queued raw changed');exact(queued['identity'],captured['queued_identity'],'selected source identity changed')
    result=reader.select(reader.manifest(binding['result']),{'execution','revision','model_result'}if kind=='card_v2'else {'execution','revision'} )
    exact(result.get('revision'),binding['revision'],'source result revision changed')
    e=result['execution'];exact(e.get('request_manifest'),binding['request'],'source request manifest changed')
    deadline=e.get('publication_deadline_unix')
    if type(deadline)not in(int,float)or not math.isfinite(deadline):raise ValueError('finite completed source deadline provenance required')
    for k in ('binary_sha256','source_sha256'):
        if type(q.get(k))is not str or len(q[k])!=64:raise ValueError('source executed identity absent')
        exact(e.get(k),q[k],'completed source executed identity changed')
    if e.get('backend')!='rust':raise ValueError('Rust completed source required')
    if kind=='card_v2':
        exact(e.get('schema'),KINDS[kind][2],'Card2 source execution schema')
        if type(q.get('threads'))is not int or not 1<=q['threads']<=256:raise ValueError('source threads domain')
        envelope=dict(schema=q['schema'],threads=q['threads'],request=raw)
        digest=lambda value:hashlib.sha256(json.dumps(value,allow_nan=False,separators=(',',':')).encode()).hexdigest()
        exact(e.get('input_sha256'),digest(envelope),'Card2 source input bytes identity')
        from portfolio_risk.core.product_credit_mixed_native import command
        exact(e.get('executed_input_sha256'),digest(command(raw,timeout=q['timeout'])),'Card2 executed command identity')
        native=_restore(result['model_result'],e['table_layouts'])
        exact(native.get('complete'),True,'Card2 source incomplete');exact(native.get('protocol'),3,'Card2 protocol differs')
        mixed_clones(raw,native)
        from portfolio_risk.analytics.product_rights_reference import verify
        verification=verify(raw,native,current)
        exact(verification,e.get('source_verification'),'Card2 independent source receipt differs')
        exact(e.get('summary'),{k:v for k,v in native.items()if v is None or isinstance(v,(str,int,float,bool))or isinstance(v,dict)and all(x is None or isinstance(x,(str,int,float,bool))for x in v.values())},'Card2 summary differs')
    else:
        exact(e.get('model_version'),KINDS[kind][2],'performing deposit source schema')
        exact(deadline,q['publication_deadline_unix'],'deposit original accepted source deadline differs')
        exact(e.get('validation_library_sha256'),q.get('validation_library_sha256'),'deposit validation identity differs')
        envelope=loads(reader.body(e['input'],limit=MAX_RAW));exact(envelope.get('request'),raw,'deposit retained original input differs')
        if set(envelope)!={'request','execution_timeout_ms'}or type(envelope['execution_timeout_ms'])is not int or not 1<=envelope['execution_timeout_ms']<=1800000:raise ValueError('deposit source envelope domain')
        header=loads(reader.body(e['modeled_source']))
        if set(header)!={'schema','model_schema','prepared_request','source_receipt'}or header['schema']!=('deposit-performing-portfolio-workflow-2'if kind=='deposit_portfolio'else 'model-ledger-preparation-1')or header['model_schema']!=KINDS[kind][2]:raise ValueError('deposit retained header differs')
        top=reader.manifest(binding['result']);names={pair[0]for pair in top['items']}-{'execution','revision'}
        tables=reader.select(top,names)
        frames=StoredFrames(reader,tables,e['producer_completion'],e['partition_rows'],kind)
        if kind=='deposit_portfolio':
            from portfolio_risk.analytics.deposit_portfolio_reference import verify_preparation,verify_stream
        else:
            from portfolio_risk.analytics.deposit_continuation_reference import verify_preparation,verify_stream
        control=verify_preparation(raw,header['prepared_request'],header['source_receipt'],current)
        verification=verify_stream(raw,control,frames,current);exact(verification,e.get('validation'),'deposit independent receipt differs')
        native=header
    probe(check)
    refs=[deepcopy(binding['request']),deepcopy(binding['result'])]+reader.refs
    # Deduplicate immutable object descriptors only; do not deduplicate economic sources.
    retained=list({encoded(ref):ref for ref in refs}.values())
    if len(retained)>MAX_PARTS+4 or sum(r['bytes']for r in retained)>2*LIMIT:raise ValueError('completed source retained object admission')
    provenance=dict(kind=kind,binding=binding,queued_identity=queued['identity'],source_deadline_unix=deadline,objects=retained)
    snapshots=dict(raw=deepcopy(raw),provenance=deepcopy(provenance),verification=deepcopy(verification),native=deepcopy(native))
    storage=codec.objects
    def recheck(callback=None,conn=None):
        callback=check if callback is None else callback
        current(True,callback,conn)
        if codec.objects is not storage:raise ValueError('source storage owner rebased')
        for k,want in snapshots.items():exact(getattr(verified,k),want,'verified source snapshot rebased')
        for ref in retained:
            probe(callback);storage.get_bounded(ref,LIMIT);probe(callback)
        current(True,callback,conn)
    verified=VerifiedSource(deepcopy(raw),provenance,verification,native,_SEAL,recheck)
    verified.recheck();return verified
