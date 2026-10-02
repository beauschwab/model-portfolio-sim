"""Read immutable source tables without hydrating a complete simulation result."""
import io
import json
import polars as pl
from . import persistence as db, store, worker


def _source(jid, kind):
    allowed={'ledger':{'balance_stress','saved_balance_stress','streamed_balance_stress'},
             'cashflows':{'cohort_analytics'}}
    row=db.REPO.job(jid)
    if row['status']!='done' or row['kind'] not in allowed[kind]:
        raise ValueError('source must be a completed compatible simulation/analytics job')
    ref=db.result_ref(jid)
    return row,ref,db.CODEC.tables(ref)


def _check():
    if worker.STOP.is_set(): raise InterruptedError('worker stopping')
    active=db.ACTIVE_JOB.get()
    if active is not None:
        row=db.REPO.job(active['id'])
        if row['status']!='running' or row['token']!=active['token'] or not db.REPO.owns(active['owner']):
            raise InterruptedError('treasury bridge cancelled or worker lease lost')


def _frames(ref):
    parts=ref['parts'] if ref.get('format')=='partitioned-parquet' else [{'ref':ref}]
    for part in parts:
        _check()
        frame=pl.read_parquet(io.BytesIO(db.CODEC.objects.get(part['ref'])))
        if 'rows' in part and frame.height!=part['rows']: raise ValueError('source partition row mismatch')
        if ref.get('format')=='partitioned-parquet' and {k:str(v) for k,v in frame.schema.items()}!=ref['schema']:
            raise ValueError('source partition schema mismatch')
        yield frame


def _table(tables,path,limit):
    if path not in tables: raise ValueError(f'source lacks {path}; rerun source analytics')
    ref=tables[path]
    if ref.get('rows',0)>limit: raise ValueError('source table exceeds bridge admission')
    frames=[];count=0
    for f in _frames(ref):
        count+=f.height
        if count>limit: raise ValueError('source table exceeds bridge admission')
        frames.append(f)
    return pl.concat(frames) if frames else pl.DataFrame(schema=ref.get('schema'))


def _metadata(ref,keys):
    manifest=json.loads(db.CODEC.objects.get(ref))
    return {k:db.CODEC.decode(v) for k,v in manifest['value']['items'] if k in keys}


def run(source_job,specification):
    from portfolio_risk.analytics.treasury import bridge
    kind=specification['kind'];row,ref,tables=_source(source_job,kind)
    store.report('Reading immutable source tables',.1)
    if kind=='ledger':
        trial=_table(tables,'/trial_balance',250000)
        closing=_table(tables,'/closing_statements',10000)
        currencies={r['account']:r['currency'] for r in closing.iter_rows(named=True)}
        for policy in specification['policies']:
            if currencies.get(policy['account'])!=policy['capital']['currency']:
                raise ValueError('capital policy currency must match ledger account')
        if '/journal' not in tables: raise ValueError('source has no journal to verify')
        out=bridge(specification,trial_balance=trial,journal_frames=_frames(tables['/journal']))
        import math
        expected={(r['scenario'],r['account']):r for r in closing.iter_rows(named=True)}
        if len(expected)!=out['capital_bridge'].height: raise ValueError('source closing scope mismatch')
        for r in out['capital_bridge'].iter_rows(named=True):
            c=expected[r['scenario'],r['account']]
            if any(not math.isclose(r[a],c[b]*specification['amount_multiplier'],rel_tol=1e-12,abs_tol=1e-9) for a,b in
                   [('assets','assets'),('liabilities','liabilities'),('book_equity','equity')]):
                raise ValueError('native bridge differs from verified closing statements')
    else:
        meta=_metadata(ref,{'asof','horizon','calculation_basis','source_sha256'})
        if str(meta.get('asof'))!=specification['treasury']['as_of']:
            raise ValueError('FTP curve/input date must match source accounting date; rerun old source analytics')
        if meta.get('horizon')!=specification['horizon']: raise ValueError('source horizon mismatch')
        out=bridge(specification,openings=_table(tables,'/openings',60000),cashflows=_table(tables,'/cashflows',60000))
        out['cashflow_source']=meta
    _check()
    out['source_receipt']=dict(job_id=source_job,kind=row['kind'],revision=row['revision'],result_sha256=ref['sha256'])
    out['revision']=store.current_state()['revision']
    store.report('Source-linked capital/FTP report complete',1.)
    return out


def template(source_job,kind):
    """Bounded read-only inventory; unfilled policy fields deliberately cannot run."""
    from portfolio_risk.analytics.treasury import source_template
    row,ref,tables=_source(source_job,kind)
    if kind=='ledger':
        # Small account scopes only; never read journal/position inventory in a handler.
        data=_table(tables,'/closing_statements',1000).to_dicts()
        spec=source_template(kind,data)
    else:
        meta=_metadata(ref,{'asof','horizon','calculation_basis'})
        if 'asof' not in meta: raise ValueError('rerun source analytics to retain source openings and date')
        data=_table(tables,'/openings',256).to_dicts()
        spec=source_template(kind,data,asof=str(meta['asof']),horizon=meta['horizon'],basis=meta['calculation_basis'])
    return dict(specification=spec,source_revision=row['revision'],revision=store.snapshot()['revision'],
        note='Fill null policy inputs. Regulatory eligibility, risk weights, curve calibration and residual tails are explicit assumptions.')
