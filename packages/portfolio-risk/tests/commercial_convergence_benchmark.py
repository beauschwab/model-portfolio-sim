"""Independent finite-family convergence experiment; never production pricing.

The normal integrals below use the declared raw OU law and contractual coupon,
not native cash checkpoints or fitted continuation targets. Only the *learned
policy* integral consumes regression coefficients, because it values that actual
policy separately from the optimal policy. Native cash replay keeps its original
tolerance. Run this module directly for an immutable, per-cell JSON receipt.
"""
from __future__ import annotations

import argparse
from contextlib import contextmanager
from copy import deepcopy
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
import itertools
import json
import math
from pathlib import Path
import statistics
import subprocess
import time
from unittest.mock import patch
import warnings

import numpy as np
from scipy.integrate import IntegrationWarning, quad
from scipy.optimize import brentq
from scipy.special import ndtr

from portfolio_risk.analytics.commercial_stochastic_reference import verify
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]
COUPON = 100.0 * 15.0 / 365.0
EARLY = 5.0 / 365.0
FINAL = 15.0 / 365.0
DEGREES = ("linear", "quadratic", "cubic")
SEEDS = ((7, 11), (19, 23), (31, 37))
DRIVERS = ("brownian", "correlated_ou")
FINANCIAL_ATOL = 1e-9
FINANCIAL_RTOL = 1e-12
POLICY_LOSS_LIMIT = 0.02  # The existing finite-family two-bp-of-par gate.
INTEGRATION_LIMIT = 2e-7


@dataclass(frozen=True)
class Cell:
    family: str
    right: str
    degree: str
    driver: str
    grid: str = "coarse"
    paths: int = 128
    training_seed: int = 7
    evaluation_seed: int = 11
    expected_admission: str = "admit"

    @property
    def key(self):
        return "/".join(map(str, asdict(self).values()))


def matrix():
    """Predeclared 107 cells, including the known cubic-512 refusal.

    The seed/path refinements are an explicit sparse design, not a claim to
    have executed their complete Cartesian product. No failed cell is dropped.
    """
    cells = [Cell("analytic", right, degree, driver, grid)
             for right, degree, driver, grid in itertools.product(
                 ("none", "call", "put"), DEGREES, DRIVERS,
                 ("coarse", "refined"))]
    cells += [Cell("learned", right, degree, driver, paths=paths)
              for right, degree, driver, paths in itertools.product(
                  ("call", "put"), DEGREES, DRIVERS, (128, 256))]
    cells += [Cell("learned", right, degree, driver,
                   training_seed=train, evaluation_seed=evaluation)
              for right, degree, driver, (train, evaluation) in itertools.product(
                  ("call", "put"), DEGREES, DRIVERS, SEEDS[1:])]
    cells += [Cell("learned", right, degree, driver, "refined")
              for right, degree, driver in itertools.product(
                  ("call", "put"), DEGREES, DRIVERS)]
    cells += [Cell("learned", right, degree, "brownian", paths=512)
              for right, degree in itertools.product(("call", "put"), DEGREES[:2])]
    cells.append(Cell("learned", "call", "cubic", "brownian", paths=512,
                      expected_admission="refuse_work"))
    cells += [Cell("learned", right, degree, "active_two_ou")
              for right, degree in itertools.product(("call", "put"), DEGREES)]
    assert len(cells) == 107 and len({cell.key for cell in cells}) == len(cells)
    return cells


def request(cell):
    raw = json.loads((ROOT / "examples/models/commercial-generated-exercise-1.json").read_text())
    raw["identity"]["revision"] = cell.key
    raw["regression_policy"] = f"native_observable_{cell.degree}_svd_train_evaluate-1"
    for name, seed in (("training", cell.training_seed),
                       ("evaluation", cell.evaluation_seed)):
        raw[name].update(paths=cell.paths, market_seed=[seed], credit_seed=seed)
    if cell.driver in ("correlated_ou", "active_two_ou"):
        raw["drivers"][0]["mean_reversion"] = 0.7
        raw["drivers"].append(dict(driver_id="correlated-observable",
            initial_level=0.0, long_run_mean=0.0, mean_reversion=1.4,
            observation_policy="observable_date_midnight_utc-1"))
        raw["annual_covariance"] = [0.04, 0.009, 0.009, 0.01]
    elif cell.driver != "brownian":
        raise ValueError("undeclared driver family")
    if cell.driver == "active_two_ou":
        # The loading contract permits one driver per observation. Two
        # nonoverlapping future coupon periods make both coordinates economic,
        # without inventing a multiple-driver loading for one observation.
        for name in ("original", "current"):
            source = raw["source"]["source"][name]
            first = source["periods"][1]
            second = deepcopy(first)
            origin = source["asof_ordinal"]
            first["period_id"] = "reset-first-factor"
            first["end_ordinal"] = origin + 22
            first["coupon"]["observations"][0].update(
                interest_end_ordinal=origin + 22, observation_end_ordinal=origin + 22)
            second.update(period_id="reset-second-factor", start_ordinal=origin + 22)
            second["coupon"]["observations"][0].update(
                interest_start_ordinal=origin + 22, observation_start_ordinal=origin + 22)
            source["periods"].append(second)
            source["principal_plan"]["principal_payments"] = [0.0, 0.0, 100.0]
        raw["coupon_loadings"] = [dict(period_id=period, observation_index=0,
            driver_index=index, loading=1.0) for index, period in enumerate(
                ("reset-first-factor", "reset-second-factor"))]
    dates = raw["policy"]["exercise_dates"]
    if cell.right == "none":
        dates.clear()
    elif cell.right == "put":
        dates[0].update(call_clean_price_factor=None, put_clean_price_factor=1.0)
    elif cell.right != "call":
        raise ValueError("undeclared exercise right")
    origin = raw["time_ordinals"][0]
    if cell.family == "learned":
        early = deepcopy(dates[0])
        early["ordinal"] = origin + 5
        early[cell.right + "_clean_price_factor"] = 0.999 if cell.right == "call" else 1.001
        dates.insert(0, early)
    elif cell.family != "analytic":
        raise ValueError("undeclared experiment family")
    days = [0, 15, 35] if cell.family == "analytic" else [0, 5, 15, 35]
    if cell.grid == "refined":
        days = sorted(set(days + [2, 5, 10, 25]))
    elif cell.grid != "coarse":
        raise ValueError("undeclared step refinement")
    raw["time_ordinals"] = [origin + day for day in days]
    return raw


def density(z):
    return math.exp(-0.5 * z * z) / math.sqrt(2.0 * math.pi)


def ou_covariance(raw, years):
    rates = np.array([driver["mean_reversion"] for driver in raw["drivers"]])
    covariance = np.array(raw["annual_covariance"]).reshape(len(rates), len(rates))
    decay = rates[:, None] + rates[None, :]
    factors = np.empty_like(decay)
    np.divide(-np.expm1(-decay * years), decay, out=factors, where=decay != 0.0)
    factors[decay == 0.0] = years
    return covariance * factors


def analytic_value(raw, right):
    weights = coupon_weights(raw)
    optionality = math.sqrt(weights @ ou_covariance(raw, FINAL) @ weights) / math.sqrt(2.0 * math.pi)
    return 100.0 + {"none": 0.0, "call": -optionality, "put": optionality}[right]


def coupon_weights(raw):
    source = raw["source"]["source"]["current"]
    weights = np.zeros(len(raw["drivers"]))
    for loading in raw["coupon_loadings"]:
        period = next(period for period in source["periods"]
                      if period["period_id"] == loading["period_id"])
        weights[loading["driver_index"]] += (source["opening_principal"]
            * (period["end_ordinal"] - period["start_ordinal"]) / 365.0 * loading["loading"])
    return weights


def conditional_parameters(raw):
    weights = coupon_weights(raw)
    a = np.array([driver["mean_reversion"] for driver in raw["drivers"]])
    mean_loading = weights * np.exp(-a * (FINAL - EARLY))
    covariance = ou_covariance(raw, EARLY)
    variance = float(mean_loading @ covariance @ mean_loading)
    sigma = math.sqrt(weights @ ou_covariance(raw, FINAL - EARLY) @ weights)
    return mean_loading, covariance, variance, sigma


def conditional_coupon_value(right, mean, sigma):
    ratio = mean / sigma
    if right == "call":
        return 100.0 + mean * ndtr(-ratio) - sigma * density(ratio)
    return 100.0 + mean * ndtr(ratio) + sigma * density(ratio)


def conditional_value(raw, right, levels):
    loading, _, _, sigma = conditional_parameters(raw)
    if np.isscalar(levels):
        assert not np.any(loading[1:])
        levels = [levels] + [0.0] * (len(loading) - 1)
    return conditional_coupon_value(right, float(loading @ np.array(levels)), sigma)


def optimal_value(raw, right):
    _, _, variance, future_sigma = conditional_parameters(raw)
    sigma = math.sqrt(variance)
    strike = 99.9 if right == "call" else 100.1
    boundary = brentq(lambda z: conditional_coupon_value(right, sigma * z, future_sigma) - strike,
                      -12.0, 12.0, xtol=1e-13)
    choose = min if right == "call" else max
    value, error = quad(lambda z: choose(strike - 100.0,
                        conditional_coupon_value(right, sigma * z, future_sigma) - 100.0)
                        * density(z), -12.0, 12.0, points=[boundary],
                        epsabs=1e-10, epsrel=1e-12)
    return float(100.0 + value), float(error)


def polynomial(regression, features):
    """Independent tensor-product enumeration in the declared term order."""
    x = [(np.polynomial.Polynomial(features[i]) - regression["means"][i])
         / regression["scales"][i] for i in regression["active_features"]]
    terms = [np.polynomial.Polynomial([1.0])]
    for power in range(1, regression.get("basis_degree", 2) + 1):
        for indices in itertools.combinations_with_replacement(range(len(x)), power):
            term = np.polynomial.Polynomial([1.0])
            for index in indices:
                term *= x[index]
            terms.append(term)
    assert len(terms) == len(regression["coefficients"])
    return sum((term * coefficient for term, coefficient in
                zip(terms, regression["coefficients"], strict=True)),
               np.polynomial.Polynomial([0.0]))


def final_policy_residual(raw, regression):
    """Prove the final learned polynomial represents its linear known coupon.

    At day15 the sole projected reset is observed, both period entitlements
    have no earned balance, and principal is100. The remaining cash target is
    exactly100+coupon*x. A degree<=3 polynomial is checked on a five-point
    tensor grid covering more than twelve marginal standard deviations.
    Constant feature entries use their training constants. No native target
    or path cash supplies the expected amount.
    """
    d = len(raw["drivers"])
    loadings = raw["coupon_loadings"]
    assert set(regression["active_features"]) <= set(range(d)) | set(range(d + 4, d + 4 + len(loadings)))
    scale = math.sqrt(ou_covariance(raw, FINAL)[0, 0])
    maximum = 0.0
    for levels in itertools.product(np.linspace(-12.0 * scale, 12.0 * scale, 5), repeat=d):
        features = [[mean] for mean in regression["means"]]
        for index, level in enumerate(levels):
            features[index] = [float(level)]
        for index, loading in enumerate(loadings):
            features[d + 4 + index] = [float(levels[loading["driver_index"]])]
        predicted = float(polynomial(regression, features)(0.0))
        maximum = max(maximum, abs(predicted - (100.0 + coupon_weights(raw) @ np.array(levels))))
    return float(maximum)


def normal_stop_probability(poly, right):
    # Roots divide every stop/continue interval, including cubic policies with
    # multiple boundaries. Gaussian tails beyond12 have negligible mass here.
    coefficients = np.trim_zeros(poly.coef, "b")
    if len(coefficients) <= 1:
        value = float(coefficients[0]) if len(coefficients) else 0.0
        return float(value > 0.0 if right == "call" else value < 0.0)
    roots = np.polynomial.polynomial.polyroots(coefficients)
    splits = [-12.0, 12.0] + [float(root.real) for root in roots
        if abs(root.imag) < 1e-9 and -12.0 < root.real < 12.0]
    splits.sort()
    mass = 0.0
    for left, right_bound in zip(splits, splits[1:]):
        value = float(poly((left + right_bound) * 0.5))
        stop = value > 0.0 if right == "call" else value < 0.0
        if stop:
            mass += ndtr(right_bound) - ndtr(left)
    return float(mass)


def bivariate_polynomial(regression, features):
    """Coefficients in independent standard normals(z0,z1), degree<=3."""
    def multiply(left, right):
        product = {}
        for (i, j), a in left.items():
            for (k, ell), b in right.items():
                product[i + k, j + ell] = product.get((i + k, j + ell), 0.0) + a * b
        return product

    normalized = []
    for index in regression["active_features"]:
        constant, z0, z1 = features[index]
        scale = regression["scales"][index]
        normalized.append({(0, 0): (constant - regression["means"][index]) / scale,
                           (1, 0): z0 / scale, (0, 1): z1 / scale})
    terms = [{(0, 0): 1.0}]
    degree = regression.get("basis_degree", 2)
    for power in range(1, degree + 1):
        for indices in itertools.combinations_with_replacement(range(len(normalized)), power):
            term = {(0, 0): 1.0}
            for index in indices:
                term = multiply(term, normalized[index])
            terms.append(term)
    coefficients = np.zeros((degree + 1, degree + 1))
    for term, coefficient in zip(terms, regression["coefficients"], strict=True):
        for (i, j), value in term.items():
            coefficients[i, j] += value * coefficient
    return coefficients


def outer_breakpoints(coefficients):
    """Partition where inner real roots merge, lose degree or cross tails.

    A cubic stopping surface can create square-root cusps in its conditional
    Gaussian mass. Merely increasing adaptive subdivisions hid roundoff
    warnings in the initial experiment. Explicit discriminants expose those
    cusps without changing an integration tolerance or a native policy.
    """
    inner = [np.polynomial.Polynomial(column).trim() for column in coefficients.T]
    while len(inner) > 1 and np.all(inner[-1].coef == 0.0):
        inner.pop()
    candidates = [inner[-1]]
    if len(inner) == 3:
        c, b, a = inner
        candidates.append(b * b - 4.0 * a * c)
    if len(inner) == 4:
        d, c, b, a = inner
        candidates.append(b * b * c * c - 4.0 * a * c ** 3 - 4.0 * b ** 3 * d
                          - 27.0 * a * a * d * d + 18.0 * a * b * c * d)
    for tail in (-12.0, 12.0):
        candidates.append(sum((coefficient * tail ** power
            for power, coefficient in enumerate(inner)), np.polynomial.Polynomial([0.0])))
    points = []
    for candidate in candidates:
        for root in candidate.roots():
            if abs(root.imag) < 1e-7 and -12.0 < root.real < 12.0:
                point = float(root.real)
                if all(abs(point - previous) > 1e-9 for previous in points):
                    points.append(point)
    return sorted(points)


def learned_value(raw, right, regression):
    d = len(raw["drivers"])
    assert regression["active_features"] == list(range(d))
    loading, covariance, variance, future_sigma = conditional_parameters(raw)
    sigma = math.sqrt(variance)
    strike = 99.9 if right == "call" else 100.1
    # Condition on the economically sufficient projected future coupon mean,
    # not on just one component when both drivers affect the future cash.
    cov_mean = covariance @ loading
    along = cov_mean / sigma
    residual = covariance - np.outer(cov_mean, cov_mean) / variance
    eigenvalues, eigenvectors = np.linalg.eigh(residual)
    assert eigenvalues[0] >= -1e-16
    perpendicular = eigenvectors[:, -1] * math.sqrt(max(float(eigenvalues[-1]), 0.0)) if d > 1 else np.zeros(1)
    features = [[mean, 0.0, 0.0] for mean in regression["means"]]
    for index in range(d):
        features[index] = [0.0, float(along[index]), float(perpendicular[index])]
    coefficients = bivariate_polynomial(regression, features)
    coefficients[0, 0] -= strike
    points = outer_breakpoints(coefficients)

    def conditional(z):
        inner = np.polynomial.Polynomial(np.polynomial.polynomial.polyval(z, coefficients))
        probability = normal_stop_probability(inner, right)
        continuation = conditional_coupon_value(right, sigma * z, future_sigma)
        # Integrating the deviation from par avoids unnecessary cancellation
        # and absolute quadrature error from the unrelated constant principal.
        return (probability * (strike - 100.0)
                + (1.0 - probability) * (continuation - 100.0)) * density(z)

    with warnings.catch_warnings(record=True) as notices:
        warnings.simplefilter("always", IntegrationWarning)
        value, error = quad(conditional, -12.0, 12.0, points=points,
                            epsabs=2e-9, epsrel=1e-11, limit=300)
    return float(100.0 + value), float(error), [str(item.message) for item in notices]


def assess(cell, raw, actual):
    expected = verify(raw, actual)  # Independent raw source/regression/stopped cash.
    assert actual["raw_source"]["fixed_base_oas"] == raw["fixed_base_oas"] == 0.0
    assert actual["raw_source"]["source"]["source"]["current"]["measure"] == "pricing"
    assert not any(actual[key] for key in (
        "evaluation_outcomes_used_for_fit", "finite_distribution",
        "convergence_established", "empirical_validation", "physical_journal_eligible"))
    values = [row["value"] for row in actual["evaluation_paths"]]
    half = cell.paths // 2
    clusters = [(values[index] + values[index + half]) * 0.5 for index in range(half)]
    se = statistics.stdev(clusters) / math.sqrt(half)
    assert actual["evaluation_clusters"] == half
    assert abs(se - actual["evaluation_standard_error"]) <= 1e-12
    optimal = learned = analytic_value(raw, cell.right)
    error, notices, residual = 0.0, [], 0.0
    if cell.family == "learned":
        optimal, optimal_error = optimal_value(raw, cell.right)
        learned, error, notices = learned_value(raw, cell.right, actual["regressions"][0])
        error += optimal_error
        residual = final_policy_residual(raw, actual["regressions"][-1])
    loss = learned - optimal if cell.right != "put" else optimal - learned
    sampling_error = actual["evaluation_value"] - learned
    gate_errors = []
    if error > INTEGRATION_LIMIT or notices:
        gate_errors.append("independent quadrature did not meet its stated accuracy")
    if residual > 1e-8:
        gate_errors.append("final learned continuation is not the independently known linear coupon")
    if not -INTEGRATION_LIMIT <= loss <= POLICY_LOSS_LIMIT:
        gate_errors.append("finite-family policy loss exceeds two bp of par")
    if abs(sampling_error) > 5.0 * se + FINANCIAL_ATOL:
        gate_errors.append("evaluation outside predeclared five-SE diagnostic band")
    return dict(optimal_value=optimal, learned_policy_value=learned,
        policy_loss=loss, policy_loss_limit=POLICY_LOSS_LIMIT,
        quadrature_error_bound=error, quadrature_warnings=notices,
        final_policy_linear_residual=residual,
        evaluation_value=actual["evaluation_value"], evaluation_standard_error=se,
        training_value=actual["training_value"], antithetic_cluster_values=clusters,
        evaluation_clusters=half, evaluation_sampling_error=sampling_error,
        sampling_standardized_error=sampling_error / se if se > 1e-14 else None,
        diagnostic_95pct_covers_policy=abs(sampling_error) <= 1.96 * se + FINANCIAL_ATOL,
        five_se_gate=abs(sampling_error) <= 5.0 * se + FINANCIAL_ATOL,
        admitted_work_upper=actual["admitted_work_upper"],
        retained_bytes_upper=actual["retained_bytes_upper"],
        raw_independent_verified=True, financial_atol=FINANCIAL_ATOL,
        financial_rtol=FINANCIAL_RTOL, gate_errors=gate_errors,
        regression_ranks=[row["rank"] for row in expected["regressions"]],
        regression_columns=[len(row["coefficients"]) for row in expected["regressions"]])


@contextmanager
def native_binary(path):
    """Explicit standalone benchmark selection; production adapters unchanged."""
    path = Path(path).resolve()
    if not path.is_file():
        raise FileNotFoundError(path)
    identity = hashlib.sha256(path.read_bytes()).hexdigest()
    with patch.object(model_native, "binary_path", return_value=path):
        yield identity
    if hashlib.sha256(path.read_bytes()).hexdigest() != identity:
        raise RuntimeError("benchmark executable changed; all evidence invalidated")


def run_cell(cell):
    started = time.monotonic()
    record = dict(cell=asdict(cell), key=cell.key)
    raw = request(cell)
    record["request_sha256"] = hashlib.sha256(json.dumps(raw, sort_keys=True,
        separators=(",", ":"), allow_nan=False).encode()).hexdigest()
    try:
        actual, execution = model_native.run(raw["schema"], raw, timeout=120.0)
    except Exception as exc:
        record.update(status="admission_refused" if "aggregate work/byte admission exceeded" in str(exc)
                      else "execution_failed", error=f"{type(exc).__name__}: {exc}")
        record["expected_result"] = (cell.expected_admission == "refuse_work"
                                     and record["status"] == "admission_refused")
    else:
        record["execution"] = execution
        try:
            record["metrics"] = assess(cell, raw, actual)
            record["status"] = "passed" if not record["metrics"]["gate_errors"] else "numerical_gate_failed"
            record["expected_result"] = cell.expected_admission == "admit" and record["status"] == "passed"
        except Exception as exc:
            record.update(status="independent_verification_failed",
                          error=f"{type(exc).__name__}: {exc}", expected_result=False)
    record["elapsed_seconds"] = time.monotonic() - started
    return record


def source_identity():
    """Pin the isolated benchmark/reference source, without changing artifacts."""
    files = list((ROOT / "packages/portfolio-risk/src/portfolio_risk").rglob("*.py"))
    files += [Path(__file__).resolve(), Path(__file__).with_name("test_commercial_convergence.py"),
              ROOT / "examples/models/commercial-generated-exercise-1.json"]
    native = []
    for package in ("portfolio-risk-native", "portfolio-model-core", "portfolio-compute-control"):
        folder = ROOT / "packages" / package
        native += list((folder / "src").rglob("*.rs"))
        native += [file for file in (folder / "Cargo.toml", folder / "Cargo.lock") if file.is_file()]
    return {"benchmark_reference_files": {str(file.relative_to(ROOT)): hashlib.sha256(file.read_bytes()).hexdigest()
                                           for file in sorted(set(files))},
            "native_source_files": {str(file.relative_to(ROOT)): hashlib.sha256(file.read_bytes()).hexdigest()
                                    for file in sorted(set(native))}}


def summarize(records):
    successful = [row for row in records if row["status"] == "passed"]
    summary = dict(selected=len(records), statuses={status: sum(row["status"] == status for row in records)
        for status in sorted({row["status"] for row in records})},
        all_selected_expected=all(row["expected_result"] for row in records))
    if not successful:
        return summary
    summary.update(maximum_policy_loss=max(row["metrics"]["policy_loss"] for row in successful),
        maximum_quadrature_error=max(row["metrics"]["quadrature_error_bound"] for row in successful),
        maximum_work=max(row["metrics"]["admitted_work_upper"] for row in successful),
        maximum_retained_bytes=max(row["metrics"]["retained_bytes_upper"] for row in successful),
        descriptive_95pct_coverage=dict(covered=sum(row["metrics"]["diagnostic_95pct_covers_policy"] for row in successful),
                                      evaluated=len(successful)),
        training_seed_variation=[], grid_refinement=[], basis_paired_differences=[])
    for right, degree, driver in itertools.product(("call", "put"), DEGREES, DRIVERS):
        rows = [row for row in successful if row["cell"]["family"] == "learned"
                and all(row["cell"][key] == value for key, value in
                        (("right", right), ("degree", degree), ("driver", driver),
                         ("grid", "coarse"), ("paths", 128)))]
        values = [row["metrics"]["learned_policy_value"] for row in rows]
        if len(values) > 1:
            summary["training_seed_variation"].append(dict(right=right, degree=degree, driver=driver,
                seed_pairs=[[row["cell"]["training_seed"], row["cell"]["evaluation_seed"]] for row in rows],
                policy_value_range=max(values)-min(values), policy_value_sample_sd=statistics.stdev(values),
                interpretation="small descriptive training-policy replication; not a confidence interval"))
    by_cell = {tuple(row["cell"].items()): row for row in successful}
    for row in successful:
        cell = row["cell"]
        if cell["grid"] == "refined":
            previous = dict(cell, grid="coarse")
            baseline = by_cell.get(tuple(previous.items()))
            if baseline:
                summary["grid_refinement"].append(dict(refined=row["key"], coarse=baseline["key"],
                    evaluation_difference=row["metrics"]["evaluation_value"]-baseline["metrics"]["evaluation_value"],
                    exact_learned_policy_difference=row["metrics"]["learned_policy_value"]-baseline["metrics"]["learned_policy_value"],
                    interpretation="same exact OU law; grid changes draw assignment, no pathwise identity or monotonic error claim"))
        if cell["degree"] in ("quadratic", "cubic"):
            previous = dict(cell, degree="linear" if cell["degree"] == "quadratic" else "quadratic")
            baseline = by_cell.get(tuple(previous.items()))
            if baseline:
                # Same law/grid/seed/path count means the same independent
                # antithetic clusters on both sides; unlike changed grids.
                differences = [a-b for a,b in zip(row["metrics"]["antithetic_cluster_values"],
                    baseline["metrics"]["antithetic_cluster_values"], strict=True)]
                summary["basis_paired_differences"].append(dict(selected=row["key"], baseline=baseline["key"],
                    paired_clusters=len(differences), paired_sampling_standard_error=statistics.stdev(differences)/math.sqrt(len(differences)),
                    evaluation_difference=statistics.mean(differences),
                    exact_learned_policy_difference=row["metrics"]["learned_policy_value"]-baseline["metrics"]["learned_policy_value"]))
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--cell", type=int, action="append", help="Zero-based explicit matrix index; default all")
    args = parser.parse_args()
    cells = matrix()
    selected = list(range(len(cells))) if args.cell is None else args.cell
    args.output.parent.mkdir(parents=True, exist_ok=True)
    initial_source = source_identity()
    with native_binary(args.native) as binary_sha:
        receipt = dict(schema="commercial-convergence-benchmark-1",
            scope="synthetic supplied Pricing laws; no Physical/empirical/production acceptance",
            worktree_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
            native_binary=str(args.native.resolve()), native_sha256=binary_sha,
            started_utc=datetime.now(timezone.utc).isoformat(), source_identity=initial_source,
            full_predeclared_matrix=[asdict(cell) for cell in cells], selected_indices=selected,
            broad_convergence_established=False, empirical_validation=False, results=[])
        for index in selected:
            result = run_cell(cells[index])
            result["matrix_index"] = index
            receipt["results"].append(result)
            args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n", encoding="utf-8")
            print(f"{index:03d} {result['status']} {result['elapsed_seconds']:.2f}s {cells[index].key}", flush=True)
        if source_identity() != initial_source:
            raise RuntimeError("benchmark/reference source changed; all evidence invalidated")
        receipt["summary"] = summarize(receipt["results"])
    # An accepted completion is written only after the context has checked the
    # executable again. Earlier per-cell files are visibly incomplete if either
    # source or executable fencing raises; process failure is never a pass.
    receipt["all_selected_expected"] = all(row["expected_result"] for row in receipt["results"])
    receipt["completed_utc"] = datetime.now(timezone.utc).isoformat()
    args.output.write_text(json.dumps(receipt, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    return 0 if receipt["all_selected_expected"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
