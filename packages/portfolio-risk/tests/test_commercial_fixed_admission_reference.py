"""Raw-only source dimensions; no producer counter can choose the expected bound."""

from copy import deepcopy
import gzip
import json
from pathlib import Path

import pytest

from portfolio_risk.analytics import commercial_fixed_admission_reference as census
from portfolio_risk.analytics.commercial_cut_capture_reference import (
    finite_source_order,
)
from portfolio_risk.analytics.commercial_sequential_packet_reference import (
    financial_parts,
)
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity
from portfolio_risk.core import model_native, product_strategy_native, quant_native

ROOT = Path(__file__).resolve().parents[3]


@pytest.fixture(scope="module")
def inputs():
    raw = json.loads(
        (ROOT / "examples/models/commercial-cut-mixed-journal-1.json").read_text()
    )
    packet = json.loads(
        gzip.decompress(
            (
                Path(__file__).with_name("fixtures")
                / "commercial-sequential-packet-witness-2.json.gz"
            ).read_bytes()
        )
    )
    return raw, financial_parts(raw, packet)


@pytest.fixture(autouse=True)
def forbid_native(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("native callback during independent admission")

    monkeypatch.setattr(model_native, "run", forbidden)
    monkeypatch.setattr(product_strategy_native, "_run", forbidden)
    monkeypatch.setattr(quant_native, "call", forbidden)
    monkeypatch.setattr(quant_native, "term_call", forbidden)


def test_all_twenty_seven_raw_sources_reproduce_complete_native_dimensions(inputs):
    raw, parts = inputs
    records = parts["source_records"]
    assert len(records) == 27
    for item, record in zip(finite_source_order(raw), records, strict=True):
        admission, original, current, expanded = census.source(
            item.history["source"], record["role"]
        )
        projection = record["proof"]["projection"]
        for actual, expected in [
            (projection["admission"], admission),
            (projection["original_source"]["admission"], original),
            (projection["current_source"]["admission"], current),
            (projection["carry"]["raw_request"], expanded),
        ]:
            assert exact_json_identity(actual, expected)


@pytest.mark.parametrize(
    "side,work,retained",
    [("completed", 7297778, 40235472), ("revised", 14485964, 77716168)],
)
def test_raw_preparation_census_matches_native_with_no_resource_authority(
    inputs, side, work, retained
):
    raw, parts = inputs
    got = census.verify(raw[side + "_candidate"], parts[side + "_preparation"])
    assert got == dict(
        schema="commercial-fixed-preparation-census-1",
        work_upper=work,
        retained_bytes_upper=retained,
        full_end_ordinal=738916,
        raw_derived_source_preparation=True,
        complete_resource_proof=False,
        publication_eligible=False,
    )


@pytest.mark.parametrize(
    "field",
    [
        "admitted_work_upper",
        "retained_bytes_upper",
        "resolved_specification",
        "raw_request",
    ],
)
def test_agreeing_or_forged_producer_counters_do_not_supply_expected_dimensions(
    inputs, field
):
    raw, parts = inputs
    changed = deepcopy(parts["completed_preparation"])
    if field == "resolved_specification":
        changed[field]["horizon_days"] += 1
    elif field == "raw_request":
        changed[field]["allocation"][0]["face"] = 0.0
    else:
        changed[field] += 1
    with pytest.raises(ArithmeticError):
        census.verify(raw["completed_candidate"], changed)


def test_zero_allocation_and_unicode_never_remove_paths_or_source_dimensions(inputs):
    raw, _ = inputs
    full = raw["revised_candidate"]
    zero = deepcopy(full)
    for allocation in zero["allocation"]:
        allocation["face"] = 0.0
    before, _ = census.candidate(full)
    after, _ = census.candidate(zero)
    # Only the exact typed wire spelling changes; complete group/index dimensions remain.
    delta = census.length(census.graph_typed(full)) - census.length(
        census.graph_typed(zero)
    )
    assert before["work_upper"] - after["work_upper"] == 8 * delta
    assert before["retained_bytes_upper"] - after["retained_bytes_upper"] == 20 * delta
    longer = deepcopy(full)
    longer["owners"][0]["source"]["id"] = "source 雪😀"
    got, _ = census.candidate(longer)
    delta = census.length(census.graph_typed(longer)) - census.length(
        census.graph_typed(full)
    )
    assert got["work_upper"] - before["work_upper"] == 8 * delta
    assert got["retained_bytes_upper"] - before["retained_bytes_upper"] == 20 * delta


@pytest.mark.parametrize(
    "selection",
    [
        "transfer",
        "reviewed_credit_actions",
        "prepayment_terms",
        "maturity_basis_policy",
        "terminal_obligations",
    ],
)
def test_advanced_owners_require_a_separate_census_even_if_the_selection_is_empty(
    inputs, selection
):
    raw, _ = inputs
    owner = deepcopy(next(iter(finite_source_order(raw))).history["source"])
    if selection == "transfer":
        owner["source"][selection] = {}
    elif selection == "terminal_obligations":
        owner["factor_policy"][selection] = {}
    else:
        owner[selection] = {}
    with pytest.raises(ArithmeticError, match="separate advanced source"):
        census.source(owner, "physical")


def test_complete_calendar_bound_precedes_expanded_daily_allocation(
    inputs, monkeypatch
):
    raw, _ = inputs
    owner = deepcopy(next(iter(finite_source_order(raw))).history["source"])
    owner["source"]["current"]["tail_end_ordinal"] = (
        owner["source"]["current"]["asof_ordinal"] + 1081
    )

    def forbidden(*args, **kwargs):
        raise AssertionError("expanded before calendar admission")

    monkeypatch.setattr(census, "prepared", forbidden)
    with pytest.raises(ArithmeticError, match="1080days"):
        census.source(owner, "physical")


def test_original_deadline_and_callback_cancellation_stop_census(inputs):
    raw, _ = inputs

    def expired():
        raise TimeoutError("original deadline")

    with pytest.raises(TimeoutError, match="original deadline"):
        census.candidate(raw["completed_candidate"], expired)
    with pytest.raises(InterruptedError, match="cancelled"):
        census.candidate(raw["completed_candidate"], lambda: True)


def test_declared_integer_money_is_normalized_but_opaque_ledger_numbers_are_retained(
    inputs,
):
    raw, parts = inputs
    changed = deepcopy(raw["completed_candidate"])
    for allocation in changed["allocation"]:
        allocation["face"] = int(allocation["face"])
    for owner in changed["owners"]:
        for key in (
            "pre_purchase_principal",
            "pre_purchase_accrued",
            "pre_purchase_basis",
        ):
            owner[key] = int(owner[key])
    assert census.verify(changed, parts["completed_preparation"]) == census.verify(
        raw["completed_candidate"], parts["completed_preparation"]
    )
    opaque = census._candidate_dto(changed)["specification"]
    assert exact_json_identity(opaque, changed["specification"])


@pytest.mark.parametrize(
    "field",
    ["face", "pre_purchase_principal", "pre_purchase_accrued", "pre_purchase_basis"],
)
def test_boolean_candidate_money_cannot_supply_a_native_float_dimension(inputs, field):
    raw, _ = inputs
    changed = deepcopy(raw["completed_candidate"])
    rows = changed["allocation"] if field == "face" else changed["owners"]
    rows[0][field] = False
    with pytest.raises(ArithmeticError, match="finite native candidate"):
        census.candidate(changed)
