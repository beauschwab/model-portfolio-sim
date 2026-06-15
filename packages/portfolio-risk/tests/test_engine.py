"""Accuracy and invariant tests. Run: pytest tests/ -v
Small portfolio / path counts to keep JIT-dominated runtime tolerable."""
import numpy as np
import pytest

from portfolio_risk import config, demo
from portfolio_risk.config import (MOY, PREPAY_PARAMS, RATIONAL_SIGMOID,
                             SEASONALITY, SEED, STRESS_HORIZONS_M)
from portfolio_risk.kernels import engine, stress_engine
from portfolio_risk.prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS, LTV_KNOTS,
                             SMM_LUT, SMM_SCALE)
from portfolio_risk.pricing import solve_oas_from_A
from portfolio_risk.scenarios import (CRN, build_paths, run_engine, setup,
                                shocked_paths)

N_SEC, N_P = 200, 64


@pytest.fixture(scope="module")
def env():
    port = demo.demo_portfolio(N_SEC)
    sr, vp = demo.demo_market()
    cch, psh = demo.demo_histories()
    models, B, abcd0, sec, tgt, face = setup(port, sr, vp, cch, psh)
    crn = CRN(N_P, SEED)
    base = build_paths(sr, vp, abcd0, B, models, crn)
    return dict(port=port, models=models, sec=sec, tgt=tgt, base=base,
                crn=crn)


def _engine_call(paths, sec, oas, horizons, want_fwd, rational):
    return engine(paths["mtg"], paths["hpi"], paths["yoy"], paths["df"],
                  MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
                  SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE, *sec,
                  oas, horizons, want_fwd, rational)


def test_zero_shock_invariant(env):
    """stress_engine restarted from checkpoints with a ZERO shock must
    reproduce the base engine's forward values column-exactly."""
    sec, base, models = env["sec"], env["base"], env["models"]
    oas = np.full(N_SEC, 0.012)
    hz = STRESS_HORIZONS_M
    _, FVb, _, ck_bal, ck_burn, *_ = _engine_call(base, sec, oas, hz, True,
                                              RATIONAL_SIGMOID)
    for hi in (0, 13, 26):
        h = int(hz[hi])
        sp = shocked_paths(base, h, 0.0, models)
        fv = stress_engine(sp["mtg"], sp["hpi"], sp["yoy"], sp["df"],
                           MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS,
                           LTV_COEFS, SMM_LUT, SMM_SCALE, BURN_LUT,
                           BURN_SCALE, *sec, oas, h, hi, ck_bal, ck_burn,
                           RATIONAL_SIGMOID)
        np.testing.assert_allclose(fv, FVb[:, hi], rtol=1e-6,
                                   err_msg=f"horizon {h}")


def test_rational_sigmoid_oas_accuracy(env):
    """Pade(7,6) logistics vs exact exp: OAS within 0.05bp everywhere."""
    sec, base, tgt = env["sec"], env["base"], env["tgt"]
    hz = np.zeros(1, dtype=np.int64)
    z = np.zeros(N_SEC)
    A_r, *_ = _engine_call(base, sec, z, hz, False, True)
    A_e, *_ = _engine_call(base, sec, z, hz, False, False)
    oas_r, _ = solve_oas_from_A(A_r, N_P, tgt)
    oas_e, _ = solve_oas_from_A(A_e, N_P, tgt)
    assert np.abs(oas_r - oas_e).max() * 1e4 < 0.05


def test_oas_roundtrip(env):
    """Solved OAS reprices to target within solver tolerance."""
    sec, base, tgt = env["sec"], env["base"], env["tgt"]
    A, *_ = _engine_call(base, sec, np.zeros(N_SEC),
                         np.zeros(1, dtype=np.int64), False,
                         RATIONAL_SIGMOID)
    oas, px = solve_oas_from_A(A, N_P, tgt)
    assert np.abs(px - tgt).max() < 1e-7


def test_stress_pnl_signs(env):
    """Up-shock loses money, down-shock gains, monotone in shock size."""
    sec, base, models = env["sec"], env["base"], env["models"]
    oas = np.full(N_SEC, 0.012)
    hz = STRESS_HORIZONS_M
    _, FVb, _, ck_bal, ck_burn, *_ = _engine_call(base, sec, oas, hz, True,
                                              RATIONAL_SIGMOID)
    hi, h = 8, int(STRESS_HORIZONS_M[8])

    def fv(dbp):
        sp = shocked_paths(base, h, dbp, models)
        return stress_engine(sp["mtg"], sp["hpi"], sp["yoy"], sp["df"],
                             MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS,
                             LTV_COEFS, SMM_LUT, SMM_SCALE, BURN_LUT,
                             BURN_SCALE, *sec, oas, h, hi, ck_bal, ck_burn,
                             RATIONAL_SIGMOID).sum()

    f0, fup1, fup2, fdn = FVb[:, hi].sum(), fv(100.0), fv(200.0), fv(-100.0)
    assert fdn > f0 > fup1 > fup2


def test_batched_pv_matches_engine():
    """Gate for the 3rd MODEL-BLOCK copy: batched_pv_engine on a single
    scenario must equal pv_from_A(engine A) at the same OAS."""
    import numpy as np
    from portfolio_risk import demo
    from portfolio_risk.config import (MOY, PREPAY_PARAMS, RATIONAL_SIGMOID,
                                 SEASONALITY)
    from portfolio_risk.kernels import batched_pv_engine
    from portfolio_risk.prepay import (BURN_LUT, BURN_SCALE, LTV_COEFS,
                                 LTV_KNOTS, SMM_LUT, SMM_SCALE)
    from portfolio_risk.pricing import pv_from_A
    from portfolio_risk.scenarios import CRN, build_paths, run_engine, setup
    sr, vp = demo.demo_market()
    port = demo.demo_portfolio(25)
    cc, ps = demo.demo_histories()
    models, B, abcd0, sec, tgt, face = setup(port, sr, vp, cc, ps)
    crn = CRN(64, 7)
    paths = build_paths(sr, vp, abcd0, B, models, crn)
    oas = np.full(25, 0.004)
    A, *_ = run_engine(paths, sec)
    ref = pv_from_A(A, oas, crn.n)
    scen = np.zeros(crn.n, dtype=np.int64)
    pv = batched_pv_engine(
        np.ascontiguousarray(paths["mtg"]),
        np.ascontiguousarray(paths["hpi"]),
        np.ascontiguousarray(paths["yoy"]),
        np.ascontiguousarray(paths["df"]),
        scen, 1, MOY, SEASONALITY, PREPAY_PARAMS, LTV_KNOTS, LTV_COEFS,
        SMM_LUT, SMM_SCALE, BURN_LUT, BURN_SCALE, *sec, oas,
        np.zeros(25), RATIONAL_SIGMOID)[0] / crn.n
    np.testing.assert_allclose(pv, ref, rtol=1e-10)
