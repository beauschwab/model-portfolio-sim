"""CLI: python -m mbs_risk [N_SEC] [bench|risk|stress|all]"""
from __future__ import annotations

import sys
import time

import numpy as np
import polars as pl

from .config import (N_PATHS_SENS, RATIONAL_SIGMOID, STRESS_HORIZONS_M,
                     STRESS_SHOCKS_BP, SEED, USE_FLOAT32)
from .demo import demo_histories, demo_market, demo_portfolio
from .risk import run_risk
from .scenarios import CRN, build_paths, run_engine, setup, shocked_paths
from .stress import run_stress


def _bench(n_sec: int):
    port = demo_portfolio(n_sec)
    swap_rates, vol_pts = demo_market()
    cc_hist, ps_hist = demo_histories()
    models, B, abcd0, sec, tgt, face = setup(
        port, swap_rates, vol_pts, cc_hist, ps_hist)
    crn = CRN(N_PATHS_SENS, SEED)
    base = build_paths(swap_rates, vol_pts, abcd0, B, models, crn)
    oas = np.full(n_sec, 0.01)

    # JIT warmup on a small slice
    from .kernels import stress_engine
    from .config import MOY, PREPAY_PARAMS, SEASONALITY
    from .prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS,
                         SMM_LUT, SMM_SCALE)
    sec8 = tuple(a[:8] for a in sec)
    _, _, _, cb8, cu8 = run_engine(base, sec8, oas[:8], STRESS_HORIZONS_M,
                                   True)
    _ = stress_engine(base["mtg"], base["hpi"], base["yoy"], base["df"],
                      MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
                      SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE, *sec8,
                      oas[:8], 14, 13, cb8, cu8, RATIONAL_SIGMOID)

    t0 = time.perf_counter()
    _, FV, BAL, ck_bal, ck_burn, *_ = run_engine(base, sec, oas,
                                             STRESS_HORIZONS_M, True)
    t_eng = time.perf_counter() - t0

    sp = shocked_paths(base, 14, 100.0, models)
    t0 = time.perf_counter()
    _ = stress_engine(sp["mtg"], sp["hpi"], sp["yoy"], sp["df"],
                      MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
                      SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE, *sec,
                      oas, 14, 13, ck_bal, ck_burn, RATIONAL_SIGMOID)
    t_str = time.perf_counter() - t0

    t0 = time.perf_counter()
    _ = build_paths(swap_rates, vol_pts, abcd0, B, models, crn)
    t_pth = time.perf_counter() - t0

    n_str = len(STRESS_SHOCKS_BP) * len(STRESS_HORIZONS_M)
    total = 38 * (t_eng + t_pth) + n_str * t_str + 2 * (t_eng + t_pth)
    print(f"\n[bench] {n_sec} secs x {N_PATHS_SENS} paths "
          f"(f32={USE_FLOAT32}, rational={RATIONAL_SIGMOID})")
    print(f"[bench] engine pass (A+FV+ckpts, 27 hz): {t_eng:.2f}s | "
          f"stress pass (1 hz, ckpt restart @h=14): {t_str:.2f}s | "
          f"path build: {t_pth:.2f}s")
    print(f"[bench] projected full risk+stress "
          f"(38 reval + {n_str} stress passes): ~{total:.0f}s single-core")


def main():
    n_sec = int(sys.argv[1]) if len(sys.argv) > 1 else 10_000
    mode = sys.argv[2] if len(sys.argv) > 2 else "all"

    if mode == "bench":
        _bench(n_sec)
        return

    port = demo_portfolio(n_sec)
    swap_rates, vol_pts = demo_market()
    cc_hist, ps_hist = demo_histories()
    t0 = time.perf_counter()
    if mode in ("risk", "all"):
        risk = run_risk(port, swap_rates, vol_pts, cc_hist, ps_hist)
        print(risk.select("cusip", "oas_bps", "dv01", "krd01_10y",
                          "vega_1x10").head(5))
        risk.write_parquet("risk_results.parquet")
        print(f"[risk] done at {time.perf_counter()-t0:.1f}s")
    if mode in ("stress", "all"):
        pos, agg, prof = run_stress(port, swap_rates, vol_pts,
                                    cc_hist, ps_hist)
        print("\nStress P&L by horizon x shock ($):")
        print(agg.filter(pl.col("horizon_m").is_in([3, 9, 18, 27])))
        if prof is not None:
            print("\nForward DV01 profile (quarter-ends, $/bp):")
            print(prof.filter(pl.col("horizon_m") % 3 == 0))
        pos.write_parquet("stress_results.parquet")
    print(f"\nTotal wall: {time.perf_counter()-t0:.1f}s ({n_sec} positions)")


if __name__ == "__main__":
    main()
