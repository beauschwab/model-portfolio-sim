"""Independent finite default-loss experiment; never a production pricer.

This family has zero coupons, deterministic zero discounting, no optional
exercise and immediate fractional principal recovery. Its exact expected cash
is face*(1-LGD*p), where p=1-exp(-hazard*contractual cash horizon).
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys

import scipy
import subprocess

from scipy.stats import binomtest

from commercial_convergence_benchmark import Cell, native_binary, request as base_request
from portfolio_risk.analytics.commercial_stochastic_reference import verify
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]
FACE = 100.0
LGD = 0.4
FAMILY_ALPHA = 0.01


@dataclass(frozen=True)
class CreditCell:
    probability: float
    paths: int
    evaluation_seed: int

    @property
    def key(self):
        return f"p={self.probability}/paths={self.paths}/seed={self.evaluation_seed}"


def matrix():
    return tuple(CreditCell(p, n, seed)
                 for p in (0.0, 0.000001, 0.01, 0.05, 0.5, 0.95)
                 for n in (128, 256) for seed in (11, 23))


def request(cell: CreditCell):
    if cell not in matrix():
        raise ValueError("undeclared finite credit sampling cell")
    raw = base_request(Cell("analytic", "none", "linear", "brownian", paths=cell.paths))
    raw["identity"]["revision"] = "default-cash/" + cell.key
    raw["evaluation"].update(credit_seed=cell.evaluation_seed, market_seed=[cell.evaluation_seed])
    raw["annual_covariance"] = [0.0]
    raw["coupon_loadings"] = []
    raw["hazard_loadings"] = []
    for role in ("original", "current"):
        for period in raw["source"]["source"][role]["periods"]:
            period["coupon"] = dict(kind="fixed", annual_rate=0.0)
    contract = raw["source"]["source"]["current"]
    horizon = (contract["periods"][-1]["payment_ordinal"] - contract["asof_ordinal"]) / 365.0
    hazard = -math.log1p(-cell.probability) / horizon
    raw["source"]["credit"]["max_jumps_per_path"] = 1
    for interval in raw["source"]["credit"]["intervals"]:
        interval["hazards_per_year"] = [[0.0] * 5 for _ in range(3)]
        interval["hazards_per_year"][0][3] = hazard
        interval["loss_given_default"] = [LGD] * 3
        interval["complete_recovery_tails"] = [
            [dict(lag_years=0.0, weight=1.0)] for _ in range(3)]
    return raw


def assess(cell: CreditCell, raw, result):
    """Reconstruct the distribution and cluster SE from raw law and cash outcomes.

    Credit uniforms use independent path keys. Native market antithetic clusters
    still pair paths for the reported SE; credit outcomes need not be opposite.
    Clopper-Pearson coverage is a model-based diagnostic under ideal IID credit
    uniforms, rather than a statement about deterministic PRNG sequences or banks.
    """
    if raw != request(cell):
        raise ArithmeticError("credit benchmark raw cell identity differs")
    values = [row["value"] for row in result["evaluation_paths"]]
    if len(values) != cell.paths:
        raise ArithmeticError("credit benchmark path coverage differs")
    recovered = FACE * (1.0 - LGD)
    if any(v not in (FACE, recovered) for v in values):
        raise ArithmeticError("cash is outside the exact surviving/default support")
    defaults = sum(value == recovered for value in values)
    half = cell.paths // 2
    clusters = [(values[i] + values[half+i]) / 2.0 for i in range(half)]
    value = statistics.fmean(values)
    se = statistics.stdev(clusters) / math.sqrt(len(clusters))
    if not math.isclose(result["evaluation_value"], value, rel_tol=0.0, abs_tol=1e-12):
        raise ArithmeticError("credit sample mean differs")
    if not math.isclose(result["evaluation_standard_error"], se, rel_tol=0.0, abs_tol=1e-12):
        raise ArithmeticError("credit cluster standard error differs")
    if result["evaluation_clusters"] != len(clusters):
        raise ArithmeticError("credit cluster count differs")
    if result["regressions"]:
        raise ArithmeticError("option-free credit benchmark unexpectedly fits continuation")
    alpha = FAMILY_ALPHA / len(matrix())
    # Each interval uses the actual event count, including a zero-event sample.
    ci = binomtest(defaults, cell.paths).proportion_ci(confidence_level=1.0-alpha, method="exact")
    contains = ci.low <= cell.probability <= ci.high
    exact = FACE * (1.0 - LGD * cell.probability)
    mean_interval = [FACE * (1.0 - LGD * ci.high), FACE * (1.0 - LGD * ci.low)]
    return dict(cell=asdict(cell), key=cell.key, defaults=defaults,
                value=value, exact_value=exact, error=value-exact,
                clusters=len(clusters), cluster_standard_error=se,
                ideal_independent_path_standard_error=FACE*LGD*math.sqrt(
                    cell.probability*(1.0-cell.probability)/cell.paths),
                per_cell_alpha=alpha, default_probability_interval=[ci.low,ci.high],
                mean_value_interval=mean_interval, exact_interval_contains_oracle=contains,
                descriptive_normal95_contains_oracle=abs(value-exact)<=1.959963984540054*se,
                zero_event_sample=defaults==0)


def run_cell(cell):
    raw = request(cell)
    record = dict(raw=raw, cell=asdict(cell), key=cell.key, status="pending",
                  request_sha256=hashlib.sha256(json.dumps(raw, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode()).hexdigest())
    try:
        actual, execution = model_native.run(raw["schema"], raw, timeout=120.0)
        record.update(actual=actual, execution=execution)
        record["independent_verification"] = verify(raw, actual)
        record["assessment"] = assess(cell, raw, actual)
        if not record["assessment"]["exact_interval_contains_oracle"]:
            raise ArithmeticError("predeclared model-based binomial interval missed analytic law")
        record["status"] = "passed"
    except Exception as error:
        # Every declared cell remains visible, including execution/replay failures.
        record.update(status="failed", error_type=type(error).__name__, error=str(error))
    return record


def benchmark_sources():
    paths = set((ROOT/"packages/portfolio-risk/src/portfolio_risk").rglob("*.py"))
    paths.update(Path(__file__).parent/name for name in (
        "commercial_convergence_benchmark.py", "commercial_credit_convergence_benchmark.py",
        "test_commercial_credit_convergence.py"))
    paths.add(ROOT/"examples/models/commercial-generated-exercise-1.json")
    paths.update(ROOT/name for name in ("uv.lock", "pyproject.toml"))
    return [dict(path=str(p.relative_to(ROOT)).replace("\\", "/"),
                 sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(paths) if p.is_file()]


def run_matrix():
    sources = benchmark_sources()
    identity = model_native.binary_identity()
    records = [run_cell(cell) for cell in matrix()]
    if sources != benchmark_sources():
        raise RuntimeError("benchmark/reference source changed; evidence invalidated")
    if identity != model_native.binary_identity():
        raise RuntimeError("native executable changed; all credit benchmark evidence invalidated")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    return dict(schema="commercial-credit-convergence-evidence-1",
                recorded_at=datetime.now(timezone.utc).isoformat(),
                benchmark_worktree_commit=commit, native_identity=identity,
                benchmark_and_reference_sources=sources, python_version=sys.version,
                scipy_version=scipy.__version__,
                declared_cells=[asdict(cell) for cell in matrix()], records=records,
                passed=all(row["status"] == "passed" for row in records),
                family_alpha=FAMILY_ALPHA, law="constant_default_zero_coupon_immediate_fractional_recovery-1",
                numerical_family_only=True, observed_validation=False, full_issue_complete=False)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise FileExistsError("retain prior benchmark receipt rather than replacing it")
    with native_binary(args.native):
        result = run_matrix()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True), encoding="utf-8")
    print(json.dumps(dict(cells=len(result["records"]), passed=result["passed"],
                          receipt_sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))
    if not result["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
