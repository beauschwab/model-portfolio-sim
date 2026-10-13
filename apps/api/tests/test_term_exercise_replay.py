"""Complete exercise policy and persisted financial-source publication gates."""
from copy import deepcopy
import pytest
from app import model_jobs, model_routes, persistence as db, worker
from test_model_runs import queued
from test_persistence import durable


@pytest.fixture(scope="module")
def source():
    from portfolio_risk.core import model_native
    raw = model_routes.example("conditional-exercise-1")["request"]
    result, _ = model_native.run("conditional-exercise-1", raw)
    return raw, result


@pytest.mark.parametrize("path", [
    ("value",), ("fixed_base_oas",), ("expected_principal_after_reporting",),
    ("nodes", 0, "continuation_value"), ("nodes", 0, "value"),
    ("nodes", 1, "decision"), ("nodes", 1, "live_pricing_probability"),
    ("nodes", 1, "canceled_future_principal"), ("nodes", 2, "continuation_value"),
    ("nodes", 2, "live_pricing_probability"), ("tree", "revision"),
    ("oas_anchor", "revision"), ("policy", "revision"), ("common_random_numbers", "id"),
    ("schedule", "as_of_ordinal"), ("schedule", "horizon_days"),
    ("schedule", "events", 0, "components", "principal"),
    ("schedule", "events", 0, "components", "cash_interest"),
    ("schedule", "events", 0, "components", "accrued_interest"),
    ("schedule", "events", 0, "components", "fees"),
    ("schedule", "events", 0, "components", "book_amortization"),
    ("schedule", "events", 0, "payment_ordinal"),
    ("schedule", "events", 0, "day_offset"),
    ("schedule", "events", 0, "measure"),
    ("schedule", "events", 0, "provenance", "input_revision"),
])
def test_complete_policy_cash_rights_and_identities_are_independently_verified(source, path):
    raw, result = source
    receipt = model_jobs.verify_source(dict(schema="conditional-exercise-1", request=raw), result)
    assert receipt["verified"] and receipt["physical_journal_eligible"] is False
    changed = deepcopy(result)
    cursor = changed
    for key in path[:-1]:
        cursor = cursor[key]
    value = cursor[path[-1]]
    cursor[path[-1]] = value + 1 if isinstance(value, (int, float)) else "unverified"
    with pytest.raises(ValueError, match="conditional exercise"):
        model_jobs.verify_source(dict(schema="conditional-exercise-1", request=raw), changed)


@pytest.mark.parametrize("change", ["extra", "missing", "node_count", "event_count", "bool_amount", "float_date", "nonfinite", "tiny_oas", "tiny_dead_probability"])
def test_shape_type_and_finite_domains_reject(source, change):
    raw, output = source
    result = deepcopy(output)
    if change == "extra": result["nodes"][0]["unverified"] = 0
    elif change == "missing": result["nodes"][0].pop("decision")
    elif change == "node_count": result["nodes"].pop()
    elif change == "event_count": result["schedule"]["events"].pop()
    elif change == "bool_amount": result["nodes"][2]["live_pricing_probability"] = False
    elif change == "float_date": result["schedule"]["as_of_ordinal"] = float(raw["asof_ordinal"])
    elif change == "tiny_oas": result["fixed_base_oas"] += 1e-10
    elif change == "tiny_dead_probability": result["nodes"][2]["live_pricing_probability"] += 1e-10
    else: result["value"] = float("nan")
    with pytest.raises(ValueError, match="conditional exercise"):
        model_jobs.verify_source(dict(schema="conditional-exercise-1", request=raw), result)


@pytest.mark.parametrize("phase", [1, 2])
def test_source_and_persisted_replay_cancel_without_manifest(durable, monkeypatch, phase):
    from portfolio_risk.analytics import term_exercise_reference as reference
    jid, job = queued("conditional-exercise-1")
    original, calls = reference.verify, [0]
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
