"""Known synthetic laws, real Rust fits, independent likelihood and holdout checks.

This suite is explicit verification support. No generated history is observed
evidence and no example policy is represented as professionally reviewed.
"""
from copy import deepcopy
import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]


def _script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT/"scripts"/f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


generator = _script("make_synthetic_model_validation")
oracle = _script("validate_synthetic_model_pack")


def test_pack_is_reproducible_and_never_relabels_example_policies(tmp_path):
    a, b = tmp_path/"a", tmp_path/"b"
    first = generator.write_pack(a, count=80)
    assert first == generator.write_pack(b, count=80)
    assert set(first["files"]) == {p.name for p in a.iterdir()}-{"manifest.json"}
    assert all((a/name).read_bytes() == (b/name).read_bytes() for name in first["files"])
    policies = json.loads((a/"example-policies.json").read_bytes())
    assert policies["data_kind"] == "synthetic"
    assert policies["reviewed"] is False and policies["approved_for_production"] is False
    with pytest.raises(ValueError, match="never overwritten"):
        generator.write_pack(a, count=80)


def test_chronology_complete_sojourns_and_out_of_time_regime_change():
    requests, truth = generator.build_pack(count=80)
    for name, raw in requests.items():
        assert raw["data_kind"] == "synthetic"
        if "spells" not in raw:
            assert all(0 < generator.seconds(s["end"])-generator.seconds(s["start"]) <= generator.YEAR_SECONDS
                       for s in raw["snapshots"])
            assert all(not(generator.seconds(s["start"]) < generator.SPLIT < generator.seconds(s["end"]))
                       for s in raw["snapshots"])
            continue
        by_account = {}
        for spell in raw["spells"]:
            by_account.setdefault(spell["account_id"], []).append(spell)
        for account, spells in by_account.items():
            boundary = generator.SPLIT if account.startswith("train-") else generator.END
            assert all(generator.seconds(s["start"]) < generator.seconds(s["end"]) <= boundary for s in spells)
            for left, right in zip(spells, spells[1:]):
                assert left["end"] == right["start"]
                assert left["ending"]["destination"] == right["origin" if "credit" in name else "segment_id"]
    for base in ["commercial-credit", "deposits", "panel-credit"]:
        a, b = requests[base], requests[base+"-shift"]
        key = "snapshots" if "snapshots" in a else "spells"
        train = lambda raw: [s for s in raw[key] if generator.seconds(s["end"]) <= generator.SPLIT]
        assert train(a) == train(b)
        assert a[key] != b[key]
        assert truth["cases"][base] != truth["cases"][base+"-shift"]


def test_raw_oracle_respects_left_open_right_closed_endpoints():
    # One year of exposure, then a default exactly at the training boundary.
    stamp = lambda ordinal: dict(ordinal=ordinal, seconds_of_day=0)
    raw = dict(schema="observed-credit-calibration-1", spells=[dict(
        origin="performing", start=stamp(1000), end=stamp(1365),
        ending=dict(kind="transition", destination="default"))])
    exposure, counts, censored = oracle._evidence(raw, 1000*86400, 1365*86400)
    assert exposure == [1., 0., 0.]
    assert counts == [[0, 0, 0, 1, 0], [0]*5, [0]*5] and censored == 0
    exposure, counts, _ = oracle._evidence(raw, 1365*86400, 1730*86400)
    assert exposure == [0.]*3 and counts == [[0]*5 for _ in range(3)]


@pytest.fixture(scope="module")
def validated(tmp_path_factory):
    directory = tmp_path_factory.mktemp("synthetic-native")/"data"
    generator.write_pack(directory, count=600)
    return directory, oracle.validate_pack(directory)


@pytest.mark.parametrize("case", ["auto-credit", "personal-credit", "card-credit", "mortgage-credit",
    "commercial-credit", "commercial-credit-shift", "deposits", "deposits-shift", "panel-credit", "panel-credit-shift"])
def test_each_product_family_has_actual_native_known_truth_validation(validated, case):
    _, report = validated
    assert report["data_kind"] == "synthetic"
    assert report["observed_accuracy_established"] is False
    assert report["reviewed_policy_approval_established"] is False
    assert report["production_adoption_eligible"] is False
    assert report["native_results"][case]["data_kind"] == "synthetic"
    if "panel" in case:
        assert report["cases"][case]["complete_realized_lgd_ccf_verified"]
    else:
        assert report["cases"][case]["exact_counts_exposures_likelihood_verified"]
    assert len(report["native_library_sha256"]) == 64


def test_independent_raw_likelihood_check_refuses_fitted_parameter_tamper(validated):
    directory, report = validated
    raw = json.loads((directory/"commercial-credit.json").read_bytes())
    truth = json.loads((directory/"truth.json").read_bytes())["cases"]["commercial-credit"]
    bad = deepcopy(report["native_results"]["commercial-credit"])
    bad["transition_hazards_per_year"][0][3] += .01
    with pytest.raises(ArithmeticError, match="training MLE"):
        oracle.check_exact_history(raw, bad, truth)


def test_changed_input_manifest_refuses_before_any_native_fit(tmp_path, monkeypatch):
    generator.write_pack(tmp_path/"data", count=80)
    path = tmp_path/"data"/"auto-credit.json"
    path.write_bytes(path.read_bytes()+b" ")
    from portfolio_risk.analytics import model_contracts
    def forbidden(*args, **kwargs):
        raise RuntimeError("native fit must not start after source corruption")
    monkeypatch.setattr(model_contracts, "fit_observed_credit", forbidden)
    with pytest.raises(AssertionError):
        oracle.validate_pack(path.parent)
