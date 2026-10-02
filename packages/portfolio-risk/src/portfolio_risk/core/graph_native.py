"""Opaque native graph ownership and raw table transport; no pricing callbacks."""
import weakref
from functools import partial

import numpy as np
import polars as pl

from .quant_native import term_call, _load
from .native import library_path
from .runtime import run_context
from .config import SWAP_TENORS


class NativeGraph:
    def __init__(self, max_bytes, max_entries):
        self._path=library_path()
        self._library=_load(str(self._path))[0]
        self._check_identity()
        self._request=partial(term_call, _library=self._library)
        self.handle = self._request('graph-1', dict(op='create', max_bytes=max_bytes,
                                               max_entries=max_entries))['handle']
        self._finalizer = weakref.finalize(self, self._request, 'graph-1',
                                          dict(op='drop', handle=self.handle))

    def call(self, op, **kwargs):
        if not self._finalizer.alive: raise RuntimeError('native graph is closed')
        self._check_identity()
        return self._request('graph-1', dict(op=op, handle=self.handle, **kwargs))

    def _check_identity(self):
        if library_path()!=self._path:
            raise RuntimeError('MODEL_VERSION_CHANGED: native graph library changed; restart worker')
        stat=self._path.stat()
        if (stat.st_size,stat.st_mtime_ns,stat.st_ino)!=self._library._portfolio_stamp:
            raise RuntimeError('MODEL_VERSION_CHANGED: native product binary changed; restart worker')

    def close(self):
        self._finalizer()


def graph_request(books, *, valuation_books, asof, swap_rates, vol_pts, config, seed,
                  mbs_hists, dep_hist, scenario_market=None, spread_shift=0.,
                  spread_overrides_bp=None, include_analytics=True,
                  balance_sheet_extras=None, include_key_rates=False):
    from .lifecycle_native import accounting_request, balance_risk_request, kpi_request
    current_rates, current_vols = (swap_rates, vol_pts) if scenario_market is None else scenario_market
    extras = balance_sheet_extras or {}
    def raw(rows, rates, vols):
        return accounting_request(rows | {'mbs_hists': mbs_hists}, rates, vols, dep_hist,
                                  config.horizon, seed, asof, None, None, False, None, False)
    request = dict(base=raw(books, swap_rates, vol_pts),
                   current=raw(valuation_books, current_rates, current_vols),
                   spread_shift=spread_shift, overrides=spread_overrides_bp or {},
                   include_analytics=include_analytics, include_key_rates=include_key_rates,
                   kpis=kpi_request('eve', valuation_books | extras, asof) if include_analytics else None,
                   auxiliary=None)
    if include_analytics and (extras.get('mm') is not None or extras.get('hedges') is not None):
        request['auxiliary'] = balance_risk_request(extras | {'asof': asof}, current_rates,
                                                    current_vols, dep_hist, seed, 25., None)
        request['auxiliary']['books']['horizon'] = config.horizon
    return request


def price_books(books, *, valuation_books, asof, swap_rates, vol_pts, cache,
                config, seed, mbs_hists, dep_hist, scenario_market, spread_shift,
                spread_overrides_bp, include_analytics, balance_sheet_extras,
                include_key_rates, backend_identity, model_revision):
    from .lifecycle_native import kpi_output
    extras = balance_sheet_extras or {}
    with run_context(config):
        request = graph_request(books, valuation_books=valuation_books, asof=asof,
            swap_rates=swap_rates, vol_pts=vol_pts, config=config, seed=seed,
            mbs_hists=mbs_hists, dep_hist=dep_hist, scenario_market=scenario_market,
            spread_shift=spread_shift, spread_overrides_bp=spread_overrides_bp,
            include_analytics=include_analytics, balance_sheet_extras=extras,
            include_key_rates=include_key_rates)
        with cache._lock:
            if cache._entries or cache._shared:
                cache.clear()
            if cache._native is None:
                cache._native = NativeGraph(cache.max_bytes, cache.max_entries)
            result = cache._native.call('price', request=request)
    return graph_output(result, books, config, include_analytics, include_key_rates, extras, backend_identity, model_revision)


def graph_output(result, books, config, include_analytics, include_key_rates, extras, backend_identity, model_revision):
    from .lifecycle_native import kpi_output
    columns = ['id', 'base_oas_bp', 'applied_oas_bp', 'model_price', 'notional', 'market_value']
    positions = {}
    for book in books:
        rows = result['positions'][book]
        if not rows:
            positions[book] = pl.DataFrame(schema={k: pl.String if k == 'id' else pl.Float64 for k in columns})
        else:
            fields = columns + (['dv01'] + ([f'krd01_{int(t)}y' for t in SWAP_TENORS] if include_key_rates else []) + ['nii_total'] if include_analytics else [])
            positions[book] = pl.DataFrame(rows).select(fields)
    result['positions'] = positions
    result.update(backend=backend_identity, model_revision=model_revision,
                  calibration_policy='saved books/base market recalibrate; temporary instrument assumptions/scenarios hold base OAS')
    if include_analytics:
        raw = result['nii']; months = np.arange(1, config.horizon + 1)
        result['nii'] = dict(monthly=pl.DataFrame({'month': months, **{k: raw[k] for k in ['interest_income', 'interest_expense', 'nii']}}),
                             runoff=pl.DataFrame({'month': months, **{k: raw['runoff'][k] for k in books if k in raw['runoff']}}),
                             total=raw['total'], accounting='effective yields frozen to baseline; CDs/deposits contractual accrual')
        result['kpis'] = kpi_output(result['kpis']) | dict(scope=list(books), includes_auxiliary=bool(extras))
    return result



def compare_books(books, *, assumption_overrides, calibration_mode, asof, swap_rates,
                  vol_pts, cache, config, seed=7, mbs_hists=None, dep_hist=None,
                  scenario_market=None, spread_shift=0., spread_overrides_bp=None,
                  include_analytics=False, include_key_rates=True, balance_sheet_extras=None,
                  backend=None):
    from ..analytics.incremental import SUPPORTED_BOOKS, MODEL_REVISION
    from .native import discount_backend
    if set(books)-set(SUPPORTED_BOOKS) or set(assumption_overrides)-set(books):
        raise ValueError('assumption overrides must refer to selected supported books')
    patches={f'{book}:{ident}':patch for book,rows in assumption_overrides.items() for ident,patch in rows.items()}
    with run_context(config):
        request=graph_request(books,valuation_books=books,asof=asof,swap_rates=swap_rates,
            vol_pts=vol_pts,config=config,seed=seed,mbs_hists=mbs_hists,dep_hist=dep_hist,
            scenario_market=scenario_market,spread_shift=spread_shift,spread_overrides_bp=spread_overrides_bp,
            include_analytics=include_analytics,include_key_rates=include_key_rates,balance_sheet_extras=balance_sheet_extras)
        with cache._lock:
            if cache._entries or cache._shared:cache.clear()
            if cache._native is None:cache._native=NativeGraph(cache.max_bytes,cache.max_entries)
            result=cache._native.call('compare',request=request,patches=patches,calibration_mode=calibration_mode)
    identity=discount_backend('rust').identity
    for key in ('baseline','revised'):
        result[key]=graph_output(result[key],books,config,include_analytics,include_key_rates,balance_sheet_extras or {},identity,MODEL_REVISION)
    comparison={}
    for book,rows in result['comparison'].items():
        fields=result['revised']['positions'][book].columns+['original_price','original_value','value_change','price_change']
        comparison[book]=(pl.DataFrame(rows).select(fields) if rows else result['revised']['positions'][book].with_columns(
            [pl.Series(k,[],dtype=pl.Float64) for k in fields if k not in result['revised']['positions'][book].columns]))
    result['comparison']=comparison;result['assumption_overrides']=assumption_overrides
    return result
