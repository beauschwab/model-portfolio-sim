"""Base-market OAS calibration, reusable by named-scenario revaluations."""
from __future__ import annotations

import numpy as np

from ..core.config import N_PATHS_SENS, SWAP_TENORS
from ..core.curve import bootstrap_curve, forwards_from_dfs
from ..core.pricing import solve_oas_from_A
from ..core.runtime import path_count
from ..core.scenarios import (CRN, build_rate_paths, port_delay, setup,
                              solve_base_oas)
from ..core.vol import calibrate_abcd, factor_loadings


def calibrate_books(bs, swap_rates, vol_pts, dep_hist, seed=7):
    """Solve once under the original market, never under a named scenario."""
    from ..core.quant_native import enabled,term_call
    if enabled():
        from ..core.lifecycle_native import accounting_request
        selected={key:bs[key] for key in ('mbs','loans','debt','cds','deposits')
                  if bs.get(key) is not None and len(bs[key])}
        if 'mbs' in selected:selected['mbs_hists']=bs['mbs_hists']
        # Non-calibrated books are outside this contract. Empty books are omitted
        # just as in the reference; only dated term contracts require asof.
        asof=bs['asof'] if any(key in selected for key in ('loans','debt','cds')) else bs.get('asof')
        request=accounting_request(selected,swap_rates,vol_pts,dep_hist,1,seed,asof,None,None,False,None,False)
        return {k:np.asarray(v) for k,v in term_call('financial-controller-1',dict(op='calibrate',books=request)).items()}
    from ..products.corp import CorpDeck, _corp_A, corp_solve_oas
    from ..products.cds import CDDeck, _cd_A
    from ..products.deposits import DepositDeck, LogisticBetaECM, _deposit_A

    B = factor_loadings()
    dfs = bootstrap_curve(SWAP_TENORS, swap_rates)
    abcd = calibrate_abcd(vol_pts, forwards_from_dfs(dfs), dfs, B)
    crn = CRN(path_count(N_PATHS_SENS), seed)
    paths = build_rate_paths(swap_rates, vol_pts, abcd, B, crn)
    result = {}
    for name in ("mbs", "loans", "debt", "cds", "deposits"):
        frame = bs.get(name)
        if frame is None or not len(frame):
            continue
        if name == "mbs":
            models, b, a, sec, target, _ = setup(
                frame, swap_rates, vol_pts, *bs["mbs_hists"])
            result[name] = solve_base_oas(
                swap_rates, vol_pts, a, b, models, sec, target,
                seed=seed, delay_y=port_delay(frame))[0]
        elif name in ("loans", "debt", "cds"):
            cls, engine = (CDDeck, _cd_A) if name == "cds" else (CorpDeck, _corp_A)
            deck = cls(frame, bs["asof"])
            result[name] = corp_solve_oas(deck, engine(deck, paths), crn.n)[0]
        else:
            deck = DepositDeck(frame)
            model = LogisticBetaECM()
            params = model.fit(dep_hist)
            r0 = float(model.equilibrium(params, paths["short"][:, 0].mean()))
            dep = model.paths(paths["short"].astype(np.float64), params, r0)
            A = _deposit_A(deck, paths, dep, r0)[0]
            result[name] = solve_oas_from_A(A, crn.n, deck.tgt, lo0=-0.15)[0]
    return result


def spread_oas(oas_by_book, shift):
    """Apply an explicit spread shock without recalibrating base OAS."""
    from ..core.quant_native import enabled,term_call
    if enabled():
        result=term_call('financial-controller-1',dict(op='spread',oas={k:np.asarray(v).tolist() for k,v in oas_by_book.items()},shift=shift))
        return {k:np.asarray(v) for k,v in result.items()}
    return {k: v + (shift if k in {"mbs", "loans", "debt", "cds"} else 0)
            for k, v in oas_by_book.items()}
