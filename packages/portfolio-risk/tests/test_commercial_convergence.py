"""Finite-family evidence, with sampling intervals separate from cash parity."""
from copy import deepcopy
import math
import os

import numpy as np
import pytest
from scipy.optimize import brentq
from scipy.special import ndtr

from commercial_convergence_benchmark import (
    Cell, DEGREES, DRIVERS, EARLY, analytic_value, assess, conditional_value,
    conditional_parameters, conditional_coupon_value, coupon_weights,
    learned_value, matrix, native_binary, normal_stop_probability,
    optimal_value, ou_covariance, outer_breakpoints, request, run_cell,
)
from portfolio_risk.analytics.commercial_stochastic_reference import verify
from portfolio_risk.core import model_native


@pytest.fixture
def selected_native():
    selected = os.environ.get("PORTFOLIO_CONVERGENCE_NATIVE")
    if selected:
        with native_binary(selected):
            yield
    else:
        # The regular installed artifact remains mandatory in the full suite.
        # A missing artifact fails rather than silently using a Python model.
        model_native.binary_identity()
        yield


def test_matrix_is_predeclared_sparse_and_keeps_refusal_visible():
    cells = matrix()
    assert len(cells) == 107
    assert sum(cell.family == "analytic" for cell in cells) == 36
    assert sum(cell.expected_admission == "refuse_work" for cell in cells) == 1
    assert cells[100] == Cell("learned", "call", "cubic", "brownian", paths=512,
                             expected_admission="refuse_work")
    assert sum(cell.driver == "active_two_ou" for cell in cells) == 6
    for degree in DEGREES:
        for driver in DRIVERS:
            for right in ("call", "put"):
                selected = [cell for cell in cells if cell.family == "learned"
                    and cell.degree == degree and cell.driver == driver and cell.right == right]
                assert {cell.paths for cell in selected} >= {128, 256}
                assert {cell.training_seed for cell in selected} >= {7, 19, 31}
                assert {cell.grid for cell in selected} == {"coarse", "refined"}


@pytest.mark.parametrize("driver", DRIVERS + ("active_two_ou",))
def test_ou_covariance_refinement_has_exact_semigroup_law(driver):
    raw = request(Cell("analytic", "none", "linear", driver))
    t, s = 5.0 / 365.0, 10.0 / 365.0
    decay = np.diag([math.exp(-d["mean_reversion"] * s) for d in raw["drivers"]])
    expected = decay @ ou_covariance(raw, t) @ decay + ou_covariance(raw, s)
    np.testing.assert_allclose(ou_covariance(raw, t + s), expected, rtol=0.0, atol=1e-18)


@pytest.mark.parametrize("driver", DRIVERS + ("active_two_ou",))
def test_normal_oracle_call_put_symmetry_and_independent_optimal_boundary(driver):
    raw = request(Cell("learned", "call", "linear", driver))
    call, call_error = optimal_value(raw, "call")
    put, put_error = optimal_value(raw, "put")
    assert call + put == pytest.approx(200.0, abs=1e-10)
    assert call_error + put_error < 1e-8
    assert call <= 99.9 < analytic_value(raw, "call")
    assert analytic_value(raw, "put") < 100.1 <= put
    if driver == "brownian":
        assert call == pytest.approx(99.8904049813, abs=1e-9)


def test_stop_mass_integrates_every_cubic_boundary_and_constant_policies():
    polynomial = np.polynomial.Polynomial.fromroots([-2.0, -0.5, 1.0])
    expected = ndtr(-0.5) - ndtr(-2.0) + 1.0 - ndtr(1.0)
    assert normal_stop_probability(polynomial, "call") == pytest.approx(expected, abs=1e-14)
    assert normal_stop_probability(polynomial, "put") == pytest.approx(1.0 - expected, abs=1e-14)
    for value in (-2.0, 0.0, 3.0):
        assert normal_stop_probability(np.polynomial.Polynomial([value]), "call") == float(value > 0.0)
        assert normal_stop_probability(np.polynomial.Polynomial([value]), "put") == float(value < 0.0)


def test_outer_integral_splits_quadratic_and_cubic_conditional_root_mergers():
    # y^2-z has a repeated root atz=0. The cubic y^3-3*z*y+2 has
    # discriminant108*(z^3-1), so its conditional roots merge atz=1.
    quadratic = np.zeros((3, 3))
    quadratic[1, 0], quadratic[0, 2] = -1.0, 1.0
    assert outer_breakpoints(quadratic) == pytest.approx([0.0], abs=1e-12)
    cubic = np.zeros((4, 4))
    cubic[0, 0], cubic[1, 1], cubic[0, 3] = 2.0, -3.0, 1.0
    assert outer_breakpoints(cubic) == pytest.approx([1.0], abs=1e-12)


def test_pinned_benchmark_executable_change_invalidates_evidence(tmp_path):
    binary = tmp_path / "pinned-native.exe"
    binary.write_bytes(b"immutable-benchmark-identity")
    with pytest.raises(RuntimeError, match="all evidence invalidated"):
        with native_binary(binary):
            binary.write_bytes(b"changed-before-completion")


@pytest.mark.parametrize("driver", DRIVERS + ("active_two_ou",))
@pytest.mark.parametrize("right", ("call", "put"))
def test_learned_integral_recovers_known_boundary_with_correlated_nuisance(driver, right):
    raw = request(Cell("learned", right, "linear", driver))
    loading, _, variance, future_sigma = conditional_parameters(raw)
    sigma = math.sqrt(variance)
    strike = 99.9 if right == "call" else 100.1
    boundary = brentq(lambda z: conditional_coupon_value(right, sigma * z, future_sigma) - strike, -12.0, 12.0)
    d = len(raw["drivers"])
    # Both exercise families continue below the increasing continuation's
    # boundary for calls and above it for puts. The extra OU observable is
    # correlated but economically redundant given the first Markov driver.
    regression = dict(means=[0.0] * d, scales=[1.0] * d,
        active_features=list(range(d)), coefficients=[strike - sigma * boundary] + list(map(float, loading)),
        basis_degree=1)
    learned, error, notices = learned_value(raw, right, regression)
    optimal, _ = optimal_value(raw, right)
    assert notices == [] and error < 2e-7
    assert learned == pytest.approx(optimal, abs=2e-8)


def test_both_unequal_reversion_drivers_economically_change_conditional_future_coupon():
    raw = request(Cell("learned", "call", "linear", "active_two_ou"))
    np.testing.assert_allclose(coupon_weights(raw), [100.0 * 7.0 / 365.0, 100.0 * 8.0 / 365.0])
    loading, covariance, variance, sigma = conditional_parameters(raw)
    assert all(loading > 0.0) and covariance[0, 1] > 0.0 and variance > 0.0 and sigma > 0.0
    base = conditional_value(raw, "call", [0.0, 0.0])
    assert conditional_value(raw, "call", [0.02, 0.0]) > base
    assert conditional_value(raw, "call", [0.0, 0.02]) > base
    assert raw["drivers"][0]["mean_reversion"] != raw["drivers"][1]["mean_reversion"]
    source = raw["source"]["source"]
    assert source["current"]["periods"][1]["end_ordinal"] == source["current"]["periods"][2]["start_ordinal"]
    assert {p["payment_ordinal"] for p in source["current"]["periods"][1:]} == {source["current"]["tail_end_ordinal"]}


@pytest.mark.parametrize("cell", matrix(), ids=lambda cell: cell.key)
def test_native_predeclared_convergence_cell_keeps_all_admission_and_numerical_results(cell, selected_native):
    result = run_cell(cell)
    assert result["expected_result"], result


@pytest.mark.parametrize("degree", DEGREES)
@pytest.mark.parametrize("driver", DRIVERS + ("active_two_ou",))
def test_actual_native_evaluation_stream_cannot_change_training_policy(degree, driver, selected_native):
    cell = Cell("learned", "call", degree, driver)
    raw = request(cell)
    before, _ = model_native.run(raw["schema"], raw, timeout=120.0)
    changed = deepcopy(raw)
    changed["evaluation"].update(market_seed=[41], credit_seed=41)
    after, _ = model_native.run(changed["schema"], changed, timeout=120.0)
    verify(raw, before)
    verify(changed, after)
    assert before["regressions"] == after["regressions"]
    assert before["training_value"] == after["training_value"]
    assert before["evaluation_paths"] != after["evaluation_paths"]
    assert before["raw_source"]["fixed_base_oas"] == after["raw_source"]["fixed_base_oas"]


def test_sampling_band_cannot_authorize_a_mutated_financial_path(selected_native):
    cell = Cell("learned", "call", "linear", "brownian")
    raw = request(cell)
    actual, _ = model_native.run(raw["schema"], raw, timeout=120.0)
    assess(cell, raw, actual)
    changed = deepcopy(actual)
    # Much smaller than this family's sampling interval, still far above the
    # immutable 1e-9+1e-12*abs(value) financial replay tolerance.
    changed["evaluation_paths"][0]["value"] += 1e-5
    with pytest.raises(ArithmeticError):
        assess(cell, raw, changed)
