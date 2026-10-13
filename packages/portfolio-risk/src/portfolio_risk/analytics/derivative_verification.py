"""Independent raw-contract derivative/journal verification, never a backend.

No native callbacks or application pricers are called. Dates, coupon arithmetic,
conditional European values, legal collateral and journal policies are reconstructed
from immutable raw inputs. Only complete supported results receive a receipt.
"""
from collections import defaultdict
from copy import deepcopy
from datetime import date
import hashlib
import json
import math


class DerivativeVerificationError(ArithmeticError):
    """Raw source, economic result or reviewed journal failed verification."""


def _require(condition, message):
    if not condition:
        raise DerivativeVerificationError(message)


def _same(actual, expected, path="result"):
    if isinstance(expected, dict):
        extra = {"disclosure"} if path in {"result", "result.native", "result.pricing_xva_diagnostic"} else set()
        _require(isinstance(actual, dict) and set(actual) - extra == set(expected), f"{path}: fields differ")
        if "disclosure" in actual:
            _require(isinstance(actual["disclosure"], str), f"{path}: invalid disclosure")
        for key, value in expected.items():
            _same(actual[key], value, f"{path}.{key}")
    elif isinstance(expected, (list, tuple)):
        _require(isinstance(actual, (list, tuple)) and len(actual) == len(expected), f"{path}: count differs")
        for i, value in enumerate(expected):
            _same(actual[i], value, f"{path}[{i}]")
    elif isinstance(expected, float):
        _require(isinstance(actual, (float, int)) and not isinstance(actual, bool)
                 and math.isfinite(actual) and math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-9),
                 f"{path}: financial amount differs ({actual} != {expected})")
    else:
        _require(type(actual) is type(expected) and actual == expected, f"{path}: identity/date/policy differs")


def _keys(value, names, optional=()):
    _require(isinstance(value, dict) and set(value) <= set(names) | set(optional)
             and set(names) <= set(value), "unsupported/missing raw fields")


def _identity(value):
    _keys(value, ("id", "revision"))
    _require(all(isinstance(v, str) and 0 < len(v.encode()) <= 4096 for v in value.values()), "invalid source identity")
    return value


def _money(value):
    _require(isinstance(value, (int, float)) and not isinstance(value, bool)
             and math.isfinite(value) and abs(value) <= 1e15, "invalid monetary domain")
    return float(value)


def _finite(value):
    _require(isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value), "nonfinite supplied rate")
    return float(value)


def _day(value):
    _require(type(value) is int and 1 <= value <= 3652059, "invalid actual date")
    return date.fromordinal(value)


def _utc_day(value):
    _require(type(value) is int and 0 <= value <= 253402300799, "invalid exact UTC cutoff")
    return 719163 + value // 86400


def _tau(start, end, convention):
    a, b = _day(start), _day(end)
    _require(start < end and end-start <= 36600, "invalid interest interval")
    if convention == "act360":
        return (end-start)/360.
    if convention == "act365_fixed":
        return (end-start)/365.
    _require(convention == "act_act_isda", "unsupported day-count policy")
    pieces = []
    while a.year != b.year:
        stop = date(a.year+1, 1, 1)
        pieces.append((stop-a).days / (366. if date(a.year, 12, 31).timetuple().tm_yday == 366 else 365.))
        a = stop
    pieces.append((b-a).days / (366. if date(a.year, 12, 31).timetuple().tm_yday == 366 else 365.))
    return math.fsum(pieces)


def _clip(rate, bounds):
    _keys(bounds, ("floor", "cap"))
    floor, cap = bounds["floor"], bounds["cap"]
    _require(all(v is None or isinstance(v, (float, int)) and math.isfinite(v) for v in (floor, cap)), "invalid rate bounds")
    _require(floor is None or cap is None or floor <= cap, "inverted rate bounds")
    return min(max(rate, floor if floor is not None else -math.inf), cap if cap is not None else math.inf)


def _coupon(period, cutoff, measure, path, override=None):
    c = period["coupon"]
    _keys(c, ("kind", "index", "calendar", "complete_calendar_mapping", "calculation", "margin_treatment", "margin", "index_bounds", "all_in_bounds", "reset_cutoff_unix_seconds", "lookback_business_days", "observation_shift", "observations"))
    _require(c["kind"] == "indexed" and c["complete_calendar_mapping"] is True, "unsupported indexed mapping")
    _identity(c["calendar"])
    obs = override["observations"] if override else c["observations"]
    selected_path = override["path"] if override else path
    calc = c["calculation"]
    term = calc == "term_simple"
    _require(calc in {"term_simple", "daily_simple", "daily_compounded"}
             and c["margin_treatment"] in {"simple", "compounded"}
             and (calc == "daily_compounded" or c["margin_treatment"] == "simple"), "unsupported compounding/margin policy")
    index = c["index"]
    _keys(index, ("index_id", "kind", "tenor_months", "average_calendar_days"))
    _require(isinstance(index["index_id"], str) and index["index_id"], "missing index identity")
    valid = ((index["kind"] == "term" and type(index["tenor_months"]) is int and 1 <= index["tenor_months"] <= 1200 and index["average_calendar_days"] is None)
             or (index["kind"] == "overnight" and index["tenor_months"] is None and index["average_calendar_days"] is None)
             or (index["kind"] == "published_average" and index["tenor_months"] is None and type(index["average_calendar_days"]) is int and 1 <= index["average_calendar_days"] <= 36600))
    _require(valid and type(c["lookback_business_days"]) is int and 0 <= c["lookback_business_days"] <= 366
             and type(c["observation_shift"]) is bool and 0 < len(obs) <= 250000, "unsupported index/observation convention")
    if term:
        reset = c["reset_cutoff_unix_seconds"]
        _require(len(obs) == 1 and not c["observation_shift"] and index["kind"] != "overnight"
                 and _utc_day(reset) <= period["start_ordinal"], "invalid term reset")
    else:
        _require(index["kind"] == "overnight" and c["reset_cutoff_unix_seconds"] is None, "invalid overnight reset")
    margin = _finite(c["margin"])
    simple, logs, margin_weights, sources = [], [], [], []
    cursor, prior_end = period["start_ordinal"], None
    for o in obs:
        _keys(o, ("interest_start_ordinal", "interest_end_ordinal", "observation_start_ordinal", "observation_end_ordinal", "fixing_ordinal", "selection_cutoff_unix_seconds", "rate", "source"))
        it = _tau(o["interest_start_ordinal"], o["interest_end_ordinal"], period["day_count"])
        ot = _tau(o["observation_start_ordinal"], o["observation_end_ordinal"], period["day_count"])
        _day(o["fixing_ordinal"]); _utc_day(o["selection_cutoff_unix_seconds"])
        _require(o["interest_start_ordinal"] == cursor and o["interest_end_ordinal"] <= period["end_ordinal"]
                 and o["observation_start_ordinal"] <= cursor and o["fixing_ordinal"] <= cursor
                 and (term or o["fixing_ordinal"] == o["observation_start_ordinal"])
                 and (not c["observation_shift"] or prior_end is None or prior_end == o["observation_start_ordinal"]), "incomplete interest/observation dates")
        s = o["source"]
        if s["kind"] == "observed":
            _keys(s, ("kind", "fixing", "publication_unix_seconds")); _identity(s["fixing"])
            availability = s["publication_unix_seconds"]
            _require(o["fixing_ordinal"] <= _utc_day(availability) and availability <= cutoff, "unavailable observed fixing")
        else:
            _keys(s, ("kind", "model", "path", "information_unix_seconds", "measure"))
            _require(s["kind"] == "conditional_projection" and s["measure"] == measure
                     and s["path"] == selected_path and s["information_unix_seconds"] <= cutoff, "projection measure/path/lookahead mismatch")
            _identity(s["model"]); _identity(s["path"])
            availability = s["information_unix_seconds"]; _utc_day(availability)
            _require(term or _utc_day(o["selection_cutoff_unix_seconds"]) <= o["observation_start_ordinal"], "overnight projection lookahead")
        _require(availability <= o["selection_cutoff_unix_seconds"]
                 and (not term or availability <= reset and o["fixing_ordinal"] <= _utc_day(reset)), "exact selection/reset lookahead")
        rate = _clip(_clip(_finite(o["rate"]), c["index_bounds"])+margin, c["all_in_bounds"])
        weight = ot if c["observation_shift"] else it
        if calc == "daily_compounded":
            increment = (rate if c["margin_treatment"] == "compounded" else rate-margin)*weight
            _require(increment > -1 and math.isfinite(increment), "nonpositive compound factor")
            logs.append(math.log1p(increment)); margin_weights.append(it)
        else:
            simple.append((rate-margin)*weight + margin*it)
        cursor, prior_end = o["interest_end_ordinal"], o["observation_end_ordinal"]
        sources.append(deepcopy(s))
    _require(cursor == period["end_ordinal"], "incomplete coupon coverage")
    factor = (math.expm1(math.fsum(logs)) + (margin*math.fsum(margin_weights) if c["margin_treatment"] == "simple" else 0.)) if calc == "daily_compounded" else math.fsum(simple)
    _require(math.isfinite(factor) and factor > -1., "invalid complete coupon factor")
    return factor, sources


def _discount(curve, asof, currency, cutoff):
    _keys(curve, ("source", "collateral_discount_policy", "currency", "asof_ordinal", "information_unix_seconds", "basis", "points"))
    _identity(curve["source"]); _identity(curve["collateral_discount_policy"])
    _require(curve["asof_ordinal"] == asof and curve["currency"] == currency and curve["basis"] == "collateralized_clean"
             and _utc_day(curve["information_unix_seconds"]) <= asof and curve["information_unix_seconds"] <= cutoff
             and 0 < len(curve["points"]) <= 8192, "invalid exact collateral curve")
    values = {}
    prior = None
    for p in curve["points"]:
        _keys(p, ("ordinal", "discount_factor")); _day(p["ordinal"])
        _require(asof <= p["ordinal"] <= asof+36600 and (prior is None or prior < p["ordinal"])
                 and math.isfinite(p["discount_factor"]) and p["discount_factor"] > 0., "invalid collateral discount point")
        values[p["ordinal"]] = float(p["discount_factor"]); prior = p["ordinal"]
    _require(curve["points"][0]["ordinal"] == asof and values[asof] == 1., "missing exact discount anchor")
    return values


def _active(payment, asof, timing):
    _require(timing in {"before_scheduled_payments", "after_scheduled_payments"}, "unsupported cash timing")
    return payment >= asof if timing == "before_scheduled_payments" else payment > asof


def _swap(s, due_date=None, physical_paths=None, measure="pricing"):
    _keys(s, ("contract", "index_basis", "rate_path", "position_id", "scenario_id", "currency", "monetary_unit", "asof_ordinal", "valuation_cutoff_unix_seconds", "tail_end_ordinal", "payment_timing", "side", "notional", "fixed_rate", "fixed_periods", "floating_periods", "discount_curve"))
    for k in ("contract", "index_basis", "rate_path"): _identity(s[k])
    _require(s["side"] in {"payer", "receiver"} and s["monetary_unit"] == "currency_units" and _utc_day(s["valuation_cutoff_unix_seconds"]) == s["asof_ordinal"], "invalid swap side/unit/time")
    notional = _money(s["notional"]); _require(notional >= 0., "negative notional")
    _day(s["asof_ordinal"]); _day(s["tail_end_ordinal"])
    dfs = _discount(s["discount_curve"], s["asof_ordinal"], s["currency"], s["valuation_cutoff_unix_seconds"])
    sign = 1. if s["side"] == "receiver" else -1.
    cash, pvs = [], []
    for leg in ("fixed_periods", "floating_periods"):
        ps = s[leg]; _require(0 < len(ps) <= 4096, "invalid leg count")
        ids, prior = set(), None
        for p in ps:
            names = ("period_id", "start_ordinal", "end_ordinal", "payment_ordinal", "day_count")
            _keys(p, names if leg == "fixed_periods" else names+("coupon", "fee"))
            tau = _tau(p["start_ordinal"], p["end_ordinal"], p["day_count"]); _day(p["payment_ordinal"])
            _require(p["period_id"] not in ids and (prior is None or prior == p["start_ordinal"])
                     and max(p["payment_ordinal"], p["end_ordinal"]) <= s["tail_end_ordinal"], "leg identity/date/tail mismatch")
            ids.add(p["period_id"]); prior = p["end_ordinal"]
            active = due_date is None and _active(p["payment_ordinal"], s["asof_ordinal"], s["payment_timing"])
            if leg == "floating_periods":
                _require(p["fee"] == 0. and p["coupon"]["kind"] == "indexed", "unsupported floating swap contract")
            if not active and p["payment_ordinal"] != due_date: continue
            override = physical_paths.get(p["period_id"]) if physical_paths is not None else None
            sources = []
            if leg == "fixed_periods":
                value = sign*notional*_finite(s["fixed_rate"])*tau
                event = "fixed:"+p["period_id"]
            else:
                _require(p["fee"] == 0., "unsupported swap fees")
                factor, sources = _coupon(p, s["valuation_cutoff_unix_seconds"], measure, s["rate_path"], override)
                value = -sign*notional*factor; event = "floating:"+p["period_id"]
            value = _money(value)
            if active:
                _require(p["payment_ordinal"] in dfs, "missing exact payment discount")
                pvs.append(value*dfs[p["payment_ordinal"]])
            if p["payment_ordinal"] == due_date:
                cash.append({"event_id":event, "amount":value, "sources":sources, "physical_path": override["path"] if override else None})
    _require(s["fixed_periods"][0]["start_ordinal"] == s["floating_periods"][0]["start_ordinal"] and s["fixed_periods"][-1]["end_ordinal"] == s["floating_periods"][-1]["end_ordinal"], "swap leg term mismatch")
    return _money(math.fsum(pvs)), cash


def _swap_key(s):
    fixed = deepcopy(s["fixed_periods"])
    floating = deepcopy(s["floating_periods"])
    for p in floating: p["coupon"].pop("observations", None); p.pop("fee", None)
    return [s[k] for k in ("contract", "index_basis", "currency", "monetary_unit", "side", "notional", "fixed_rate")]+[fixed, floating]


def _option(o):
    _keys(o, ("contract", "exercise_policy", "conditional_model", "crn", "position_id", "scenario_id", "currency", "monetary_unit", "measure", "asof_ordinal", "expiry_ordinal", "settlement_ordinal", "tail_end_ordinal", "side", "settlement", "states"))
    for k in ("contract", "exercise_policy", "conditional_model", "crn"): _identity(o[k])
    _require(o["measure"] == "pricing" and o["side"] in {"long", "short"} and o["settlement"] in {"physical_delivery", "collateralized_cash_price"} and 0 < len(o["states"]) <= 256, "unsupported European contract")
    _require(o["asof_ordinal"] <= o["expiry_ordinal"] <= o["settlement_ordinal"] <= o["tail_end_ordinal"] and (o["settlement"] != "physical_delivery" or o["settlement_ordinal"] == o["expiry_ordinal"]), "invalid European dates")
    values, states, ids, descriptor = [], [], set(), None
    for s in o["states"]:
        _keys(s, ("state_id", "probability", "discount_to_expiry", "discount_source", "underlying", "delivered_position_id", "netting_set_id")); _identity(s["discount_source"])
        u = s["underlying"]
        _require(s["state_id"] not in ids and math.isfinite(s["probability"]) and s["probability"] >= 0. and math.isfinite(s["discount_to_expiry"]) and s["discount_to_expiry"] > 0. and (o["asof_ordinal"] != o["expiry_ordinal"] or s["discount_to_expiry"] == 1.), "invalid terminal state")
        ids.add(s["state_id"])
        key = _swap_key(u); _require(descriptor is None or descriptor == key, "option changes contractual economics"); descriptor = key
        _require(u["asof_ordinal"] == o["expiry_ordinal"] and u["scenario_id"] == o["scenario_id"] and u["currency"] == o["currency"] and u["fixed_periods"][0]["start_ordinal"] >= o["expiry_ordinal"] and u["tail_end_ordinal"] <= o["tail_end_ordinal"], "invalid option underlying anchor")
        v = _swap(u)[0]; intrinsic = max(v, 0.)
        cash = 0.
        if intrinsic > 0. and o["settlement"] == "collateralized_cash_price":
            dfs = _discount(u["discount_curve"], u["asof_ordinal"], u["currency"], u["valuation_cutoff_unix_seconds"])
            _require(o["settlement_ordinal"] in dfs, "missing settlement carry discount")
            cash = intrinsic/dfs[o["settlement_ordinal"]]
        states.append((intrinsic, _money(cash)))
        values.append(s["probability"]*s["discount_to_expiry"]*intrinsic)
    _same(float(math.fsum(s["probability"] for s in o["states"])), 1., "terminal probability")
    return (1. if o["side"] == "long" else -1.)*math.fsum(values), states


def _position(t):
    return (t["terminal"] if t["kind"] == "expired_european" else t["request"])["position_id"]


def _owner(t):
    return t["delivered_underlying"]["position_id"] if t["kind"] == "expired_european" and t["delivered_underlying"] else _position(t)


def _sources(t):
    if t["kind"] == "swap":
        s = t["request"]; return [s[k] for k in ("contract", "index_basis", "rate_path")]+[s["discount_curve"]["source"]]
    o = t["terminal"] if t["kind"] == "expired_european" else t["request"]
    if t["kind"] != "expired_european": return [o[k] for k in ("contract", "exercise_policy", "conditional_model", "crn")]
    ids = [o[k] for k in ("contract", "exercise_policy", "conditional_model")]+[t["election"]]
    for s in o["states"]: ids += [s["discount_source"], s["underlying"]["discount_curve"]["source"]]
    if t["delivered_underlying"]: ids += _sources({"kind":"swap", "request":t["delivered_underlying"]})
    return ids


def _trade_key(t):
    if t["kind"] == "swap": return _swap_key(t["request"])
    o = t["terminal"] if t["kind"] == "expired_european" else t["request"]
    return [o[k] for k in ("contract", "exercise_policy", "currency", "expiry_ordinal", "settlement_ordinal", "tail_end_ordinal", "side", "settlement")]+[_swap_key(o["states"][0]["underlying"])]


def _trade(t, node, c, bucket):
    kind = t["kind"]
    _require(kind in {"swap", "european_swaption", "expired_european"}, "unsupported trade kind")
    if kind == "swap":
        _keys(t, ("kind", "request")); s=t["request"]
        _require(s["asof_ordinal"] == node["ordinal"] and s["valuation_cutoff_unix_seconds"] == node["valuation_cutoff_unix_seconds"] and s["payment_timing"] == node["payment_timing"] and s["currency"] == c["policy"]["currency"] and s["scenario_id"] == node["_path_id"], "swap node source mismatch")
        return _swap(s)[0]
    o=t["terminal"] if kind == "expired_european" else t["request"]
    _require(o["currency"] == c["policy"]["currency"] and o["scenario_id"] == node["_path_id"] and all(s["netting_set_id"] == bucket for s in o["states"]), "option legal/path mismatch")
    v, states = _option(o)
    if kind != "expired_european":
        _keys(t, ("kind", "request")); _require(o["asof_ordinal"] == node["ordinal"] < o["expiry_ordinal"], "live option expiry mismatch"); return v
    _keys(t, ("kind", "terminal", "election", "delivered_underlying", "settlement_discount")); _identity(t["election"])
    _require(o["asof_ordinal"] == o["expiry_ordinal"] <= node["ordinal"] and len(o["states"]) == 1 and o["states"][0]["probability"] == 1. and o["states"][0]["discount_to_expiry"] == 1., "invalid selected expiry state")
    if o["settlement"] == "collateralized_cash_price":
        _require(t["delivered_underlying"] is None, "cash option delivered a swap")
        if not _active(o["settlement_ordinal"], node["ordinal"], node["payment_timing"]): return 0.
        dfs = _discount(t["settlement_discount"], node["ordinal"], o["currency"], node["valuation_cutoff_unix_seconds"])
        return (1. if o["side"] == "long" else -1.)*states[0][1]*dfs[o["settlement_ordinal"]]
    _require(t["settlement_discount"] is None, "physical option has cash carry curve")
    if states[0][0] == 0.:
        _require(t["delivered_underlying"] is None, "unexercised delivered swap"); return 0.
    u=t["delivered_underlying"]; _require(u is not None, "missing physical delivery")
    expected=deepcopy(o["states"][0]["underlying"])
    if o["side"] == "short": expected["side"]="receiver" if expected["side"] == "payer" else "payer"
    _require(_swap_key(u) == _swap_key(expected) and u["position_id"] == o["states"][0]["delivered_position_id"], "changed physical delivery economics/owner")
    return _trade({"kind":"swap", "request":u}, node, c, bucket)


def _due(t, node, measure, paths):
    """All contractual due rights, derived directly from raw schedules."""
    mapped = _position(t)
    if t["kind"] == "european_swaption": return []
    if t["kind"] == "expired_european":
        o=t["terminal"]
        if o["settlement"] == "collateralized_cash_price" and o["settlement_ordinal"] == node["ordinal"]:
            _, states=_option(o)
            return [{"event_id":"option_cash:"+o["states"][0]["state_id"], "amount":(1. if o["side"] == "long" else -1.)*states[0][1], "sources":[], "physical_path":None}]
        if t["delivered_underlying"] is None: return []
        s=t["delivered_underlying"]
    else: s=t["request"]
    ps={period: p for (pos, period), p in paths.items() if pos == mapped} if measure == "physical" else None
    # Cash arithmetic does not require a post-payment curve point already removed
    # from an After snapshot; calculate on the explicit due-date raw contract.
    r=deepcopy(s); r["payment_timing"]="after_scheduled_payments"
    return _swap(r, node["ordinal"], ps, measure)[1]


def _path_map(paths):
    _require(len(paths) <= 4096, "too many fixing paths")
    out = {}
    for p in paths:
        _keys(p, ("position_id", "period_id", "path", "observations")); _identity(p["path"])
        key=(p["position_id"],p["period_id"])
        _require(key not in out, "duplicate Physical fixing path"); out[key]=p
    return out


def _native(raw):
    """Independent collateral state recurrence on raw clean trade values."""
    _keys(raw, ("schema", "model_version", "operation"))
    _require(raw["schema"] == "derivative-contract-1" and raw["model_version"] in {"derivative-contracts-1", "derivative-contracts-2"}, "unsupported derivative version")
    op=raw["operation"]; _keys(op, ("kind","request")); _require(op["kind"] == "netting", "full raw netting lifecycle required")
    c=op["request"]; _keys(c, ("asof_ordinal","measure","conditional_model","crn","policy","mapping","paths"))
    p=c["policy"]; _keys(p, ("identity","entity","counterparty","currency","monetary_unit","receive_threshold","post_threshold","minimum_transfer","settlement_lag_calendar_days","opening_vm","opening_received_im","opening_posted_im","opening_source","received_vm_reusable_for_funding","received_im_segregated","collateral_interest_day_count","tail_return_ordinal"))
    for i in (c["conditional_model"],c["crn"],p["identity"],p["opening_source"]): _identity(i)
    _require(c["measure"] in {"pricing","physical"} and p["monetary_unit"] == "currency_units" and p["received_im_segregated"] is True and type(p["received_vm_reusable_for_funding"]) is bool and type(p["settlement_lag_calendar_days"]) is int and 0 <= p["settlement_lag_calendar_days"] <= 366, "unsupported CSA policy")
    _require(isinstance(p["currency"],str) and len(p["currency"])==3 and p["currency"].isascii() and p["currency"].isupper(), "invalid actual currency")
    _day(c["asof_ordinal"]); _day(p["tail_return_ordinal"])
    _require(0 <= p["tail_return_ordinal"]-c["asof_ordinal"] <= 36600 and 0 < len(c["paths"]) <= 256 and 0 < len(c["mapping"]) <= 256, "CSA bound exceeded")
    for k in ("receive_threshold","post_threshold","minimum_transfer","opening_received_im","opening_posted_im"): _require(_money(p[k]) >= 0., "negative CSA amount")
    mappings, legal = {}, {}
    for m in c["mapping"]:
        _keys(m, ("position_id","contract","entity","counterparty","currency","netting_set_id","legal_review","enforceable_netting")); _identity(m["contract"]); _identity(m["legal_review"])
        _require(m["position_id"] not in mappings and all(m[k] == p[k] for k in ("entity","counterparty","currency")) and type(m["enforceable_netting"]) is bool, "mapping identity/unit mismatch")
        bucket=m["netting_set_id"] if m["enforceable_netting"] else "trade:"+m["position_id"]
        descriptor=[m["legal_review"],m["enforceable_netting"]]
        _require(bucket not in legal or legal[bucket] == descriptor, "inconsistent legal bucket")
        legal[bucket]=descriptor; mappings[m["position_id"]]=(m,bucket)
    buckets=sorted(legal)
    _require(len(buckets) == 1 or all(p[k] == 0. for k in ("opening_vm","opening_received_im","opening_posted_im")), "ambiguous opening collateral allocation")
    results=[]; clean=None; probabilities=[]
    for path in c["paths"]:
        _keys(path, ("path_id","source","probability","nodes","default")); _identity(path["source"])
        _require(path["path_id"] not in [q["path_id"] for q in results] and math.isfinite(path["probability"]) and path["probability"] >= 0. and 0 < len(path["nodes"]) <= 4096, "invalid path probability/identity/bound")
        nodes=path["nodes"]; _require(nodes[0]["ordinal"] == c["asof_ordinal"] and nodes[-1]["ordinal"] == p["tail_return_ordinal"] and all(a["ordinal"] < b["ordinal"] for a,b in zip(nodes,nodes[1:])), "incomplete chronological path")
        d=path["default"]; mp=d.get("mpor") if d else None
        if d:
            _keys(d, ("default_ordinal","closeout_ordinal","legal_review","replacement_value_policy","counterparty_recovery","posted_vm_return_recovery","posted_im_return_recovery","posted_collateral_setoff","received_im_available_on_default"), ("mpor",))
            _identity(d["legal_review"]); _identity(d["replacement_value_policy"])
            _require(c["asof_ordinal"] <= d["default_ordinal"] <= d["closeout_ordinal"] <= p["tail_return_ordinal"] and all(x in [n["ordinal"] for n in nodes] for x in (d["default_ordinal"],d["closeout_ordinal"])), "incomplete default tail")
            for k in ("counterparty_recovery","posted_vm_return_recovery","posted_im_return_recovery"): _require(0 <= _money(d[k]) <= 1., "invalid recovery fraction")
            _require(type(d["posted_collateral_setoff"]) is bool and type(d["received_im_available_on_default"]) is bool,"invalid reviewed closeout flags")
        if mp:
            _require(raw["model_version"] == "derivative-contracts-2", "MPOR requires explicit rebuild/version selection")
            _keys(mp, ("identity","claim_measure","carry_policy","exercise_rights_policy","settlement_instructions","physical_index_paths")); _identity(mp["identity"])
            _require(mp["claim_measure"] == c["measure"] and mp["carry_policy"] == "nominal_no_interest" and mp["exercise_rights_policy"] == "preserve_contractual_european_to_closeout", "unsupported MPOR policy")
        physical=_path_map(mp["physical_index_paths"] if mp else []); used_paths=set(); instructions={}
        for i in mp["settlement_instructions"] if mp else []:
            _keys(i, ("position_id","event_id","payment_ordinal","disposition")); key=(i["position_id"],i["event_id"],i["payment_ordinal"])
            _require(key not in instructions and d["default_ordinal"] <= i["payment_ordinal"] <= d["closeout_ordinal"] and i["payment_ordinal"] in [n["ordinal"] for n in nodes], "duplicate/invalid MPOR instruction")
            disp=i["disposition"]
            if disp["kind"] == "retain_to_closeout": _keys(disp,("kind",))
            else:
                _keys(disp,("kind","ordinal","evidence")); _identity(disp["evidence"])
                _require(disp["kind"] == "settle_on_node" and i["payment_ordinal"] <= disp["ordinal"] <= d["closeout_ordinal"] and any(n["ordinal"] == disp["ordinal"] and n["settlement_available"] for n in nodes), "unsupported/unavailable modeled settlement")
            instructions[key]=i
        vm={b:float(p["opening_vm"]) if len(buckets)==1 else 0. for b in buckets}
        rim={b:float(p["opening_received_im"]) if len(buckets)==1 else 0. for b in buckets}
        pim={b:float(p["opening_posted_im"]) if len(buckets)==1 else 0. for b in buckets}
        calls=[]; movements=[]; exposures=[]; closeouts=[]; obligations=[]; exercises=[]; used=set(); descriptors={}; elections={}
        credit_loss=0.; funding_cost=0.; default_seen=False; closed=False; previous=None
        def movement(n,b,kind,cash=0.,vd=0.,rd=0.,pd=0.,custodian=0.):
            movements.append(dict(ordinal=n["ordinal"],bucket=b,kind=kind,entity_cash=_money(cash),vm_delta=_money(vd),received_im_delta=_money(rd),posted_im_delta=_money(pd),custodian_cash=_money(custodian),modeled_measure=c["measure"]))
        for original in nodes:
            _keys(original,("ordinal","valuation_cutoff_unix_seconds","payment_timing","trades","discount_from_asof","discount_source","collateral_rate","collateral_rate_source","funding_spread","funding_source","settlement_available"))
            n=dict(original,_path_id=path["path_id"]); day=n["ordinal"]; _day(day)
            _require(type(n["settlement_available"]) is bool,"invalid settlement availability")
            _finite(n["collateral_rate"]); _finite(n["funding_spread"])
            _require(_utc_day(n["valuation_cutoff_unix_seconds"]) == day and math.isfinite(n["discount_from_asof"]) and n["discount_from_asof"] > 0. and (day != c["asof_ordinal"] or n["discount_from_asof"] == 1.), "node discount/cutoff mismatch")
            for k in ("discount_source","collateral_rate_source","funding_source"): _identity(n[k])
            if previous:
                tau=_tau(previous["ordinal"],day,p["collateral_interest_day_count"])
                for b in buckets:
                    movement(n,b,"collateral_interest",(-vm[b]+pim[b])*previous["collateral_rate"]*tau,custodian=-rim[b]*previous["collateral_rate"]*tau)
                    funded=max(-vm[b],0.)+pim[b]-(max(vm[b],0.) if p["received_vm_reusable_for_funding"] else 0.)
                    funding_cost+=funded*previous["funding_spread"]*tau*n["discount_from_asof"]
            at_default=bool(d and day==d["default_ordinal"]); at_closeout=bool(d and day==d["closeout_ordinal"])
            if at_default:
                default_seen=True
                for call in calls:
                    if call["settled_ordinal"] is None: call["cancelled_at_default"]=True
            if not default_seen and n["settlement_available"]:
                for call in calls:
                    if call["settled_ordinal"] is None and not call["cancelled_at_default"] and not call["cancelled_at_tail"] and call["due_ordinal"] <= day:
                        call["settled_ordinal"]=day; vm[call["bucket"]]+=call["amount"]; movement(n,call["bucket"],"variation_margin",call["amount"],vd=call["amount"])
            values={b:[] for b in buckets}; embedded=defaultdict(float); seen=set()
            _require(len(n["trades"]) == len(mappings), "incomplete daily raw trade mapping")
            for t in n["trades"]:
                pos=_position(t); _require(pos in mappings and pos not in seen, "duplicate/unmapped trade"); seen.add(pos)
                m,b=mappings[pos]; contract=(t["terminal"] if t["kind"] == "expired_european" else t["request"])["contract"]
                _require(_tail(t)<=p["tail_return_ordinal"],"incomplete contractual tail")
                if d and not mp: _require(not any(d["default_ordinal"]<x<d["closeout_ordinal"] for x in _dates(t)),"interim rights require MPOR")
                if mp: _require(all(x in [q["ordinal"] for q in nodes] for x in _dates(t) if d["default_ordinal"]<=x<=d["closeout_ordinal"]),"missing MPOR event date")
                _require(contract == m["contract"], "raw contract/legal source differs")
                descriptor=_trade_key(t); _require(pos not in descriptors or descriptors[pos] == descriptor, "daily contract economics changed"); descriptors[pos]=descriptor
                if t["kind"] == "expired_european":
                    key=[t["terminal"],t["election"]]; _require(pos not in elections or elections[pos] == key, "expiry election source changed")
                    if pos not in elections and c["asof_ordinal"] < t["terminal"]["expiry_ordinal"]: _require(day == t["terminal"]["expiry_ordinal"], "missing exercise handoff node")
                    elections[pos]=key
                    if day == t["terminal"]["expiry_ordinal"]: _require(all(s["underlying"]["valuation_cutoff_unix_seconds"] <= n["valuation_cutoff_unix_seconds"] for s in t["terminal"]["states"]), "expiry source lookahead")
                value=_trade(t,n,c,b)
                eligible=bool(mp and default_seen and not closed and day <= d["closeout_ordinal"] and (day > d["default_ordinal"] or n["payment_timing"] == "before_scheduled_payments"))
                if eligible:
                    if t["kind"] == "expired_european" and t["terminal"]["expiry_ordinal"] == day:
                        o=t["terminal"]; state=o["states"][0]; intrinsic=_option(o)[1][0][0]
                        exercises.append(dict(position_id=pos,expiry_ordinal=day,contract=o["contract"],election=t["election"],state_id=state["state_id"],exercised=intrinsic>0.,settlement=o["settlement"],delivered_position_id=state["delivered_position_id"] if intrinsic>0. and o["settlement"]=="physical_delivery" else None,netting_set_id=state["netting_set_id"],underlying_contract=state["underlying"]["contract"],pricing_intrinsic_value=float(intrinsic),claim_measure=c["measure"],conditional_model=o["conditional_model"],collateral_discount_source=state["underlying"]["discount_curve"]["source"]))
                    pricing=_due(t,n,"pricing",{}); actual=_due(t,n,c["measure"],physical)
                    if n["payment_timing"] == "before_scheduled_payments": embedded[b]+=math.fsum(x["amount"] for x in pricing)
                    for due in actual:
                        key=(pos,due["event_id"],day); _require(key in instructions and key not in used, "missing/duplicate due instruction"); used.add(key)
                        if due["physical_path"]: used_paths.add((pos,due["event_id"].split(":",1)[1]))
                        cash_option=t["kind"]=="expired_european" and t["terminal"]["settlement"]=="collateralized_cash_price"
                        o=t.get("terminal")
                        obligations.append(dict(position_id=pos,event_id=due["event_id"],bucket=b,payment_ordinal=day,captured_ordinal=day,currency=p["currency"],amount=float(due["amount"]),measure=c["measure"],contract=contract,selected_rate_sources=due["sources"],origin_election=t.get("election"),delivered_position_id=_owner(t) if _owner(t)!=pos else None,settlement_price_sources=[o["exercise_policy"],o["conditional_model"],o["states"][0]["underlying"]["discount_curve"]["source"]] if cash_option else [],disposition=deepcopy(instructions[key]["disposition"]),settled_ordinal=None,included_in_closeout=False))
                values[b].append([pos,0. if closed else float(value)])
            if default_seen and not closed and mp:
                for obligation in obligations:
                    disp=obligation["disposition"]
                    if obligation["settled_ordinal"] is None and not obligation["included_in_closeout"] and disp["kind"]=="settle_on_node" and disp["ordinal"]==day:
                        obligation["settled_ordinal"]=day; movement(n,obligation["bucket"],"mpor_contractual_settlement",obligation["amount"])
            node_clean=math.fsum(v for ts in values.values() for _,v in ts)
            if day==c["asof_ordinal"]:
                if clean is not None: _same(float(node_clean),float(clean),"shared opening clean")
                clean=float(node_clean)
            for b in buckets:
                v=math.fsum(v for _,v in values[b]); replacement=v-embedded[b]
                unpaid=math.fsum(o["amount"] for o in obligations if o["bucket"]==b and o["settled_ordinal"] is None and not o["included_in_closeout"]) if not closed else 0.
                claim=replacement+unpaid
                exposures.append(dict(ordinal=day,bucket=b,clean_value=float(v),vm=float(vm[b]),received_im=float(rim[b]),posted_im=float(pim[b]),unsecured_receivable=max(claim-max(vm[b],0.),0.),posted_collateral_return_claim=max(-vm[b],0.)+pim[b],clean_trade_values=values[b],extinguished_after_closeout=closed,replacement_after_due_obligations=float(replacement),unpaid_obligation_claim=float(unpaid),total_closeout_claim=float(claim)))
                if at_closeout:
                    close=_closeout(claim,vm[b],rim[b],pim[b],d,day,b); close["clean_replacement_value"]=float(replacement); close["unpaid_obligation_claim"]=float(unpaid)
                    credit_loss+=(close["counterparty_loss"]+close["posted_return_loss"])*n["discount_from_asof"]
                    movement(n,b,"default_closeout",close["entity_cash"],-vm[b],-rim[b],-pim[b],close["custodian_return"])
                    closeouts.append(close); vm[b]=rim[b]=pim[b]=0.
                    for obligation in obligations:
                        if obligation["bucket"]==b and obligation["settled_ordinal"] is None: obligation["included_in_closeout"]=True
                elif day==p["tail_return_ordinal"] and not default_seen:
                    _require(n["settlement_available"],"unavailable full-tail return")
                    for call in calls:
                        if call["bucket"]==b and call["settled_ordinal"] is None: call["cancelled_at_tail"]=True
                    movement(n,b,"tail_return",-vm[b]+pim[b],-vm[b],-rim[b],-pim[b],-rim[b]); vm[b]=rim[b]=pim[b]=0.
                elif not default_seen:
                    target=v-p["receive_threshold"] if v>p["receive_threshold"] else v+p["post_threshold"] if v < -p["post_threshold"] else 0.
                    pending=math.fsum(x["amount"] for x in calls if x["bucket"]==b and x["settled_ordinal"] is None and not x["cancelled_at_default"] and not x["cancelled_at_tail"])
                    delta=target-vm[b]-pending
                    if delta!=0. and abs(delta)>=p["minimum_transfer"]:
                        due=day+p["settlement_lag_calendar_days"]; _require(due<=p["tail_return_ordinal"],"margin tail incomplete")
                        immediate=p["settlement_lag_calendar_days"]==0 and n["settlement_available"]
                        calls.append(dict(call_ordinal=day,due_ordinal=due,settled_ordinal=day if immediate else None,cancelled_at_default=False,cancelled_at_tail=False,bucket=b,amount=float(delta)))
                        if immediate: vm[b]+=delta; movement(n,b,"variation_margin",delta,vd=delta)
            if at_closeout: closed=True
            previous=n
        _require(used==set(instructions) and used_paths==set(physical), "unmatched MPOR source/instruction")
        results.append(dict(mpor_policy=mp["identity"] if mp else None,closing_legal_review=d["legal_review"] if d else None,replacement_value_policy=d["replacement_value_policy"] if d else None,path_id=path["path_id"],probability=float(path["probability"]),source=path["source"],exposures=exposures,calls=calls,movements=movements,closeouts=closeouts,discounted_credit_loss=float(credit_loss),discounted_funding_cost=float(funding_cost),residual_vm=float(math.fsum(vm.values())),residual_received_im=float(math.fsum(rim.values())),residual_posted_im=float(math.fsum(pim.values())),pending_calls=float(math.fsum(x["amount"] for x in calls if x["settled_ordinal"] is None and not x["cancelled_at_default"] and not x["cancelled_at_tail"])),obligations=obligations,mpor_exercises=exercises))
        probabilities.append(path["probability"])
    _same(float(math.fsum(probabilities)),1.,"path probability")
    pricing=c["measure"]=="pricing"; cva=math.fsum(x["probability"]*x["discounted_credit_loss"] for x in results); fva=math.fsum(x["probability"]*x["discounted_funding_cost"] for x in results)
    return dict(policy=p["identity"],legal_mapping=c["mapping"],conditional_model=c["conditional_model"],crn=c["crn"],clean_value=clean,cva=cva if pricing else None,fva=fva if pricing else None,clean_less_cva_fva=clean-cva-fva if pricing else None,paths=results)


def _closeout(claim, vm, received_im, posted_im, policy, day, bucket):
    received=max(vm,0.); posted=max(-vm,0.); vm_applied=im_applied=0.
    if claim >= 0.:
        vm_applied=min(received,claim); im_applied=min(received_im,claim-vm_applied) if policy["received_im_available_on_default"] else 0.
        unsecured=max(claim-vm_applied-im_applied,0.); recovery=unsecured*policy["counterparty_recovery"]; loss=unsecured-recovery
        return_loss=posted*(1-policy["posted_vm_return_recovery"])+posted_im*(1-policy["posted_im_return_recovery"])
        cash=im_applied+recovery+posted*policy["posted_vm_return_recovery"]+posted_im*policy["posted_im_return_recovery"]-(received-vm_applied)
        custodian=-(received_im-im_applied)
    else:
        used_vm=min(posted,-claim) if policy["posted_collateral_setoff"] else 0.
        used_im=min(posted_im,-claim-used_vm) if policy["posted_collateral_setoff"] else 0.
        unused_vm,unused_im=posted-used_vm,posted_im-used_im
        unsecured=recovery=loss=0.; return_loss=unused_vm*(1-policy["posted_vm_return_recovery"])+unused_im*(1-policy["posted_im_return_recovery"])
        cash=unused_vm*policy["posted_vm_return_recovery"]+unused_im*policy["posted_im_return_recovery"]-(-claim-used_vm-used_im)-received; custodian=-received_im
    return dict(ordinal=day,bucket=bucket,clean_replacement_value=float(claim),unpaid_obligation_claim=0.,total_closeout_claim=float(claim),vm_applied=float(vm_applied),received_im_applied=float(im_applied),unsecured_claim=float(unsecured),unsecured_recovery=float(recovery),counterparty_loss=float(loss),posted_return_loss=float(return_loss),entity_cash=float(cash),custodian_return=float(custodian))


_ACCOUNTS=("cash","derivative_asset","derivative_liability","unpaid_receivable","unpaid_payable","posted_vm_asset","received_vm_liability","posted_im_asset","opening_equity","mark_gain_loss","contractual_income","collateral_interest_income","counterparty_loss_expense","posted_collateral_return_loss_expense","memo_custodian_asset","memo_custodian_obligation")


def _posting(account, value, owner=None, bucket=None):
    return dict(account=account,position_id=owner,bucket=bucket,debit_amount=float(value))


def _book(old, new, owner, bucket):
    return [_posting("derivative_asset",max(new,0.)-max(old,0.),owner,bucket),_posting("derivative_liability",min(new,0.)-min(old,0.),owner,bucket)]


class _JournalReference:
    """Raw-derived rights -> reviewed economic entries -> independently summed GL."""
    def __init__(self, r, c, native):
        self.r=r; self.cash=float(r["opening"]["cash"]); self.intents=[]; self.daily=[]; self.unpaid={}; self.trades={}
        o=r["opening"]; path=native["paths"][0]; buckets=sorted({e["bucket"] for e in path["exposures"]})
        marks={pos:v for e in path["exposures"] if e["ordinal"] == o["ordinal"] for pos,v in e["clean_trade_values"]}
        mapping={m["position_id"]:m for m in c["mapping"]}; opening_trades={_position(t):t for t in c["paths"][0]["nodes"][0]["trades"]}
        for anchor in o["trades"]:
            pos=anchor["position_id"]; _same(_money(anchor["clean_value"]),float(marks[pos]),"opening source mark")
            m=mapping[pos]; b=m["netting_set_id"] if m["enforceable_netting"] else "trade:"+pos
            self.trades[pos]=dict(owner=_owner(opening_trades[pos]),bucket=b,value=float(anchor["clean_value"]))
        self.vm={b:float(c["policy"]["opening_vm"]) if len(buckets)==1 else 0. for b in buckets}
        self.pim={b:float(c["policy"]["opening_posted_im"]) if len(buckets)==1 else 0. for b in buckets}
        self.rim={b:float(c["policy"]["opening_received_im"]) if len(buckets)==1 else 0. for b in buckets}
        self.gl=dict(cash=self.cash,posted_vm_asset=float(o["posted_vm_asset"]),received_vm_liability=-float(o["received_vm_liability"]),posted_im_asset=float(o["posted_im_asset"]),opening_equity=-float(o["equity_credit_balance"]),derivative_asset=math.fsum(max(t["value"],0.) for t in self.trades.values()),derivative_liability=math.fsum(min(t["value"],0.) for t in self.trades.values()))
        self.memo=dict(memo_custodian_asset=float(o["received_im_custody"]),memo_custodian_obligation=-float(o["received_im_custody"]))
        _same(float(math.fsum(self.gl.values())),0.,"opening balanced anchors")
        for actual,expected in [(o["posted_vm_asset"],max(-c["policy"]["opening_vm"],0.)),(o["received_vm_liability"],max(c["policy"]["opening_vm"],0.)),(o["posted_im_asset"],c["policy"]["opening_posted_im"]),(o["received_im_custody"],c["policy"]["opening_received_im"])]: _same(_money(actual),float(expected),"opening collateral source")

    def emit(self, kind, day, lines, sources, mapped=None, owner=None, bucket=None, ledger="financial"):
        lines=[p for p in lines if p["debit_amount"]!=0.]
        if not lines: return
        _require(len(self.intents)<250000,"journal output bound exceeded")
        _same(float(math.fsum(p["debit_amount"] for p in lines)),0.,"reference balanced intent")
        gl=self.gl if ledger=="financial" else self.memo
        for line in lines: gl[line["account"]]=_money(gl.get(line["account"],0.)+line["debit_amount"])
        r=self.r
        self.intents.append(dict(event_id=f"{kind}:{len(self.intents)}",ordinal=day,ledger=ledger,run=r["run"],scenario_id=r["scenario_id"],entity=r["entity"],currency=r["currency"],policy=r["policy"]["identity"],sources=deepcopy(sources),mapped_position_id=mapped,contract_position_id=owner,bucket=bucket,measure="physical",basis=r["basis"],postings=lines))

    def mark(self, pos, target, day, sources):
        t=self.trades[pos]; lines=_book(t["value"],target,t["owner"],t["bucket"])
        lines.append(_posting("mark_gain_loss",t["value"]-target,t["owner"],t["bucket"]))
        self.emit("clean_mark",day,lines,sources,pos,t["owner"],t["bucket"]); t["value"]=_money(target)

    def custody(self, day, bucket, delta, sources):
        self.rim[bucket]=_money(self.rim[bucket]+delta)
        self.emit("custodian_principal",day,[_posting("memo_custodian_asset",delta,bucket=bucket),_posting("memo_custodian_obligation",-delta,bucket=bucket)],sources,bucket=bucket,ledger="custody_memo")

    def collateral(self, movement, sources):
        m=movement; b=m["bucket"]; old=self.vm[b]; new=old+m["vm_delta"]
        lines=[_posting("cash",m["entity_cash"],bucket=b),_posting("posted_vm_asset",max(-new,0.)-max(-old,0.),bucket=b),_posting("received_vm_liability",-max(new,0.)+max(old,0.),bucket=b),_posting("posted_im_asset",m["posted_im_delta"],bucket=b)]
        if m["kind"]=="collateral_interest": lines.append(_posting("collateral_interest_income",-m["entity_cash"],bucket=b))
        self.emit(m["kind"],m["ordinal"],lines,sources,bucket=b)
        self.cash=_money(self.cash+m["entity_cash"]); self.vm[b]=new; self.pim[b]+=m["posted_im_delta"]
        self.custody(m["ordinal"],b,m["received_im_delta"],sources)

    def due(self, right, unpaid, review, sources):
        pos=right["position_id"]; t=self.trades[pos]; value=right["physical_amount"]; b=t["bucket"]; owner=t["owner"]
        key=(pos,right["event_id"],right["payment_ordinal"])
        if unpaid:
            _require(key not in self.unpaid,"duplicate recognized right"); self.unpaid[key]=value
            lines=[_posting("unpaid_receivable" if value>=0. else "unpaid_payable",value,owner,b)]
        else: self.cash=_money(self.cash+value); lines=[_posting("cash",value,owner,b)]
        if review["coupon_recognition"]=="settle_derivative_carrying":
            lines+=_book(t["value"],t["value"]-value,owner,b); t["value"]=_money(t["value"]-value)
        else: lines.append(_posting("contractual_income",-value,owner,b))
        self.emit("contractual_due_unpaid" if unpaid else "contractual_cash_settlement",right["payment_ordinal"],lines,sources,pos,owner,b)

    def settle(self, right, day, sources):
        pos=right["position_id"]; t=self.trades[pos]; key=(pos,right["event_id"],right["payment_ordinal"])
        value=self.unpaid.pop(key); _same(float(value),float(right["physical_amount"]),"immutable retained settlement")
        self.cash=_money(self.cash+value)
        self.emit("mpor_cash_settlement",day,[_posting("unpaid_receivable" if value>=0. else "unpaid_payable",-value,t["owner"],t["bucket"]),_posting("cash",value,t["owner"],t["bucket"])],sources,pos,t["owner"],t["bucket"])

    def closeout(self, close, day, sources):
        b=close["bucket"]
        lines=[_posting("cash",close["entity_cash"],bucket=b),_posting("counterparty_loss_expense",close["counterparty_loss"],bucket=b),_posting("posted_collateral_return_loss_expense",close["posted_return_loss"],bucket=b)]
        claims=[]
        for pos in sorted(self.trades):
            t=self.trades[pos]
            if t["bucket"]==b:
                claims.append(t["value"]); lines+=_book(t["value"],0.,t["owner"],b); t["value"]=0.
        for key in sorted(list(self.unpaid)):
            t=self.trades[key[0]]
            if t["bucket"]==b:
                value=self.unpaid.pop(key); claims.append(value)
                lines.append(_posting("unpaid_receivable" if value>=0. else "unpaid_payable",-value,t["owner"],b))
        _same(float(math.fsum(claims)),float(close["total_closeout_claim"]),"closeout conserved rights")
        lines += [_posting("posted_vm_asset",-max(-self.vm[b],0.),bucket=b),_posting("received_vm_liability",max(self.vm[b],0.),bucket=b),_posting("posted_im_asset",-self.pim[b],bucket=b)]
        self.emit("reviewed_default_closeout",day,lines,sources,bucket=b)
        self.cash=_money(self.cash+close["entity_cash"]); self.vm[b]=self.pim[b]=0.; self.custody(day,b,-self.rim[b],sources)

    def closing(self, day):
        amounts=dict(cash=self.cash,derivative_asset=math.fsum(max(t["value"],0.) for t in self.trades.values()),derivative_liability=math.fsum(min(t["value"],0.) for t in self.trades.values()),unpaid_receivable=math.fsum(max(v,0.) for v in self.unpaid.values()),unpaid_payable=math.fsum(min(v,0.) for v in self.unpaid.values()),posted_vm_asset=math.fsum(max(-v,0.) for v in self.vm.values()),received_vm_liability=-math.fsum(max(v,0.) for v in self.vm.values()),posted_im_asset=math.fsum(self.pim.values()))
        for account,value in amounts.items(): _same(float(self.gl.get(account,0.)),float(value),"independent closing rights/GL:"+account)
        _same(float(math.fsum(self.gl.values())),0.,"closing financial trial balance")
        _same(float(math.fsum(self.memo.values())),0.,"closing custody trial balance")
        self.daily.append(dict(ordinal=day,cash=self.cash,trades=[dict(mapped_position_id=pos,contract_position_id=self.trades[pos]["owner"],clean_carrying=float(self.trades[pos]["value"])) for pos in sorted(self.trades)],unpaid_receivable=float(amounts["unpaid_receivable"]),unpaid_payable=float(-amounts["unpaid_payable"]),posted_vm_asset=float(amounts["posted_vm_asset"]),received_vm_liability=float(-amounts["received_vm_liability"]),posted_im_asset=float(amounts["posted_im_asset"]),received_im_custody=float(math.fsum(self.rim.values())),financial_trial_balance=[dict(account=a,debit_balance=float(self.gl[a])) for a in _ACCOUNTS if a in self.gl],custody_trial_balance=[dict(account=a,debit_balance=float(self.memo[a])) for a in _ACCOUNTS if a in self.memo]))


def _dates(t):
    if t["kind"]=="swap":
        s=t["request"]; return [p["payment_ordinal"] for leg in ("fixed_periods","floating_periods") for p in s[leg]]
    o=t["terminal"] if t["kind"]=="expired_european" else t["request"]
    dates=[o["expiry_ordinal"],o["settlement_ordinal"]]
    if t["kind"]=="expired_european" and t["delivered_underlying"]:
        dates += _dates({"kind":"swap","request":t["delivered_underlying"]})
    return dates


def _tail(t):
    if t["kind"]=="swap": return max(_dates(t))
    return (t["terminal"] if t["kind"]=="expired_european" else t["request"])["tail_end_ordinal"]


def _work(t):
    def swap_work(s): return len(s["fixed_periods"])+len(s["floating_periods"])+sum(len(p["coupon"].get("observations",[])) for p in s["floating_periods"])
    if t["kind"]=="swap": return swap_work(t["request"])
    o=t["terminal"] if t["kind"]=="expired_european" else t["request"]
    return sum(swap_work(s["underlying"]) for s in o["states"])+(swap_work(t["delivered_underlying"]) if t["kind"]=="expired_european" and t["delivered_underlying"] else 0)


def _admit(r):
    _keys(r,("schema","model_version","run","scenario_id","entity","currency","monetary_unit","measure","basis","policy","opening","derivative","physical_index_paths","pricing_xva_reference"))
    _require(r["schema"]==r["model_version"]=="derivative-accounting-1" and r["measure"]=="physical" and r["basis"]=="modeled_physical_scenario" and r["monetary_unit"]=="currency_units", "unsupported accounting schema/measure/unit")
    _identity(r["run"]); p=r["policy"]; _identity(p["identity"])
    _keys(p,("identity","trades","custody","default","xva","modeled_funding_cost"))
    _require(p["custody"]=="segregated_custodian_memo_only" and p["default"]=="reviewed_modeled_closeout_shortfall_in_earnings" and p["xva"]==p["modeled_funding_cost"]=="report_only", "unsupported accounting recognition policy")
    reviews={}
    for review in p["trades"]:
        _keys(review,("position_id","designation","designation_review","mark_recognition","coupon_recognition","delivery_review")); _identity(review["designation_review"])
        if review["delivery_review"]: _identity(review["delivery_review"])
        _require(review["position_id"] not in reviews and review["designation"] in {"undesignated_trading","undesignated_economic_hedge"} and review["mark_recognition"]=="clean_value_changes_in_earnings" and review["coupon_recognition"] in {"settle_derivative_carrying","contractual_income_at_due_with_mark_offset"}, "unsupported/duplicate designation policy")
        reviews[review["position_id"]]=review
    o=r["opening"]; _keys(o,("source","ordinal","cash","trades","posted_vm_asset","received_vm_liability","posted_im_asset","received_im_custody","equity_credit_balance")); _identity(o["source"])
    c=r["derivative"]["operation"]["request"]
    _require(r["derivative"]["operation"]["kind"]=="netting" and c["measure"]=="physical" and len(c["paths"])==1 and c["paths"][0]["probability"]==1.,"one complete Physical path required")
    path=c["paths"][0]; nodes=path["nodes"]; _require(path["path_id"]==r["scenario_id"] and o["ordinal"]==c["asof_ordinal"] and all(c["policy"][k]==r[k] for k in ("entity","currency","monetary_unit")), "accounting/native source identity mismatch")
    _require(0 < len(nodes) <= 4096 and len(nodes)==c["policy"]["tail_return_ordinal"]-c["asof_ordinal"]+1 and all(n["ordinal"]==c["asof_ordinal"]+i for i,n in enumerate(nodes)),"full daily tail without mark interpolation required")
    ids=[m["position_id"] for m in c["mapping"]]
    _require(0 < len(ids) <= 256 and len(set(ids))==len(ids) and set(reviews)==set(ids) and len(o["trades"])==len(ids) and {a["position_id"] for a in o["trades"]}==set(ids),"incomplete exact opening/review coverage")
    for a in o["trades"]: _keys(a,("position_id","clean_value")); _money(a["clean_value"])
    for k in ("cash","equity_credit_balance"): _money(o[k])
    for k in ("posted_vm_asset","received_vm_liability","posted_im_asset","received_im_custody"): _require(_money(o[k])>=0.,"negative opening collateral")
    work=0
    for raw in [r["derivative"]]+([r["pricing_xva_reference"]] if r["pricing_xva_reference"] else []):
        source=raw["operation"]["request"]; _require(len(source["paths"])<=256 and len(source["mapping"])<=256,"reference bounds exceeded")
        for q in source["paths"]:
            _require(len(q["nodes"])<=4096,"reference node bound exceeded")
            default=q["default"]; mp=default.get("mpor") if default else None
            if mp: work+=sum(len(p["observations"]) for p in mp["physical_index_paths"])
            for n in q["nodes"]:
                _require(len(n["trades"])<=256,"trade count bound exceeded")
                for t in n["trades"]:
                    w=_work(t); work+=w
                    if mp and default["default_ordinal"]<=n["ordinal"]<=default["closeout_ordinal"]: work+=w
                    _require(work<=250000,"combined work exceeds admission before financial allocation")
                    work+=w*_dates(t).count(n["ordinal"])
                    _require(work<=250000,"due repricing work exceeds admission before financial allocation")
    work+=sum(len(p["observations"]) for p in r["physical_index_paths"])
    _require(work<=250000,"combined Physical work exceeds admission before financial allocation")
    _require(len(json.dumps(r,allow_nan=False,separators=(",",":")).encode())<=32*1024*1024,"immutable metadata bound exceeded")
    dates={n["ordinal"] for n in nodes}
    for n in nodes:
        for t in n["trades"]:
            _require(all(x in dates for x in _dates(t) if c["asof_ordinal"]<=x<=c["policy"]["tail_return_ordinal"]),"missing contractual node")
            if t["kind"]=="expired_european" and t["delivered_underlying"]: _require(reviews[_position(t)]["delivery_review"] is not None,"unreviewed physical delivery")
            default=path["default"]
            if default and not default.get("mpor"):
                _require(not any(default["default_ordinal"]<x<default["closeout_ordinal"] for x in _dates(t)),"interim rights require explicit MPOR policy")
    return c, path, reviews


def _derive(r):
    c,path,reviews=_admit(r)
    physical=_path_map(r["physical_index_paths"]); used_paths=set(); normal={}
    for n in path["nodes"]:
        d=path["default"]; after_default=bool(d and n["ordinal"]>=d["default_ordinal"])
        opening_paid=n["ordinal"]==c["asof_ordinal"] and n["payment_timing"]=="after_scheduled_payments"
        default_after=bool(d and n["ordinal"]==d["default_ordinal"] and n["ordinal"]>c["asof_ordinal"] and n["payment_timing"]=="after_scheduled_payments")
        if not ((not after_default and not opening_paid) or default_after): continue
        for t in n["trades"]:
            actual=_due(t,n,"physical",physical); pricing={x["event_id"]:x for x in _due(t,n,"pricing",{})}; pos=_position(t); derived=[]
            for due in actual:
                if due["physical_path"]: used_paths.add((pos,due["event_id"].split(":",1)[1]))
                if t["kind"]=="swap":
                    contract=t["request"]["contract"]; recognition=_sources(t)
                elif t["delivered_underlying"]:
                    u=t["delivered_underlying"]; contract=u["contract"]; recognition=_sources({"kind":"swap","request":u})+_sources(t)
                else: contract=t["terminal"]["contract"]; recognition=_sources(t)
                derived.append(dict(position_id=pos,contract_position_id=_owner(t),event_id=due["event_id"],payment_ordinal=n["ordinal"],physical_amount=float(due["amount"]),pricing_due_amount=float(pricing[due["event_id"]]["amount"]),contract=contract,recognition_sources=recognition,sources=due["sources"],physical_path=due["physical_path"],settled_ordinal=n["ordinal"],closeout_ordinal=None))
            normal[(n["ordinal"],pos)]=derived
    _require(used_paths==set(physical),"unused normal Physical fixing source")
    native=_native(r["derivative"])
    xva=None
    if r["pricing_xva_reference"]:
        source=r["pricing_xva_reference"]["operation"]["request"]
        _require(source["measure"]=="pricing" and source["asof_ordinal"]==c["asof_ordinal"] and all(source["policy"][k]==c["policy"][k] for k in ("currency","entity")) and source["mapping"]==c["mapping"],"incompatible diagnostic base/legal policy")
        xva=_native(r["pricing_xva_reference"]); _same(xva["clean_value"],native["clean_value"],"diagnostic clean source"); _require(xva["crn"]==native["crn"],"diagnostic CRN differs")
    journal=_JournalReference(r,c,native); source=native["paths"][0]; rights=[]; index={}; closed=False
    for n in path["nodes"]:
        day=n["ordinal"]; d=path["default"]; after_default=bool(d and day>=d["default_ordinal"])
        default_after=bool(d and day==d["default_ordinal"] and n["payment_timing"]=="after_scheduled_payments")
        opening_paid=day==c["asof_ordinal"] and n["payment_timing"]=="after_scheduled_payments"
        for movement in source["movements"]:
            if movement["ordinal"]==day and movement["kind"] not in {"default_closeout","mpor_contractual_settlement"}:
                ids=[c["policy"]["identity"],n["collateral_rate_source"] if movement["kind"]=="collateral_interest" else source["source"]]
                journal.collateral(movement,ids)
        marks={pos:value for e in source["exposures"] if e["ordinal"]==day for pos,value in e["clean_trade_values"]}
        for t in n["trades"]:
            pos=_position(t); review=reviews[pos]; raw_mark=marks[pos]; derived=normal.pop((day,pos),[])
            pricing={due["event_id"]:due for due in _due(t,n,"pricing",{})}
            for obligation in source["obligations"]:
                if obligation["position_id"]!=pos or obligation["captured_ordinal"]!=day: continue
                ids=_sources(t)+obligation["settlement_price_sources"]
                if obligation["origin_election"]: ids += [obligation["origin_election"]]
                if source["mpor_policy"]: ids += [source["mpor_policy"]]
                derived.append(dict(position_id=pos,contract_position_id=_owner(t),event_id=obligation["event_id"],payment_ordinal=day,physical_amount=float(obligation["amount"]),pricing_due_amount=float(pricing[obligation["event_id"]]["amount"]),contract=obligation["contract"],recognition_sources=ids,sources=obligation["selected_rate_sources"],physical_path=None,settled_ordinal=obligation["settled_ordinal"],closeout_ordinal=d["closeout_ordinal"] if obligation["included_in_closeout"] else None))
            due_pricing=math.fsum(right["pricing_due_amount"] for right in derived)
            pre=0. if closed else raw_mark+(due_pricing if n["payment_timing"]=="after_scheduled_payments" and not opening_paid else 0.)
            ids=_sources(t)+[review["designation_review"]]; journal.mark(pos,pre,day,ids)
            current=journal.trades[pos]
            if not closed and _owner(t)!=current["owner"]:
                _require(review["delivery_review"] is not None,"missing reviewed owner transfer")
                if t["terminal"]["expiry_ordinal"]==day:
                    intrinsic=_option(t["terminal"])[1][0][0]*(1. if t["terminal"]["side"]=="long" else -1.)
                    _same(float(pre),float(intrinsic),"exercise delivery mark/source")
                ids += [review["delivery_review"]]
                lines=_book(current["value"],0.,current["owner"],current["bucket"])+_book(0.,current["value"],_owner(t),current["bucket"])
                journal.emit("physical_delivery",day,lines,ids,pos,_owner(t),current["bucket"]); current["owner"]=_owner(t)
            for right in derived:
                key=(pos,right["event_id"],right["payment_ordinal"]); _require(key not in index,"duplicate raw-derived accounting right"); index[key]=len(rights)
                sources=ids+right["recognition_sources"]
                for selected in right["sources"]:
                    sources += [selected["fixing"]] if selected["kind"]=="observed" else [selected["model"],selected["path"]]
                if right["physical_path"]: sources += [right["physical_path"]]
                journal.due(right,after_default and not default_after and not closed,review,sources); rights.append(right)
            remaining=0. if closed else raw_mark-(due_pricing if n["payment_timing"]=="before_scheduled_payments" else 0.)
            journal.mark(pos,remaining,day,ids)
        for obligation in source["obligations"]:
            if obligation["settled_ordinal"]!=day: continue
            right=rights[index[(obligation["position_id"],obligation["event_id"],obligation["payment_ordinal"])]]
            ids=[obligation["contract"],r["policy"]["identity"]]
            if obligation["disposition"]["kind"]=="settle_on_node": ids += [obligation["disposition"]["evidence"]]
            journal.settle(right,day,ids)
        for close in source["closeouts"]:
            if close["ordinal"]==day:
                ids=[c["policy"]["identity"],source["source"]]+[source[k] for k in ("mpor_policy","closing_legal_review","replacement_value_policy") if source[k] is not None]
                journal.closeout(close,day,ids)
        if d and d["closeout_ordinal"]==day: closed=True
        journal.closing(day)
    _require(not normal and all(abs(t["value"])<=1e-9 for t in journal.trades.values()) and not journal.unpaid,"unresolved full rights tail")
    # Native typed f64 anchors may originate as JSON integers (e.g. JS emits 0
    # for 0.0). Normalize only declared monetary fields; dates/cutoffs and all
    # identity/enum fields retain exact comparison and large-integer precision.
    opening=deepcopy(r["opening"])
    for k in ("cash","posted_vm_asset","received_vm_liability","posted_im_asset","received_im_custody","equity_credit_balance"):
        opening[k]=_money(opening[k])
    for trade in opening["trades"]:trade["clean_value"]=_money(trade["clean_value"])
    return dict(schema="derivative-accounting-1",model_version="derivative-accounting-1",run=r["run"],measure="physical",basis=r["basis"],entity=r["entity"],currency=r["currency"],monetary_unit=r["monetary_unit"],policy=r["policy"],opening=opening,intents=journal.intents,daily=journal.daily,cash_rights=rights,native=native,pricing_xva_diagnostic=xva)


def verify_derivative_accounting(raw_request, result):
    """Verify the complete supported raw-source result or raise without a receipt.

    This is post-calculation independent verification, not a Python pricing API.
    A model JSON receipt is distinct from persisted Parquet/journal verification.
    """
    try:
        expected=_derive(raw_request)
        _same(result,expected)
        encoded=json.dumps(raw_request,sort_keys=True,allow_nan=False,separators=(",",":")).encode()
        return dict(schema="derivative-accounting-verification-1",verified=True,raw_request_sha256=hashlib.sha256(encoded).hexdigest(),checked_daily_rows=len(expected["daily"]),checked_journal_intents=len(expected["intents"]),checked_cash_rights=len(expected["cash_rights"]),checked_native_paths=len(expected["native"]["paths"]),pricing_xva_checked=expected["pricing_xva_diagnostic"] is not None,persisted_journal_verified=False)
    except DerivativeVerificationError:
        raise
    except (KeyError, TypeError, ValueError, IndexError, OverflowError, ZeroDivisionError) as exc:
        raise DerivativeVerificationError(f"unsupported/incomplete derivative source or output: {exc}") from exc
