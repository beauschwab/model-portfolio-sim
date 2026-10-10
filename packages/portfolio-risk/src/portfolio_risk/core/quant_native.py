"""Explicit per-run native product computation; Python remains the reference.

Arrays cross one synchronous C ABI call per batch, never per instrument. No
Python callbacks execute inside Rust. Adapters own all buffers through return.
"""
import ctypes
from functools import lru_cache, wraps
import inspect

import numpy as np
from numba import get_num_threads

from .native import library_path
from .runtime import run_cached


class Buffer(ctypes.Structure):
    _fields_ = [('data', ctypes.POINTER(ctypes.c_double)), ('length', ctypes.c_size_t),
                ('shape', ctypes.c_size_t * 3)]


def enabled():
    from .runtime import assumption
    return assumption('compute_backend', 'rust') == 'rust'


@lru_cache(maxsize=4)
def _load(path):
    from pathlib import Path
    if not Path(path).is_file():
        raise RuntimeError('Rust product backend is not built; run scripts/build_native.py')
    stat=Path(path).stat()
    stamp=(stat.st_size,stat.st_mtime_ns,stat.st_ino)
    lib = ctypes.CDLL(path)
    after=Path(path).stat()
    if (after.st_size,after.st_mtime_ns,after.st_ino)!=stamp:
        raise RuntimeError('MODEL_VERSION_CHANGED: native product binary changed; restart worker')
    lib._portfolio_stamp=stamp
    try:
        call = lib.portfolio_quant_call
        version = lib.portfolio_quant_abi_version
    except AttributeError as exc:
        raise RuntimeError('Native product ABI unavailable; rebuild scripts/build_native.py and restart') from exc
    version.argtypes = []
    version.restype = ctypes.c_uint32
    if version() != 9:
        raise RuntimeError('Unsupported native product ABI; rebuild and restart')
    call.argtypes = [ctypes.c_uint32, ctypes.POINTER(Buffer), ctypes.c_size_t,
                     ctypes.POINTER(Buffer), ctypes.c_size_t, ctypes.c_size_t]
    call.restype = ctypes.c_int
    return lib, call


def call(op, inputs, shapes):
    path = library_path()
    _, invoke = _load(str(path))
    arrays = [np.require(np.atleast_1d(v), dtype=np.float64, requirements=['C', 'A']) for v in inputs]
    if any(a.ndim > 3 or np.isnan(a).any() for a in arrays):
        raise ValueError('native input must have at most three dimensions and no NaNs')
    outputs = [np.empty(shape, dtype=np.float64) for shape in shapes]
    def descriptors(values):
        return (Buffer * len(values))(*(Buffer(a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)),
            a.size, (ctypes.c_size_t * 3)(*a.shape, *([1] * (3-a.ndim)))) for a in values))
    status = invoke(op, descriptors(arrays), len(arrays), descriptors(outputs), len(outputs), get_num_threads())
    if status:
        raise ValueError(f'Rust product computation failed (status {status}); no fallback was used')
    return tuple(outputs)


def shared_draws(n_paths, seed, months, factors):
    """Transport integer entropy losslessly; all seed mixing/draws stay native."""
    import operator
    n_paths, seed, months, factors = map(operator.index, (n_paths, seed, months, factors))
    if seed < 0 or seed.bit_length() > 32768:
        raise ValueError('native seed must be a nonnegative integer of at most 32768 bits')
    if min(n_paths, months, factors) < 1:
        raise ValueError('shared draw dimensions must be positive')
    # Check before allocating output buffers. Rust enforces the same ceiling.
    if n_paths * months * (factors + 2) > 128 * 1024 * 1024:
        raise ValueError('shared draw tapes exceed 1 GiB admission limit')
    words = [0] if seed == 0 else []
    while seed:
        words.append(seed & 0xffffffff)
        seed >>= 32
    return call(27, [words, [n_paths, months, factors]],
                [(n_paths, months, factors), (n_paths, months), (n_paths, months)])


def mortgage_cache_statistics(*, clear=False):
    """Native market-stage retention only; active buffers and output copies are extra."""
    values, = call(30, [[int(clear)]], [(5,)])
    return dict(zip(('entries', 'bytes', 'hits', 'misses', 'evictions'), map(int, values)))


def term_call(schema, request, *, _library=None):
    """One bounded raw-contract batch; native code owns all financial stages."""
    import json
    import datetime
    def encode(value):
        if isinstance(value, datetime.date):
            return value.toordinal()
        raise TypeError(f'Unsupported term input {type(value).__name__}')
    payload = json.dumps(dict(schema=schema, threads=get_num_threads(), request=request),
                         default=encode, allow_nan=False, separators=(',', ':')).encode()
    if len(payload) > 64*1024*1024:
        raise ValueError('native term request exceeds 64 MiB')
    lib = _library if _library is not None else _load(str(library_path()))[0]
    numeric = schema == 'term-risk-1'
    invoke = lib.portfolio_term_risk_into if numeric else lib.portfolio_term_request
    release = lib.portfolio_term_free
    invoke.argtypes = [ctypes.c_char_p, ctypes.c_size_t] + ([ctypes.POINTER(Buffer), ctypes.c_size_t] if numeric else [])
    invoke.restype = ctypes.c_void_p
    release.argtypes = [ctypes.c_void_p]
    release.restype = None
    if numeric:
        n = len(request['deck']['contracts'])
        nc = len(request['tenors']) + len(request['vol_quotes'])//3
        if n*(nc+3) > 128*1024*1024:
            raise ValueError('native term numeric outputs exceed 1 GiB admission')
        arrays = [np.empty(size, dtype=np.float64) for size in (n,n,n,n*nc)]
        descriptors = (Buffer * 4)(*(Buffer(a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)),
            a.size, (ctypes.c_size_t * 3)(a.size,1,1)) for a in arrays))
        pointer = invoke(payload, len(payload), descriptors, 4)
    else:
        pointer = invoke(payload, len(payload))
    if not pointer:
        raise RuntimeError('Native term allocation failed; no fallback was used')
    try:
        response = json.loads(ctypes.string_at(pointer))
    finally:
        release(pointer)
    if not response['ok']:
        raise ValueError(f"Rust term computation failed: {response['error']}; no fallback was used")
    return dict(zip(('oas','price','dv01','sensitivities'),arrays)) if numeric else response['result']


def _term_deck_request(book, asof, product, cal=None, bdc=None):
    from .config import N_STEPS
    from .conventions import Calendar, BDC
    from ..products.corp import CORP_COLS
    from ..products.cds import CD_COLS
    required = CORP_COLS if product == 'corporate' else CD_COLS
    if missing := required - set(book.columns):
        raise ValueError(f'term book missing columns: {missing}')
    cal = cal or Calendar('US')
    if type(cal) is not Calendar:
        raise ValueError('Custom Python calendar classes are not supported by the Rust backend')
    optional = ['call_threshold','call_schedule']
    optional += ['is_float','cap','floor','amort_type','sink_schedule','put_schedule'] if product == 'corporate' else ['channel','penalty_months','ew_mult']
    contracts = []
    for row in book.to_dicts():
        contract = {key:row[key] for key in ['maturity','freq_months','daycount','price']}
        contract.update(coupon=row['coupon_or_spread' if product == 'corporate' else 'rate'],
                        notional=row['face' if product == 'corporate' else 'balance'])
        contract.update({key:row[key] for key in optional if key in row})
        if product == 'corporate':
            if contract['is_float'] not in (0, 1, False, True):
                raise ValueError('is_float must be a boolean or 0/1')
            contract['is_float'] = bool(contract['is_float'])
        contracts.append(contract)
    return dict(product=product, asof=asof, months=N_STEPS, bdc=(bdc or BDC.MODIFIED_FOLLOWING).value,
                calendar=dict(name=cal.name, extra_holidays=sorted(cal.extra_holidays)), contracts=contracts)


def term_deck(book, asof, product, cal=None, bdc=None):
    result = term_call('term-deck-1', _term_deck_request(book, asof, product, cal, bdc))
    return _term_deck_fields(result, product)


def _term_deck_fields(result, product):
    integer = {'per_off','pay_m','acc_m','fix_m','is_float'}
    shared = ['per_off','pay_m','pay_frac','acc_m','t_pay','tau','call_px','call_thr','tgt']
    selected = shared + (['fix_m','fix_w','prin','put_px','is_float','cpn','cap','floor'] if product == 'corporate'
                         else ['rem_y','pen_m','ew_mult'])
    fields = {key:np.asarray(result[key],dtype=np.int64 if key in integer else np.float64) for key in selected}
    fields['n'] = result['n']
    fields['face' if product == 'corporate' else 'bal'] = np.asarray(result['notional'],dtype=np.float64)
    if product == 'cd':
        fields['rate'] = np.asarray(result['cpn'],dtype=np.float64)
    return fields


def term_risk(book, asof, swap_rates, vol_pts, product, seed, cal, oas):
    import operator
    import polars as pl
    from . import config as cfg
    from .runtime import path_count, assumption
    from ..products.cds import CD_EW_PARAMS
    seed = operator.index(seed)
    if seed < 0 or seed.bit_length() > 32768:
        raise ValueError('native seed must be a nonnegative integer of at most 32768 bits')
    words = [0] if seed == 0 else []
    while seed:
        words.append(seed & 0xffffffff)
        seed >>= 32
    request = dict(deck=_term_deck_request(book,asof,product,cal),
        config=dict(paths=path_count(cfg.N_PATHS_SENS),months=cfg.N_STEPS,forwards=cfg.N_FWD,factors=cfg.N_FACTORS,
                    dt=cfg.DT,tenor=cfg.TENOR,shift=cfg.SHIFT,curve_bump=cfg.CURVE_BUMP,vol_bump=cfg.VOL_BUMP),
        tenors=np.asarray(cfg.SWAP_TENORS).tolist(),swap_rates=np.asarray(swap_rates).tolist(),
        vol_quotes=np.asarray(vol_pts).ravel().tolist(),seed=words,
        fixed_oas=[] if oas is None else np.asarray(oas).tolist(),
        withdrawal_parameters=np.asarray(assumption('cd_ew_params',CD_EW_PARAMS)).tolist() if product == 'cd' else [])
    result = term_call('term-risk-1',request)
    labels = [f'krd01_{int(t)}y' for t in cfg.SWAP_TENORS] + [f'vega_{int(e)}x{int(t)}' for e,t,_ in vol_pts]
    sensitivities = np.asarray(result['sensitivities']).reshape(len(labels),len(book))
    return book.with_columns(pl.Series('oas_bps',np.asarray(result['oas'])*1e4),
        pl.Series('model_price',np.asarray(result['price'])*100),pl.Series('dv01',result['dv01']),
        *[pl.Series(name,values) for name,values in zip(labels,sensitivities)])


def _mortgage_inputs(port, swap_rates, vol_pts, cc_hist, ps_hist, seed, suite, oas):
    """Raw table/enum transport; Rust owns all financial stages."""
    import operator
    from . import config as cfg
    from .runtime import path_count
    from .scenarios import REQUIRED_COLS
    import importlib
    mdl = importlib.import_module("portfolio_risk.models.models")
    prepay = importlib.import_module("portfolio_risk.models.prepay")
    if suite is not None and (type(suite.cc) is not mdl.TrendingCC or
            type(suite.ps) is not mdl.OUSpread or type(suite.hpi) is not mdl.RateLinkedHPI or suite.prepay_step is not None):
        raise ValueError('Custom Python model suites are not supported by the Rust backend')
    if missing := REQUIRED_COLS - set(port.columns):
        raise ValueError(f'portfolio missing columns: {missing}')
    seed = operator.index(seed)
    if seed < 0 or seed.bit_length() > 32768:
        raise ValueError('native seed must be a nonnegative integer of at most 32768 bits')
    words = [0] if seed == 0 else []
    while seed:
        words.append(seed & 0xffffffff)
        seed >>= 32
    n = len(port)
    state_codes = {name: i+1 for i, name in enumerate(prepay.STATE_MULT)}
    channel_codes = {name: i+1 for i, name in enumerate(prepay.CHANNEL_MULT)}
    book = np.column_stack([port.select(['wac', 'net_coupon', 'wam', 'age', 'oltv', 'factor', 'fico', 'avg_loan_size']).to_numpy(),
        [state_codes.get(x, 0) for x in port['state'].to_list()],
        [channel_codes.get(x, 0) for x in port['channel'].to_list()],
        port['price'].to_numpy(), port['current_face'].to_numpy(),
        port['pay_delay_days'].to_numpy() if 'pay_delay_days' in port.columns else np.zeros(n)])
    configuration = [path_count(cfg.N_PATHS_BASE, base=True), path_count(cfg.N_PATHS_SENS), cfg.N_STEPS,
        cfg.N_FWD, cfg.N_FACTORS, cfg.DT, cfg.TENOR, cfg.SHIFT, cfg.ADT == np.float32,
        cfg.CURVE_BUMP, cfg.VOL_BUMP, cfg.HPI_MU, cfg.HPI_BETA, cfg.HPI_SIG, cfg.INC_LAG, .012, cfg.RATIONAL_SIGMOID]
    inputs = [cfg.SWAP_TENORS, swap_rates, vol_pts, cc_hist.select(mdl.CC_FEATURES+['cc']).to_numpy(), ps_hist['ps'].to_numpy(),
        book, port['hpi_orig_ratio'].to_numpy() if 'hpi_orig_ratio' in port.columns else [], words, configuration,
        [] if oas is None else oas, cfg.MOY, cfg.SEASONALITY, cfg.PREPAY_PARAMS, prepay.LTV_KNOTS, prepay.LTV_COEFS,
        prepay.SMM_LUT, [prepay.SMM_SCALE, prepay.BURN_SCALE], prepay.BURN_LUT, cfg.CC_VOL_POINTS,
        prepay.FICO_X, prepay.FICO_Y, prepay.SIZE_X, prepay.SIZE_Y,
        [1., *prepay.STATE_MULT.values()], [1., *prepay.CHANNEL_MULT.values()]]
    return inputs


def prepay_speed(port):
    """Optional per-pool prepay speed multipliers; empty when the column is absent."""
    from .scenarios import prepay_multiplier
    return prepay_multiplier(port).tolist() if 'prepay_mult' in port.columns else []


def mortgage_risk(port, swap_rates, vol_pts, cc_hist, ps_hist, seed, suite, oas):
    import polars as pl
    from . import config as cfg
    inputs = _mortgage_inputs(port, swap_rates, vol_pts, cc_hist, ps_hist, seed, suite, oas)
    n = len(port)
    spreads, prices, dv01, sensitivities = call(28, inputs + [prepay_speed(port)], [(n,), (n,), (n,), (len(cfg.SWAP_TENORS)+len(vol_pts), n)])
    labels = [f'krd01_{int(t)}y' for t in cfg.SWAP_TENORS] + [f'vega_{int(e)}x{int(t)}' for e,t,_ in vol_pts]
    return port.with_columns(pl.Series('oas_bps', spreads*1e4), pl.Series('model_price', prices*100), pl.Series('dv01', dv01),
                            *[pl.Series(name, values) for name, values in zip(labels, sensitivities)])


def mortgage_stress(port, swap_rates, vol_pts, cc_hist, ps_hist, shocks_bp, seed, suite, oas):
    """Format Rust-owned position results and aggregates; no financial replay."""
    import polars as pl
    from .config import STRESS_HORIZONS_M
    from .runtime import stress_horizons
    inputs = _mortgage_inputs(port, swap_rates, vol_pts, cc_hist, ps_hist, seed, suite, oas)
    hz = np.asarray(stress_horizons(STRESS_HORIZONS_M), dtype=np.int64)
    shocks = np.asarray(shocks_bp, dtype=np.float64)
    n, nh, nj = len(port), len(hz), len(shocks)
    if 2*n*nh*nj + 2*n*nh + 2*nj*nh + n + nh > 128*1024*1024:
        raise ValueError('mortgage stress outputs exceed 1 GiB admission limit')
    has_profile = -100. in shocks and 100. in shocks
    base, price, shocked, pnl, agg_base, agg_pnl, profile, _ = call(
        29, inputs + [hz, shocks, prepay_speed(port)], [(nh,n)]*2 + [(nj,nh,n)]*2 + [(nj,nh)]*2 + [(nh if has_profile else 0,), (n,)])
    frames = [pl.DataFrame(dict(cusip=port['cusip'], horizon_m=np.full(n,h,dtype=np.int64),
                shock_bp=np.full(n,shock), fwd_value_base=base[hi], fwd_price_base=price[hi],
                fwd_value_shock=shocked[j,hi], stress_pnl=pnl[j,hi]))
              for j,shock in enumerate(shocks) for hi,h in enumerate(hz)]
    aggregate = pl.DataFrame({'horizon_m': np.tile(hz,nj), 'shock_bp': np.repeat(shocks,nh),
                             'pnl_$':agg_pnl.ravel(), 'base_mv_$':agg_base.ravel()}).sort(['shock_bp','horizon_m'])
    prof = pl.DataFrame({'horizon_m':hz,'fwd_dv01_$':profile}) if has_profile else None
    return pl.concat(frames), aggregate, prof


def dispatch(name, reference):
    signature = inspect.signature(reference)
    @wraps(reference)
    def wrapped(*args, **kwargs):
        if not enabled():
            return reference(*args, **kwargs)
        bound = signature.bind(*args, **kwargs)
        bound.apply_defaults()
        a = list(bound.arguments.values())
        if name == 'lmm':
            from .config import DT, TENOR
            result = call(1, a[:5] + [DT, TENOR], [x.shape for x in a[5:]])
            for dst, src in zip(a[5:], result): dst[...] = src
            return None
        if name in ('corp', 'cd'):
            size = len(a[4 if name == 'corp' else 3])
            return call(2 if name == 'corp' else 3, a, [(size,)]*3)
        p, t = a[0].shape
        if name == 'mbs':
            s, h = len(a[13]), len(a[23])   # horizons follow the nine per-pool vectors and OAS
            out = list(call(4, a, [(s,t),(s,h),(s,h),(s,p,h),(s,p,h),(s,t),(s,t)]))
            out[3], out[4] = out[3].astype(np.float32), out[4].astype(np.float32)
            return tuple(out)
        if name == 'mbs_stress':
            return call(5, a, [(len(a[13]),)])[0]
        if name == 'deposit':
            s, h = len(a[6]), len(a[17])
            out = list(call(6, a, [(s,t),(s,t),(s,h),(s,h),(s,p,h),(s,t)]))
            out[4] = out[4].astype(np.float32)
            return tuple(out)
        if name == 'deposit_stress':
            return call(7, a, [(len(a[6]),)])[0]
        if name == 'mbs_batch':
            return call(10, a, [(a[5],len(a[15]))])[0]
        raise ValueError(f'unsupported native product operation {name}')
    return wrapped


def kernel(name):
    return lambda reference: dispatch(name, reference)


def csr_price(offsets, times, sums, spread, paths):
    return call(9, [offsets,times,sums,spread,paths], [(len(offsets)-1,)])[0]


@run_cached
def _market_call(op, inputs, shapes):
    # Retain bounded run-local reuse while graph/cache ownership is migrated.
    return call(op, inputs, shapes)


def market_paths(swap_rates, abcd_p, loadings, crn, models=None):
    """One native context owns curve -> volatility table -> rates -> behavior.

    Calibrated models and the shared draw tape are explicit inputs; this adapter
    performs serialization and output storage conversion, not financial stages.
    """
    from .config import (ADT, CC_VOL_POINTS, DT, HPI_BETA, HPI_MU, HPI_SIG,
                         INC_LAG, SHIFT, SWAP_TENORS, TENOR)
    p,t,_ = crn.Z.shape
    inputs = [SWAP_TENORS, swap_rates, abcd_p, loadings, crn.Z, [DT,TENOR,SHIFT]]
    shapes = [(p,t),(p,4,t),(p,t)]
    if models is None:
        df,swaps,short = _market_call(19, inputs, shapes)
        return dict(df=df,swaps=swaps,short=short)
    ps = models['ps']
    inputs += [CC_VOL_POINTS,models['cc']['beta'],models['cc']['lam'],
               [ps['kappa'],ps['theta'],ps['sigma'],models['ps_spot']],
               crn.eps_ps,crn.eps_h,[HPI_MU,HPI_BETA,HPI_SIG],INC_LAG]
    df,swaps,short,mtg,hpi,yoy = _market_call(20,inputs,shapes+[(p,t)]*3)
    return dict(df=df.astype(ADT),swaps=swaps,short=short.astype(ADT),
                mtg=mtg.astype(ADT),hpi=hpi.astype(ADT),yoy=yoy.astype(ADT))


def csr_solve(offsets, times, sums, target, paths, tol=1e-8, max_iter=40, lo=-.05, hi=.30):
    return call(8, [offsets,times,sums,target,paths,tol,max_iter,lo,hi], [(len(target),)]*2)


def monthly_inputs(a, delay, grid):
    s, t = a.shape
    times = np.broadcast_to(grid, (s,t)) if delay is None else grid + np.asarray(delay)[:,None]
    return np.arange(s+1,dtype=np.int64)*t, times.reshape(-1), a.reshape(-1)
