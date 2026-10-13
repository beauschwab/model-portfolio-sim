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

    def call(self, op, *, runtime=None, **kwargs):
        if not self._finalizer.alive: raise RuntimeError('native graph is closed')
        self._check_identity()
        if runtime is not None:
            return self._request('graph-1', dict(op=op,handle=self.handle,**kwargs),runtime=runtime)
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
                include_key_rates, backend_identity, model_revision, market_indexed_bindings=None, graph_runtime=None):
    from .lifecycle_native import kpi_output
    if graph_runtime is not None and market_indexed_bindings is None:raise ValueError('graph runtime requires explicit indexed book selection')
    extras = balance_sheet_extras or {}
    with run_context(config):
        request = graph_request(books, valuation_books=valuation_books, asof=asof,
            swap_rates=swap_rates, vol_pts=vol_pts, config=config, seed=seed,
            mbs_hists=mbs_hists, dep_hist=dep_hist, scenario_market=scenario_market,
            spread_shift=spread_shift, spread_overrides_bp=spread_overrides_bp,
            include_analytics=include_analytics, balance_sheet_extras=extras,
            include_key_rates=include_key_rates)
        if market_indexed_bindings is not None:
            selected = market_indexed_book_request(request, market_indexed_bindings, balance_sheet_extras)
            with cache._lock:
                result = select_market_indexed_owner(cache,selected,runtime=graph_runtime)
            formatted = graph_output(result, books, config, include_analytics, include_key_rates, extras, backend_identity, model_revision)
            formatted['calibration_policy'] = 'explicit indexed original quote reconciled at supplied original OAS; unbound legacy base calibration retained'
            return formatted
        with cache._lock:
            if isinstance(cache._native, NativeMarketIndexedBooks):
                raise RuntimeError('GRAPH_OWNER_CHANGED: rebuild for the historical owner')
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


class NativeMarketIndexedGraph(NativeGraph):
    """Explicit named-index pricing cache; no Physical or saved-book execution."""
    def __init__(self, request, max_bytes=8 * 1024 * 1024, max_entries=1024, *, runtime=None):
        self._path = library_path()
        self._library = _load(str(self._path))[0]
        self._check_identity()
        self._request = partial(term_call, _library=self._library)
        options={} if runtime is None else {'runtime':runtime}
        created = self._request('graph-1',dict(op='create_market_indexed',max_bytes=max_bytes,max_entries=max_entries,request=request),**options)
        self.handle = created['handle']
        self.initial = created['initial']
        self._finalizer = weakref.finalize(self, self._request, 'graph-1',
                                         dict(op='drop', handle=self.handle))

    def price(self, request, *, runtime=None):
        return self.call('price_market_indexed',request=request,runtime=runtime)

    def info(self, *, runtime=None):
        return self.call('info',runtime=runtime)

    def clear(self, *, runtime=None):
        return self.call('clear',runtime=runtime)


class NativeMarketIndexedBooks(NativeGraph):
    """Explicit supplied Pricing book owner; full raw binding receipt is native."""
    def __init__(self, request, max_bytes, max_entries, *, runtime=None):
        self._path = library_path()
        self._library = _load(str(self._path))[0]
        self._check_identity()
        self._request = partial(term_call, _library=self._library)
        options={} if runtime is None else {'runtime':runtime}
        created = self._request('graph-1',dict(op='create_market_indexed_books',max_bytes=max_bytes,max_entries=max_entries,request=request),**options)
        self.handle = created['handle']
        self.initial = created['initial']
        self._finalizer = weakref.finalize(self, self._request, 'graph-1',
                                         dict(op='drop', handle=self.handle))

    def price(self, request, *, runtime=None):
        return self.call('price_market_indexed_books',request=request,runtime=runtime)


def market_indexed_book_request(legacy, bindings, balance_sheet_extras=None):
    # Raw transport only. Rust reconciles dates, rows, amounts, quotes and parents.
    if not isinstance(bindings, dict) or set(bindings) != {'schema', 'identity', 'bindings'}:
        raise ValueError('complete versioned market indexed binding transport required')
    return dict(schema=bindings['schema'], identity=bindings['identity'],
                legacy=legacy, bindings=bindings['bindings'], balance_sheet_extras=balance_sheet_extras)


def select_market_indexed_owner(cache, request, *, runtime=None):
    if cache._entries or cache._shared:
        raise RuntimeError('GRAPH_OWNER_CHANGED: rebuild an empty explicit pricing cache')
    if cache._native is None:
        cache._native = NativeMarketIndexedBooks(request,cache.max_bytes,cache.max_entries,**({} if runtime is None else {'runtime':runtime}))
        return cache._native.initial
    if not isinstance(cache._native, NativeMarketIndexedBooks):
        raise RuntimeError('GRAPH_OWNER_CHANGED: rebuild with the explicit portfolio owner')
    return cache._native.price(request,runtime=runtime)
