"""Cross-driver parity, model boundaries, and allocation replay regressions."""
import datetime as dt
import importlib

import numpy as np
import polars as pl
import pytest

from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.core.scenarios import CRN


def test_odd_crn_has_exact_path_count():
    for n in (1, 3, 33):
        crn = CRN(n, 7)
        assert crn.Z.shape[0] == crn.eps_ps.shape[0] == crn.eps_h.shape[0] == n


def test_long_tenor_uses_explicit_terminal_forward_extrapolation():
    from portfolio_risk.core.lmm import _par_rate
    forwards = np.linspace(0.02, 0.06, 161)
    cash_dfs = np.cumprod(1 / (1 + 0.25 * forwards[np.minimum(np.arange(80, 200), 160)]))
    expected = (1 - cash_dfs[-1]) / (0.25 * cash_dfs.sum())
    assert _par_rate(forwards, 80, 120) == pytest.approx(expected, abs=1e-13)


def test_full_mbs_driver_parity_with_custom_model():
    from portfolio_risk.demo import demo_market, demo_histories, demo_portfolio
    from portfolio_risk.core.interfaces import ModelSuite
    from portfolio_risk.core.scenarios import setup, build_paths, run_engine, solve_base_oas
    from portfolio_risk.core.pricing import pv_from_A
    risk = importlib.import_module("portfolio_risk.analytics.risk")
    suite = ModelSuite.default()
    default_hpi = suite.hpi

    class CustomHPI:
        def paths(self, rates, eps):
            return default_hpi.paths(rates, eps) * 1.25

    suite.hpi = CustomHPI()
    sr, vp = demo_market()
    port = demo_portfolio(3)
    histories = demo_histories()
    with run_context(RunConfig(32, 32, compute_backend='python')):
        batch = risk.run_risk(port, sr, vp, *histories, suite=suite)
        models, B, abcd, sec, tgt, face = setup(port, sr, vp, *histories, suite=suite)
        oas, px = solve_base_oas(sr, vp, abcd, B, models, sec, tgt, suite=suite)
        crn = CRN(32, 7)

        def price(rates, vols, recal):
            paths = build_paths(rates, vols, abcd, B, models, crn,
                                recalibrate=recal, abcd_warm=abcd, suite=suite)
            return pv_from_A(run_engine(paths, sec, suite=suite)[0], oas, crn.n)

        reference = risk._run_risk_sequential(port, sr, vp, price, oas, px, face)
    cols = [c for c in batch.columns if c.startswith(("krd01_", "vega_"))]
    np.testing.assert_allclose(batch[cols].to_numpy(), reference[cols].to_numpy(), atol=1e-6, rtol=1e-8)


def test_run_context_reuses_paths_without_cross_run_leakage():
    from portfolio_risk.demo import demo_market
    from portfolio_risk.core.config import SWAP_TENORS
    from portfolio_risk.core.curve import bootstrap_curve, forwards_from_dfs
    from portfolio_risk.core.lmm import simulate_rates
    from portfolio_risk.core.vol import calibrate_abcd, factor_loadings
    sr, vp = demo_market()
    dfs = bootstrap_curve(SWAP_TENORS, sr)
    forwards = forwards_from_dfs(dfs)
    B = factor_loadings()
    with run_context(RunConfig(33, 33, compute_backend='python')) as context:
        a = calibrate_abcd(vp, forwards, dfs, B)
        assert calibrate_abcd(vp.copy(), forwards, dfs, B) is a
        first = simulate_rates(forwards, a, B, CRN(33, 7).Z)
        assert simulate_rates(forwards, a, B, CRN(33, 7).Z) is first
        assert context.hits >= 2
        assert first[0].shape[0] == 33
        assert not first[0].flags.writeable
    with run_context(RunConfig(32, 32, compute_backend='python')):
        second = simulate_rates(forwards, a, B, CRN(32, 7).Z)
        assert second[0].shape[0] == 32
        assert second is not first


def synthetic_library():
    weights = dict(hqla_l2a=0., outflow30=0., asf=0., rsf=0., rwa=0.)
    lib = dict(units=[dict(template="asset", h=0, side=1), dict(template="asset", h=24, side=1)],
               templates={"asset": weights}, horizon=27,
               nii=np.array([[1.] * 27, [2.] * 27]), balance=np.ones((2, 27)), dv01=np.zeros(2))
    base = {"nii_total_$": 100.}
    base.update(eve={"dv01_net_$": 0., "eve_$": 1000., "mv_assets_$": 1000.},
                lcr={"hqla_$": 1000., "hqla_l1_$": 1000., "net_outflows_$": 1.},
                nsfr={"asf_$": 1000., "rsf_$": 1.},
                capital={"cet1_path": [{"cet1_$": 1000.}], "rwa_total_$": 1.})
    return lib, base


def test_optimizer_replays_late_purchases_and_base_earnings():
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    from portfolio_risk.strategy.unitlib import evaluate_strategy
    lib, base = synthetic_library()
    result = optimize_balance_sheet([(lib, base)], max_total_assets=100, cash_budget=100)
    assert result["feasible"] and result["validated"]
    replay = evaluate_strategy(lib, result["allocation"], base)
    assert result["worst_case_nii_$"] == pytest.approx(100 + replay["nii_total_$"])
    assert result["allocation"][0]["purchase_m"] == 0
    assert result["worst_case_nii_$"] == pytest.approx(2800)


def test_optimizer_requires_funding_by_default():
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    lib, base = synthetic_library()
    result = optimize_balance_sheet([(lib, base)], max_total_assets=100)
    assert result["feasible"]
    assert result["total_new_assets_$"] == pytest.approx(0)


def test_inactive_allocations_change_no_current_kpis():
    from portfolio_risk.strategy.unitlib import evaluate_strategy
    lib, base = synthetic_library()
    lib["templates"]["asset"].update(hqla_l2a=.85, rsf=.85, rwa=1)
    empty = evaluate_strategy(lib, [], base)
    later = evaluate_strategy(lib, [dict(template="asset", purchase_m=24, notional=100)], base)
    beyond = evaluate_strategy(lib, [dict(template="asset", purchase_m=27, notional=100)], base)
    assert later["kpis"]["lcr_pct"] == empty["kpis"]["lcr_pct"]
    assert later["kpis"]["nsfr_pct"] == empty["kpis"]["nsfr_pct"]
    assert beyond["kpis"] == empty["kpis"]
    assert beyond["nii_total_$"] == 0


def test_oas_failure_is_explicit():
    from portfolio_risk.core.pricing import solve_oas_from_A
    with pytest.raises(ValueError, match="converge"):
        solve_oas_from_A(np.ones((1, 360)), 1, np.array([1e100]))


def test_scenario_corporate_driver_does_not_solve_again(monkeypatch):
    from portfolio_risk.demo import demo_market, demo_histories, model_balance_sheet
    corp = importlib.import_module("portfolio_risk.products.corp")
    sr, vp = demo_market()
    bs = model_balance_sheet(scale=.001)
    frame = bs["loans"].filter(pl.col("is_float") == 0).head(1)
    with run_context(RunConfig(32, 32, compute_backend='python')):
        base = corp.run_corp_risk(frame, bs["asof"], sr, vp, *demo_histories())
        oas = base["oas_bps"].to_numpy() / 1e4
        def forbidden(*args, **kwargs):
            raise AssertionError("scenario tried to solve OAS")
        monkeypatch.setattr(corp, "corp_solve_oas", forbidden)
        shifted = corp.run_corp_risk(frame, bs["asof"], sr + .01, vp, *demo_histories(), oas=oas)
    np.testing.assert_allclose(shifted["oas_bps"], base["oas_bps"])
    assert shifted["model_price"][0] < base["model_price"][0]


def test_nsfr_uses_full_equity_with_markets_book():
    from portfolio_risk.demo import model_balance_sheet
    from portfolio_risk.analytics.kpis import nsfr
    bs = model_balance_sheet(scale=.001, include_markets_bs=True)
    explicit = nsfr(bs, bs["asof"])
    implicit_bs = {k: v for k, v in bs.items() if k != "equity"}
    implicit = nsfr(implicit_bs, bs["asof"])
    assert explicit["asf_$"] == pytest.approx(implicit["asf_$"])


def test_fixed_oas_kpis_reprice_all_books_without_resolving(monkeypatch):
    from portfolio_risk.demo import demo_market, demo_deposit_history, model_balance_sheet
    from portfolio_risk.analytics.calibration import calibrate_books
    from portfolio_risk.analytics.kpis import compute_kpis
    bs = model_balance_sheet(scale=.001, include_markets_bs=True)
    for name in ("mbs", "loans", "debt", "deposits", "cds"):
        bs[name] = bs[name].head(2)
    sr, vp = demo_market()
    hist = demo_deposit_history()
    with run_context(RunConfig(32, 32, compute_backend='python')):
        fixed = calibrate_books(bs, sr, vp, hist)
        def forbidden(*args, **kwargs):
            raise AssertionError("KPI scenario tried to solve OAS again")
        for module, function in (("core.scenarios", "solve_base_oas"),
                                 ("core.pricing", "solve_oas_from_A"),
                                 ("products.corp", "corp_solve_oas")):
            monkeypatch.setattr(importlib.import_module("portfolio_risk." + module), function, forbidden)
        base = compute_kpis(bs, sr, vp, hist, oas_by_book=fixed)
        scenario = compute_kpis(bs, sr + .01, vp, hist, oas_by_book=fixed)
    assert scenario["eve"]["mv_assets_$"] != pytest.approx(base["eve"]["mv_assets_$"])
    assert scenario["eve"]["mv_liabilities_$"] != pytest.approx(base["eve"]["mv_liabilities_$"])


def test_matured_unit_has_no_forward_liquidity_or_dv01():
    from portfolio_risk.strategy.unitlib import evaluate_strategy
    lib, base = synthetic_library()
    lib["balance"][:, 3:] = 0
    lib["dv01"][:] = 2
    result = evaluate_strategy(lib, [dict(template="asset", purchase_m=1, notional=10)], base)
    assert result["fwd_dv01"][3] == 20
    assert not result["fwd_dv01"][4:].any()
    assert not result["funding_gap"][4:].any()


def test_capital_retains_income_in_partial_last_quarter():
    from portfolio_risk.demo import model_balance_sheet
    from portfolio_risk.analytics.kpis import capital, NI_TO_NII, PAYOUT
    bs = model_balance_sheet(scale=.001)
    out = capital(bs, pl.DataFrame({"nii": [1., 2., 3., 4.]}))
    assert out["cet1_path"][-1]["cet1_$"] == pytest.approx(out["cet1_t0_$"] + 10 * NI_TO_NII * (1 - PAYOUT))


def test_zero_named_mbs_scenario_preserves_mark_with_distinct_path_counts():
    from portfolio_risk.demo import demo_market, demo_histories, demo_portfolio
    from portfolio_risk.analytics.risk import run_risk
    sr, vp = demo_market()
    port = demo_portfolio(2)
    with run_context(RunConfig(32, 64, compute_backend='python')):
        base = run_risk(port, sr, vp, *demo_histories())
        zero = run_risk(port, sr, vp, *demo_histories(), oas=base["oas_bps"].to_numpy() / 1e4)
    np.testing.assert_allclose(zero["model_price"], base["model_price"], atol=1e-10)


def test_unit_library_grid_extends_to_the_configured_horizon():
    from portfolio_risk.demo import demo_market, demo_histories, demo_deposit_history
    from portfolio_risk.strategy.unitlib import build_unit_library
    with run_context(RunConfig(32, 32, horizon=39, compute_backend='python')):
        library = build_unit_library(*demo_market(), demo_histories(), demo_deposit_history(), horizon=39)
    assert library["grid_m"] == [0, 6, 12, 18, 24, 30, 36]
    assert library["nii"].shape[1] == 39
    assert ("agency_mbs", 38) in library["vectors"]
