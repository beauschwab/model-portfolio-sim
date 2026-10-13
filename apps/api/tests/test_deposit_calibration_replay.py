"""Gate complete deposit evidence and immutable worker publication."""
import copy
import pytest
from app import model_jobs, model_routes, persistence as db, worker
from test_model_runs import queued
from test_persistence import durable


def native_result():
    from portfolio_risk.core import model_native
    raw = model_routes.example("observed-deposit-calibration-1")["request"]
    result, _ = model_native.run("observed-deposit-calibration-1", raw)
    return raw, result


@pytest.mark.parametrize("field", [
    "expected_migration", "migration_shape", "exposure_shape", "standard_error",
    "spell_count", "account_count", "observation_mode", "sampling_unit", "data_kind",
    "unknown_field", "integer_count_type", "provenance",
])
def test_independent_replay_rejects_every_diagnostic_and_identity(field):
    raw, result = native_result()
    assert model_jobs.verify_source(dict(schema=raw["schema"], request=raw), result)["verified"]
    corrupted = copy.deepcopy(result)
    if field == "expected_migration":
        corrupted["holdout"]["expected_migration_counts"][0][1] += 1
    elif field == "migration_shape":
        corrupted["segment_hazards"][0]["migration_per_year"].pop()
    elif field == "exposure_shape":
        corrupted["training"]["exposure_years_by_segment"].pop()
    elif field == "standard_error":
        corrupted["segment_hazards"][0]["standard_error_withdrawal"] += 1
    elif field in ("spell_count", "account_count"):
        corrupted[field] += 1
    elif field in ("observation_mode", "sampling_unit", "data_kind"):
        corrupted[field] = "unverified"
    elif field == "unknown_field":
        corrupted["holdout"]["extra"] = 1
    elif field == "integer_count_type":
        corrupted["training"]["withdrawal_counts"][0] = True
    elif field == "provenance":
        corrupted["segment_hazards"][0]["estimate_origin"] = {"kind": "observed_no_events"}
    with pytest.raises(ValueError, match="deposit calibration"):
        model_jobs.verify_source(dict(schema=raw["schema"], request=raw), corrupted)


@pytest.mark.parametrize("phase", [1, 2])
def test_replay_cancellation_before_and_after_persistence_has_no_manifest(durable, monkeypatch, phase):
    from portfolio_risk.analytics import deposit_calibration_reference as reference
    jid, job = queued("observed-deposit-calibration-1")
    original = reference.verify
    calls = [0]

    def interrupted(raw, result, check=lambda: False):
        calls[0] += 1
        steps = [0]

        def checking():
            steps[0] += 1
            if calls[0] == phase and steps[0] == 8:
                worker.STOP.set()
            return check()

        return original(raw, result, check=checking)

    monkeypatch.setattr(reference, "verify", interrupted)
    worker.execute(job)
    row = db.REPO.job(jid)
    assert calls[0] == phase and row["status"] != "done" and row["result"] is None
    assert "cancelled" in row["detail"]


@pytest.mark.parametrize("field", ["work_units", "residual_tolerance", "count_type", "nonfinite"])
def test_quote_bootstrap_verification_retains_admission_and_residual_gate(field):
    from portfolio_risk.core import model_native
    raw = model_routes.example("market-quote-bootstrap-1")["request"]
    result, _ = model_native.run(raw["schema"], raw)
    assert model_jobs.verify_source(dict(schema=raw["schema"], request=raw), result)["verified"]
    if field == "work_units":
        result["work_units"] += 1
    elif field == "residual_tolerance":
        result["diagnostics"]["residual_tolerance"] = 0.1
    elif field == "count_type":
        result["diagnostics"]["bootstrap_quote_count"] = float(result["diagnostics"]["bootstrap_quote_count"])
    else:
        result["quote_diagnostics"][0]["unit_par_pv"] = float("inf")
    with pytest.raises(ValueError, match="market quote"):
        model_jobs.verify_source(dict(schema=raw["schema"], request=raw), result)
