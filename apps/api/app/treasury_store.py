"""Read immutable source tables without hydrating a complete simulation result."""
import io
import json
import polars as pl
from . import persistence as db, store, worker


def _source(jid, kind):
    allowed={'ledger':{'balance_stress','saved_balance_stress','streamed_balance_stress','modeled_balance_stress','model_analysis'},
             'credit_ledger':{'credit_balance_stress','credit_path_balance_stress','modeled_balance_stress'},
             'cashflows':{'cohort_analytics'}}
    row=db.REPO.job(jid)
    if row['status']!='done' or row['kind'] not in allowed[kind]:
        raise ValueError('source must be a completed compatible simulation/analytics job')
    ref=db.result_ref(jid)
    tables=db.CODEC.tables(ref)
    if row['kind']=='model_analysis':
        _mixed_protocol(ref)
        prefix='/model_result/'
        tables={path.removeprefix('/model_result'):value for path,value in tables.items() if path.startswith(prefix)}
        if not {'/journal','/trial_balance','/closing_statements'}<=set(tables):
            raise ValueError('mixed dated source lacks independently verified closing tables')
    if row['kind']=='modeled_balance_stress':
        model=_metadata(ref,{'execution'})['execution'].get('model_version')
        expected_models={'driven-credit-ledger-1'} if kind=='credit_ledger' else {'graph-dated-ledger-1','deposit-graph-dated-ledger-1','deposit-graph-income-ledger-1'}
        if kind not in {'ledger','credit_ledger'} or model not in expected_models:raise ValueError('incompatible modeled source capital boundary')

    if kind=='credit_ledger':
        expected=5 if row['kind'] in {'credit_path_balance_stress','modeled_balance_stress'} else 4
        if _credit_protocol(ref)!=expected:
            raise ValueError('credit source job/publication protocol mismatch')
    return row,ref,tables


def _mixed_protocol(ref):
    """Only explicitly source-verified mixed contracts supply closing ledger cash."""
    execution=_metadata(ref,{'execution'})['execution']
    receipt=execution.get('source_verification',{})
    summary=execution.get('summary',{})
    card_rights=receipt.get('card_v2_rights_owned') is True
    if card_rights and (receipt.get('closing_capital_eligible') is not True or
                        summary.get('closing_capital_eligible') is not True):
        raise ValueError('card interest-basis source requires its reviewed closing capital mapping gate')
    if not card_rights and 'capital_ftp_eligible' in summary and summary['capital_ftp_eligible'] is not True:
        raise ValueError('mixed source has not passed capital and FTP eligibility gates')
    ownership={'product-mixed-dated-1':'mixed_performing_only',
               'product-mixed-consumer-credit-1':'mixed_consumer_credit_only',
               'product-mixed-product-credit-1':'mixed_product_credit_only'}
    required=('verified',ownership.get(execution.get('schema'),'unsupported_source'),
              'ordered_mixed_journal_recomputed','complete_cash_rights_basis_tail_verified')
    if (execution.get('schema') not in ownership or execution.get('backend')!='rust' or
            type(summary.get('protocol')) is not int or summary.get('protocol')!=3 or summary.get('complete') is not True or
            any(receipt.get(key) is not True for key in required)):
        raise ValueError('mixed dated capital source requires its independent persisted source gate')
    ordinal=summary.get('full_end_ordinal')
    if type(ordinal) is not int or not 1<=ordinal<=3652059:
        raise ValueError('mixed dated capital source requires its exact closing date')
    return ordinal


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
    return db.CODEC.metadata(ref,keys)


def _credit_protocol(ref):
    """Admit only a retained independent verified physical-credit publication."""
    execution=_metadata(ref,{'execution'})['execution']
    version=execution.get('version');validation=execution.get('validation',{})
    if version=='credit-balance-partitions-1':
        protocol=4;required=('credit_source_reconciled','journal_replayed')
    elif version=='credit-path-balance-partitions-1':
        protocol=5;required=('credit_path_source_reconciled','journal_replayed','physical_paths_recomputed')
    else:raise ValueError('unsupported credit source publication version')
    if any(validation.get(key) is not True for key in required):
        raise ValueError('credit source must retain independent verified publication')
    return protocol


def _credit_closing(ref,tables):
    """Select immutable source closing rows; no capital arithmetic in this adapter."""
    protocol=_credit_protocol(ref)
    latest={}
    for frame in _frames(tables['/daily_balance']):
        for row in frame.filter(pl.col('severity')==1.).iter_rows(named=True):
            key=(row['scenario'],row['account'])
            previous=latest.get(key)
            if previous and (previous['run_id']!=row['run_id'] or row['day']<=previous['day']):
                raise ValueError('credit closing scopes must have one ordered severity-one run')
            latest[key]=row
            if len(latest)>10000:raise ValueError('credit closing source exceeds bridge scope admission')
    if not latest:raise ValueError('credit source lacks severity-one closing rows')
    return pl.DataFrame([{k:r[k] for k in ('scenario','account','entity','currency','assets','equity')}|
        {'liabilities':r['liabilities'] if protocol==5 else 0.} for r in latest.values()])


def run(source_job,specification):
    from portfolio_risk.analytics.treasury import bridge
    kind=specification['kind'];row,ref,tables=_source(source_job,kind)
    store.report('Reading immutable source tables',.1)
    if kind in {'ledger','credit_ledger'}:
        trial=_table(tables,'/trial_balance',250000)
        closing=_credit_closing(ref,tables) if kind=='credit_ledger' else _table(tables,'/closing_statements',10000)
        currencies={r['account']:r['currency'] for r in closing.iter_rows(named=True)}
        for policy in specification['policies']:
            if currencies.get(policy['account'])!=policy['capital']['currency']:
                raise ValueError('capital policy currency must match ledger account')
        if '/journal' not in tables: raise ValueError('source has no journal to verify')
        mixed=row['kind']=='model_analysis'
        if mixed:
            from datetime import date
            if specification['treasury']['as_of']!=date.fromordinal(_mixed_protocol(ref)).isoformat():
                raise ValueError('capital report date must match mixed dated closing date')
        out=bridge(specification,trial_balance=trial,journal_frames=_frames(tables['/journal']),
            credit_protocol=_credit_protocol(ref) if kind=='credit_ledger' else 4,
            ledger_journal_protocol=3 if mixed else 2)
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
    if row['kind']=='model_analysis':
        out['source_receipt'].update(model_schema=_metadata(ref,{'execution'})['execution']['schema'],
            closing_ordinal=_mixed_protocol(ref),basis='complete_tail_expected_closing-1')
    out['revision']=store.current_state()['revision']
    store.report('Source-linked capital/FTP report complete',1.)
    return out


def template(source_job,kind):
    """Bounded read-only inventory; unfilled policy fields deliberately cannot run."""
    from portfolio_risk.analytics.treasury import source_template
    row,ref,tables=_source(source_job,kind)
    if kind in {'ledger','credit_ledger'}:
        # Small account scopes only; never read journal/position inventory in a handler.
        data=(_credit_closing(ref,tables) if kind=='credit_ledger' else _table(tables,'/closing_statements',1000)).to_dicts()
        asof=None
        if row['kind']=='model_analysis':
            from datetime import date
            asof=date.fromordinal(_mixed_protocol(ref)).isoformat()
        spec=source_template(kind,data,asof=asof)
    else:
        meta=_metadata(ref,{'asof','horizon','calculation_basis'})
        if 'asof' not in meta: raise ValueError('rerun source analytics to retain source openings and date')
        data=_table(tables,'/openings',256).to_dicts()
        spec=source_template(kind,data,asof=str(meta['asof']),horizon=meta['horizon'],basis=meta['calculation_basis'])
    note='Fill null policy inputs. Regulatory eligibility, risk weights, curve calibration and residual tails are explicit assumptions.'
    if row['kind']=='model_analysis':
        note+=' This source is expected capital at the complete source-tail closing date shown above. Report-date capital, inferred regulatory eligibility and modeled FTP require separate adoption.'
    return dict(specification=spec,source_revision=row['revision'],revision=store.snapshot()['revision'],note=note)
