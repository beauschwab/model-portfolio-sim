"""Independent fixtures for shared native model and new contract entrypoints.

Production ownership runs in a fresh process outside the reference conftest.
Expected values below use closed forms; they never invoke a Rust reference.
"""
import datetime as dt
import math
import os
from pathlib import Path
import subprocess
import sys

import numpy as np
import polars as pl
import pytest

from portfolio_risk.core import conventions
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.products.corp import CorpDeck
from portfolio_risk.analytics.model_contracts import (
    evaluate_credit, evaluate_floating_coupons, fit_observed_credit,
    generate_dated_term_flows, resolve_dated_cashflows,
)


def level_payment_book(amort="level_payment", coupon=.06):
    return pl.DataFrame({
        "id": ["L"], "face": [100_000.], "maturity": [dt.date(2031, 1, 1)],
        "freq_months": [3], "daycount": ["30/360"], "is_float": [0],
        "coupon_or_spread": [coupon], "price": [100.], "amort_type": [amort],
    })


@pytest.mark.parametrize("coupon", [0., .06, -.01, 1e-12])
def test_level_payment_adapter_matches_independent_payment(coupon):
    assert library_path().is_file(), "build native product artifacts; skipping is not acceptance"
    with run_context(RunConfig(compute_backend="rust")):
        deck = CorpDeck(level_payment_book(coupon=coupon), dt.date(2026, 1, 1),
                        cal=conventions.Calendar("weekends"), bdc=conventions.BDC.NONE)
    n, rate = len(deck.prin), coupon / 4
    expected_payment = 1 / n if rate == 0 else rate / -math.expm1(-n * math.log1p(rate))
    balance = 1.
    for principal, tau in zip(deck.prin, deck.tau):
        assert principal + coupon * tau * balance == pytest.approx(expected_payment, abs=2e-13)
        balance -= principal
    assert balance == pytest.approx(0., abs=2e-13)


def test_level_payment_preserves_saved_annuity_economics():
    with run_context(RunConfig(compute_backend="rust")):
        deck = CorpDeck(level_payment_book("annuity"), dt.date(2026, 1, 1),
                        cal=conventions.Calendar("weekends"), bdc=conventions.BDC.NONE)
    np.testing.assert_array_equal(deck.prin, np.full(20, .05))


def test_level_payment_native_cashflows_oas_and_fixed_spread_marks_match_closed_form():
    from scipy.optimize import brentq
    from portfolio_risk.products import corp
    from portfolio_risk.core.config import N_STEPS

    asof = dt.date(2026, 1, 1)
    coupon, rate, n = .06, .06 / 4, 20
    payment = rate / -math.expm1(-n * math.log1p(rate))
    # Calendar dates and payments come from an independent quarterly schedule.
    dates = [dt.date(2026 + j // 4, 1 + 3 * (j % 4), 1) for j in range(1, n + 1)]
    times = np.array([(d - asof).days / 365. for d in dates])

    def independent_price(short_rate, spread):
        value = 0.
        for time in times:
            month = int(time * 12)
            fraction = time - month / 12
            discount = (1 + short_rate / 12) ** (-month) / (1 + short_rate * fraction)
            value += payment * discount * math.exp(-spread * time)
        return value

    def controlled_paths(short_rate):
        short = np.full((1, N_STEPS), short_rate)
        return {"short": short, "df": np.cumprod(1 / (1 + short / 12), axis=1),
                "swaps": np.zeros((1, 4, N_STEPS))}

    with run_context(RunConfig(compute_backend="rust")):
        deck = CorpDeck(level_payment_book(coupon=coupon), asof,
                        cal=conventions.Calendar("weekends"), bdc=conventions.BDC.NONE)
        base = corp._corp_A(deck, controlled_paths(.04))
        spread, price = corp.corp_solve_oas(deck, base, 1, tol=1e-12)
        expected_spread = brentq(lambda s: independent_price(.04, s) - 1, -.05, .30)
        assert spread[0] == pytest.approx(expected_spread, abs=2e-12)
        assert price[0] == pytest.approx(1., abs=1e-12)
        for shock in [-.02, 0., .02]:
            marked = corp.corp_pv(deck, corp._corp_A(deck, controlled_paths(.04 + shock)),
                                  spread, 1)[0]
            assert marked == pytest.approx(independent_price(.04 + shock, expected_spread),
                                            abs=2e-12)
        np.testing.assert_allclose(deck.t_pay, times, rtol=0., atol=1e-15)


def test_level_payment_fresh_production_default_forbids_python_financial_callbacks():
    script = '''
import datetime as dt
import polars as pl
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.core import conventions
from portfolio_risk.products import corp
assert RunConfig().compute_backend == 'rust'
def forbidden(*args, **kwargs):
    raise AssertionError('Python financial callback executed')
corp.gen_schedule = forbidden
conventions.year_fraction = forbidden
conventions.Calendar.adjust = forbidden
book = pl.DataFrame({'id':['L'],'face':[100000.], 'maturity':[dt.date(2031,1,1)],
    'freq_months':[3],'daycount':['30/360'],'is_float':[0],
    'coupon_or_spread':[0.],'price':[100.],'amort_type':['level_payment']})
deck = corp.CorpDeck(book, dt.date(2026,1,1), cal=conventions.Calendar('weekends'),
    bdc=conventions.BDC.NONE)
assert len(deck.prin) == 20
assert abs(sum(deck.prin)-1) < 1e-13
'''
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(Path(__file__).resolve().parents[1] / "src")
    result = subprocess.run([sys.executable, "-c", script], env=environment,
                            capture_output=True, text=True, timeout=60)
    assert result.returncode == 0, result.stdout + result.stderr


def credit_request(default_hazard=.2, prepay_hazard=.3, periods=1):
    return {
        "schema": "credit-model-1", "monetary_unit": "USD",
        "initial_live_probability": [1., 0., 0.],
        "periods": [{
            "duration_years": .5,
            "transition_hazards_per_year": [[0., 0., 0., default_hazard, prepay_hazard],
                                             [0., 0., 0., 0., 0.], [0., 0., 0., 0., 0.]],
            "exposure_at_default": [{"drawn": 100., "undrawn": 20.,
                                     "credit_conversion_factor": .5}] * 3,
            "loss_given_default": [.4] * 3,
            "recovery_distribution": [{"lag_years": .25, "weight": .25},
                                       {"lag_years": 1., "weight": .75}],
        } for _ in range(periods)],
    }


def test_native_credit_competing_exits_term_survival_and_recovery_tail():
    result = evaluate_credit(credit_request(periods=2))
    surviving = math.exp(-.5)
    defaults = (1 - surviving) * .2 / .5
    prepaid = (1 - surviving) * .3 / .5
    np.testing.assert_allclose(result["periods"][-1]["state_probability"],
                               [surviving, 0., 0., defaults, prepaid], rtol=0., atol=2e-12)
    assert result["total_defaulted_exposure"] == pytest.approx(110 * defaults, abs=2e-11)
    assert result["total_expected_loss"] == pytest.approx(44 * defaults, abs=2e-11)
    assert result["total_eventual_nominal_recovery"] == pytest.approx(66 * defaults, abs=2e-11)
    assert sum(x["amount"] for x in result["recoveries"]) == pytest.approx(66 * defaults, abs=2e-11)
    first_default = (1 - math.exp(-.25)) * .4
    second_default = math.exp(-.25) * first_default
    expected_tail = 66 * (first_default * .75 + second_default)
    assert result["recovery_after_horizon"] == pytest.approx(expected_tail, abs=2e-11)


def test_native_credit_cures_match_independent_matrix_exponential():
    from scipy.linalg import expm
    request = credit_request()
    hazards = np.array([[0., .4, .1, .2, .3], [.5, 0., .2, .3, .1], [.2, .4, 0., .5, .1]])
    request["periods"][0]["transition_hazards_per_year"] = hazards.tolist()
    generator = np.zeros((5, 5))
    generator[:3] = hazards
    for i in range(3):
        generator[i, i] = -hazards[i].sum()
    expected = np.array([1., 0., 0., 0., 0.]) @ expm(generator * .5)
    actual = evaluate_credit(request)["periods"][0]["state_probability"]
    np.testing.assert_allclose(actual, expected, rtol=0., atol=2e-12)


@pytest.mark.parametrize("change", [
    lambda r: r.update(schema="unknown"),
    lambda r: r.update(initial_live_probability=[.5, .5, .5]),
    lambda r: r["periods"][0].update(duration_years=-1),
    lambda r: r["periods"][0].update(loss_given_default=[1.1, .4, .4]),
    lambda r: r["periods"][0].update(recovery_distribution=[{"lag_years": 1., "weight": .5}]),
])
def test_native_credit_rejects_invalid_inputs_without_partial_output(change):
    request = credit_request()
    change(request)
    with pytest.raises(ValueError, match="Rust term computation failed"):
        evaluate_credit(request)


def dated_schedule():
    origin = dt.date(2024, 1, 31).toordinal()
    return {"schema": "dated-cashflow-1", "as_of_ordinal": origin,
            "horizon_days": 90, "scenarios": ["base", "shock"], "events": [{
                "position_id": "L", "event_id": "payment-1", "scenario": {"kind": "all"},
                "payment_ordinal": dt.date(2024, 2, 29).toordinal(), "day_offset": 29,
                "accrual_period": {"start_ordinal": origin,
                                   "end_ordinal": dt.date(2024, 2, 29).toordinal()},
                "currency": "USD", "components": {"principal": 10., "cash_interest": 2.,
                    "accrued_interest": 2., "book_amortization": .1, "fees": .5},
                "measure": "contractual", "provenance": {"kind": "contractual",
                    "source_id": "contract-L", "source_revision": "1"},
                "adjustment": {"kind": "none"},
            }]}


def observed_credit_request():
    start, split, end = [
        {"ordinal": dt.date(y, 1, 1).toordinal(), "seconds_of_day": 0}
        for y in [2021, 2022, 2023]
    ]
    def spell(account, stop, ending):
        return {"spell_id": account + "-1", "account_id": account, "start": start,
                "end": stop, "origin": "performing", "ending": ending}
    return {"schema": "observed-credit-calibration-1", "source_id": "synthetic-fixture",
            "source_revision": "1", "population_id": "fixture-population",
            "model_policy_id": "constant-hazard-mle", "training_split_id": "2021-train",
            "holdout_split_id": "2022-holdout", "observation_mode": "exact_transition_times",
            "data_kind": "synthetic", "train_start": start, "train_end": split,
            "holdout_end": end, "unobserved_row_policy": [{"kind": "reject"}] + [{
                "kind": "supplied_row", "transition_hazards_per_year": [0.] * 5,
                "assumption_id": state + "-off", "rationale": "Explicit synthetic fixture"}
                for state in ["watch", "delinquent"]], "spells": [
                    spell("A", split, {"kind": "transition", "destination": "default"}),
                    spell("B", split, {"kind": "transition", "destination": "prepaid"}),
                    spell("H", end, {"kind": "right_censored"}),
                ]}


def test_observed_credit_mle_matches_exposure_counts_and_excludes_holdout_fit():
    request = observed_credit_request()
    result = fit_observed_credit(request)
    np.testing.assert_allclose(result["transition_hazards_per_year"][0],
                               [0., 0., 0., 1 / 3, 1 / 3], rtol=0., atol=1e-15)
    assert result["training"]["exposure_years_by_origin"] == [3., 0., 0.]
    assert result["holdout"]["exposure_years_by_origin"] == [1., 0., 0.]
    assert result["training"]["log_likelihood"] == pytest.approx(2 * math.log(1 / 3) - 2)
    assert result["holdout"]["log_likelihood"] == pytest.approx(-2 / 3)
    assert result["data_kind"] == "synthetic"
    assert result["estimate_origins"][1]["kind"] == "supplied_assumption"
    request["spells"][-1]["ending"] = {"kind": "transition", "destination": "default"}
    revised = fit_observed_credit(request)
    assert revised["transition_hazards_per_year"] == result["transition_hazards_per_year"]
    assert revised["training"] == result["training"]
    assert revised["holdout"]["event_counts"][0][3] == 1


def dated_term_request():
    asof = dt.date(2024, 1, 31).toordinal()
    return {"schema": "dated-term-1", "product": "corporate", "asof": asof,
            "horizon_days": 60, "calendar": {"name": "WEEKEND", "extra_holidays": []},
            "bdc": "MF", "accrual_dates": "unadjusted_scheduled_dates",
            "scenarios": ["base", "shock"], "positions": [{
                "position_id": "L", "currency": "USD", "source_id": "contract-L",
                "source_revision": "1", "accrual_anchor_ordinal": asof,
                "direction": "receipt", "contract": {
                    "maturity": dt.date(2024, 3, 31).toordinal(), "freq_months": 1,
                    "daycount": "ACT/360", "coupon": .06, "notional": 100.,
                    "price": 1., "amort_type": "bullet",
                }}]}


def test_dated_term_native_producer_retains_leap_and_negative_settlement_lag():
    result = generate_dated_term_flows(dated_term_request())
    events = result["schedule"]["events"]
    assert [e["payment_ordinal"] for e in events] == [
        dt.date(2024, 2, 29).toordinal(), dt.date(2024, 3, 29).toordinal()]
    assert [e["day_offset"] for e in events] == [29, 58]
    assert [e["components"]["principal"] for e in events] == [0., 100.]
    np.testing.assert_allclose([e["components"]["cash_interest"] for e in events],
                               [100 * .06 * 29 / 360, 100 * .06 * 31 / 360], atol=1e-15)
    assert result["periods"][-1]["settlement_lag_days"] == -2
    assert resolve_dated_cashflows(result["schedule"], "base") == events
    assert resolve_dated_cashflows(result["schedule"], "shock") == events


def floating_request():
    def day(n):
        return dt.date(2026, 1, n).toordinal()
    def stamp(n):
        return int(dt.datetime(2026, 1, n, 13, tzinfo=dt.timezone.utc).timestamp())
    return {"schema": "floating-rate-1", "position_id": "L", "currency": "USD",
            "monetary_unit": "currency_units", "direction": "receipt",
            "index": {"index_id": "USD-SOFR", "kind": "overnight",
                      "tenor_months": None, "average_calendar_days": None},
            "reference_calendar": {"id": "supplied-calendar", "revision": "1"},
            "fixing_snapshot": {"id": "supplied-fixings", "revision": "1"},
            "publication_cutoff_unix_seconds": stamp(12), "complete_observation_coverage": True,
            "fixings": [{"fixing_id": name, "revision": "1", "index_id": "USD-SOFR",
                         "value_ordinal": day(value_day), "publication_unix_seconds": stamp(pub_day),
                         "rate": rate} for name, value_day, pub_day, rate in [
                             ("fri", 2, 5, .05), ("mon", 5, 6, .06)]],
            "periods": [{"period_id": "coupon-1", "start_ordinal": day(2),
                         "end_ordinal": day(6), "payment_ordinal": day(6),
                         "calculation": "daily_compounded", "margin_treatment": "simple",
                         "day_count": "act360", "margin": .01,
                         "reset_cutoff_unix_seconds": None,
                         "index_bounds": {"floor": None, "cap": None},
                         "all_in_bounds": {"floor": None, "cap": None},
                         "lookback_business_days": 0, "observation_shift": False,
                         "principal": [{"start_ordinal": day(2), "end_ordinal": day(6),
                                        "amount": 100_000.}],
                         "observations": [{"interest_start_ordinal": day(start),
                                           "interest_end_ordinal": day(end),
                                           "observation_start_ordinal": day(start),
                                           "observation_end_ordinal": day(end), "fixing_id": name}
                                          for start, end, name in [(2, 5, "fri"), (5, 6, "mon")]]}]}


def test_floating_native_arithmetic_matches_independent_weekend_compounding():
    request = floating_request()
    result = evaluate_floating_coupons(request)
    expected = 100_000 * ((1 + .05 * 3 / 360) * (1 + .06 / 360) - 1 + .01 * 4 / 360)
    assert result["periods"][0]["interest_amount"] == pytest.approx(expected, abs=2e-9)
    request["publication_cutoff_unix_seconds"] -= 10 * 86_400
    with pytest.raises(ValueError, match="Rust term computation failed"):
        evaluate_floating_coupons(request)


def test_native_actual_date_events_preserve_leap_payment_and_zero_change():
    schedule = dated_schedule()
    assert resolve_dated_cashflows(schedule, "base") == schedule["events"]
    assert resolve_dated_cashflows(schedule, "shock") == schedule["events"]
    schedule["events"][0]["day_offset"] = 30
    with pytest.raises(ValueError, match="Rust term computation failed"):
        resolve_dated_cashflows(schedule, "base")


def test_new_shared_protocol_runs_standalone_without_python():
    import json
    executable = library_path().parent / ("portfolio-lifecycle.exe" if sys.platform == "win32"
                                           else "portfolio-lifecycle")
    for schema, request, reference in [
        ("credit-model-1", credit_request(), evaluate_credit(credit_request())),
        ("dated-cashflow-1", {"schedule": dated_schedule(), "scenario": "base"},
         resolve_dated_cashflows(dated_schedule(), "base")),
        ("observed-credit-calibration-1", observed_credit_request(),
         fit_observed_credit(observed_credit_request())),
        ("dated-term-1", dated_term_request(), generate_dated_term_flows(dated_term_request())),
        ("floating-rate-1", floating_request(), evaluate_floating_coupons(floating_request())),
    ]:
        result = subprocess.run([str(executable)], input=json.dumps({"schema": schema,
            "threads": 1, "request": request}), capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, result.stderr + result.stdout
        assert json.loads(result.stdout)["result"] == reference
