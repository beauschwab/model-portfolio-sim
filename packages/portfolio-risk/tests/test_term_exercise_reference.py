"""Raw-tree oracle against Rust, including recombination and exercise bounds."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts, term_exercise_reference as reference

ROOT = Path(__file__).resolve().parents[3]


def fixture():
    return json.loads((ROOT / "examples/models/term-exercise.json").read_text())["request"]


@pytest.mark.parametrize("timing", ["before_scheduled_cash", "after_scheduled_cash"])
@pytest.mark.parametrize("priority", ["issuer", "holder"])
@pytest.mark.parametrize("direction", ["receipt", "payment"])
def test_independent_replay_of_cash_priority_signed_rights_and_locked_oas(timing, priority, direction):
    raw = fixture()
    raw.update(direction=direction, simultaneous_priority=priority, fixed_base_oas=0.0125)
    rule = raw["exercise_dates"][0]
    rule.update(timing=timing, call_clean_price_factor=0.9, put_clean_price_factor=1.1, call_fee=0.75, put_fee=-0.5)
    raw["nodes"][1]["exercise_accrued_interest"] = -0.25
    result = model_contracts.value_conditional_exercise(raw)
    receipt = reference.verify(raw, result)
    assert receipt["verified"] and not receipt["physical_journal_eligible"]


def test_recombining_tree_replays_dead_and_shared_tail_nodes_without_foresight():
    raw = fixture()
    root, low, tail = raw["nodes"]
    high = deepcopy(low)
    high["node_id"] = "high"
    low["node_id"] = "low"
    root["branches"] = [dict(child=1, conditional_probability=0.5, market_discount_factor=0.98),
                         dict(child=2, conditional_probability=0.5, market_discount_factor=0.98)]
    low["branches"][0]["child"] = high["branches"][0]["child"] = 3
    high["branches"][0]["market_discount_factor"] = 0.9
    raw["nodes"] = [root, low, high, tail]
    result = model_contracts.value_conditional_exercise(raw)
    reference.verify(raw, result)
    assert result["value"] == pytest.approx(0.98 * (105. + 99.5) / 2., abs=1e-12)
    assert result["nodes"][3]["live_pricing_probability"] == 0.5
    assert result["expected_principal_after_reporting"] == 50.


@pytest.mark.parametrize("kind", ["none", "call", "put"])
def test_native_stochastic_refinement_against_independent_normal_integral(kind):
    prices = []
    for steps in (16, 64, 256):
        raw = fixture()
        root, middle, maturity = raw["nodes"]
        raw["exercise_dates"] = [] if kind == "none" else [dict(raw["exercise_dates"][0],
            timing="before_scheduled_cash", call_clean_price_factor=1. if kind == "call" else None,
            put_clean_price_factor=1. if kind == "put" else None)]
        width = 0.3 / math.sqrt(steps)
        normalization = math.cosh(width) ** steps
        root["branches"] = []
        intermediate, terminal = [], []
        probability = 2. ** -steps
        for state in range(steps + 1):
            root["branches"].append(dict(child=state + 1, conditional_probability=probability, market_discount_factor=1.))
            center, leaf = deepcopy(middle), deepcopy(maturity)
            center["node_id"], leaf["node_id"] = f"state:{state}", f"tail:{state}"
            center["scheduled_cash"]["cash_interest"] = center["scheduled_cash"]["accrued_interest"] = 0.
            center["branches"] = [dict(child=steps + 2 + state, conditional_probability=1., market_discount_factor=1.)]
            right = 100. * math.exp(width * (2 * state - steps)) / normalization - 100.
            leaf["scheduled_cash"]["cash_interest"] = leaf["scheduled_cash"]["accrued_interest"] = right
            intermediate.append(center)
            terminal.append(leaf)
            if state < steps:
                probability *= (steps - state) / (state + 1)
        raw["nodes"] = [root] + intermediate + terminal
        result = model_contracts.value_conditional_exercise(raw)
        reference.verify(raw, result)
        prices.append(result["value"])
    # Mean-one lognormal payoff: capped/floored expectations from its normal CDF.
    cap = 100. * math.erfc(0.15 / math.sqrt(2.))
    target = {"none": 100., "call": cap, "put": 200. - cap}[kind]
    if kind == "none":
        assert prices == pytest.approx([target] * 3, abs=2e-12)
    else:
        errors = [abs(price - target) for price in prices]
        assert errors[2] < errors[1] < errors[0]
        assert errors[2] < 0.02


def test_verifier_never_invokes_native_or_production_pricing(monkeypatch):
    raw = fixture()
    result = model_contracts.value_conditional_exercise(raw)
    from portfolio_risk.core import quant_native, model_native
    def forbidden(*args, **kwargs):
        raise AssertionError("native callback in independent exercise oracle")
    monkeypatch.setattr(quant_native, "term_call", forbidden)
    monkeypatch.setattr(model_native, "run", forbidden)
    assert reference.verify(raw, result)["verified"]
