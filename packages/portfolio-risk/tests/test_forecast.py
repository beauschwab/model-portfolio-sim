"""Source-period semantics, conditional paths and accounting invariants."""
import datetime as dt
import numpy as np
import polars as pl
import pytest
from portfolio_risk.analytics.forecast import compile_forecast, condition_paths, run_forecast_nii
from portfolio_risk.core.config import N_STEPS, DT
from portfolio_risk.core.runtime import RunConfig, run_context


def row(day, variable, value, convention="quarter_average", scenario="baseline", unit="percent"):
    return dict(date=day, variable=variable, series=variable, value=value,
                convention=convention, scenario=scenario, unit=unit)


def test_quarterly_averages_units_tail_and_hpi_rebase():
    rows = [row("2026-01-01", "short_rate", 4), row("2026-04-01", "short_rate", 2),
            row("2025-10-01", "hpi", 200, "quarter_end", "history", "index"),
            row("2026-01-01", "hpi", 180, "quarter_end", unit="index"),
            row("2026-04-01", "hpi", 160, "quarter_end", unit="index")]
    p = compile_forecast(rows, "baseline", "2026-01-01", 9)
    np.testing.assert_allclose(p["targets"]["short_rate"][:9], [.04]*3 + [.02]*6)
    assert p["targets"]["hpi"][2] == pytest.approx(.9)
    assert p["targets"]["hpi"][5] == pytest.approx(.8)
    assert p["coverage"]["short_rate"]["tail_months_in_report"] == 3


def test_annual_endpoints_interpolate_and_long_run_is_not_a_dated_anchor():
    rows = [row("2026-12-31", "policy_rate", 4, "year_end", "median"),
            row("2027-12-31", "policy_rate", 2, "year_end", "median"),
            row("2026-09-16", "policy_rate", 3, "longer_run", "median")]
    p = compile_forecast(rows, "median", "2026-12-01", 15)
    assert p["targets"]["policy_rate"][6] == pytest.approx(.03)
    assert p["targets"]["policy_rate"][12] == pytest.approx(.02)


def test_missing_hpi_jump_off_and_duplicate_conflict_rejected():
    rows = [row("2026-01-01", "short_rate", 4), row("2026-01-01", "hpi", 200, "quarter_end", unit="index")]
    with pytest.raises(ValueError, match="preceding"):
        compile_forecast(rows, "baseline", "2026-01-01", 6)
    with pytest.raises(ValueError, match="conflicting"):
        compile_forecast([rows[0], row("2026-01-01", "short_rate", 3)], "baseline", "2026-01-01", 6)


def test_conditioning_zero_change_and_discount_identity_copy_on_write():
    short = np.tile(np.linspace(.02, .04, N_STEPS), (2, 1))
    base = {"short": short, "swaps": np.repeat(short[:, None, :], 4, axis=1),
            "df": np.cumprod(1 / (1 + short * DT), axis=1)}
    p = {"targets": {"short_rate": short.mean(0)}}
    same = condition_paths(base, p)
    for k in base:
        np.testing.assert_allclose(same[k], base[k], rtol=1e-12, atol=1e-14)
    low = condition_paths(base, {"targets": {"short_rate": np.full(N_STEPS, .001)}})
    np.testing.assert_allclose(low["short"].mean(0), .001, atol=1e-14)
    np.testing.assert_allclose(low["df"], np.cumprod(1/(1+low["short"]*DT), axis=1))
    np.testing.assert_array_equal(base["short"], short)


def test_hpi_and_mortgage_targets_with_incentive_lag():
    a = np.ones((2, N_STEPS))
    base = dict(short=a*.04, swaps=np.ones((2, 4, N_STEPS))*.04, df=a,
                mtg=a*.06, hpi=a, yoy=a*0)
    targets = dict(short_rate=np.full(N_STEPS, .02), mortgage_rate=np.full(N_STEPS, .03), hpi=np.full(N_STEPS, .8))
    out = condition_paths(base, {"targets": targets})
    np.testing.assert_allclose(out["mtg"][0,:4], [.06,.06,.03,.03])
    np.testing.assert_allclose(out["hpi"].mean(0), .8)


def test_income_replay_uses_live_engines_and_preserves_base_inputs():
    from portfolio_risk.demo import demo_market, demo_deposit_history
    sr, vp = demo_market()
    # A single par floating loan: contractual quarterly interest responds to rates.
    port = pl.DataFrame([dict(id="float", face=1_000_000., maturity=dt.date(2027,6,10),
        freq_months=3, daycount="ACT/360", is_float=True, coupon_or_spread=.01, price=100.)])
    bs = {"loans": port, "asof": dt.date(2026,6,10)}
    p = compile_forecast([row("2026-01-01", "short_rate", 1)], "baseline", "2026-01-01", 9)
    with run_context(RunConfig(4,4, compute_backend='python')):
        out = run_forecast_nii(bs, sr, vp, demo_deposit_history(), p)
    assert out["monthly"]["nii"].sum() < out["monthly"]["base_nii"].sum()
    assert out["monthly"].height == 9
    assert np.isfinite(out["monthly"].to_numpy()).all()
    assert bs["loans"].equals(port)


def test_zero_conditioning_income_matches_base_with_fixed_accounting():
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import demo_market, demo_deposit_history
    sr, vp = demo_market()
    port = pl.DataFrame([dict(id="fixed", face=1_000_000., maturity=dt.date(2027,6,10),
        freq_months=3, daycount="ACT/360", is_float=False, coupon_or_spread=.05, price=103.)])
    bs = {"loans": port, "asof": dt.date(2026,6,10)}
    with run_context(RunConfig(4,4, compute_backend='python')):
        base = run_balance_sheet_nii(bs,sr,vp,demo_deposit_history(),horizon=9,asof=bs["asof"],capture_anchor=True)
        again = run_balance_sheet_nii(bs,sr,vp,demo_deposit_history(),horizon=9,asof=bs["asof"],accounting_anchor=base["accounting_anchor"])
    np.testing.assert_allclose(base["monthly"].to_numpy(), again["monthly"].to_numpy(), rtol=1e-12)
