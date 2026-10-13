"""Actual native transports with independent hand calculations and no fallback.

Examples are synthetic numerical fixtures, never empirical bank-validation evidence.
"""
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import subprocess
import sys

import pytest

from portfolio_risk.analytics import model_contracts as models
from portfolio_risk.core.native import library_path

ROOT = Path(__file__).resolve().parents[3]


def example(name):
    assert library_path().is_file(), "build native products; skipped tests are not acceptance"
    return json.loads((ROOT / "examples" / "models" / name).read_text(encoding="utf-8"))


def test_supplied_draw_replay_has_exact_default_origin_money_and_complete_tail():
    raw = example("credit-path-1.json")
    original = deepcopy(raw)
    result = models.evaluate_credit_paths(raw)
    path = result["paths"][0]
    claim = path["default"]
    # E=.15, default/prepay total=.5, so realized default is at .3 years.
    assert claim["year"] == pytest.approx(.3, abs=1e-15)
    assert claim["origin"] == "performing"
    assert path["outcome"] == "default"
    assert path["earned_interest"] == pytest.approx(3., abs=2e-14)
    assert path["earned_interest_by_origin"] == pytest.approx([3., 0., 0.])
    assert claim["contingent_draw"] == 10.
    assert claim["principal"] == 110.
    assert claim["economic_principal_loss"] == 44.
    assert path["cash_funded"] == 10.
    assert path["cash_received"] == 0.
    assert claim["recoveries"][0] == pytest.approx(
        {"year": 2.3, "principal": 66., "interest": 3.}, abs=2e-14
    )
    assert path["closing_principal"] == 110.
    assert path["closing_accrued_interest"] == pytest.approx(3., abs=2e-14)
    assert path["closing_book_adjustment"] == -3.
    assert len(path["events"]) == 1  # no scheduled maturity applied to default rights
    assert result["measure"] == raw["measure"]
    assert result["origin_time"] == raw["origin_time"]
    assert result["source_revision"] == raw["source_revision"]
    assert result["stream_id"] == raw["draw_stream"]["stream_id"]
    assert result == models.evaluate_credit_paths(raw)
    assert raw == original


def test_supplied_crn_wait_is_carried_across_refinement_and_missing_draws_fail():
    raw = example("credit-path-1.json")
    base = models.evaluate_credit_paths(raw)
    first = deepcopy(raw["intervals"][0])
    first["end_year"] = .1
    raw["intervals"].insert(0, first)
    refined = models.evaluate_credit_paths(raw)
    assert refined["paths"][0]["default"]["year"] == pytest.approx(
        base["paths"][0]["default"]["year"], abs=2e-15
    )
    assert refined["paths"][0]["earned_interest"] == pytest.approx(3., abs=2e-14)
    assert refined["paths"][0]["random_pairs_used"] == 1
    raw["draw_stream"]["paths"][0]["pairs"] = []
    with pytest.raises(ValueError, match="exhausted.*no random fallback"):
        models.evaluate_credit_paths(raw)


def test_snapshot_fit_matches_binomial_oracle_and_holdout_never_changes_parameters():
    raw = example("credit-snapshot-calibration-1.json")
    original = deepcopy(raw)
    result = models.fit_credit_snapshots(raw)
    # P_survive=.75 -> annual q=-log(.75). No Rust output supplies this oracle.
    q = -math.log(.75)
    assert math.exp(result["parameters"][0]["value"]) == pytest.approx(q, abs=1e-6)
    assert result["training"]["log_likelihood"] == pytest.approx(
        3 * math.log(.75) + math.log(.25), abs=1e-10
    )
    assert result["training"]["predicted_endpoint_counts"][0][3] == pytest.approx(1., abs=1e-5)
    assert result["convergence"] == "coordinate_mesh_converged"
    assert result["information_rank"] == 1
    assert result["adoption_eligible"] is False
    assert result["measure"] == "physical"
    assert result["source_revision"] == raw["source_revision"]
    assert result["snapshot_provenance"][0]["source_id"] == "synthetic-tape"
    assert raw == original

    revised = deepcopy(raw)
    revised["snapshots"][-1]["destination"] = "prepaid"
    changed = models.fit_credit_snapshots(revised)
    assert changed["parameters"] == result["parameters"]
    assert changed["observed_information_eigenvalues"] == result["observed_information_eigenvalues"]
    assert changed["training"] == result["training"]
    assert changed["holdout"]["log_likelihood"] is None
    assert changed["holdout"]["impossible_intervals"] == 1
    assert changed["training_severity"]["ead_weighted_realized_lgd"] is None
    assert changed["training_severity"]["undrawn_weighted_realized_ccf"] is None


@pytest.mark.parametrize("name,adapter", [
    ("credit-path-1.json", models.evaluate_credit_paths),
    ("credit-snapshot-calibration-1.json", models.fit_credit_snapshots),
])
def test_credit_analysis_rejects_unknown_nested_options_and_missing_units(name, adapter):
    raw = example(name)
    with pytest.raises(ValueError, match="unknown field"):
        adapter(dict(raw, python_backend=True))
    altered = deepcopy(raw)
    altered.pop("monetary_unit")
    with pytest.raises(ValueError, match="monetary_unit"):
        adapter(altered)
    altered = deepcopy(raw)
    nested = altered["intervals"][0] if "intervals" in altered else altered["snapshots"][0]
    nested["inferred_event_time"] = 0.5
    with pytest.raises(ValueError, match="unknown field"):
        adapter(altered)


def test_fresh_production_context_uses_native_without_python_financial_callbacks():
    script = r'''
import json
from pathlib import Path
import numpy as np
import scipy.linalg
import scipy.optimize
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.analytics import model_contracts as models
assert RunConfig().compute_backend == "rust"
def forbidden(*args, **kwargs):
    raise AssertionError("Python financial callback executed")
np.random.default_rng = forbidden
scipy.linalg.expm = forbidden
scipy.optimize.minimize = forbidden
directory = Path(__import__("sys").argv[1])
path = json.loads((directory / "credit-path-1.json").read_text(encoding="utf-8"))
panel = json.loads((directory / "credit-snapshot-calibration-1.json").read_text(encoding="utf-8"))
assert models.evaluate_credit_paths(path)["paths"][0]["outcome"] == "default"
assert models.fit_credit_snapshots(panel)["information_rank"] == 1
'''
    environment = dict(os.environ)
    environment["PYTHONPATH"] = str(Path(__file__).resolve().parents[1] / "src")
    result = subprocess.run(
        [sys.executable, "-c", script, str(ROOT / "examples" / "models")],
        env=environment, capture_output=True, text=True, timeout=60,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_credit_analysis_missing_native_library_has_no_fallback(monkeypatch):
    from portfolio_risk.core import quant_native
    raw = example("credit-path-1.json")
    monkeypatch.setattr(quant_native, "library_path", lambda: Path("absent-credit-analysis.dll"))
    quant_native._load.cache_clear()
    with pytest.raises(RuntimeError, match="not built"):
        models.evaluate_credit_paths(raw)
