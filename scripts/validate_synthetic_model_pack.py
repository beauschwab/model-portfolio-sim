"""Explicit independent validation of hypothetical data against native fitters.

This offline oracle reads raw observations, not the generator's internal state.
It checks exact exposure/count likelihoods, known generating laws, complete
recovery outcomes and chronology. It cannot establish empirical model accuracy.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
YEAR_SECONDS = 365 * 86400


def _time(value):
    return value["ordinal"] * 86400 + value["seconds_of_day"]


def _close(actual, expected, label, tolerance=1e-12):
    if not math.isfinite(actual) or not math.isclose(actual, expected, rel_tol=tolerance, abs_tol=tolerance):
        raise ArithmeticError(f"{label}: {actual!r} != independent {expected!r}")


def _evidence(raw, start, end):
    credit = raw["schema"] == "observed-credit-calibration-1"
    states = ["performing", "watch", "delinquent", "default", "prepaid"] if credit else raw["segment_ids"]+["withdrawal", "closure"]
    live = 3 if credit else len(raw["segment_ids"])
    exposure = [0] * live
    counts = [[0] * len(states) for _ in range(live)]
    censored = 0
    for spell in raw["spells"]:
        i = states.index(spell["origin" if credit else "segment_id"])
        a, b = _time(spell["start"]), _time(spell["end"])
        exposure[i] += max(0, min(b, end)-max(a, start))
        if start < b <= end:
            ending = spell["ending"]
            if ending["kind"] == "right_censored":
                censored += 1
            else:
                destination = ending.get("destination", ending["kind"])
                counts[i][states.index(destination)] += 1
    return [v/YEAR_SECONDS for v in exposure], counts, censored


def check_exact_history(raw, result, truth):
    credit = raw["schema"] == "observed-credit-calibration-1"
    training = _evidence(raw, _time(raw["train_start"]), _time(raw["train_end"]))
    if credit:
        rates = result["transition_hazards_per_year"]
    else:
        rates = [row["migration_per_year"]+[row["withdrawal_per_year"], row["closure_per_year"]]
                 for row in result["segment_hazards"]]
    maximum_z = 0.
    for i, (exposure, counts, row) in enumerate(zip(training[0], training[1], rates)):
        assert exposure > 0., "no silently supplied unobserved origin"
        for j, (count, fitted) in enumerate(zip(counts, row)):
            _close(fitted, count/exposure, f"training MLE {i},{j}")
            true_rate = truth["training_hazards"][i][j]
            if true_rate:
                # Predeclared broad sampling check, separate from numerical parity.
                sampling_scale = math.sqrt(max(count, 1))/exposure
                maximum_z = max(maximum_z, abs(fitted-true_rate)/sampling_scale)
                if abs(fitted-true_rate) > 6*sampling_scale+2/exposure:
                    raise ArithmeticError("known synthetic hazard outside the declared sampling envelope")
    holdout_default_residual = 0.
    holdout_default_expected = 0.
    for name, a, b in [("training", raw["train_start"], raw["train_end"]),
                        ("holdout", raw["train_end"], raw["holdout_end"])]:
        exposure, counts, censored = _evidence(raw, _time(a), _time(b))
        diagnostics = result[name]
        observed_counts = diagnostics["event_counts"] if credit else [
            row+[diagnostics["withdrawal_counts"][i], diagnostics["closure_counts"][i]]
            for i, row in enumerate(diagnostics["migration_counts"])]
        expected_counts = diagnostics["expected_event_counts"] if credit else [
            row+[diagnostics["expected_withdrawal_counts"][i], diagnostics["expected_closure_counts"][i]]
            for i, row in enumerate(diagnostics["expected_migration_counts"])]
        assert observed_counts == counts
        assert diagnostics["right_censored_spell_count"] == censored
        expected_exposure = diagnostics["exposure_years_by_origin" if credit else "exposure_years_by_segment"]
        for i, value in enumerate(exposure):
            _close(expected_exposure[i], value, f"{name} exposure {i}")
        ll = 0.
        for i, row in enumerate(counts):
            for j, count in enumerate(row):
                hazard = rates[i][j]
                assert not count or hazard > 0.
                _close(expected_counts[i][j], hazard*exposure[i], f"{name} compensator {i},{j}")
                ll += (count*math.log(hazard) if count else 0.)-hazard*exposure[i]
                if name == "holdout" and j == (3 if credit else len(rates)):
                    expected = hazard*exposure[i]
                    holdout_default_residual += count-expected
                    holdout_default_expected += expected
        _close(diagnostics["log_likelihood"], ll, f"{name} complete path likelihood", 2e-12)
        assert diagnostics["impossible_event_count"] == 0
    assert result["data_kind"] == "synthetic" and result["measure"] == "physical"
    if not credit:
        assert result["adoption_eligible"] is False
    return dict(exact_counts_exposures_likelihood_verified=True, maximum_training_hazard_z=maximum_z,
                holdout_exit_pearson_residual=holdout_default_residual/math.sqrt(max(holdout_default_expected, 1.)))


def check_snapshot_history(raw, result, truth):
    train = [row for row in raw["snapshots"] if _time(row["end"]) <= _time(raw["train_end"])]
    defaults = sum(row["destination"] == "default" for row in train)
    total = len(train)
    assert 0 < defaults < total
    years = (_time(train[0]["end"])-_time(train[0]["start"]))/YEAR_SECONDS
    probability = defaults/total
    closed_hazard = -math.log1p(-probability)/years
    fitted = math.exp(result["parameters"][0]["value"])
    _close(fitted, closed_hazard, "snapshot binomial MLE", 1e-6)
    se = math.sqrt(probability/(total*(1-probability)))/years
    assert abs(fitted-truth["training_hazard"]) <= 6*se+2/total
    assert result["convergence"] == "coordinate_mesh_converged"
    assert result["information_rank"] == 1 and not result["active_bound_parameters"]
    assert result["adoption_eligible"] is False and result["data_kind"] == "synthetic"
    for name, a, b in [("training", raw["train_start"], raw["train_end"]),
                        ("holdout", raw["train_end"], raw["holdout_end"])]:
        observed = [row for row in raw["snapshots"] if _time(a) <= _time(row["start"]) and _time(row["end"]) <= _time(b)]
        ll = 0.
        for row in observed:
            time = (_time(row["end"])-_time(row["start"]))/YEAR_SECONDS
            ll += math.log(-math.expm1(-fitted*time)) if row["destination"] == "default" else -fitted*time
        _close(result[name]["log_likelihood"], ll, f"{name} endpoint likelihood", 2e-12)
        outcomes = [r for r in raw["default_outcomes"] if _time(a) < _time(r["default_time"]) <= _time(b)]
        severity = result[f"{name}_severity"]
        ead = sum(r["realized_ead"] for r in outcomes)
        recovery = sum(c["principal"] for r in outcomes for c in r["recoveries"])
        undrawn = sum(r["pre_default_undrawn"] for r in outcomes)
        contingent = sum(r["realized_ead"]-r["pre_default_drawn"] for r in outcomes)
        assert severity["complete_outcomes"] == len(outcomes)
        _close(severity["total_ead"], ead, f"{name} EAD")
        _close(severity["total_recovered_principal"], recovery, f"{name} recoveries")
        _close(severity["ead_weighted_realized_lgd"], 1-recovery/ead, f"{name} realized LGD")
        _close(severity["undrawn_weighted_realized_ccf"], contingent/undrawn, f"{name} realized CCF")
        _close(severity["ead_weighted_realized_lgd"], truth["realized_lgd"], f"{name} known LGD")
        _close(severity["undrawn_weighted_realized_ccf"], truth["realized_ccf"], f"{name} known CCF")
    return dict(endpoint_likelihood_and_binomial_mle_verified=True,
                complete_realized_lgd_ccf_verified=True, fitted_hazard=fitted)


def validate_pack(directory):
    from portfolio_risk.analytics import model_contracts as models
    from portfolio_risk.core.native import library_path
    directory = Path(directory)
    manifest = json.loads((directory/"manifest.json").read_bytes())
    assert manifest["data_kind"] == "synthetic"
    for name, identity in manifest["files"].items():
        assert Path(name).name == name
        assert hashlib.sha256((directory/name).read_bytes()).hexdigest() == identity["sha256"]
    library = library_path()
    binary_hash = hashlib.sha256(library.read_bytes()).hexdigest()
    truth = json.loads((directory/"truth.json").read_bytes())["cases"]
    results, checks = {}, {}
    for name, policy in truth.items():
        raw = json.loads((directory/f"{name}.json").read_bytes())
        if raw["schema"] == "credit-snapshot-calibration-1":
            result = models.fit_credit_snapshots(raw)
            checks[name] = check_snapshot_history(raw, result, policy)
        else:
            adapter = models.fit_observed_credit if raw["schema"] == "observed-credit-calibration-1" else models.fit_observed_deposit_behavior
            result = adapter(raw)
            checks[name] = check_exact_history(raw, result, policy)
        results[name] = result
    for base, shifted in [("commercial-credit", "commercial-credit-shift"),
                           ("deposits", "deposits-shift"), ("panel-credit", "panel-credit-shift")]:
        # Only holdout observations/law change; training estimates must be BITWISE unchanged.
        for field in ["training", "parameters", "observed_information_eigenvalues",
                      "transition_hazards_per_year", "segment_hazards", "training_severity"]:
            if field in results[base]:
                assert json.dumps(results[base][field], sort_keys=True) == json.dumps(results[shifted][field], sort_keys=True)
        assert results[base]["holdout"] != results[shifted]["holdout"]
        if "panel" not in base:
            assert checks[shifted]["holdout_exit_pearson_residual"] > 6.
    assert hashlib.sha256(library.read_bytes()).hexdigest() == binary_hash
    return dict(schema="synthetic-model-validation-result-1", data_kind="synthetic",
                observed_accuracy_established=False, reviewed_policy_approval_established=False,
                production_adoption_eligible=False, manifest_sha256=hashlib.sha256((directory/"manifest.json").read_bytes()).hexdigest(),
                native_library=str(library), native_library_sha256=binary_hash,
                source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                verification_scripts_sha256={name: hashlib.sha256((ROOT/"scripts"/name).read_bytes()).hexdigest()
                    for name in ["make_synthetic_model_validation.py", "validate_synthetic_model_pack.py"]},
                cases=checks, native_results=results,
                limitations=["Known hypothetical laws and independent cohorts; no observed portfolio accuracy",
                             "Example policies are unreviewed; no accounting or regulatory approval",
                             "Shared credit fitter tests do not activate product behavior or saved economics",
                             "No deposit beta, mortgage incentive or card payment/utilization fit is implied"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    if args.report.exists():
        raise ValueError("previous validation evidence cannot be overwritten")
    report = validate_pack(args.directory)
    args.report.write_text(json.dumps(report, indent=2, allow_nan=False)+"\n", encoding="utf-8")
    print(f"PASS {len(report['cases'])} synthetic cases, independent known-truth checks and three holdout-leakage pairs")


if __name__ == "__main__":
    main()
