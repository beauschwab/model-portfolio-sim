"""Independent generated exercise, exact controls and out-of-sample gates."""

from copy import deepcopy
import json
import math
from pathlib import Path

import numpy as np
import pytest

from portfolio_risk.analytics.commercial_stochastic_reference import (
    _admit,
    _tape,
    _typed,
    evaluate,
    fit_pricing_law,
    generated_policy_sources,
    verify,
)
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads(
        (ROOT / "examples/models/commercial-generated-exercise-1.json").read_text()
    )


def correlated():
    request = raw()
    request["drivers"][0]["mean_reversion"] = 0.7
    request["drivers"].append(
        dict(
            driver_id="observable-credit",
            initial_level=0.1,
            long_run_mean=0.2,
            mean_reversion=1.4,
            observation_policy="observable_date_midnight_utc-1",
        )
    )
    request["annual_covariance"] = [0.04, 0.009, 0.009, 0.01]
    asof = request["time_ordinals"][0]
    request["time_ordinals"].insert(1, asof + 5)
    early = deepcopy(request["policy"]["exercise_dates"][0])
    early.update(
        ordinal=asof + 5, call_clean_price_factor=1.001, timing="before_scheduled_cash"
    )
    request["policy"]["exercise_dates"].insert(0, early)
    request["source"]["credit"]["intervals"][1]["hazards_per_year"][0][3] = 3.0
    request["source"]["credit"]["intervals"][1]["loss_given_default"] = [0.4] * 3
    request["source"]["credit"]["intervals"][1]["interest_recovery_fraction"] = [
        0.5
    ] * 3
    request["hazard_loadings"] = [
        dict(interval_index=1, origin_index=0, destination_index=3, betas=[0.2, 1.0])
    ]
    for source in ("original", "current"):
        request["source"]["source"][source]["periods"][1]["coupon"]["observations"][0][
            "rate"
        ] = 0.1
    return request


def test_exact_gaussian_integral_and_antithetic_error_use_independent_clusters():
    request = raw()
    expected = 100.0 - 100.0 * 15.0 / 365.0 * 0.2 * math.sqrt(15.0 / 365.0) / math.sqrt(
        2.0 * math.pi
    )
    for paths in (128, 256, 512):
        request["training"]["paths"] = paths
        request["evaluation"]["paths"] = paths
        out = evaluate(request)
        assert (
            abs(out["evaluation_value"] - expected)
            <= 5.0 * out["evaluation_standard_error"] + 1e-9
        )
        assert out["evaluation_clusters"] == paths // 2
        values = [p["value"] for p in out["evaluation_paths"]]
        pairs = [(values[i] + values[i + paths // 2]) / 2 for i in range(paths // 2)]
        assert out["evaluation_standard_error"] == pytest.approx(
            np.std(pairs, ddof=1) / math.sqrt(paths // 2), abs=1e-12
        )
        assert all(p["full_native_cash_replayed"] for p in out["evaluation_paths"])
        assert out["evaluation_value"] <= 100.0


def test_evaluation_stream_cannot_fit_training_coefficients_or_create_initial_information():
    request = raw()
    first = evaluate(request)
    other = deepcopy(request)
    other["evaluation"]["market_seed"] = [19]
    other["evaluation"]["credit_seed"] += 1
    changed = evaluate(other)
    assert first["regressions"] == changed["regressions"]
    assert first["training_value"] == changed["training_value"]
    assert first["evaluation_paths"] != changed["evaluation_paths"]
    assert not first["evaluation_outcomes_used_for_fit"]
    assert not first["finite_distribution"]
    assert not first["convergence_established"]
    assert not first["empirical_validation"]


def test_correlated_ou_antithetic_pairs_have_exact_transition_mean_and_finite_correlation():
    request = _typed(correlated())
    _admit(request)
    tape = _tape(request, request["evaluation"], lambda: None)
    half = request["evaluation"]["paths"] // 2
    for step, ordinal in enumerate(request["time_ordinals"]):
        elapsed = (ordinal - request["time_ordinals"][0]) / 365.0
        expected = [
            d["long_run_mean"]
            + (d["initial_level"] - d["long_run_mean"])
            * math.exp(-d["mean_reversion"] * elapsed)
            for d in request["drivers"]
        ]
        np.testing.assert_allclose(
            (tape[:half, step, :] + tape[half:, step, :]) * 0.5,
            np.tile(expected, (half, 1)),
            rtol=0.0,
            atol=1e-12,
        )
    assert np.corrcoef(tape[:, 1, :].T)[0, 1] > 0.1


def test_zero_volatility_option_free_identity_retains_actual_complete_tail():
    request = raw()
    request["annual_covariance"] = [0.0]
    request["policy"]["exercise_dates"] = []
    out = evaluate(request)
    assert out["evaluation_value"] == 100.0
    assert out["evaluation_standard_error"] == 0.0
    assert out["regressions"] == []
    assert all(p["first_exercise_ordinal"] is None for p in out["evaluation_paths"])


@pytest.mark.parametrize(
    "target",
    [
        "seed_alias",
        "missing_fixing",
        "observed_future",
        "zero_hazard",
        "covariance",
        "unknown",
        "hidden",
    ],
)
def test_incomplete_or_lookahead_inputs_reject_before_generation(target):
    request = raw()
    if target == "seed_alias":
        request["evaluation"]["market_seed"] = request["training"]["market_seed"]
    elif target == "missing_fixing":
        request["coupon_loadings"] = []
    elif target == "observed_future":
        request["source"]["source"]["current"]["periods"][1]["coupon"]["observations"][
            0
        ]["source"]["kind"] = "observed"
    elif target == "zero_hazard":
        request["hazard_loadings"] = [
            dict(interval_index=1, origin_index=0, destination_index=3, betas=[1.0])
        ]
    elif target == "covariance":
        request["annual_covariance"] = [-0.04]
    elif target == "unknown":
        request["caller_continuation_value"] = 100.0
    else:
        request["drivers"][0]["observation_policy"] = "future_hidden"
    with pytest.raises((ArithmeticError, ValueError)):
        _admit(_typed(request))


@pytest.mark.parametrize("factory", [raw, correlated])
def test_native_generated_exercise_has_independent_raw_path_regression_and_cash_replay(
    factory,
):
    request = factory()
    actual, receipt = model_native.run(request["schema"], request, timeout=60.0)
    verify(request, actual)
    assert receipt["backend"] == "rust"
    assert actual["raw_source"]["fixed_base_oas"] == request["fixed_base_oas"]


@pytest.mark.parametrize(
    "target",
    [
        "value",
        "coefficient",
        "standard_error",
        "fit_flag",
        "native_replay",
        "domain",
        "rank",
        "work_bool",
    ],
)
def test_result_or_source_mutation_cannot_pass_independent_verification(target):
    request = raw()
    actual = evaluate(request)
    actual.update(admitted_work_upper=1, retained_bytes_upper=1)
    if target == "value":
        actual["evaluation_paths"][0]["value"] += 0.1
    elif target == "coefficient":
        actual["regressions"][0]["coefficients"][0] += 0.1
    elif target == "standard_error":
        actual["evaluation_standard_error"] += 0.1
    elif target == "fit_flag":
        actual["evaluation_outcomes_used_for_fit"] = True
    elif target == "native_replay":
        actual["evaluation_paths"][0]["full_native_cash_replayed"] = 1
    elif target == "domain":
        actual["raw_source"]["evaluation"]["randomness_domain"]["revision"] = "tampered"
    elif target == "rank":
        actual["regressions"][0]["rank"] += 1
    else:
        actual["admitted_work_upper"] = True
    with pytest.raises(ArithmeticError):
        verify(request, actual)


def test_original_duplicate_json_and_native_deadline_fail_without_result():
    import subprocess

    request = raw()
    original = json.dumps(
        dict(
            schema=request["schema"], threads=1, execution_timeout_ms=1, request=request
        )
    )
    timed = subprocess.run(
        [str(model_native.binary_path())],
        input=original,
        capture_output=True,
        text=True,
        check=False,
    )
    response = json.loads(timed.stdout)
    assert not response["ok"] and "result" not in response
    duplicate = original.replace(
        '"parallel_bump": 0.0', '"parallel_bump": 0.0, "parallel_bump": 0.01'
    )
    rejected = subprocess.run(
        [str(model_native.binary_path())],
        input=duplicate,
        capture_output=True,
        text=True,
        check=False,
    )
    response = json.loads(rejected.stdout)
    assert (
        not response["ok"]
        and "duplicate" in response["error"]
        and "result" not in response
    )


def test_retained_q_policy_sources_reproduce_fresh_evaluation_cash():
    request = raw()
    fitted = fit_pricing_law(request)
    expected = evaluate(request)
    assert fitted["regressions"] == expected["regressions"]
    sources = generated_policy_sources(request, fitted)
    value = sum(
        s["weight"]
        * sum(
            c["principal"] + c["interest"] + c["fees"]
            for c in s["cash"]
            if c["year"] > 0.0
        )
        for s in sources
    )
    assert value == pytest.approx(expected["evaluation_value"], abs=1e-9, rel=1e-12)
    for source, outcome in zip(sources, expected["evaluation_paths"], strict=True):
        assert (
            source["native_stops"][0]["ordinal"] if source["native_stops"] else None
        ) == outcome["first_exercise_ordinal"]


def test_separately_supplied_physical_law_never_refits_retained_q_policy():
    request = raw()
    fitted = fit_pricing_law(request)
    before = deepcopy(fitted)
    physical = deepcopy(request)
    for name in ("original", "current"):
        physical["source"]["source"][name]["measure"] = "physical"
        for period in physical["source"]["source"][name]["periods"]:
            for observation in period["coupon"].get("observations", []):
                observation["source"]["measure"] = "physical"
    physical["evaluation"]["market_seed"] = [991]
    physical["evaluation"]["credit_seed"] = 991
    sources = generated_policy_sources(physical, fitted, original=True)
    changed = deepcopy(physical)
    for period in changed["source"]["source"]["current"]["periods"]:
        for observation in period["coupon"].get("observations", []):
            observation["rate"] += 0.01
    assert generated_policy_sources(changed, fitted, original=True) == sources
    assert fitted == before
    assert len(sources) == physical["evaluation"]["paths"]
    for source in sources:
        generated = source["raw_request"]["source"]
        assert generated["original"] == generated["current"]
        assert generated["current"]["measure"] == "physical"
