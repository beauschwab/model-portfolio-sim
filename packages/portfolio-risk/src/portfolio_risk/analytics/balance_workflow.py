"""Saved-book adapter, candidate replay and held-out outcome diagnostics.

Product engines retain ownership of expected cashflows. The adapter makes their
monthly resolution explicit (30-day reporting months); it does not invent daily
coupon dates, legal classifications, credit calibration or regulatory mappings.
"""
from copy import deepcopy
import hashlib
import json
import math
import polars as pl

from .balance_stress import run_balance_stress, validate, Position, _decode

BOOKS = ('mbs', 'loans', 'debt', 'deposits', 'cds', 'mm')
REQUIRED = {'account', 'kind', 'classification', 'risk_weight', 'asf_weight',
            'rsf_weight', 'lcr_outflow_weight', 'hqla_weight'}


def inventory(bs):
    return [{'source_id': f'{book}:{sid}', 'book': book, 'id': str(sid)}
            for book in BOOKS if bs.get(book) is not None
            for sid in bs[book]['cusip' if book == 'mbs' else 'id'].to_list()]


def check_mapping(bs, mapping):
    expected = {r['source_id'] for r in inventory(bs)}
    if set(mapping) != expected:
        raise ValueError(f'incomplete saved-book mapping: missing={sorted(expected-set(mapping))}, extra={sorted(set(mapping)-expected)}')
    for sid, values in mapping.items():
        if not isinstance(values, dict):
            raise ValueError(f'{sid}: mapping must be an object')
        missing = REQUIRED-set(values)
        if missing:
            raise ValueError(f'{sid}: missing explicit accounting/risk mappings {sorted(missing)}')
        if {'id', 'source_id', 'balance', 'book_adjustment', 'start_day', 'opening_market_price'} & set(values):
            raise ValueError('mapping cannot override engine identity, principal, basis or start date')
        _decode(Position, values | dict(id=sid, source_id=sid, balance=0.))
    # Dealer contracts need trade/netting-set valuation and settlement adapters.
    # Never silently discard these from a supposedly complete saved-book run.
    hedges = bs.get('hedges')
    if hedges is not None and any(len(frame) for frame in hedges if frame is not None):
        raise ValueError('saved hedge trades require a trade-to-netting-set cashflow adapter; use explicit netting sets until available')


def from_accounting(specification, accounting, mapping, amount_scale):
    from ..core.quant_native import enabled, term_call
    if enabled():
        spec = term_call('ledger-map-1', dict(specification=specification, mapping=mapping, amount_scale=amount_scale,
            openings=accounting['instrument_openings'].to_dicts(),flows=accounting['instrument_cashflows'].to_dicts(),allocation=None,library=None))
        validate(spec)
        return spec
    if not math.isfinite(amount_scale) or amount_scale <= 0:
        raise ValueError('amount_scale must be a positive conversion from book units to stress units')
    spec = deepcopy(specification)
    if spec.get('positions') or spec.get('cashflows'):
        raise ValueError('saved-book specification must not also supply positions or cashflows')
    spec['positions'], spec['cashflows'] = [], []
    for r in accounting['instrument_openings'].to_dicts():
        sid = f"{r['book']}:{r['id']}"
        meta = deepcopy(mapping[sid])
        if REQUIRED-set(meta):
            raise ValueError(f'missing explicit risk mapping: {sid}')
        if {'id', 'source_id', 'balance', 'book_adjustment', 'start_day', 'opening_market_price'} & set(meta):
            raise ValueError('mapping cannot override engine identity, principal, basis or start date')
        asset = meta['kind'] in {'loan', 'security', 'reverse_repo'}
        if asset != (r['side'] == 'asset'):
            raise ValueError(f'side mismatch: {sid}')
        if meta['kind'] == 'security' and meta['classification'] in {'afs', 'trading'} and 'market_price' not in r:
            raise ValueError(f'marked security requires an explicit engine opening quote: {sid}')
        spec['positions'].append(meta | dict(id=sid, source_id=sid,
            balance=r['balance']*amount_scale, book_adjustment=r['book_adjustment']*amount_scale,
            opening_market_price=r.get('market_price', 1.) if meta['kind'] == 'security' else 1.))
    for r in accounting['instrument_cashflows'].to_dicts():
        spec['cashflows'].append(dict(position=f"{r['book']}:{r['id']}", day=30*r['month'],
            **{k: r[k]*amount_scale for k in ('principal', 'cash_interest', 'accrual_interest', 'book_amortization')}))
    spec['provenance'] = dict(adapter='saved-book-monthly-v1', amount_scale=amount_scale,
                              timing='30-day reporting months; expected cashflows conditional on surviving principal',
                              scenario_cashflows='base product paths plus explicit daily stress overlays',
                              calibrated=False)
    validate(spec)
    return spec


def add_candidate(specification, allocation, library, template_mapping, amount_scale=1.):
    """Originate exact-grid unit contracts and replay their cash/accrual schedules.

    Unit-library deterministic forward coupons and shifted monthly cashflows are
    inherited, not relabeled as fully repriced forward contracts.
    """
    from ..core.quant_native import enabled, term_call
    if enabled():
        raw_library=dict(units=[{k:u[k] for k in ('template','h','side')} for u in library['units']],horizon=library['horizon'],
            **{k:library[k].tolist() for k in ('runoff','cash_interest','nii')})
        spec=term_call('ledger-map-1',dict(specification=specification,mapping=template_mapping,amount_scale=amount_scale,
            openings=None,flows=None,allocation=allocation,library=raw_library))
        validate(spec)
        return spec
    spec = deepcopy(specification)
    spec.setdefault('positions', [])
    spec.setdefault('cashflows', [])
    if len(allocation) > 2000:
        raise ValueError('too many candidate legs')
    seen = set()
    for i, leg in enumerate(allocation):
        if set(leg) != {'template', 'purchase_m', 'notional'}:
            raise ValueError('candidate requires template, purchase_m and notional')
        name, month, amount = leg['template'], leg['purchase_m'], leg['notional']
        if type(month) is not int or month < 0 or not isinstance(amount, (int, float)) or isinstance(amount, bool) or not math.isfinite(amount) or amount <= 0:
            raise ValueError('invalid candidate month or notional')
        if (name, month) in seen:
            raise ValueError('duplicate candidate leg')
        seen.add((name, month))
        match = [j for j, u in enumerate(library['units']) if u['template'] == name and u['h'] == month]
        if len(match) != 1 or name not in template_mapping:
            raise ValueError('candidate requires an exact unit grid and explicit template mapping')
        j = match[0]
        meta = deepcopy(template_mapping[name])
        if REQUIRED-set(meta) or {'id', 'source_id', 'balance', 'start_day', 'book_adjustment', 'opening_accrued'} & set(meta):
            raise ValueError('incomplete or conflicting template accounting mapping')
        if (meta['kind'] in {'loan', 'security', 'reverse_repo'}) != (library['units'][j]['side'] > 0):
            raise ValueError('candidate template side mismatch')
        sid = f'candidate:{i}:{name}@{month}'
        notional = amount*amount_scale
        spec['positions'].append(meta | dict(id=sid, source_id=sid, balance=notional, start_day=month*30+1))
        for m in range(library['horizon']-month):
            spec['cashflows'].append(dict(position=sid, day=(month+m+1)*30,
                principal=float(library['runoff'][j, m])*notional,
                cash_interest=float(library['cash_interest'][j, m])*notional,
                accrual_interest=float(library['nii'][j, m])*notional))
    validate(spec)
    return spec


def backtest(path, observations):
    """Compare independently supplied realized observations; never fit to holdout.

    An observation identifies scenario/account/day/metric and realized value.
    Coverage is reported; a small or synthetic holdout is not model validation.
    """
    rows, seen = [], set()
    indexed = {(r['scenario'], r['account'], r['day']): r for r in path.to_dicts()}
    for obs in observations:
        if set(obs) != {'scenario', 'account', 'day', 'metric', 'actual', 'source'} or not obs['source']:
            raise ValueError('observations require dated metric, actual and source provenance')
        key = (obs['scenario'], obs['account'], obs['day'])
        metric = obs['metric']
        unique = (*key, metric)
        if unique in seen or key not in indexed or metric not in {'cash', 'equity', 'assets', 'liabilities', 'cet1', 'rwa'}:
            raise ValueError('duplicate, missing or unsupported backtest observation')
        seen.add(unique)
        actual = obs['actual']
        if isinstance(actual, bool) or not isinstance(actual, (int, float)) or not math.isfinite(actual):
            raise ValueError('backtest actual must be finite')
        predicted = indexed[key][metric]
        rows.append(dict(obs, predicted=predicted, error=predicted-actual, absolute_error=abs(predicted-actual)))
    return pl.DataFrame(rows, schema={'scenario': pl.String, 'account': pl.String, 'day': pl.Int64,
        'metric': pl.String, 'actual': pl.Float64, 'source': pl.String,
        'predicted': pl.Float64, 'error': pl.Float64, 'absolute_error': pl.Float64})


def run_saved_book_stress(bs, swap_rates, vol_pts, dep_hist, request, *, seed=7, asof=None, progress=None):
    from .accounting import run_balance_sheet_nii
    from ..strategy.unitlib import build_unit_library
    mapping, specification = request['position_mapping'], request['specification']
    from ..core.quant_native import enabled
    allocation = request.get('allocation', [])
    if enabled():
        import tempfile
        from .owned_workflow import run_saved_workflow
        from .balance_stream import load_streamed_result
        from .balance_stress import _result_metadata, validation_status
        days=specification.get('horizon_days',360)
        if type(days) is not int or not 30<=days<=1080 or days%30:
            raise ValueError('saved-book horizon must be 30-day reporting months in [30,1080]')
        if len(inventory(bs))*days//30>250000:
            raise ValueError('saved-book cashflow budget exceeded; use streamed workflow output')
        if bs.get('hedges') is not None and any(len(f) for f in bs['hedges'] if f is not None):
            raise ValueError('saved hedge trades require a trade-to-netting-set cashflow adapter')
        with tempfile.TemporaryDirectory(prefix='native-saved-book-') as directory:
            path=run_saved_workflow(bs,swap_rates,vol_pts,dep_hist,request,directory,seed=seed,asof=asof,
                progress=(lambda stage:progress(stage,50)) if progress else None,materialize_specification=True)
            result=load_streamed_result(path)
        manifest=result.pop('manifest');spec=manifest['workflow']['specification']
        _result_metadata(result,spec);result['validation']=validation_status(result)
        result['execution']=dict(financial_events='rust',orchestration='rust',native_pilot=False,
            journal_backend='rust-state',journal_identity=manifest['binary_sha256'],partition_validation=manifest['validation'])
        fingerprint=hashlib.sha256(json.dumps(spec,sort_keys=True,allow_nan=False).encode()).hexdigest()
    else:
        check_mapping(bs, mapping)
        days = specification.get('horizon_days', 360)
        if type(days) is not int or not 30 <= days <= 1080 or days % 30:
            raise ValueError('saved-book horizon must be 30-day reporting months in [30,1080]')
        if len(inventory(bs))*days//30 > 250000:
            raise ValueError('saved-book cashflow budget exceeded; reduce horizon or aggregate cohorts explicitly')
        if progress:
            progress('Build product cashflow schedules', 5)
        accounting = run_balance_sheet_nii(bs, swap_rates, vol_pts, dep_hist, horizon=days//30,
                                          seed=seed, asof=asof, capture_cashflows=True)
        spec = from_accounting(specification, accounting, mapping, request['amount_scale'])
        allocation = request.get('allocation', [])
        if allocation:
            library = build_unit_library(swap_rates, vol_pts, bs['mbs_hists'], dep_hist,
                grid_m=sorted({r['purchase_m'] for r in allocation}), horizon=days//30,
                template_names=sorted({r['template'] for r in allocation}), seed=seed, asof=asof)
            spec = add_candidate(spec, allocation, library, request.get('template_mapping', {}), request['amount_scale'])
        fingerprint = hashlib.sha256(json.dumps(spec, sort_keys=True, allow_nan=False).encode()).hexdigest()
        result = run_balance_stress(spec, progress=progress)
    result['source'] = dict(specification_sha256=fingerprint, adapter='saved-book-monthly-v1',
                             allocation=deepcopy(allocation), amount_scale=request['amount_scale'])
    result['backtest'] = backtest(result['path'], request.get('observations', []))
    result['validation']['observed_points'] = result['backtest'].height
    calibration = request.get('calibration')
    if calibration is not None:
        import datetime as dt
        from .balance_calibration import fit_joint_drivers, empirical_joint_scenarios
        if set(calibration) != {'history', 'training_end', 'source'} or not isinstance(calibration['history'], list) or len(calibration['history']) > 100000:
            raise ValueError('calibration requires bounded history, training_end and source')
        history = pl.DataFrame(calibration['history']).with_columns(pl.col('date').str.to_date())
        cutoff = dt.date.fromisoformat(calibration['training_end'])
        result['calibration'] = fit_joint_drivers(history, cutoff, calibration['source'])
        result['suggested_scenarios'] = empirical_joint_scenarios(history, cutoff)
        result['driver_history'] = history
    result['warnings'][1] = 'Saved books mapped explicitly. Cashflow timing is monthly; stress overlays use proportional survival and base expected product cashflows.'
    return result
