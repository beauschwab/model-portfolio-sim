"""Validated, temporary instrument assumptions. Never mutates saved contracts."""
from __future__ import annotations

import math
import polars as pl

# Bounds are engine input domains, not empirical calibration recommendations.
FIELDS = {
    'mbs': {'wac': (0.0001, .5), 'net_coupon': (0., .5), 'wam': (1., 359.),
            'age': (0., 600.), 'oltv': (.01, 2.), 'factor': (.001, 1.),
            'fico': (300., 850.), 'avg_loan_size': (1., 1e8), 'hpi_orig_ratio': (.01, 100.)},
    'loans': {'coupon_or_spread': (-.1, .5), 'cap': (-.1, 1.), 'floor': (-.1, 1.),
              'call_threshold': (0., .5)},
    'debt': {'coupon_or_spread': (-.1, .5), 'cap': (-.1, 1.), 'floor': (-.1, 1.),
             'call_threshold': (0., .5)},
    'cds': {'rate': (0., .5), 'penalty_months': (0., 120.), 'ew_mult': (0., 10.),
            'call_threshold': (0., .5)},
    'deposits': {'rate_paid': (0., .5), 'age_months': (0., 600.),
                 'avg_account_size': (1., 1e10), 'svc_cost': (0., .1),
                 'attrition_base': (0., 1.), 'attrition_amp': (0., 1.),
                 'attrition_slope': (0., 1000.), 'attrition_gap': (0., 1.)},
}
DEFAULTS = {'cap': 10., 'floor': -10., 'call_threshold': .005, 'ew_mult': 1., 'svc_cost': 0.}


def apply_overrides(books, overrides):
    from ..core.quant_native import enabled,term_call
    if enabled():
        from ..core.config import HPI_MU
        columns=term_call("whatif-overrides-1",dict(books={k:v.select([c for c in v.columns if c in FIELDS.get(k,{}) or c in ("id","cusip")]).to_dicts() for k,v in books.items()},overrides=overrides,hpi_mu=HPI_MU))
        return {k:frame.with_columns([pl.Series(field,values,dtype=pl.Float64) for field,values in columns.get(k,{}).items()]) for k,frame in books.items()}
    if set(overrides) - set(books):
        raise ValueError('assumption overrides must refer to selected books')
    result = dict(books)
    for book, changes in overrides.items():
        frame = books[book]
        id_col = 'cusip' if book == 'mbs' else 'id'
        ids = frame[id_col].to_list()
        if set(changes) - set(ids):
            raise ValueError(f'{book}: unknown instrument in assumption overrides')
        fields = set()
        for ident, patch in changes.items():
            for field, value in patch.items():
                if field not in FIELDS[book]:
                    raise ValueError(f'{book}: unsupported assumption {field}')
                lo, hi = FIELDS[book][field]
                if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not lo <= value <= hi:
                    raise ValueError(f'{book}/{ident}: {field} must be finite in [{lo}, {hi}]')
                if field == 'wam' and value != int(value):
                    raise ValueError('wam must be an integer number of months')
                fields.add(field)
        # Resolve derived HPI defaults after all age edits, independent of
        # dict/set iteration order when a request edits several instruments.
        for field in sorted(fields, key=lambda f: (f == 'hpi_orig_ratio', f)):
            values = frame[field].to_list() if field in frame.columns else [DEFAULTS.get(field)] * len(frame)
            for i, ident in enumerate(ids):
                if field in changes.get(ident, {}):
                    values[i] = float(changes[ident][field])
            # Null virtual fields mean use that row's model default. A missing
            # HPI ratio retains the original age-derived default for other rows.
            if field == 'hpi_orig_ratio':
                from ..core.config import HPI_MU
                values = [(1 + HPI_MU) ** (float(frame['age'][i]) / 12) if v is None else v
                          for i, v in enumerate(values)]
            frame = frame.with_columns(pl.Series(field, values, dtype=pl.Float64))
        if book in ('loans', 'debt') and 'floor' in frame.columns and 'cap' in frame.columns:
            if (frame['floor'] > frame['cap']).any():
                raise ValueError('coupon floor cannot exceed cap')
        result[book] = frame
    return result


def compare_books(books, *, assumption_overrides=None, calibration_mode='hold', **kwargs):
    """Baseline and revised valuations under identical numerical conventions.

    Recalibration creates a new comparison baseline for the modified contracts;
    it does not save the modified contracts or alter application input state.
    """
    from .incremental import price_books
    if calibration_mode not in ('hold', 'recalibrate'):
        raise ValueError('unknown calibration mode')
    from ..core.runtime import RunConfig, run_context
    kwargs['config'] = kwargs.get('config') or RunConfig()
    if kwargs['config'].compute_backend=='rust' and kwargs.get('backend') is None:
        from ..core.graph_native import compare_books as native_compare
        return native_compare(books,assumption_overrides=assumption_overrides or {},calibration_mode=calibration_mode,**kwargs)
    with run_context(kwargs.get("config")):
        revised = apply_overrides(books, assumption_overrides or {})
    baseline_args = {k: v for k, v in kwargs.items() if k not in ('scenario_market', 'spread_shift', 'spread_overrides_bp')}
    baseline = price_books(books, **baseline_args)
    candidate = price_books(revised if calibration_mode == 'recalibrate' else books,
                            valuation_books=revised, **kwargs)
    differences = {}
    for book, after in candidate['positions'].items():
        before = baseline['positions'][book]
        differences[book] = after.with_columns([
            before['model_price'].alias('original_price'), before['market_value'].alias('original_value'),
            (after['market_value'] - before['market_value']).alias('value_change'),
            (after['model_price'] - before['model_price']).alias('price_change'),
        ])
    return {'baseline': baseline, 'revised': candidate, 'comparison': differences,
            'calibration_mode': calibration_mode,
            'net_value_change': candidate['scope_net_value'] - baseline['scope_net_value'],
            'assumption_overrides': assumption_overrides or {}}
