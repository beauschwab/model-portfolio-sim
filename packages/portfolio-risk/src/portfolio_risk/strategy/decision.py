"""Rust-owned decision graph with existing product engines as a batch service.

Only validated candidates publish. Financial conventions and independent replay
stay in this package; API callers supply immutable snapshots and publication guards.
"""
from __future__ import annotations

from contextlib import nullcontext
from dataclasses import asdict, replace
from functools import lru_cache
import copy
import ctypes
import hashlib
import json
import os
from pathlib import Path
import threading
import time

import numpy as np

from ..analytics.incremental import price_books, SUPPORTED_BOOKS
from ..analytics.whatif import apply_overrides, FIELDS
from ..core.dependency import DependencyCache, fingerprint
from ..core.runtime import run_context
from .optimizer import _kpi_vectors
from .unitlib import (build_unit_library, allocation_vectors, evaluate_strategy,
                      prepare_library, resolve_templates)


_DECISION_NAME = ('portfolio_decision_native.dll' if os.name == 'nt'
    else 'libportfolio_decision_native.dylib' if __import__('sys').platform == 'darwin' else 'libportfolio_decision_native.so')
_DECISION_DEFAULT = Path(__file__).resolve().parents[4] / 'portfolio-decision-native/target/release' / _DECISION_NAME


def _file_stamp(path):
    stat=path.stat()
    return stat.st_size,stat.st_mtime_ns,stat.st_ino


@lru_cache(maxsize=4)
def _load_decision(path):
    source=Path(path);stamp=_file_stamp(source)
    lib=ctypes.CDLL(path)
    identity='decision-abi-v1:'+hashlib.sha256(source.read_bytes()).hexdigest()
    if _file_stamp(source)!=stamp:
        raise RuntimeError('MODEL_VERSION_CHANGED: native decision binary changed; restart worker')
    lib.decision_request.argtypes=[ctypes.c_char_p]
    lib.decision_request.restype=ctypes.c_void_p
    lib.decision_free.argtypes=[ctypes.c_void_p]
    lib.decision_free.restype=None
    return lib,identity,stamp


class NativeDecision:
    """Owned JSON ABI. No fallback, borrowed pointers, or per-instrument FFI calls."""
    def __init__(self, path=None):
        configured=path or os.environ.get('PORTFOLIO_DECISION_LIBRARY')
        path=Path(configured).resolve() if configured else _DECISION_DEFAULT
        if not path.is_file():
            raise RuntimeError('Rust decision library is not built. Run scripts/build_decision_native.py with cmake and libclang.')
        self.lib,self.identity,stamp = _load_decision(str(path))
        if _file_stamp(path)!=stamp:
            raise RuntimeError('MODEL_VERSION_CHANGED: native decision binary changed; restart worker')
        self.bytes_sent = 0

    def call(self, **request):
        import datetime
        def encode(value):
            if isinstance(value, datetime.date): return value.toordinal()
            raise TypeError(f'Unsupported decision input {type(value).__name__}')
        payload = json.dumps(request, default=encode, allow_nan=False, separators=(',', ':')).encode()
        if len(payload) > 128 * 1024 * 1024:
            raise ValueError('native request exceeds 128 MiB')
        self.bytes_sent += len(payload)
        pointer = self.lib.decision_request(payload)
        if not pointer:
            raise RuntimeError('native decision returned a null response')
        try:
            result = json.loads(ctypes.string_at(pointer))
        finally:
            self.lib.decision_free(pointer)
        if 'error' in result:
            raise ValueError(result['error'])
        return result['ok']


def _base(lib, base):
    return _kpi_vectors(lib, base)['base'] | {'mv_assets': base['eve']['mv_assets_$']}


def _vectors(lib):
    return [allocation_vectors(lib, u['template'], u['h']).tolist() for u in lib['units']]


def _contributions(result):
    out = {}
    for book, frame in result['positions'].items():
        sign = 1 if book in ('mbs', 'loans') else -1
        for r in frame.iter_rows(named=True):
            mv = r['market_value']
            out[f"{book}:{r['id']}"] = [sign * mv, sign * r['dv01'], r['nii_total'],
                mv * .85 if book == 'mbs' and not r['id'].startswith('HL') else 0.,
                mv if sign > 0 else 0.]
    return out


def _apply_delta(base, delta):
    from ..analytics.kpis import NI_TO_NII, PAYOUT
    base['nii_total_$'] += delta[2]
    base['eve']['eve_$'] += delta[0]
    base['eve']['dv01_net_$'] += delta[1]
    base['eve']['mv_assets_$'] += delta[4]
    base['lcr']['hqla_l2a_uncapped_$'] += delta[3]
    base['capital']['cet1_path'][-1]['cet1_$'] += delta[2] * NI_TO_NII * (1 - PAYOUT)


def validate_replay(result, libraries, bases, constraints):
    """Independent Python coefficient replay, including every monthly constraint."""
    if not result['feasible']:
        return  # Infeasible is a solver outcome, never a validated allocation.
    allocation = result['allocation']
    from ..analytics.treasury import replay_capital_limits
    capital = replay_capital_limits(constraints.get('capital_limits',[]),libraries[0]['units'],allocation)
    for actual,native in zip(capital,result.get('capital_replay',[]),strict=True):
        for key in ('numerator','denominator','ratio','headroom'):
            np.testing.assert_allclose(actual[key],native[key],rtol=1e-9,atol=1e-5)
    replay = [evaluate_strategy(lib, allocation, base) for lib, base in zip(libraries, bases)]
    for actual, native in zip(replay, result['replay']):
        for key in ('nii_total_$', 'nii_incremental', 'funding_gap'):
            np.testing.assert_allclose(actual[key], native[key], rtol=1e-9, atol=1e-5)
        for key in actual['kpi_path']:
            np.testing.assert_allclose(actual['kpi_path'][key], native['kpi_path'][key], rtol=1e-9, atol=1e-7)
        np.testing.assert_allclose(actual['kpis']['cet1_horizon_pct'], native['kpis']['cet1_horizon_pct'], rtol=1e-9, atol=1e-7)
        p = actual['kpi_path']
        if (min(p['lcr_pct']) < constraints['lcr_min'] * 100 - 1e-5
                or min(p['nsfr_pct']) < constraints['nsfr_min'] * 100 - 1e-5
                or max(abs(p['d_eve_pct_eve_+200'])) > constraints['eve_limit'] * 100 + 1e-5
                or actual['kpis']['cet1_horizon_pct'] < constraints['cet1_min'] * 100 - 1e-5
                or max(actual['funding_gap']) > constraints['cash_budget'] + max(.01, constraints['cash_budget'] * 1e-7)):
            raise RuntimeError('independent replay failed a monthly or capital constraint')
    objective = min(b['nii_total_$'] + r['nii_total_$'] for b, r in zip(bases, replay))
    np.testing.assert_allclose(objective, result['worst_case_nii_$'], rtol=1e-7, atol=.01)
    sides = {u['template']: u['side'] for u in libraries[0]['units']}
    def amount(target):
        return sum(a['notional'] for a in allocation if a['template'] == target or
                   target == 'ALL_ASSET' and sides[a['template']] > 0 or
                   target == 'ALL_LIAB' and sides[a['template']] < 0)
    checks = list(constraints['commercial'])
    if constraints['max_total_assets'] is not None:
        checks.append({'template': 'ALL_ASSET', 'sense': '<=', 'rhs': constraints['max_total_assets']})
    for c in checks:
        value, rhs = amount(c['template']), c['rhs']
        tol = max(.01, abs(rhs) * 1e-8)
        if c['sense'] == '<=' and value > rhs + tol or c['sense'] == '>=' and value < rhs - tol:
            raise RuntimeError('independent commercial constraint replay failed')


class DecisionSession:
    """One immutable pricing context, mutable transactional assumptions/constraints.

    Session state is process-local. Changing saved books/market/runtime settings
    requires a new session. Instrument deltas never scan the full portfolio.
    """
    def __init__(self, *, books, asof, swap_rates, vol_pts, config, mbs_hists,
                 dep_hist, constraints, markets=None, seed=7, extras=None, cache=None):
        if config.horizon > 120:
            raise ValueError('decision prototype supports horizons up to 120 months')
        self.native = NativeDecision()
        config = copy.deepcopy(config)
        swap_rates, vol_pts = np.array(swap_rates, copy=True), np.array(vol_pts, copy=True)
        mbs_hists, dep_hist = copy.deepcopy(mbs_hists), copy.deepcopy(dep_hist)
        self.handle = None
        self.lock = threading.RLock()
        self.version = 0
        self.books = {k: v.clone() for k, v in books.items() if k in SUPPORTED_BOOKS and len(v)}
        self.index = {f'{b}:{ident}': (b, i, ident) for b, frame in self.books.items()
                      for i, ident in enumerate(frame['cusip' if b == 'mbs' else 'id'])}
        self.config, self.seed, self.asof = config, seed, asof
        self.histories, self.dep_hist = mbs_hists, dep_hist
        self.markets = [(name, np.array(sr, copy=True), np.array(vp, copy=True), spread)
                        for name, sr, vp, spread in (markets or [('base', swap_rates, vol_pts, 0.)])]
        if not 1 <= len(self.markets) <= 13:
            raise ValueError('decision prototype supports 1..13 market scenarios')
        self.cache = cache or DependencyCache(max_bytes=512 * 1024 * 1024, max_entries=500_000)
        self.pricing = dict(asof=asof, swap_rates=swap_rates, vol_pts=vol_pts, config=config,
                            seed=seed, mbs_hists=mbs_hists, dep_hist=dep_hist, cache=self.cache,
                            include_analytics=True, include_key_rates=False)
        self.context = fingerprint(self.native.identity, self.books, asof, swap_rates, vol_pts, asdict(config), seed,
                                   self.markets, mbs_hists, dep_hist, extras)
        started = time.perf_counter()
        self._owned = config.compute_backend == 'rust'
        if self._owned:
            from numba import get_num_threads
            from ..core.graph_native import graph_request
            from ..core.lifecycle_native import unit_request
            with run_context(config):
                graph = graph_request(self.books, valuation_books=self.books, asof=asof,
                    swap_rates=swap_rates, vol_pts=vol_pts, config=config, seed=seed,
                    mbs_hists=mbs_hists, dep_hist=dep_hist, balance_sheet_extras=extras)
                units = unit_request(swap_rates, vol_pts, mbs_hists, dep_hist, None,
                                     config.horizon, seed, asof, resolve_templates())
            response = self.native.call(op='create_owned', constraints=constraints, input=dict(
                graph=graph, units=units, threads=get_num_threads(),
                max_bytes=self.cache.max_bytes, max_entries=self.cache.max_entries,
                markets=[dict(name=name, swap_rates=sr.tolist(), vol_quotes=vp.ravel().tolist(), spread=spread)
                         for name, sr, vp, spread in self.markets]))
            self.handle=response['handle']
            self.libraries, self.bases=self._snapshot(response['snapshot'])
            self.initialization_ms=(time.perf_counter()-started)*1000
            self.result=None
            return
        self.libraries, self.bases = [], []
        metrics = {key: [] for key in self.index}
        with run_context(config):
            for _, sr, vp, spread in self.markets:
                result = price_books(self.books, **self.pricing, scenario_market=(sr, vp),
                                     spread_shift=spread, balance_sheet_extras=extras)
                base = result['kpis'] | {'nii_total_$': result['nii']['total']}
                lib = self._build_units(sr, vp)
                self.libraries.append(lib)
                self.bases.append(base)
                for key, value in _contributions(result).items():
                    metrics[key].append(value)
        payload = dict(units=self.libraries[0]['units'],
            scenarios=[{'base': _base(lib, base), 'vectors': _vectors(lib)} for lib, base in zip(self.libraries, self.bases)],
            records=[{'key': key, 'metrics': value} for key, value in metrics.items()], constraints=constraints)
        self.handle = self.native.call(op='create', **payload)['handle']
        self.initialization_ms = (time.perf_counter() - started) * 1000
        self.result = None

    def _build_units(self, sr, vp, **kwargs):
        return build_unit_library(sr, vp, self.histories, self.dep_hist, horizon=self.config.horizon,
                                  seed=self.seed, asof=self.asof, **kwargs)

    def _call(self, op, **kwargs):
        if self.handle is None:
            raise RuntimeError('decision session is closed')
        return self.native.call(op=op, handle=self.handle, version=self.version, **kwargs)

    def update(self, *, version, edits=None, templates=None, constraints=None, publish_guard=None):
        with self.lock:
            if version != self.version:
                raise ValueError('stale session version')
            edits, templates = edits or {}, templates or {}
            if self._owned:
                return self._update_owned(version, edits, templates, constraints, publish_guard)
            # Validate null resets too: they must not provide a route around the whitelist.
            for key, patch in edits.items():
                if key not in self.index or set(patch) - set(FIELDS[self.index[key][0]]):
                    raise ValueError('unknown instrument or unsupported assumption')
            for name, patch in templates.items():
                resolve_templates({name: {k: v for k, v in patch.items() if v is not None}})
                if set(patch) - {'spread_bp'}:
                    raise ValueError('only template spread_bp is supported')
            started, sent = time.perf_counter(), self.native.bytes_sent
            plan = self._call('plan', edits=edits, templates=templates,
                              **({'constraints': constraints} if constraints is not None else {}))
            token = plan['token']
            try:
                bases = copy.deepcopy(self.bases)
                libraries = list(self.libraries)
                groups, overrides = {}, {}
                for item in plan['dirty']:
                    book, row, ident = self.index[item['key']]
                    groups.setdefault(book, []).append(row)
                    overrides.setdefault(book, {})[ident] = item['patch']
                selected = {b: self.books[b][indices] for b, indices in groups.items()}
                with run_context(self.config):
                    revised = apply_overrides(selected, overrides)
                metrics = {item['key']: [] for item in plan['dirty']}
                old = {item['key']: item['old_metrics'] for item in plan['dirty']}
                graph = []
                columns = {name: [] for name in plan['templates']}
                with run_context(self.config):
                    for si, (_, sr, vp, spread) in enumerate(self.markets):
                        if selected:
                            result = price_books(selected, **self.pricing, valuation_books=revised,
                                                 scenario_market=(sr, vp), spread_shift=spread)
                            graph.append(result['graph'])
                            for key, value in _contributions(result).items():
                                metrics[key].append(value)
                                _apply_delta(bases[si], np.asarray(value) - old[key][si])
                        if columns:
                            part = self._build_units(sr, vp, template_names=list(columns), template_overrides=plan['templates'])
                            full = dict(libraries[si])
                            for field in ('nii', 'runoff', 'balance', 'dv01'):
                                full[field] = full[field].copy()
                            full['templates'] = dict(full['templates']) | part['templates']
                            indices = {(u['template'], u['h']): i for i, u in enumerate(full['units'])}
                            for j, u in enumerate(part['units']):
                                i = indices[(u['template'], u['h'])]
                                for field in ('nii', 'runoff', 'balance', 'dv01'):
                                    full[field][i] = part[field][j]
                            full.pop('vectors', None)
                            prepare_library(full)
                            libraries[si] = full
                            for name in columns:
                                columns[name].append([allocation_vectors(full, u['template'], u['h']).tolist()
                                    for u in full['units'] if u['template'] == name])
                priced = time.perf_counter()
                candidate = self._call('stage', token=token, records=[{'key': k, 'metrics': v} for k, v in metrics.items()], columns=columns)
                solved = time.perf_counter()
                for lib, base, native in zip(libraries, bases, candidate['bases']):
                    for key, value in _base(lib, base).items():
                        np.testing.assert_allclose(value, native[key], rtol=1e-10, atol=1e-5)
                validate_replay(candidate, libraries, bases, plan['constraints'])
                with publish_guard() if publish_guard else nullcontext():
                    result = self._call('publish', token=token)
                    self.version = result['version']
                    self.bases, self.libraries = bases, libraries
                result.update(validated=bool(result['feasible']), context=self.context, native_backend=self.native.identity,
                    validation='Independent Python allocation replay passed' if result['feasible'] else 'No feasible allocation; no allocation published',
                    scenarios=[m[0] for m in self.markets], horizon_months=self.config.horizon,
                    initialization_ms=self.initialization_ms,
                    work={'positions_repriced': len(metrics), 'templates_rebuilt': len(columns),
                          'total_positions': len(self.index), 'pricing_graph': graph,
                          'ffi_request_bytes': self.native.bytes_sent - sent},
                    timings_ms={'pricing_and_coefficients': (priced-started)*1000,
                                'native_solve': (solved-priced)*1000,
                                'validation_and_publication': (time.perf_counter()-solved)*1000,
                                'total': (time.perf_counter()-started)*1000})
                self.result = result
                return result
            except BaseException:
                if self.version == version:
                    try:
                        self._call('abort', token=token)
                    except Exception:
                        # A failed native actor cannot safely resume; preserve the
                        # original validation error and retire its handle.
                        try:
                            self.close()
                        except Exception:
                            self.handle = None
                raise

    @staticmethod
    def _snapshot(snapshot):
        from ..core.lifecycle_native import unit_output, kpi_output
        libraries=[unit_output(row['data'], {t['name']:t['template'] for t in row['templates']})
                   for row in snapshot['libraries']]
        return libraries, kpi_output(snapshot['bases'])

    def _update_owned(self, version, edits, templates, constraints, publish_guard):
        started, sent=time.perf_counter(), self.native.bytes_sent
        candidate=self._call('update_owned', edits=edits, templates=templates,
                             **({'constraints':constraints} if constraints is not None else {}))
        token=candidate['token']
        try:
            libraries,bases=(self._snapshot(candidate['snapshot']) if 'snapshot' in candidate
                             else (self.libraries,self.bases))
            solved=time.perf_counter()
            with run_context(replace(self.config, compute_backend='python')):
                validate_replay(candidate,libraries,bases,candidate['constraints'])
            with publish_guard() if publish_guard else nullcontext():
                result=self._call('publish',token=token)
                self.version=result['version']
                self.bases,self.libraries=bases,libraries
            result.update(validated=bool(result['feasible']),context=self.context,native_backend=self.native.identity,
                validation='Independent Python allocation replay passed' if result['feasible'] else 'No feasible allocation; no allocation published',
                scenarios=[m[0] for m in self.markets],horizon_months=self.config.horizon,
                initialization_ms=self.initialization_ms,execution=candidate['execution'],
                work=candidate['work'] | {'ffi_request_bytes':self.native.bytes_sent-sent},
                timings_ms=candidate['timings_ms'] | {'validation_and_publication':(time.perf_counter()-solved)*1000,
                    'total':(time.perf_counter()-started)*1000})
            self.result=result
            return result
        except BaseException:
            if self.version==version:
                try: self._call('abort',token=token)
                except Exception:
                    try: self.close()
                    except Exception: self.handle=None
            raise

    def evaluate(self, allocation, version):
        with self.lock:
            if version != self.version:
                raise ValueError('stale session version')
            return self._call('eval', allocation=allocation)

    def close(self):
        with self.lock:
            if self.handle is not None:
                self._call('close')
                self.handle = None
