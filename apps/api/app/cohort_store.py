"""Tape IO, durable cohort jobs and revision-checked position publication."""
import base64
import os
import json
from pathlib import Path
from urllib.parse import urlparse

from . import store, persistence as db


def result(jid, kind):
    if db.REPO is None: raise RuntimeError('Tape workflows require durable SQLite or PostgreSQL storage')
    row = db.REPO.job(jid)
    if row['kind'] != kind: raise ValueError(f'expected a completed {kind} job')
    return db.CODEC.load(db.result_ref(jid))


def metadata(jid):
    if db.REPO.job(jid)['kind'] != 'cohort_build': raise ValueError('expected cohort build')
    manifest = json.loads(db.CODEC.objects.get(db.result_ref(jid)))
    wanted = {'build_id','source_sha256','config','summary','warnings','revision','tape_id','comparison','pricing_support','refresh_summary'}
    return {key:db.CODEC.decode(value) for key,value in manifest['value']['items'] if key in wanted}


def import_tape(request):
    from portfolio_risk.analytics.cohorts import read_tape, MAX_BYTES
    source = request.get('uri')
    metadata = {}
    if source:
        parsed = urlparse(source)
        if parsed.scheme == 's3':
            allowed = urlparse(os.environ.get('WORKBENCH_TAPE_S3_PREFIX', ''))
            prefix = allowed.path.strip('/')
            key = parsed.path.lstrip('/')
            if allowed.scheme != 's3' or parsed.netloc != allowed.netloc or not prefix or not key.startswith(prefix + '/') or parsed.query or parsed.fragment:
                raise ValueError('S3 tape URI is outside WORKBENCH_TAPE_S3_PREFIX')
            import boto3
            obj = boto3.client('s3', endpoint_url=os.getenv('S3_ENDPOINT_URL')).get_object(Bucket=parsed.netloc, Key=key)
            with obj['Body'] as body:
                if obj['ContentLength'] > MAX_BYTES: raise ValueError('tape exceeds 32 MiB')
                raw = body.read(MAX_BYTES+1)
            metadata = {'etag': obj.get('ETag'), 'version_id': obj.get('VersionId')}
        elif parsed.scheme in ('', 'file') or (os.name=='nt' and Path(source).drive):
            root = os.environ.get('WORKBENCH_TAPE_ROOT')
            if not root: raise ValueError('Configure WORKBENCH_TAPE_ROOT before reading server files')
            from urllib.parse import unquote
            local = unquote(parsed.path) if parsed.scheme=='file' else source
            if os.name=='nt' and local.startswith('/') and len(local)>3 and local[2]==':': local=local[1:]
            path = Path(local).resolve()
            if not path.is_relative_to(Path(root).resolve()): raise ValueError('tape is outside WORKBENCH_TAPE_ROOT')
            with path.open('rb') as body: raw = body.read(MAX_BYTES+1)
        else:
            raise ValueError('Only configured local files or S3 sources are supported')
    else:
        raw = base64.b64decode(request['content_base64'], validate=True)
        source = request['name']
    tape = read_tape(raw, request['format'])
    tape.update(source=source, source_metadata=metadata, source_bytes=raw)
    return tape


def build(tape_id, config, baseline_job=None, previous_job=None):
    from portfolio_risk.analytics.cohorts import build as compile_cohorts
    state = store.current_state()
    tape = state['tapes'][tape_id]
    baseline = result(baseline_job, 'cohort_build') if baseline_job else None
    output = compile_cohorts(tape, config, baseline)
    output.update(tape_id=tape_id, revision=state['revision'], asof=state['asof'], source_uri=tape['source'])
    if previous_job:
        from portfolio_risk.analytics.cohorts import reconcile_refresh
        previous = result(previous_job, 'cohort_build')
        if previous['tape_id'] != tape_id: raise ValueError('refresh comparison requires the same tape name')
        refresh = reconcile_refresh(previous, output)
        output['refresh'] = refresh.pop('rows')
        output['refresh_summary'] = refresh
    return output


def analytics(build_job, products, loan_ids=None):
    """Heavy raw-loan/representative pricing always executes in the worker."""
    from portfolio_risk.analytics.cohorts import position_books
    from portfolio_risk.analytics.incremental import price_books
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.core.runtime import RunConfig, run_context
    from portfolio_risk.core.dependency import DependencyCache
    import polars as pl
    saved = result(build_job, 'cohort_build')
    state = store.current_state()
    if saved['asof'] != state['asof']: raise ValueError('valuation date changed; rebuild the tape')
    books = position_books(saved, products=products, loan_ids=loan_ids)
    count = sum(len(f) for f in books.values())
    if count > 256: raise ValueError('interactive cohort analytics is bounded to 256 positions; publish larger books for normal batch runs')
    settings = state['settings']
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
        state['assumptions'].get('deposit_segments'), compute_backend='rust')
    sr, vp = state['market']['swap_rates'], state['market']['vol_pts']
    with run_context(config):
        risk = price_books(books, asof=state['asof'], swap_rates=sr, vol_pts=vp, config=config, cache=DependencyCache(),
            seed=settings.seed, mbs_hists=state['mbs_hists'], dep_hist=state['dep_hist'], include_analytics=True)
        income = run_balance_sheet_nii(books | {'asof':state['asof'], 'mbs_hists':state['mbs_hists']},
            sr, vp, state['dep_hist'], horizon=settings.horizon_months, seed=settings.seed,
            asof=state['asof'], capture_cashflows=True)
    return dict(build_id=saved['build_id'], revision=state['revision'],
        calculation_basis='individual_model_reprice' if loan_ids else 'representative_cohort_reprice',
        loan_ids=loan_ids, positions=risk['positions'],
        risk=pl.concat([v.with_columns(pl.lit(k).alias('book')) for k,v in risk['positions'].items()],how='diagonal_relaxed'),
        cashflows=income['instrument_cashflows'], openings=income['instrument_openings'],
        asof=state['asof'], horizon=settings.horizon_months,
        nii=income['monthly'], kpis=risk['kpis'], source_sha256=saved['source_sha256'])


def attribution(build_job, analytics_job):
    from portfolio_risk.analytics.cohorts import allocate
    import polars as pl
    saved=result(build_job, 'cohort_build')
    values=result(analytics_job, 'cohort_analytics')
    if values['build_id']!=saved['build_id'] or values['calculation_basis']!='representative_cohort_reprice':
        raise ValueError('allocation requires representative results from this exact build')
    risk=[]
    for frame in values['positions'].values():
        risk.append(frame.select(pl.col('id').str.strip_prefix('HL-').alias('cohort_id'), 'market_value','dv01',pl.col('nii_total').alias('nii')))
    flows=values['cashflows'].with_columns(pl.col('id').str.strip_prefix('HL-').alias('cohort_id'))
    return dict(build_id=saved['build_id'], analytics_job=analytics_job, calculation_basis='allocated_cohort_result',
        positions=allocate(saved,pl.concat(risk),metrics=['market_value','dv01','nii']),
        cashflows=allocate(saved,flows,metrics=['principal','cash_interest','accrual_interest','book_amortization'],dimensions=['month']))


def audit(build_job, products, options):
    """Persist audit chunks as immutable Parquet; never collect all loan flows."""
    import io
    import polars as pl
    from portfolio_risk.analytics.cohort_validation import audit as validate
    from portfolio_risk.core.runtime import RunConfig
    from .artifacts import PartitionedTable
    from . import worker
    saved = result(build_job, 'cohort_build')
    state = store.current_state()
    if saved['asof'] != state['asof']: raise ValueError('valuation date changed; rebuild the tape')
    settings = state['settings']
    config = RunConfig(settings.n_paths, settings.n_paths_base, settings.horizon_months,
        state['assumptions'].get('deposit_segments'), compute_backend='rust')
    tables = {'suggestions':PartitionedTable({'cohort_id':'String','product':'String','field':'String',
        'proposed_edge':'Float64','reason':'String'}, [])}
    buffers, buffered_rows = {}, {}
    job = db.ACTIVE_JOB.get()
    def cancelled():
        if worker.STOP.is_set(): return True
        if job is None: return False
        row = db.REPO.job(job['id'])
        return (row['status'] != 'running' or row['token'] != job['token'] or
                row['owner'] != job['owner'] or not db.REPO.owns(job['owner']))
    def flush(name):
        if not buffered_rows.get(name): return
        if cancelled(): raise InterruptedError('cohort audit cancelled')
        frame=pl.concat(buffers.pop(name));buffered_rows[name]=0
        buffer=io.BytesIO();frame.write_parquet(buffer,compression='zstd')
        tables[name].parts.append({'rows':len(frame),'ref':db.CODEC.objects.put(buffer.getvalue(),'parquet')})
    def emit(name, frame):
        if 'relative_error' in frame.columns:
            frame = frame.with_columns(pl.col('relative_error').cast(pl.Float64))
        schema = {k:str(v) for k,v in frame.schema.items()}
        if name not in tables: tables[name] = PartitionedTable(schema, [])
        if schema != tables[name].schema: raise ValueError('audit partition schema changed')
        for part in frame.iter_slices(65536):
            if cancelled(): raise InterruptedError('cohort audit cancelled')
            if buffered_rows.get(name,0)+len(part)>65536: flush(name)
            buffers.setdefault(name,[]).append(part)
            buffered_rows[name]=buffered_rows.get(name,0)+len(part)
            if buffered_rows[name]==65536: flush(name)
    summary = validate(saved, products=products, asof=state['asof'],
        swap_rates=state['market']['swap_rates'], vol_pts=state['market']['vol_pts'],
        config=config, mbs_hists=state['mbs_hists'], dep_hist=state['dep_hist'],
        seed=settings.seed, cancelled=cancelled, emit=emit,
        progress=lambda checks, failed:store.report(stage=f'Audited {checks} values; {failed} outside tolerance'), **options)
    for name in tables: flush(name)
    return dict(build_id=saved['build_id'], source_sha256=saved['source_sha256'],
        revision=state['revision'], summary=summary, **tables)


def adopt(tape_id, import_job, revision):
    tape = result(import_job, 'tape_import')
    with store._LOCK:
        db.refresh()
        if store.STATE_META['revision'] != revision: raise db.Conflict('inputs changed; reload before adopting tape')
        store.TAPES[tape_id] = tape
        store.changed()
    return {'tape_id':tape_id, 'revision':store.STATE_META['revision'], 'sha256':tape['sha256']}


def publish(build_job, products, revision, mode='replace_books'):
    from portfolio_risk.analytics.cohorts import position_books
    from .books import normalize_book
    output = result(build_job, 'cohort_build')
    if mode not in ('replace_books','replace_tape'): raise ValueError('unknown publication mode')
    books = position_books(output, products=products)
    with store._LOCK:
        db.refresh()
        if revision != store.STATE_META['revision'] or output['revision'] != revision:
            raise db.Conflict('inputs changed since cohort build; rebuild before publishing positions')
        if store.SETTINGS.compute_backend!='rust' and any(any(c.startswith('attrition_') for c in f.columns) for f in books.values()):
            raise ValueError('select the Rust simulation backend before publishing deposit attrition overrides')
        # Validate all books before mutating the snapshot. The source and build
        # receipt stay attached to the same revision as their generated positions.
        import polars as pl
        validated = {}
        for name, frame in books.items():
            import hashlib
            ident = 'cusip' if name=='mbs' else 'id'
            # Equivalent cohort keys on different tapes are separate positions.
            # The canonical cohort_id remains an explicit lineage column.
            namespace = hashlib.sha256(output['tape_id'].encode()).hexdigest()[:16]
            frame = frame.with_columns((pl.col(ident)+'@'+namespace).alias(ident))
            frame = frame.with_columns(pl.lit(output['tape_id']).alias('source_tape_id'))
            if mode == 'replace_tape':
                old = store.BOOKS[name]
                if 'source_tape_id' in old.columns:
                    old = old.filter((pl.col('source_tape_id') != output['tape_id']).fill_null(True))
                receipt = store.COHORT_PUBLICATIONS.get(name,{})
                if receipt.get('tape_id') == output['tape_id'] and 'cohort_build_id' in old.columns:
                    legacy = pl.col('cohort_build_id') == receipt['build_id']
                    if 'source_tape_id' in old.columns: legacy = legacy & pl.col('source_tape_id').is_null()
                    old = old.filter(~legacy.fill_null(False))
                if set(old[ident]) & set(frame[ident]): raise ValueError('cohort position identity collides with another source')
                from portfolio_risk.analytics.cohorts import merge_positions
                frame = merge_positions(name,old,frame)
            validated[name] = normalize_book(name,frame.to_dicts(),store.BOOKS[name],store.ASOF)
        store.BOOKS.update(validated)
        store.COHORT_PUBLICATIONS.update({name:dict(job_id=build_job, build_id=output['build_id'],
            tape_id=output['tape_id'], source_sha256=output['source_sha256'], config=output['config'], mode=mode) for name in books})
        store.changed()
    return {'revision':store.STATE_META['revision'], 'books':{k:len(v) for k,v in books.items()}}
