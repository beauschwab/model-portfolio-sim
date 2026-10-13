"""Completed modeled holding-opening transport and independent evidence checks.

No financial generation, checkpoint construction or permission from receipt flags.
The new job's original budget covers every historical read and independent replay.
"""
from copy import deepcopy
from dataclasses import dataclass
import hashlib,json,math,time
from .completed_source_reader import Reader,descriptor,encoded,exact,probe

SCHEMA='commercial-holding-journal-1'
MAX_RAW=2*1024*1024
MAX_OBJECTS=1024
_SEAL=object()


def _row(row):
    if row['status']!='done'or row['kind']!='model_analysis'or type(row['revision'])is not int or row['revision']<0:
        raise ValueError('completed modeled holding-journal source required')
    if type(row.get('attempt'))is not int or row['attempt']<1 or any(type(row.get(k))is not str or not row[k]for k in ('owner','token')):
        raise ValueError('completed source attempt/owner identity required')
    for k in ('created_at','finished_at'):
        if type(row.get(k))is not float or not math.isfinite(row[k]):raise ValueError('prior completed source timestamps required')
    if row['finished_at']<row['created_at']:raise ValueError('prior completed source timestamp order')
    value={k:deepcopy(row[k])for k in ('id','kind','status','revision','attempt','owner','token','request','result','created_at','finished_at')}
    for k in ('request','result'):descriptor(value[k],'json')
    return value


def _queued(reader,binding):
    queued=reader.select(reader.manifest(binding['request']),{'operation','args','identity'})
    if queued.get('operation')!='store.run_model_analysis'or type(queued.get('args'))not in(list,tuple)or len(queued['args'])!=1 or type(queued.get('identity'))is not dict:
        raise ValueError('completed holding source operation differs')
    q=queued['args'][0]
    required={'schema','request','timeout','threads','binary_sha256','source_sha256','publication_deadline_unix'}
    if type(q)is not dict or set(q)!=required or q['schema']!=SCHEMA:
        raise ValueError('ordinary holding source required; recursive prior-publication chains unsupported')
    raw=q['request']
    if type(raw)is not dict or raw.get('schema')!=SCHEMA or len(encoded(raw))>MAX_RAW:
        raise ValueError('bounded compatible holding raw source required')
    if type(q['threads'])is not int or not 1<=q['threads']<=256 or type(q['timeout'])not in(int,float)or not math.isfinite(q['timeout'])or not .001<=q['timeout']<=1800.:
        raise ValueError('holding source thread/timeout domain')
    if type(q['publication_deadline_unix'])is not float or not math.isfinite(q['publication_deadline_unix']):
        raise ValueError('holding source original deadline provenance required')
    if binding['finished_at']>q['publication_deadline_unix']:
        raise ValueError('prior source was not completed within its original budget')
    for k in ('binary_sha256','source_sha256'):
        if type(q[k])is not str or len(q[k])!=64 or any(c not in '0123456789abcdef'for c in q[k]):
            raise ValueError('holding source executed identity required')
    return queued,q,raw


def capture(repo,codec,source_job,cut_ordinal,check):
    """Metadata-only server capture; source finance is replayed by the worker."""
    probe(check);binding=_row(repo.job(source_job));reader=Reader(codec,check)
    queued,q,raw=_queued(reader,binding)
    if type(cut_ordinal)is not int or cut_ordinal!=raw['cut_ordinal']:
        raise ValueError('exact published holding cut required')
    meta=reader.select(reader.manifest(binding['result']),{'execution','revision'})
    exact(meta.get('revision'),binding['revision'],'prior result revision differs')
    if meta.get('execution',{}).get('schema')!=SCHEMA or meta['execution'].get('backend')!='rust':
        raise ValueError('native modeled prior holding source required')
    exact(_row(repo.job(source_job)),binding,'prior source changed during admission');probe(check)
    return dict(binding=binding,raw=deepcopy(raw),queued_identity=deepcopy(queued['identity']))


def materialize(captured,revised_sources,cut_ordinal):
    raw=captured['raw']
    if type(cut_ordinal)is not int or cut_ordinal!=raw['cut_ordinal']:
        raise ValueError('exact published opening date required')
    ledger=deepcopy(raw['origin_ledger']);ledger['commercial']=deepcopy(raw['revised_sources'])
    return dict(schema=SCHEMA,cut_ordinal=cut_ordinal,origin_ledger=ledger,revised_sources=deepcopy(revised_sources))


def _prefix(native):
    cut=native['cut_day']
    return dict(origin_asof_ordinal=native['origin_asof_ordinal'],cut_ordinal=native['cut_ordinal'],cut_day=cut,
        opening=native['window_tables']['opening'],retained_prior_pnl=native['window_tables']['retained_prior_pnl'],
        journal=[r for r in native['full_tables']['journal']if r['day']<=cut])


@dataclass(frozen=True)
class VerifiedPrior:
    provenance:dict
    _prefix_bytes:bytes
    _recheck:object
    _seal:object

    def recheck(self,check=None,connection=None):
        if self._seal is not _SEAL:raise ValueError('private verified prior source required')
        self._recheck(check,connection)

    def match(self,native,check):
        self.recheck(check)
        exact(_prefix(native),json.loads(self._prefix_bytes),'published prior opening or completed postings differ')
        self.recheck(check)

    def receipt(self):
        return dict(schema='commercial-prior-opening-verification-1',modeled_prior_publication_verified=True,
            source_raw_and_journal_recomputed=True,exact_published_cut=True,completed_postings_and_opening_matched=True,
            observed_bank_reconciliation=False,cut_started_mixed_sim_adoption=False,empirical_validation=False,
            provenance=deepcopy(self.provenance))


def reopen(repo,codec,captured,check):
    """Historical deadline is provenance; replay runs under the NEW job check."""
    binding=deepcopy(captured['binding']);storage=codec.objects;last=[-math.inf]
    def current(force=False,callback=check,connection=None):
        probe(callback)
        if force or time.monotonic()-last[0]>=.1:
            if connection is None:row=repo.job(binding['id'])
            else:
                import sqlalchemy as sa
                from .repository import jobs
                row=connection.execute(sa.select(jobs).where(repo.scoped(jobs),jobs.c.id==binding['id'])).mappings().one()
            exact(_row(row),binding,'prior source SQL authority changed');last[0]=time.monotonic()
        probe(callback);return False
    def forced():return current(True)
    current(True);reader=Reader(codec,forced);queued,q,raw=_queued(reader,binding)
    exact(raw,captured['raw'],'captured prior raw source changed')
    exact(queued['identity'],captured['queued_identity'],'captured prior identity changed')
    saved=reader.select(reader.manifest(binding['result']),{'execution','revision','model_result'})
    exact(saved.get('revision'),binding['revision'],'prior result revision changed')
    e=saved['execution'];exact(e.get('request_manifest'),binding['request'],'prior request manifest changed')
    for k in ('schema','binary_sha256','source_sha256','publication_deadline_unix'):
        exact(e.get(k),q[k],'prior executed identity or original deadline differs')
    if e.get('backend')!='rust'or e.get('empirical_validation')is not False:
        raise ValueError('prior native modeled scope differs')
    ms=e.get('native_timeout_ms')
    if type(ms)is not int or not 1<=ms<=int(q['timeout']*1000):raise ValueError('prior native budget provenance differs')
    from .commercial_execution_envelope import retained_command
    digest=lambda x:hashlib.sha256(json.dumps(x,allow_nan=False,separators=(',',':')).encode()).hexdigest()
    exact(e.get('input_sha256'),digest(dict(schema=SCHEMA,threads=q['threads'],request=raw)),'prior raw envelope identity differs')
    executed=retained_command(SCHEMA,raw,q['timeout'],ms)
    exact(e.get('executed_input_sha256'),digest(executed),'prior executed envelope identity differs')
    # The decoder reads each bounded table through the shared reader. Source
    # finance is independently recomputed, never accepted from a stored flag.
    from .completed_source_reader import restore_tables
    native=restore_tables(saved['model_result'],e['table_layouts'])
    from .model_tables import format_sources
    _,layouts=format_sources(native)
    exact(layouts,e['table_layouts'],'prior source table layouts differ')
    from portfolio_risk.analytics.commercial_holding_reference import verify
    from .model_report_metadata import source_summary
    verification=verify(raw,native,current)
    exact(verification,e.get('source_verification'),'prior independent source receipt differs')
    exact(source_summary(native),e.get('summary'),'prior source summary differs')
    prefix_bytes=encoded(_prefix(native))
    if len(prefix_bytes)>32*1024*1024:raise ValueError('prior prefix admission exceeded')
    refs=list({encoded(ref):ref for ref in reader.refs}.values())
    if len(refs)>MAX_OBJECTS or sum(r['bytes']for r in refs)>64*1024*1024:
        raise ValueError('prior retained object admission exceeded')
    provenance=dict(binding=binding,queued_identity=queued['identity'],source_deadline_unix=q['publication_deadline_unix'],
        cut_ordinal=raw['cut_ordinal'],objects=refs)
    stamp=encoded(provenance)
    def recheck(callback=None,connection=None):
        callback=check if callback is None else callback
        current(True,callback,connection)
        if codec.objects is not storage:raise ValueError('prior source storage owner rebased')
        exact(verified.provenance,json.loads(stamp),'private prior evidence changed')
        for ref in refs:probe(callback);storage.get_bounded(ref,32*1024*1024);probe(callback)
        current(True,callback,connection)
    verified=VerifiedPrior(provenance,prefix_bytes,recheck,_SEAL)
    verified.recheck();return verified
