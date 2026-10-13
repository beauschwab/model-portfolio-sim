"""Independent supplied-tree exercise replay, exclusively a publication oracle.

Reconstructs conditional values and forward stopped cash rights from raw nodes.
No native result, production pricer or simulated future path determines policy.
It verifies the supplied model, not its calibration or actual issuer behavior.
"""
from copy import deepcopy
import math

from .derivative_verification import _tau


def _require(ok, message):
    if not ok:
        raise ValueError(f"conditional exercise: {message}")


def _probe(check):
    if check():
        raise InterruptedError("conditional exercise verification cancelled")


def _same(actual, expected, check, path="result"):
    _probe(check)
    if isinstance(expected, dict):
        _require(isinstance(actual, dict) and set(actual) == set(expected), f"{path} fields")
        for name, value in expected.items():
            _same(actual[name], value, check, f"{path}.{name}")
    elif isinstance(expected, list):
        _require(isinstance(actual, list) and len(actual) == len(expected), f"{path} count")
        for index, value in enumerate(expected):
            _same(actual[index], value, check, f"{path}[{index}]")
    elif isinstance(expected, float):
        if path == "result.fixed_base_oas":
            _require(type(actual) in (float, int) and math.isfinite(actual) and actual == expected,
                     "original OAS anchor differs")
            return
        probability = path.endswith(".live_pricing_probability")
        _require(type(actual) in (float, int) and math.isfinite(actual)
                 and math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-14 if probability else 1e-9),
                 f"{path} amount")
    else:
        _require(type(actual) is type(expected) and actual == expected, f"{path} identity/type")


def _total(cash):
    return float(cash["principal"] + cash["cash_interest"] + cash["fees"])


def _redemption(node, rule, choice):
    face = float(node["principal_before_cash"])
    if rule["timing"] == "after_scheduled_cash":
        face -= node["scheduled_cash"]["principal"]
    return dict(principal=face, cash_interest=float(node["exercise_accrued_interest"]),
                accrued_interest=float(node["exercise_accrued_interest"]), book_amortization=0.,
                fees=face * (rule[f"{choice}_clean_price_factor"] - 1.) + rule[f"{choice}_fee"])


def expected(raw, check=lambda: False):
    """Complete raw-tree replay; inputs must have passed native strict admission."""
    _probe(check)
    _require(raw["schema"] == "term-enhancements-1" and raw["model_version"] == "term-contracts-1"
             and raw["measure"] == "pricing" and raw["discount_basis"] == "market_without_oas",
             "schema/measure/discount basis")
    nodes = raw["nodes"]
    _require(0 < len(nodes) <= 100000 and len(raw["exercise_dates"]) <= 4096, "bounded tree")
    rules = {rule["ordinal"]: rule for rule in raw["exercise_dates"]}
    _require(len(rules) == len(raw["exercise_dates"]), "duplicate exercise date")
    size = len(nodes)
    values, continuations, decisions = [0.] * size, [0.] * size, ["continue"] * size
    edges = 0
    for index in range(size - 1, -1, -1):
        _probe(check)
        node = nodes[index]
        future = 0.
        for branch in node["branches"]:
            _probe(check)
            edges += 1
            _require(edges <= 400000 and type(branch["child"]) is int
                     and index < branch["child"] < size, "edge admission")
            years = _tau(node["ordinal"], nodes[branch["child"]]["ordinal"], raw["oas_day_count"])
            discount = branch["market_discount_factor"] * math.exp(-raw["fixed_base_oas"] * years)
            future += branch["conditional_probability"] * discount * values[branch["child"]]
        cash = _total(node["scheduled_cash"])
        value = continuations[index] = cash + future
        rule = rules.get(node["ordinal"])
        if rule:
            common = cash if rule["timing"] == "after_scheduled_cash" else 0.
            order = ("put", "call") if raw["simultaneous_priority"] == "issuer" else ("call", "put")
            for choice in order:
                if rule[f"{choice}_clean_price_factor"] is None:
                    continue
                exercise_value = common + _total(_redemption(node, rule, choice))
                if (choice == "call" and exercise_value < value) or (choice == "put" and exercise_value > value):
                    value, decisions[index] = exercise_value, choice
        values[index] = value
    sign = 1. if raw["direction"] == "receipt" else -1.
    live, events = [0.] * size, []
    live[0] = 1.
    remaining = float(nodes[0]["principal_before_cash"])
    for index, node in enumerate(nodes):
        _probe(check)
        if live[index] == 0.:
            continue
        choice = decisions[index]
        rule = rules.get(node["ordinal"])
        pay_scheduled = choice == "continue" or rule["timing"] == "after_scheduled_cash"
        cash = {key: float(value) if pay_scheduled else 0. for key, value in node["scheduled_cash"].items()}
        if choice == "continue":
            for branch in node["branches"]:
                _probe(check)
                live[branch["child"]] += live[index] * branch["conditional_probability"]
        else:
            redemption = _redemption(node, rule, choice)
            for key in ("principal", "cash_interest", "accrued_interest", "fees"):
                cash[key] += redemption[key]
        if node["ordinal"] <= raw["reporting_end_ordinal"]:
            remaining -= live[index] * cash["principal"]
        if _total(cash) != 0. or cash["accrued_interest"] != 0. or cash["book_amortization"] != 0.:
            events.append(dict(position_id=raw["position_id"], event_id=node["node_id"],
                scenario=dict(kind="named", scenario_id=raw["scenario_id"]),
                payment_ordinal=node["ordinal"], day_offset=node["ordinal"] - raw["asof_ordinal"],
                accrual_period=deepcopy(node["scheduled_accrual_period"]) if choice == "continue" else None,
                currency=raw["currency"], components={k: sign * live[index] * v for k, v in cash.items()},
                measure="pricing", provenance=dict(kind="modeled", model_id=raw["policy"]["id"],
                    model_version=raw["model_version"], input_revision=raw["policy"]["revision"]),
                adjustment=dict(kind="none")))
    _require(math.isclose(sum(sign * e["components"]["principal"] for e in events),
                         nodes[0]["principal_before_cash"], rel_tol=64 * math.ulp(1.), abs_tol=1e-9),
             "stopped principal conservation")
    audits = []
    for index, node in enumerate(nodes):
        _probe(check)
        audits.append(dict(node_id=node["node_id"], continuation_value=continuations[index], value=values[index],
            decision=decisions[index], live_pricing_probability=live[index],
            canceled_future_principal=0. if decisions[index] == "continue" else
                float(node["principal_before_cash"] - node["scheduled_cash"]["principal"])))
    return dict(schema=raw["schema"], model_version=raw["model_version"],
        tree=deepcopy(raw["tree"]), policy=deepcopy(raw["policy"]), oas_anchor=deepcopy(raw["oas_anchor"]),
        common_random_numbers=deepcopy(raw["common_random_numbers"]), fixed_base_oas=float(raw["fixed_base_oas"]),
        value=sign * values[0], nodes=audits, expected_principal_after_reporting=max(remaining, 0.),
        schedule=dict(schema="dated-cashflow-1", as_of_ordinal=raw["asof_ordinal"],
            horizon_days=raw["tail_end_ordinal"] - raw["asof_ordinal"], scenarios=[raw["scenario_id"]], events=events))


def verify(raw, result, check=lambda: False):
    replay = expected(raw, check=check)
    _require(isinstance(result, dict) and isinstance(result.get("disclosure"), str), "missing disclosure")
    _same({key: value for key, value in result.items() if key != "disclosure"}, replay, check)
    _probe(check)
    return dict(verified=True, conditional_nodes_recomputed=len(raw["nodes"]),
                conditional_edges_recomputed=sum(len(node["branches"]) for node in raw["nodes"]),
                stopping_cashflows_recomputed=len(replay["schedule"]["events"]),
                measure="pricing", fixed_original_oas=True, empirical_validation=False,
                physical_journal_eligible=False)
