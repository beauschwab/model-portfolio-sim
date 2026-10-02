"""Incremental spot pricing on the existing product models.

Graph: market/history -> shared paths -> instrument cashflows -> base OAS
      -> scenario/spread mark -> position value -> selected-book totals.

Only missing instrument nodes are grouped into product batches. A saved book
or base-market edit establishes new calibration inputs. Temporary market and
spread shocks always use OAS from the unshocked inputs. Optional incremental
curve risk and NII reuse these nodes; stress and strategy builds remain separate.
"""
from __future__ import annotations

from types import SimpleNamespace

import numpy as np
import polars as pl

from ..core.batch import CashflowBatch, DiscountBackend, NumpyDiscountBackend
from ..core.config import SWAP_TENORS, TGRID, CURVE_BUMP, HPI_MU
from ..core.curve import bootstrap_curve, forwards_from_dfs
from ..core.dependency import DependencyCache, Evaluation, fingerprint
from ..core.pricing import solve_oas_from_A
from ..core.runtime import RunConfig, run_context
from ..core.scenarios import (CRN, build_paths, build_rate_paths, extract_sec,
                              run_engine)
from ..core.vol import calibrate_abcd, factor_loadings

SUPPORTED_BOOKS = ("mbs", "loans", "debt", "deposits", "cds")
MODEL_REVISION = "portfolio-risk-0.28.0"


def price_books(books, *, asof, swap_rates, vol_pts, cache: DependencyCache,
                config: RunConfig | None = None, seed=7, mbs_hists=None,
                dep_hist=None, scenario_market=None, spread_shift=0.0,
                spread_overrides_bp=None, backend: DiscountBackend | None = None,
                valuation_books=None, include_analytics=False, balance_sheet_extras=None,
                include_key_rates=True):
    """Return ordered position prices, scope totals and actual graph work counts.

    spread_shift is a decimal scenario shift on securities (not NMDs), matching
    calibration.spread_oas. Per-instrument overrides are ADDITIVE basis points,
    keyed {book: {stable_id: bp}}, including NMDs when explicitly requested.
    Input frames must satisfy the normal engine contracts and unique string IDs.
    config is explicit: an ambient run_context cannot change this graph's keys.
    """
    from ..products.cds import CD_EW_PARAMS
    from ..products.deposits import SEGMENTS

    config = config or RunConfig()
    valuation_books = books if valuation_books is None else valuation_books
    if set(valuation_books) != set(books):
        raise ValueError('valuation books must match the calibration scope')
    native_graph = backend is None and config.compute_backend == "rust"
    if backend is None:
        from ..core.native import discount_backend
        backend = discount_backend('rust' if config.compute_backend == 'rust' else 'numpy')
    overrides = spread_overrides_bp or {}
    unknown = set(books) - set(SUPPORTED_BOOKS)
    if unknown or set(overrides) - set(books):
        raise ValueError(f"unsupported or out-of-scope pricing books: {sorted(unknown | (set(overrides) - set(books)))}")
    if not np.isfinite(spread_shift):
        raise ValueError("scenario spread must be finite")
    if not isinstance(backend.identity, str) or not backend.identity:
        raise ValueError("backend must declare a versioned identity")
    for book, frame in books.items():
        id_col = "cusip" if book == "mbs" else "id"
        ids = frame[id_col].to_list()
        if valuation_books[book][id_col].to_list() != ids:
            raise ValueError('valuation instruments must match baseline IDs and order')
        if len(set(ids)) != len(ids) or any(not isinstance(i, str) or not i for i in ids):
            raise ValueError(f"{book}: position IDs must be unique nonempty strings")
        if set(overrides.get(book, {})) - set(ids):
            raise ValueError(f"{book}: spread override refers to an unknown position")
        if any(not np.isfinite(v) or abs(v) > 2000 for v in overrides.get(book, {}).values()):
            raise ValueError("instrument spread shifts must be finite and within +/-2000 bp")

    if native_graph:
        from ..core.graph_native import price_books as native_price_books
        return native_price_books(books, valuation_books=valuation_books, asof=asof, swap_rates=swap_rates,
            vol_pts=vol_pts, cache=cache, config=config, seed=seed, mbs_hists=mbs_hists, dep_hist=dep_hist,
            scenario_market=scenario_market, spread_shift=spread_shift, spread_overrides_bp=overrides,
            include_analytics=include_analytics, balance_sheet_extras=balance_sheet_extras,
            include_key_rates=include_key_rates, backend_identity=backend.identity, model_revision=MODEL_REVISION)

    with cache._lock:
        if cache._native is not None:
            if config.compute_backend == 'rust':
                # An explicitly supplied custom pricing service is a reference
                # extension path. It must not consume a second retained budget
                # or discard the selected native graph's immutable snapshots.
                cache = DependencyCache(max_bytes=0, max_entries=0)
            else:
                cache._native.close()
                cache._native = None
    evaluation = Evaluation(cache)
    crns = {}
    base_market = (np.asarray(swap_rates), np.asarray(vol_pts))
    current_market = base_market if scenario_market is None else tuple(map(np.asarray, scenario_market))
    histories_key = fingerprint(mbs_hists) if "mbs" in books else None
    deposit_history_key = fingerprint(dep_hist) if "deposits" in books else None
    segments = SEGMENTS if config.deposit_segments is None else config.deposit_segments
    ew_params = CD_EW_PARAMS if config.cd_ew_params is None else config.cd_ew_params

    def path_key(market, n_paths, mortgage, anchor=None):
        return fingerprint("paths", MODEL_REVISION, config.compute_backend, market, n_paths, seed,
                           histories_key if mortgage else None, market if anchor is None else anchor)

    def paths(market, n_paths, mortgage, anchor=None):
        anchor = market if anchor is None else anchor
        key = path_key(market, n_paths, mortgage, anchor)

        def build():
            rates, vols = market
            market_key = fingerprint("market", MODEL_REVISION, config.compute_backend, anchor)

            def fit_market():
                B = factor_loadings()
                dfs = bootstrap_curve(SWAP_TENORS, anchor[0])
                return B, calibrate_abcd(anchor[1], forwards_from_dfs(dfs), dfs, B)

            B, abcd = evaluation.one("market_fit", market_key, fit_market)
            if n_paths not in crns:
                crns[n_paths] = CRN(n_paths, seed)
            if mortgage:
                from ..core.interfaces import ModelSuite
                suite = ModelSuite.default()

                def fit_models():
                    if mbs_hists is None:
                        raise ValueError("MBS pricing requires current-coupon and spread histories")
                    return {"cc": suite.cc.fit(mbs_hists[0]), "ps": suite.ps.fit(mbs_hists[1]),
                            "ps_spot": mbs_hists[2] if len(mbs_hists) > 2 else 0.012}

                models = evaluation.one("mortgage_fit", fingerprint(MODEL_REVISION, config.compute_backend, histories_key), fit_models)
                return build_paths(rates, vols, abcd, B, models, crns[n_paths], suite=suite)
            return build_rate_paths(rates, vols, abcd, B, crns[n_paths])

        return evaluation.one("mortgage_paths" if mortgage else "rate_paths", key, build)

    def cashflows(book, frame, rows, market, n_paths, anchor=None):
        from .whatif import DEFAULTS
        mortgage = book == "mbs"
        pk = path_key(market, n_paths, mortgage, anchor)
        # These fields affect calibration, scaling or labels, never unit cashflows.
        excluded = {"id", "cusip", "price", "face", "current_face", "balance", "book_yield"}
        keys = []
        for row in rows:
            terms = {k: v for k, v in row.items() if k not in excluded and v is not None
                     and not (k in DEFAULTS and v == DEFAULTS[k])}
            if mortgage:
                # Adding an optional HPI override column must not invalidate
                # untouched rows whose explicit values equal the model default.
                terms.setdefault('hpi_orig_ratio', (1 + HPI_MU) ** (float(row['age']) / 12))
            assumption = (segments[row["segment"]] if book == "deposits" else
                          ew_params if book == "cds" else None)
            keys.append(fingerprint("cashflows", MODEL_REVISION, book,
                                    row["cusip" if mortgage else "id"],
                                    terms,
                                    asof, pk, assumption,
                                    deposit_history_key if book == "deposits" else None))

        def build(indices):
            subset = frame[indices]
            p = paths(market, n_paths, mortgage, anchor)
            if book in ("loans", "debt", "cds"):
                from ..products.corp import CorpDeck, _corp_full
                from ..products.cds import CDDeck, _cd_full
                cls, engine = (CDDeck, _cd_full) if book == "cds" else (CorpDeck, _corp_full)
                deck = cls(subset, asof)
                A, I, P = engine(deck, p)
                return [(A[a:b], deck.t_pay[a:b], I[a:b], P[a:b], deck.acc_m[a:b], deck.pay_m[a:b])
                        for a, b in zip(deck.per_off[:-1], deck.per_off[1:])]
            if mortgage:
                A, _, _, _, _, I, P = run_engine(p, extract_sec(subset))
                delays = (subset["pay_delay_days"].to_numpy() / 365.0
                          if "pay_delay_days" in subset.columns else np.zeros(len(subset)))
            else:
                from ..products.deposits import DepositDeck, LogisticBetaECM, _deposit_A
                model = LogisticBetaECM()
                params = evaluation.one("deposit_fit", fingerprint(MODEL_REVISION, config.compute_backend, deposit_history_key),
                                        lambda: model.fit(dep_hist))

                def paid_rates():
                    # Freeze the paid-rate anchor for risk bump sides, matching
                    # existing deposit risk. Named scenarios establish their own spot.
                    reference = p if anchor is None else paths(anchor, n_paths, False)
                    r0 = float(model.equilibrium(params, reference["short"][:, 0].mean()))
                    return model.paths(p["short"].astype(np.float64), params, r0), r0

                dep, r0 = evaluation.one("deposit_paths", fingerprint(pk, deposit_history_key), paid_rates)
                deck = DepositDeck(subset)
                for field, attr in [('attrition_base', 'base'), ('attrition_amp', 'fl_amp'),
                                    ('attrition_slope', 'fl_b'), ('attrition_gap', 'fl_g0')]:
                    if field in subset.columns:
                        values = subset[field].to_numpy()
                        setattr(deck, attr, np.where(np.isfinite(values), values, getattr(deck, attr)))
                A, P, _, _, _, I = _deposit_A(deck, p, dep, r0)
                delays = np.zeros(len(subset))
            return [(row, TGRID + delay, interest, principal) for row, delay, interest, principal in zip(A, delays, I, P)]

        return keys, evaluation.batch(f"cashflows:{book}", keys, build)

    def marks(book, keys, cf, oas_keys, applied, n_paths):
        pv_keys = [fingerprint('mark', key, ok, float(oas), backend.identity)
                   for key, ok, oas in zip(keys, oas_keys, applied)]

        def price(indices):
            batch = CashflowBatch.from_rows([cf[i][:2] for i in indices], n_paths)
            result = np.asarray(backend.price(batch, applied[indices]), dtype=float)
            if result.shape != (len(indices),) or not np.isfinite(result).all():
                raise ValueError('pricing backend must return one finite PV per instrument')
            return result
        return np.array(evaluation.batch(f'marks:{book}', pv_keys, price))

    def monthly_flows(book, cf, n_paths):
        from .accounting import smear_csr, bucket_csr
        from ..core.config import N_STEPS
        if book in ('mbs', 'deposits'):
            return np.stack([r[2] for r in cf]) / n_paths, np.stack([r[3] for r in cf]) / n_paths
        offsets = np.cumsum([0] + [len(r[0]) for r in cf])
        acc, pay = (np.concatenate([r[i] for r in cf]) for i in (4, 5))
        I, P = (np.concatenate([r[i] for r in cf]) / n_paths for i in (2, 3))
        return (smear_csr(offsets, acc, pay, I, N_STEPS, len(cf)),
                bucket_csr(offsets, pay, P, N_STEPS, len(cf)))

    output, totals, dv01s, nii_cols, runoff_cols = {}, {}, {}, {}, {}
    earning_balance = np.zeros(config.horizon)
    # Install exactly the assumptions used by the keys, even for standalone users.
    with run_context(config):
        for book, frame in books.items():
            if not len(frame):
                output[book] = pl.DataFrame(schema={"id": pl.String, "base_oas_bp": pl.Float64,
                    "applied_oas_bp": pl.Float64, "model_price": pl.Float64,
                    "notional": pl.Float64, "market_value": pl.Float64})
                totals[book] = 0.0
                continue
            rows = frame.to_dicts()
            valued_frame = valuation_books[book]
            valued_rows = valued_frame.to_dicts()
            # Polars equality can report False even against itself for Object
            # columns (CD call schedules). Identity is only a same-request
            # shortcut here; persistent cache keys remain content-addressed.
            unchanged = (fingerprint(base_market) == fingerprint(current_market)
                         and (valued_frame is frame or valued_frame.equals(frame)))
            n_paths = config.n_paths_base if book == "mbs" else config.n_paths
            base_keys, base_cf = cashflows(book, frame, rows, base_market, n_paths)
            targets = frame["price"].to_numpy().astype(float) / 100.0
            oas_keys = [fingerprint("base_oas", key, target) for key, target in zip(base_keys, targets)]

            def solve(indices):
                cf = [base_cf[i] for i in indices]
                if book in ("mbs", "deposits"):
                    delays = np.array([r[1][0] - TGRID[0] for r in cf])
                    return solve_oas_from_A(np.stack([r[0] for r in cf]), n_paths,
                        targets[indices], delay_y=delays, lo0=-0.15 if book == "deposits" else -0.05)[0]
                from ..products.corp import corp_solve_oas
                batch = CashflowBatch.from_rows([r[:2] for r in cf], n_paths)
                deck = SimpleNamespace(per_off=batch.offsets, t_pay=batch.times,
                                       tgt=targets[indices], n=len(indices))
                return corp_solve_oas(deck, batch.values, n_paths)[0]

            base_oas = np.array(evaluation.batch(f"calibration:{book}", oas_keys, solve))
            if unchanged:
                mark_keys, mark_cf = base_keys, base_cf
            else:
                mark_keys, mark_cf = cashflows(book, valued_frame, valued_rows, current_market, n_paths)
            ids = frame["cusip" if book == "mbs" else "id"].to_list()
            shift = spread_shift if book != "deposits" else 0.0
            applied = base_oas + shift + np.array([overrides.get(book, {}).get(i, 0.0) * 1e-4 for i in ids])
            pv = marks(book, mark_keys, mark_cf, oas_keys, applied, n_paths)
            notional = valued_frame[{"mbs": "current_face", "loans": "face", "debt": "face",
                              "deposits": "balance", "cds": "balance"}[book]].to_numpy()
            mv = pv * notional
            output[book] = pl.DataFrame({"id": ids, "base_oas_bp": base_oas * 1e4,
                "applied_oas_bp": applied * 1e4, "model_price": pv * 100,
                "notional": notional, "market_value": mv})
            totals[book] = float(mv.sum())
            if include_analytics:
                # Sensitivities use the existing sensitivity path count and a
                # fixed current-market abcd fit on both sides of each curve bump.
                risk_columns = {}
                measures = [('dv01', np.ones(len(SWAP_TENORS)))]
                if include_key_rates:
                    measures += [(f'krd01_{int(t)}y', np.eye(len(SWAP_TENORS))[j])
                                 for j, t in enumerate(SWAP_TENORS)]
                for label, direction in measures:
                    # Preserve the full drivers' 1bp key-rate convention and
                    # the balance-sheet KPI's 25bp parallel convention.
                    bump_bp = 25.0 if label == 'dv01' else CURVE_BUMP * 1e4
                    values = []
                    for sign in (-1, 1):
                        market = (current_market[0] + sign * bump_bp * 1e-4 * direction, current_market[1])
                        keys, cf = cashflows(book, valued_frame, valued_rows, market, config.n_paths, current_market)
                        values.append(marks(book, keys, cf, oas_keys, applied, config.n_paths))
                        del cf  # A completed sensitivity leg need not stay live through NII.
                    risk_columns[label] = (values[0] - values[1]) / (2 * bump_bp) * notional
                output[book] = output[book].with_columns([pl.Series(k, v) for k, v in risk_columns.items()])
                dv01s[book] = float(risk_columns['dv01'].sum())
                # These immutable cashflows are already live in this request.
                # Global LRU eviction must not force us to calculate them again.
                # MBS calibration may use more paths than income/sensitivities;
                # never reuse its matrices across different path counts.
                if n_paths == config.n_paths:
                    nk, ncf = mark_keys, mark_cf
                    bk, bcf = base_keys, base_cf
                else:
                    nk, ncf = cashflows(book, valued_frame, valued_rows, current_market, config.n_paths)
                    if unchanged:
                        bk, bcf = nk, ncf
                    else:
                        bk, bcf = cashflows(book, frame, rows, base_market, config.n_paths)
                income_keys = [fingerprint('income', key, base_key, target, row.get('book_yield'), config.horizon)
                               for key, base_key, target, row in zip(nk, bk, targets, rows)]

                def income(indices):
                    from .accounting import book_yield
                    I, P = monthly_flows(book, [ncf[i] for i in indices], config.n_paths)
                    H = config.horizon
                    if book in ('deposits', 'cds'):
                        return [(i[:H], p[:H], np.zeros(H)) for i, p in zip(I, P)]
                    bI, bP = monthly_flows(book, [bcf[i] for i in indices], config.n_paths)
                    if 'book_yield' in frame.columns:
                        yields = frame['book_yield'].to_numpy()[indices]
                        value = np.ones(len(indices))
                    else:
                        yields = book_yield(bI + bP, targets[indices])
                        value = targets[indices].copy()
                    amounts, balances = np.empty((len(indices), H)), np.empty((len(indices), H))
                    for month in range(H):
                        amounts[:, month] = value * yields / 12
                        value = value + amounts[:, month] - I[:, month] - P[:, month]
                        balances[:, month] = value
                    return [(i, p[:H], b) for i, p, b in zip(amounts, P, balances)]

                monthly = evaluation.batch(f'income:{book}', income_keys, income)
                amounts = np.stack([r[0] for r in monthly]) * notional[:, None]
                nii_cols[book] = amounts.sum(0)
                runoff_cols[book] = (np.stack([r[1] for r in monthly]) * notional[:, None]).sum(0)
                if book in ('mbs', 'loans'):
                    earning_balance += (np.stack([r[2] for r in monthly]) * notional[:, None]).sum(0)
                output[book] = output[book].with_columns(pl.Series('nii_total', amounts.sum(1) * (1 if book in ('mbs', 'loans') else -1)))
                del ncf, bcf, monthly, amounts
            del base_cf, mark_cf
    assets = sum(v for k, v in totals.items() if k in {"mbs", "loans"})
    liabilities = sum(v for k, v in totals.items() if k in {"debt", "deposits", "cds"})
    result = {"positions": output, "totals": totals,
            "scope_net_value": assets - liabilities,
            "graph": evaluation.stats, "cache": cache.info(), "backend": backend.identity,
            "model_revision": MODEL_REVISION,
            "calibration_policy": "saved books/base market recalibrate; temporary instrument assumptions/scenarios hold base OAS"}
    if include_analytics:
        from .kpis import eve_summary, lcr, nsfr, capital
        monthly_income = sum((v for k, v in nii_cols.items() if k in ('mbs', 'loans')), np.zeros(config.horizon))
        monthly_expense = sum((v for k, v in nii_cols.items() if k not in ('mbs', 'loans')), np.zeros(config.horizon))
        bs = {k: v for k, v in valuation_books.items() if len(v)} | {'asof': asof}
        extras = balance_sheet_extras or {}
        bs.update(extras)
        if extras.get('mm') is not None or extras.get('hedges') is not None:
            # These unedited auxiliary books are shared portfolio nodes, not
            # fabricated instrument Greeks. Their existing drivers remain authoritative.
            from .accounting import run_balance_sheet_nii
            from .kpis import parallel_dv01s
            extra_key = fingerprint('auxiliary', MODEL_REVISION, config.compute_backend, extras, asof, current_market, seed, config.n_paths, config.horizon)

            def auxiliary():
                with run_context(config):
                    other = run_balance_sheet_nii(extras | {'asof': asof}, *current_market, dep_hist,
                                                  horizon=config.horizon, seed=seed, asof=asof)
                    dv = parallel_dv01s(extras | {'asof': asof}, *current_market, dep_hist, seed=seed)
                return other['monthly']['interest_income'].to_numpy(), other['monthly']['interest_expense'].to_numpy(), dv

            extra_i, extra_e, extra_dv = evaluation.one('auxiliary', extra_key, auxiliary)
            monthly_income += extra_i
            monthly_expense += extra_e
            dv01s.update(extra_dv)
        nii = pl.DataFrame({'month': np.arange(1, config.horizon + 1), 'interest_income': monthly_income,
                           'interest_expense': monthly_expense, 'nii': monthly_income - monthly_expense})
        valued = bs | {k: valuation_books[k].with_columns(output[k]['model_price'].alias('price')) for k in bs if k in output and len(output[k])}
        result['nii'] = {'monthly': nii, 'runoff': pl.DataFrame({'month': np.arange(1, config.horizon + 1), **runoff_cols}),
                         'total': float(nii['nii'].sum()), 'accounting': 'effective yields frozen to baseline; CDs/deposits contractual accrual'}
        result['kpis'] = {'eve': eve_summary(valued, dv01s), 'dv01s': dv01s, 'lcr': lcr(valued, asof),
                          'nsfr': nsfr(bs, asof), 'capital': capital(bs, nii),
                          'scope': list(books), 'includes_auxiliary': bool(extras)}
        result['cache'] = cache.info()
    return result
