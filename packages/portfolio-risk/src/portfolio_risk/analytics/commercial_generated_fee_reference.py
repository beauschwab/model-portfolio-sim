"""Independent generated original-tranche fee law and joint stopping pieces.

Validation only: every fee state/cash/exit is derived from independently replayed
raw loan paths and explicit fee policies. Native receipts never supply money.
The functions deliberately do not import the stochastic fitter: callers retain
training/evaluation ownership and pass only their independently built paths.
"""
from copy import deepcopy
import math

from .commercial_credit_reference import replay
from .commercial_dto import graph_typed
from .commercial_fee_adoption_reference import require_complete
from .commercial_fee_reference import bind
from .commercial_reference_math import _df

SCHEMA = "commercial-generated-fee-law-1"
ACQUISITION_SCHEMA = "commercial-generated-fee-acquisition-1"
SERVICE_FIELDS = (
    "earned", "refunded_earned", "cancelled_unearned", "waived_earned",
    "recovered_waived", "net_service_cash", "fee_receivable", "fee_payable",
    "deferred_fee_income", "prepaid_fee_expense", "unearned_entitlement",
)


def validate_law(law, raw, check=lambda: False):
    check()
    if type(law) is not dict or set(law) != {"schema", "policy", "discount_spread"} or law["schema"] != SCHEMA:
        raise ArithmeticError("explicit generated fee law schema required")
    spread = law["discount_spread"]
    if type(spread) not in (int, float) or not math.isfinite(spread) or not -5.0 <= spread <= 5.0:
        raise ArithmeticError("separate bounded generated fee spread required")
    policy = graph_typed(law["policy"])
    if (policy.get("schema") != "commercial-native-fees-1"
            or policy.get("coincident_order") != "native_exit_follows_native_exercise_timing-1"):
        raise ArithmeticError("generated fee law requires explicit native exercise clock policy")
    if policy.get("transfer") is not None:
        raise ArithmeticError("generated secondary fee rights require separate ownership")
    fees = policy["fees"]
    ids = [fee["fee_id"] for fee in fees]
    if not 1 <= len(fees) <= 64 or len(ids) != len(set(ids)):
        raise ArithmeticError("generated complete unique fee contract coverage required")
    if type(policy["complete_end_ordinal"]) is not int:
        raise ArithmeticError("generated fee clock must retain an integer ordinal")
    origin = raw["source"]["current"]["asof_ordinal"]
    if (not raw["source"]["current"]["tail_end_ordinal"] <= policy["complete_end_ordinal"] <= origin + 1080):
        raise ArithmeticError("generated complete fee tail must cover supplied loan tail")
    for fee in fees:
        if fee["position_id"] != raw["source"]["current"]["position_id"] or fee["tranche_id"] != "original":
            raise ArithmeticError("generated drawn/secondary fee acquisition rights need a separate selected owner")
    return dict(schema=SCHEMA, policy=policy, discount_spread=float(spread))


def bound(law, raw, projection, check=lambda: False, stops=()):
    law = validate_law(law, raw, check)
    native_stops = [{**stop, "timing": stop.get("timing", "after_scheduled_cash")} for stop in stops]
    sources = bind(law["policy"], raw, dict(replay=projection), check, native_stops)
    if len(sources) != 1:
        raise ArithmeticError("one independently generated loan/fee path required")
    source = sources[0]
    require_complete(source, check)
    source["source"]["raw_request"]["measure"] = raw["source"]["current"]["measure"]
    return source


def cash(source, check=lambda: False):
    result = []
    for year, group in zip(source["native_years"], source["source"]["groups"], strict=True):
        check()
        amount = math.fsum(line["amount"] for line in group["postings"] if line["account"] == "cash")
        if amount != 0.0:
            result.append(dict(event_id="separate-fee:" + group["event"]["event_id"],
                kind="separate_nonintegral_fee", year=float(year), principal=0.0,
                interest=0.0, fees=float(amount)))
    return result


def discount(raw_law, year):
    law = raw_law["nonintegral_fees"]
    return _df(raw_law["discount_curve"], year) * math.exp(
        -(law["discount_spread"] + raw_law["parallel_bump"]) * year)


def observed_features(law, source, raw, date):
    year = (date["ordinal"] - raw["source"]["current"]["asof_ordinal"]) / 365.0
    states = {fee["fee_id"]: {**dict.fromkeys(SERVICE_FIELDS, 0.0),
                              "unearned_entitlement": float(fee["entitlement"])}
              for fee in law["policy"]["fees"]}
    for clock, group in zip(source["native_years"], source["source"]["groups"], strict=True):
        if clock < year or clock == year and date["timing"] == "after_scheduled_cash":
            states[group["event"]["fee_id"]] = group["next"]
    opening = raw["source"]["current"]["opening_principal"]
    return [float(states[fee["fee_id"]].get(field, 0.0)) / opening
            for fee in law["policy"]["fees"] for field in SERVICE_FIELDS]


def trial_payoffs(raw_law, raw, date, point, check=lambda: False):
    """Fee root PV of genuine stopped raw sources; no fitted forecast is used."""
    if point["state"] not in ("performing", "watch", "delinquent"):
        return {"call": None, "put": None}
    if point["principal"] == 0.0 and not point["surviving_rights"]:
        return {"call": None, "put": None}
    origin = raw["source"]["current"]["asof_ordinal"]
    year = (date["ordinal"] - origin) / 365.0
    result = {}
    for action in ("call", "put"):
        factor = date[action + "_clean_price_factor"]
        if factor is None:
            result[action] = None
            continue
        stop = dict(path_key=raw["credit"]["paths"][0]["path_key"],
            event_id=f"generated-fee-trial:{raw_law['identity']['id']}:{date['ordinal']}:{action}",
            ordinal=date["ordinal"], action=action, principal_clean_price_factor=factor,
            fee=date[action + "_fee"], review=deepcopy(raw_law["policy"]["identity"]),
            timing=date["timing"])
        if point["principal"] == 0.0:
            reviewed = raw_law["policy"].get("rights_only_termination")
            if reviewed is None:
                raise ArithmeticError("generated fee rights-only action requires native loan termination policy")
            stop["rights_only_review"] = deepcopy(reviewed["review"])
        stopped = replay(raw, check, stops=[stop])
        fee = bound(raw_law["nonintegral_fees"], raw, stopped, check, [stop])
        result[action] = math.fsum(row["fees"] * discount(raw_law, row["year"])
            for row in cash(fee, check) if row["year"] > year
            or row["year"] == year and (date["timing"] == "before_scheduled_cash"
                or row["event_id"].startswith("separate-fee:native-fee:")))
    return result


def action_value(raw_law, date, point, action):
    loan = (point["principal"] * date[action + "_clean_price_factor"]
            + point["legal"] + date[action + "_fee"])
    fee = point["fee_payoffs"][action]
    if fee is None:
        raise ArithmeticError("complete generated fee action source absent")
    year = (date["ordinal"] - raw_law["source"]["source"]["current"]["asof_ordinal"]) / 365.0
    loan_discount = _df(raw_law["discount_curve"], year) * math.exp(
        -(raw_law["fixed_base_oas"] + raw_law["parallel_bump"]) * year)
    return loan + fee / loan_discount


def acquired_projection(law, acquisition, raw, loan_projection, check=lambda: False, *, original=False, stops=()):
    """Recompute one complete service/cost path; no native checkpoints consumed."""
    from .nonintegral_fee_reference import replay as replay_fee
    law = validate_law(law, raw, check)
    physical = raw["source"]["current"]["measure"] == "physical"
    allocations = graph_typed(acquisition["allocations"]) if physical else []
    releases = (acquisition["original_physical_releases"] if original else
        next(h["acquisition_releases"] for h in acquisition["physical_releases_by_scenario"]
             if h["scenario_id"] == raw["source"]["current"]["scenario_id"])) if physical else []
    source = bound(law, raw, loan_projection, check, stops)
    history = dict(path_id=raw["credit"]["paths"][0]["path_id"], policy=deepcopy(law["policy"]))
    if releases:
        history["acquisition_releases"] = graph_typed(releases)
    if physical:
        ids = [a["fee_id"] for a in allocations]
        if len(ids) != len(set(ids)) or set(ids) != {f["fee_id"] for f in law["policy"]["fees"]}:
            raise ArithmeticError("generated separate acquired fee coverage differs")
        original_request = source["source"]["raw_request"]
        origin = original_request["asof_ordinal"]
        remaining = {a["fee_id"]: float(a["allocated_cost"]) for a in allocations}
        previous = None
        for row in releases:
            check()
            clock = (row["ordinal"], row["sequence"])
            amount = row["amount"]
            rest = remaining.get(row["fee_id"])
            if (type(row["ordinal"]) is not int or type(row["sequence"]) is not int
                    or previous is not None and previous >= clock or rest is None
                    or not origin <= row["ordinal"] <= law["policy"]["complete_end_ordinal"]
                    or type(amount) not in (float, int) or not math.isfinite(amount)
                    or amount == 0. or math.copysign(1., amount) != math.copysign(1., rest)
                    or abs(amount) > abs(rest)):
                raise ArithmeticError("generated complete reviewed fee cost releases differ")
            remaining[row["fee_id"]] -= amount
            previous = clock
        if any(abs(v) > 1e-9 + 1e-12 * abs(v) for v in remaining.values()):
            raise ArithmeticError("generated acquired fee cost tail incomplete")
        events = [(year, 1, deepcopy(event)) for year, event in
                  zip(source["native_years"], original_request["events"], strict=True)]
        fees = {f["fee_id"]: f for f in law["policy"]["fees"]}
        for index, row in enumerate(allocations):
            if row["allocated_cost"] != 0.:
                events.append((0., 0, dict(event_id=f"native-fee:{source['path_key']}:{index}:acquire",
                    fee_id=row["fee_id"], source=deepcopy(fees[row["fee_id"]]["source"]),
                    review=deepcopy(row["review"]), ordinal=origin, sequence=0,
                    action="acquire", reason="other_reviewed", amount=float(row["allocated_cost"]))))
        for index, row in enumerate(releases):
            events.append(((row["ordinal"] - origin) / 365., 2,
                dict(event_id=f"native-fee:{source['path_key']}:{index}:release-acquired-cost",
                     fee_id=row["fee_id"], source=deepcopy(fees[row["fee_id"]]["source"]),
                     review=deepcopy(row["review"]), ordinal=row["ordinal"], sequence=0,
                     action="release_acquired_cost", reason="other_reviewed", amount=float(row["amount"]))))
        events.sort(key=lambda row: (row[0], row[1]))
        original_request["events"] = []
        source["native_years"] = []
        for index, (year, _, event) in enumerate(events):
            event["sequence"] = index
            original_request["events"].append(event)
            source["native_years"].append(float(year))
        source["source"] = replay_fee(original_request, check)
    source["source"]["raw_request"]["measure"] = "physical" if physical else "pricing"
    service_cash = []
    income_points = []
    for year, group in zip(source["native_years"], source["source"]["groups"], strict=True):
        check()
        if group["event"]["action"] != "acquire":
            amount = math.fsum(p["amount"] for p in group["postings"] if p["account"] == "cash")
            if amount != 0.:
                service_cash.append(dict(event_id="separate-fee:" + group["event"]["event_id"],
                    kind="separate_nonintegral_fee", year=float(year), principal=0., interest=0., fees=float(amount)))
        if physical:
            lines = [p["amount"] for p in group["postings"] if p["account"] in
                     ("fee_income", "fee_writeoff_expense", "fee_recovery_income", "fee_acquisition_expense")]
            if lines:
                income_points.append(dict(year=float(year), income_lines=lines))
    compact = dict(schema="commercial-generated-fee-source-1", raw_fee_history=history,
        cash=service_cash, discount_spread=float(law["discount_spread"]), allocations=allocations,
        income_points=income_points, full_native_fee_tail_replayed=True, loan_original_cost_yield_unchanged=True)
    full = dict(raw_fee_history=history, bound_path=source, cash=service_cash,
        discount_spread=float(law["discount_spread"]), allocations=allocations,
        loan_original_cost_yield_unchanged=True)
    return compact, full


def loan_component(raw_law, evaluated, retained, check=lambda: False):
    """Loan-only quote under the independently learned joint stopping policy."""
    result = []
    origin = raw_law["source"]["source"]["current"]["asof_ordinal"]
    for path, outcome in zip(retained["evaluation"], evaluated["evaluation_paths"], strict=True):
        check()
        ordinal = outcome["first_exercise_ordinal"]
        index = next((i for i, d in enumerate(raw_law["policy"]["exercise_dates"]) if d["ordinal"] == ordinal), None)
        date = raw_law["policy"]["exercise_dates"][index] if index is not None else None
        def df(year):
            return _df(raw_law["discount_curve"], year) * math.exp(-(raw_law["fixed_base_oas"] + raw_law["parallel_bump"]) * year)
        year = (ordinal - origin) / 365. if ordinal is not None else None
        value = sum((c["principal"] + c["interest"] + c["fees"]) * df(c["year"])
            for c in path["cash"] if c["year"] > 0. and (year is None or c["year"] < year
                or c["year"] == year and date["timing"] == "after_scheduled_cash"))
        if date is not None:
            point = path["points"][index]
            action = outcome["first_exercise_decision"]
            value += (point["principal"] * date[action + "_clean_price_factor"]
                      + point["legal"] + date[action + "_fee"]) * df(year)
        result.append(value)
    return sum(result) / len(result)
