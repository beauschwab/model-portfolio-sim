"""Analytic default-loss expectation and rare-event uncertainty controls."""
from copy import deepcopy
import json
import math
import os
from pathlib import Path
from unittest.mock import patch

import pytest

from commercial_convergence_benchmark import native_binary
from commercial_credit_convergence_benchmark import (
    CreditCell, FACE, LGD, assess, matrix, request, run_cell, run_matrix,
)
from portfolio_risk.core import model_native


@pytest.fixture(scope="module")
def receipt(tmp_path_factory):
    selected = os.environ.get("PORTFOLIO_CONVERGENCE_NATIVE")
    if selected:
        with native_binary(selected):
            record = run_matrix()
    else:
        model_native.binary_identity()
        record = run_matrix()
    destination = Path(os.environ["PORTFOLIO_CREDIT_CONVERGENCE_OUTPUT"]) if os.environ.get(
        "PORTFOLIO_CREDIT_CONVERGENCE_OUTPUT") else tmp_path_factory.mktemp("credit-law") / "receipt.json"
    if destination.exists():
        raise FileExistsError("never overwrite prior credit matrix evidence")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(record, indent=2, sort_keys=True), encoding="utf-8")
    return record


@pytest.mark.parametrize("cell", matrix(), ids=lambda c:c.key)
def test_native_credit_matrix_cell_retains_analytic_oracle_and_independent_raw_replay(cell, receipt):
    row = next(row for row in receipt["records"] if row["key"] == cell.key)
    assert row["status"] == "passed", row.get("error")
    a = row["assessment"]
    assert a["exact_interval_contains_oracle"]
    assert a["mean_value_interval"][0] <= a["exact_value"] <= a["mean_value_interval"][1]
    assert a["exact_value"] == FACE*(1.0-LGD*cell.probability)
    assert a["clusters"] == cell.paths//2


def test_matrix_is_complete_and_all_execution_failures_remain_visible(receipt):
    assert len(receipt["declared_cells"]) == len(receipt["records"]) == 24
    assert len({row["key"] for row in receipt["records"]}) == 24
    assert receipt["passed"]
    assert receipt["numerical_family_only"] and not receipt["observed_validation"]
    cell = matrix()[0]
    with patch.object(model_native, "run", side_effect=RuntimeError("retained refusal")):
        failed = run_cell(cell)
    assert failed["status"] == "failed" and failed["error"] == "retained refusal"
    assert failed["raw"] == request(cell)


def test_zero_event_credit_samples_keep_exact_interval_and_record_degenerate_normal_interval(receipt):
    rows = [r["assessment"] for r in receipt["records"] if r["cell"]["probability"] == 0.000001]
    assert len(rows) == 4
    for row in rows:
        # The fixed declared seeds produce no observed default at this probability.
        assert row["defaults"] == 0 and row["zero_event_sample"]
        assert row["cluster_standard_error"] == 0.0
        assert row["exact_interval_contains_oracle"]
        assert not row["descriptive_normal95_contains_oracle"]
        assert row["default_probability_interval"][1] > 0.0


def test_credit_benchmark_uses_actual_principal_cash_horizon_and_covers_end_to_end_probability():
    for cell in matrix():
        raw = request(cell)
        c = raw["source"]["source"]["current"]
        horizon = (c["periods"][-1]["payment_ordinal"]-c["asof_ordinal"])/365.0
        h = raw["source"]["credit"]["intervals"][0]["hazards_per_year"][0][3]
        assert -math.expm1(-h*horizon) == pytest.approx(cell.probability,abs=1e-16)
        assert raw["policy"]["exercise_dates"] == []
        assert raw["coupon_loadings"] == []
        assert all(p["coupon"] == dict(kind="fixed",annual_rate=0.0) for p in c["periods"])


@pytest.mark.parametrize("target", ["path_cash","sample_mean","cluster_se","cluster_count","raw_law","unexpected_fit"])
def test_independent_credit_assessment_rejects_altered_native_evidence(target, receipt):
    row = next(r for r in receipt["records"] if r["cell"]["probability"] == 0.05)
    cell = CreditCell(**row["cell"])
    raw, actual = deepcopy(row["raw"]), deepcopy(row["actual"])
    if target == "path_cash":actual["evaluation_paths"][0]["value"] = 99.0
    elif target == "sample_mean":actual["evaluation_value"] += 1.0
    elif target == "cluster_se":actual["evaluation_standard_error"] += 1.0
    elif target == "cluster_count":actual["evaluation_clusters"] += 1
    elif target == "raw_law":raw["source"]["credit"]["intervals"][0]["loss_given_default"][0] = 0.5
    else:actual["regressions"] = [{}]
    with pytest.raises(ArithmeticError):assess(cell,raw,actual)


def test_hand_credit_cluster_control_pairs_first_and_second_sample_halves():
    cell = CreditCell(0.5,128,11)
    # Each actual antithetic cluster has one default and one survivor. Adjacent
    # pairing would instead invent a nonzero standard error in this control.
    actual = dict(evaluation_paths=[dict(value=60.0)]*64+[dict(value=100.0)]*64,
                  evaluation_value=80.0,evaluation_standard_error=0.0,
                  evaluation_clusters=64,regressions=[])
    result = assess(cell,request(cell),actual)
    assert result["cluster_standard_error"] == 0.0
    assert result["exact_value"] == result["value"] == 80.0
