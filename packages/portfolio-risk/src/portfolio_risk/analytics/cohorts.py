"""Immutable tape/cohort lineage and explicit model adapters.

Rust owns grouping and weighted moments. Python transports tables and resolves
content identities. Cohort allocation is attribution, never individual repricing.
"""
from copy import deepcopy
import hashlib
import io
import json
import math

import polars as pl

VERSION = 'cohort-build-1'
MAX_BYTES = 32 * 1024 * 1024
PRODUCTS = ('mortgage', 'auto', 'personal', 'credit_card', 'deposit')
HARD = ('product', 'currency', 'entity', 'accounting_category', 'assumption_set')


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def presets():
    def d(field, edges=None):
        return dict(field=field, **({'edges': edges} if edges is not None else {}))
    score = d('fico', [620, 660, 700, 740, 780])
    age = d('age_months', [6, 12, 24, 60, 120])
    term = d('remaining_term_months', [12, 24, 36, 60, 120, 240, 360])
    rules = {
        'mortgage': dict(dimensions=[score, age, term, d('oltv', [.6, .8, .9, 1.]), d('state'), d('channel'), d('delinquency'), d('rate_type')],
            averages=['wac', 'net_coupon', 'remaining_term_months', 'age_months', 'oltv', 'factor', 'fico', 'price', 'hpi_orig_ratio', 'pay_delay_days']),
        'auto': dict(dimensions=[score, age, term, d('ltv', [.8, 1., 1.2]), d('new_used'), d('delinquency'), d('channel')],
            averages=['apr', 'remaining_term_months', 'age_months', 'fico', 'ltv']),
        'personal': dict(dimensions=[score, age, term, d('dti', [.2, .35, .5]), d('delinquency'), d('channel')],
            averages=['apr', 'remaining_term_months', 'age_months', 'fico', 'dti']),
        'credit_card': dict(dimensions=[score, age, d('utilization', [.1, .3, .6, .9]), d('payment_behavior'), d('delinquency'), d('promo_status')],
            averages=['apr', 'utilization', 'payment_rate', 'age_months', 'fico']),
        'deposit': dict(dimensions=[d('segment'), age, d('insured_status'), d('relationship'), d('rate_paid', [.005, .02, .04]), d('balance', [10000, 100000, 250000, 1000000])],
            averages=['age_months', 'rate_paid', 'price', 'svc_cost']),
    }
    return dict(version=VERSION, rules={k:deepcopy(v) for k,v in rules.items()}, column_map={}, defaults={}, assumptions={})


def read_tape(data, format):
    if not isinstance(data, bytes) or not 0 < len(data) <= MAX_BYTES:
        raise ValueError('tape must contain 1..32 MiB of CSV or Parquet')
    if format == 'csv':
        import csv
        header = next(csv.reader(io.StringIO(data.decode('utf-8-sig'))))
        if len(set(header)) != len(header): raise ValueError('duplicate CSV columns')
        # Preserve identifiers, including leading zeroes. Canonical numeric casts
        # happen only after the user supplies the column mapping.
        frame = pl.read_csv(io.BytesIO(data), infer_schema=False, n_rows=100001)
    elif format == 'parquet':
        frame = pl.read_parquet(io.BytesIO(data), n_rows=100001)
    else:
        raise ValueError('format must be csv or parquet')
    if not 1 <= len(frame) <= 100000 or frame.width > 128:
        raise ValueError('tape requires 1..100000 rows and at most 128 columns')
    return dict(frame=frame, sha256=hashlib.sha256(data).hexdigest(), format=format,
                rows=len(frame), columns=frame.columns)


def normalize(frame, config):
    if set(config) - {'version', 'rules', 'column_map', 'defaults', 'assumptions'}:
        raise ValueError('unknown cohort configuration field')
    if config.get('version') != VERSION: raise ValueError('unsupported cohort rule version')
    mapping = config.get('column_map', {})  # canonical -> source
    if any(source not in frame.columns for source in mapping.values()):
        raise ValueError('mapped source column is absent')
    if len(set(mapping.values())) != len(mapping): raise ValueError('source columns cannot be mapped twice')
    rows = frame.to_dicts()
    normalized = []
    for source in rows:
        row = source | {canonical: source[column] for canonical, column in mapping.items()}
        for key, value in config.get('defaults', {}).items():
            if row.get(key) in (None, ''): row[key] = value
        for field in ('loan_id', *HARD):
            value = row.get(field)
            if not isinstance(value, str) or not value.strip(): raise ValueError(f'{field} must be a nonempty string; use explicit defaults')
            row[field] = value.strip()
        product = row['product']
        if product not in PRODUCTS or product not in config['rules']: raise ValueError(f'no supported rule for {product}')
        rule = config['rules'][product]
        fields = {'balance', *rule['averages'], *(d['field'] for d in rule['dimensions'] if 'edges' in d)}
        for field in fields:
            value = row.get(field)
            if value in (None, ''):
                row[field] = None
                continue
            if isinstance(value, bool): raise ValueError(f'{field} cannot be boolean')
            row[field] = float(value)
            if not math.isfinite(row[field]): raise ValueError(f'{field} must be finite')
        # Only canonical fields used by the build cross the native boundary.
        retain = {'loan_id', *HARD, *fields, *(d['field'] for d in rule['dimensions'])}
        normalized.append({k: row.get(k) for k in sorted(retain)})
    return normalized


def build(tape, config, baseline=None):
    from ..core.quant_native import term_call
    config = deepcopy(config)
    rows = normalize(tape['frame'], config)
    identity = digest(dict(version=VERSION, source=tape['sha256'], config=config))
    result = term_call(VERSION, dict(rows=rows, rules=config['rules']))
    ids = {r['key']: 'cohort-' + digest(dict(rule=config['rules'][r['product']], key=json.loads(r['key']),
            assumptions=config.get('assumptions', {}).get(json.loads(r['key'])['assumption_set'], {}))) for r in result['cohorts']}
    cohorts = []
    for item in result['cohorts']:
        cohorts.append(dict(cohort_id=ids[item['key']], product=item['product'], balance=item['balance'],
            members=item['members'], grouping=item['key'], representative=json.dumps(item['means'], sort_keys=True)))
    lineage = pl.DataFrame([dict(loan_id=r['loan_id'], cohort_id=ids[r['key']], product=r['product'],
        balance=r['balance'], weight=r['weight'], build_id=identity, source_sha256=tape['sha256']) for r in result['lineage']])
    dispersion = pl.DataFrame([dict(cohort_id=ids[r['key']], **{k:v for k,v in r.items() if k!='key'}) for r in result['dispersion']],
        schema={'cohort_id':pl.String,'field':pl.String,'mean':pl.Float64,'stddev':pl.Float64,'min':pl.Float64,'max':pl.Float64})
    out = dict(build_id=identity, source_sha256=tape['sha256'], config=config, source=tape['frame'],
        normalized=pl.DataFrame(rows, infer_schema_length=None), cohorts=pl.DataFrame(cohorts), lineage=lineage, dispersion=dispersion,
        summary=dict(loans=len(rows), cohorts=len(cohorts), balance=lineage['balance'].sum(),
            compression=len(rows)/len(cohorts), backend='rust'),
        warnings=['Weighted representative instruments approximate nonlinear behavior. Dispersion is not a pricing-error estimate.',
            'Credit card, auto and personal cohorts require dedicated behavioral pricers before simulation publication.'])
    if baseline is not None:
        if baseline['source_sha256'] != tape['sha256']: raise ValueError('rule comparison requires the same immutable tape')
        old = baseline['lineage'].select('loan_id', pl.col('cohort_id').alias('old_cohort_id'))
        migration = lineage.join(old, on='loan_id', how='left', validate='1:1')
        if migration['old_cohort_id'].null_count(): raise ValueError('comparison loan populations differ')
        out['migration'] = migration
        out['comparison'] = dict(baseline_build_id=baseline['build_id'],
            cohorts_before=len(baseline['cohorts']), cohorts_after=len(cohorts),
            changed_members= migration.filter(pl.col('cohort_id')!=pl.col('old_cohort_id')).height,
            balance_difference=float(out['summary']['balance']-baseline['summary']['balance']))
    out['pricing_support'] = {}
    for product in sorted(set(lineage['product'])):
        try:
            position_books(out, products=[product])
            out['pricing_support'][product] = dict(supported=True, reason='Existing whole-loan mortgage or NMD model')
        except ValueError as exc:
            out['pricing_support'][product] = dict(supported=False, reason=str(exc))
    return out


def example():
    """Synthetic mixed-product tape; all units/defaults are explicit."""
    rows=[]
    for product in PRODUCTS:
        for i in range(4):
            rows.append(dict(loan_id=f'{product}-00{i}',product=product,balance=100000.+i*1000,
                currency='USD',entity='demo-bank',accounting_category='amortized_cost',assumption_set='research-base',
                fico=745.+i,age_months=25.+i,remaining_term_months=300. if product=='mortgage' else 48.,
                oltv=.75,factor=.9,wac=.055,net_coupon=.055,price=100.,hpi_orig_ratio=1.1,pay_delay_days=0.,
                state='NY',channel='R',delinquency='current',rate_type='fixed',apr=.12,
                ltv=.9,new_used='used',dti=.3,utilization=.45,payment_rate=.04,
                payment_behavior='revolver',promo_status='none',segment='SAV',insured_status='insured',
                relationship='primary',rate_paid=.02,svc_cost=.001))
    return pl.DataFrame(rows).write_csv()


def reconcile_refresh(previous, current):
    """Compare immutable populations by ID; balance movements are not cashflows.

    Missing IDs are labeled removed, never assumed paid off: the source may be
    incomplete. No journal postings or historical balances are changed here.
    """
    old = {r['loan_id']:r for r in previous['normalized'].to_dicts()}
    new = {r['loan_id']:r for r in current['normalized'].to_dicts()}
    old_groups = dict(previous['lineage'].select('loan_id','cohort_id').iter_rows())
    new_groups = dict(current['lineage'].select('loan_id','cohort_id').iter_rows())
    rows = []
    for ident in sorted(old.keys() | new.keys()):
        a,b = old.get(ident),new.get(ident)
        fields = sorted(k for k in ((a or {}).keys() | (b or {}).keys()) if k!='loan_id' and (a or {}).get(k)!=(b or {}).get(k))
        before,after = (a or {}).get('balance',0.),(b or {}).get('balance',0.)
        rows.append(dict(loan_id=ident, status='added' if a is None else 'removed' if b is None else 'changed' if fields else 'unchanged',
            balance_before=before, balance_after=after, balance_change=after-before,
            old_cohort_id=old_groups.get(ident), cohort_id=new_groups.get(ident),
            changed_fields=json.dumps(fields),
            classification_changed=bool(a and b and any(a[k]!=b[k] for k in HARD))))
    frame = pl.DataFrame(rows, infer_schema_length=None).with_columns(
        pl.col('old_cohort_id').cast(pl.String),pl.col('cohort_id').cast(pl.String))
    before = math.fsum(r['balance'] for r in old.values())
    after = math.fsum(r['balance'] for r in new.values())
    delta = math.fsum(r['balance_change'] for r in rows)
    if not math.isclose(before+delta,after,rel_tol=1e-12,abs_tol=1e-8):
        raise ArithmeticError('tape refresh balance reconciliation failed')
    return dict(previous_build_id=previous['build_id'], current_build_id=current['build_id'],
        balance_before=before,balance_after=after,balance_change=delta,
        counts={s:sum(r['status']==s for r in rows) for s in ('added','removed','changed','unchanged')},
        warning='Removed records require source reconciliation; balance changes are not inferred payments or journal entries.',
        rows=frame)


def _position(row, ident, balance, count, config):
    """Explicit adapters into existing whole-loan mortgage and NMD models."""
    product = row['product']
    if row['currency'] != 'USD': raise ValueError('the current pricing adapter requires USD; retain other currencies as cohorts until their market curves are configured')
    params = config.get('assumptions', {}).get(row['assumption_set'], {})
    if product == 'mortgage':
        required = ['wac','net_coupon','remaining_term_months','age_months','oltv','factor','fico','price','state','channel','hpi_orig_ratio','pay_delay_days']
        if row.get('rate_type') != 'fixed' or str(row.get('delinquency')) not in ('current','0'):
            raise ValueError('mortgage adapter supports current fixed-rate loans only')
        if params: raise ValueError('mortgage assumption overrides require model calibration; use observed tape features')
        for f in required:
            if row.get(f) is None: raise ValueError(f'mortgage pricing requires {f} in dimensions or averages')
        result = {k:row[k] for k in required if k not in ('remaining_term_months','age_months')}
        if not 0 < row['factor'] <= 1 or not 0 < row['oltv'] <= 3 or not 0 < row['price'] <= 300:
            raise ValueError('invalid mortgage factor, LTV or price; rates and ratios use decimals')
        if not 1 <= row['remaining_term_months'] <= 480 or row['age_months'] < 0 or not 0 <= row['pay_delay_days'] <= 90:
            raise ValueError('invalid mortgage term, age or delay')
        if not .0001 <= row['wac'] <= .5 or not 0 <= row['net_coupon'] <= row['wac'] or not 300 <= row['fico'] <= 850 or not .01 <= row['hpi_orig_ratio'] <= 100:
            raise ValueError('invalid mortgage coupon, FICO or HPI ratio')
        result.update(cusip=ident, current_face=balance, original_face=balance/row['factor'],
            wam=int(round(row['remaining_term_months'])), age=int(round(row['age_months'])), avg_loan_size=balance/count)
        if row.get('book_yield') is not None:
            value=float(row['book_yield'])
            if not math.isfinite(value) or not -.5 <= value <= 1.: raise ValueError('invalid book_yield')
            result['book_yield']=value
        return 'mbs', result
    if product == 'deposit':
        allowed = {'attrition_base','attrition_amp','attrition_slope','attrition_gap'}
        if set(params)-allowed: raise ValueError('unsupported deposit behavioral parameter')
        from .whatif import FIELDS
        for name,value in params.items():
            lo,hi=FIELDS['deposits'][name]
            if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value) or not lo<=value<=hi:
                raise ValueError(f'invalid deposit parameter {name}')
        if row.get('segment') not in ('DDA','NOW','SAV','MMDA'): raise ValueError('invalid deposit segment')
        fields = ['segment','age_months','rate_paid','price','svc_cost']
        for f in fields:
            if row.get(f) is None: raise ValueError(f'deposit pricing requires {f} in dimensions or averages')
        if row['age_months'] < 0 or not 0 <= row['rate_paid'] <= 1 or not 0 < row['price'] <= 300 or not 0 <= row['svc_cost'] <= 1:
            raise ValueError('invalid deposit age, rate, price or servicing cost')
        return 'deposits', dict(id=ident, balance=balance, avg_account_size=balance/count, **{k:row[k] for k in fields}, **params)
    raise ValueError(f'{product} has no dedicated behavioral pricer; cohort construction and lineage are available')


def position_books(build_result, *, products, loan_ids=None):
    """Rebuild from immutable rows for exact drilldown; never fan out averages."""
    if not products or set(products)-set(PRODUCTS): raise ValueError('select known products explicitly')
    positions = {}
    if loan_ids is not None:
        if not loan_ids or len(set(loan_ids)) != len(loan_ids) or len(loan_ids)>256: raise ValueError('select 1..256 distinct loans')
        rows = build_result['normalized'].filter(pl.col('loan_id').is_in(loan_ids)).to_dicts()
        if len(rows)!=len(loan_ids): raise ValueError('unknown loan_id')
        for row in rows:
            if row['product'] not in products: raise ValueError('loan is outside selected products')
            book, pos = _position(row, 'HL-' + row['loan_id'] if row['product']=='mortgage' else row['loan_id'], row['balance'], 1, build_result['config'])
            pos.update({key:row[key] for key in HARD}, source_loan_id=row['loan_id'], cohort_build_id=build_result['build_id'])
            positions.setdefault(book, []).append(pos)
    else:
        for c in build_result['cohorts'].filter(pl.col('product').is_in(products)).to_dicts():
            keys = json.loads(c['grouping'])
            row = {k:v for k,v in keys.items() if not isinstance(v, dict)} | json.loads(c['representative'])
            ident = ('HL-' if c['product']=='mortgage' else '') + c['cohort_id']
            book, pos = _position(row, ident, c['balance'], c['members'], build_result['config'])
            pos.update({key:row[key] for key in HARD}, cohort_id=c['cohort_id'], cohort_build_id=build_result['build_id'])
            positions.setdefault(book, []).append(pos)
    if not positions: raise ValueError('selected products have no positions')
    return {book:pl.DataFrame(rows, infer_schema_length=None) for book,rows in positions.items()}


def allocate(build_result, cohort_results, *, metrics, dimensions=()):
    """Balance-proportional attribution of ADDITIVE dollars, not OAS/duration/ratios.

    Explicit additive field allowlist prevents silently allocating intensive risk.
    Repeated cohort rows require dimensions (e.g. month/scenario) to be unique.
    """
    allowed = {'market_value','dv01','nii','principal','cash_interest','accrual_interest','book_amortization'}
    if not metrics or set(metrics)-allowed: raise ValueError('only additive dollar metrics may be allocated')
    if len(set(metrics))!=len(metrics) or len(set(dimensions))!=len(dimensions) or set(dimensions)&({'loan_id','cohort_id','weight','balance',*metrics}):
        raise ValueError('metrics and dimensions must be unique and nonoverlapping')
    keys = ['cohort_id', *dimensions]
    if cohort_results.select(keys).is_duplicated().any(): raise ValueError('duplicate cohort result key')
    known = set(build_result['cohorts']['cohort_id'])
    if not set(cohort_results['cohort_id']) <= known: raise ValueError('result references unknown cohort')
    expansion = cohort_results.select('cohort_id').join(build_result['cohorts'].select('cohort_id','members'), on='cohort_id')['members'].sum()
    if expansion>250000: raise ValueError('allocation exceeds 250000 output rows; allocate a smaller result partition')
    for field in metrics:
        if cohort_results[field].null_count() or not cohort_results[field].is_finite().all(): raise ValueError('nonfinite cohort metric')
    result = build_result['lineage'].join(cohort_results.select(*keys, *metrics), on='cohort_id', how='inner')
    return result.with_columns(*[(pl.col(m)*pl.col('weight')).alias(m) for m in metrics],
        pl.lit('allocated_cohort_result').alias('calculation_basis'))


def merge_positions(book, retained, incoming):
    """Preserve implicit existing model defaults when concatenating schemas."""
    from ..core.config import HPI_MU
    if book=='mbs':
        frames=[]
        for frame in (retained,incoming):
            if 'hpi_orig_ratio' not in frame.columns:
                frame=frame.with_columns(((1.+HPI_MU)**(pl.col('age')/12.)).alias('hpi_orig_ratio'))
            if 'pay_delay_days' not in frame.columns:
                frame=frame.with_columns(pl.lit(0.).alias('pay_delay_days'))
            frames.append(frame)
        retained,incoming=frames
        # A book-wide historical-yield column changes the accounting basis.
        # Never infer historical carrying yields from a current market quote.
        if len(retained) and ('book_yield' in retained.columns) != ('book_yield' in incoming.columns):
            raise ValueError('selective mortgage publication requires a consistent accounting basis: supply explicit book_yield in tape averages when the saved book has historical yields')
    return pl.concat([retained,incoming],how='diagonal_relaxed')
