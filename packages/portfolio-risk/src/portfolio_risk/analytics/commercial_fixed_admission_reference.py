"""Independent raw-source and indexed preparation census for finite fixed owners.

This explicit diagnostic retains every supplied P/Q/bump path, including zero
allocations. It reconstructs existing admission dimensions from raw contracts;
producer counters and projected money never supply a dimension. It does not
replace independent financial replay, prove private allocator/Sim/codec bounds,
or grant execution or publication. Advanced owner policies require their own
census and fail here rather than disappearing from the count.
"""

import math

from .commercial_cut_capture_reference import _finite_tree
from .commercial_dto import graph_typed
from .commercial_credit_reference import prepared, full_end
from ._deposit_continuation_income_reference import native_json_length as length
from .modeled_json_identity import exact_json_identity


def _require(ok, message):
    if not ok:
        raise ArithmeticError(message)


def _check(check):
    if check():
        raise InterruptedError("raw commercial census cancelled")


def _guard(raw, check):
    _check(check)
    _finite_tree(raw)
    _require(
        type(raw) is dict and length(raw) <= 2 << 20,
        "bounded complete raw commercial census required",
    )
    _check(check)


def _candidate_dto(raw):
    result = graph_typed(raw)
    for rows, keys in [
        (result["allocation"], ("face",)),
        (
            result["owners"],
            ("pre_purchase_principal", "pre_purchase_accrued", "pre_purchase_basis"),
        ),
    ]:
        for row in rows:
            for key in keys:
                value = row[key]
                _require(
                    type(value) in (int, float) and math.isfinite(value),
                    "finite native candidate scalar required",
                )
                row[key] = float(value)
    return result


def shape(v):
    if v is None:
        return 4
    if type(v) is bool:
        return 5
    if type(v) in (int, float):
        return 32
    if type(v) is str:
        return length(v)
    if type(v) is list:
        return 2 + max(0, len(v) - 1) + sum(map(shape, v))
    return 2 + max(0, len(v) - 1) + sum(length(k) + 1 + shape(x) for k, x in v.items())


def factor(raw, queries):
    if raw["principal_plan"]["kind"] != "supplied" or any(
        p["coupon"]["kind"] != "fixed" for p in raw["periods"]
    ):
        raise ArithmeticError("ordinary supplied fixed-source census only")
    n = len(raw["periods"])
    size = shape(raw)
    years = math.ceil((raw["tail_end_ordinal"] - raw["asof_ordinal"]) / 365) + 2
    return dict(
        segment_bound=n,
        query_bound=queries,
        work_upper=250000 + 16 * size + 128 * n + 256 * queries * (n + years),
        retained_bytes_upper=16 * size + 4096 * (2 * n + queries * n),
        typed_source_structural_upper=size,
        calendar_year_scan_bound=years,
    )


def source(raw, role, check=lambda: False):
    _guard(raw, check)
    _require(
        role in ("physical", "pricing", "pricing_down", "pricing_up"),
        "explicit source measure/role required",
    )
    raw = graph_typed(raw)
    if (
        any(
            k in raw
            for k in (
                "performing_draw_tranches",
                "reviewed_credit_actions",
                "maturity_basis_policy",
                "prepayment_terms",
            )
        )
        or "transfer" in raw["source"]
        or raw["factor_policy"].get("terminal_obligations") is not None
    ):
        raise ArithmeticError("separate advanced source census required")
    credit = raw["credit"]
    _require(
        0 < len(credit["intervals"]) <= 4096 and 0 < len(credit["paths"]) <= 4096,
        "bounded complete source paths/intervals required",
    )
    end = full_end(raw)
    _require(
        math.isfinite(end) and 0 < end <= 1080 / 365.0,
        "source census complete calendar exceeds1080days",
    )
    p, _ = prepared(raw, check=check)
    p["raw_credit_path"]["measure"] = "physical" if role == "physical" else "pricing"
    path = p["raw_credit_path"]
    paths = len(path["paths"])
    b = len(path["boundaries"])
    j = path["max_jumps_per_path"]
    i = len(path["intervals"])
    soj = i + b + j + 1
    q = 2 * paths * soj
    old = factor(raw["source"]["original"], 0)
    new = factor(raw["source"]["current"], q)
    periods = len(raw["source"]["current"]["periods"])
    lots = periods * 3
    points = paths * (soj + b + j + 68)
    rows = (
        points
        + paths * periods
        + paths * lots * 33
        + paths * soj * new["segment_bound"]
    )
    core = length(p)
    rawlen = length(raw)
    carry = dict(
        raw_bytes=core,
        row_upper=rows,
        work_upper=4 * core + 128 * rows + old["work_upper"] + new["work_upper"],
        retained_bytes_upper=20 * core
        + 8192 * rows
        + old["retained_bytes_upper"]
        + new["retained_bytes_upper"],
    )
    bw = 128 * points * (lots + 1)
    bb = 4096 * points * (lots + 1)
    a = dict(
        raw_bytes=rawlen,
        native_stop_bytes=2,
        maximum_queries=q,
        carry_points_upper=points,
        accounting_groups_upper=points,
        lots_per_path_upper=lots,
        original_source_work=old["work_upper"],
        current_source_work=new["work_upper"],
        carry=carry,
        book_work_upper=bw,
        book_bytes_upper=bb,
        joined_work_upper=carry["work_upper"] + 8 * (rawlen + 2) + bw,
        joined_bytes_upper=carry["retained_bytes_upper"] + 20 * (rawlen + 2) + bb,
        full_credit_end_year=full_end(raw),
    )
    return a, old, new, p


def graph(raw, check=lambda: False, *, ordinary_fees=False):
    _guard(raw, check)
    raw = graph_typed(raw)
    work = 8 * length(raw)
    retained = 16 * length(raw)
    count = 0
    physical = []
    for pos in raw["positions"]:
        if any(
            pos.get(k) is not None for k in ("generated_policy", "exercise")
        ) or pos.get("original_physical") or (pos.get('fees') is not None and not ordinary_fees):
            raise ArithmeticError("separate advanced graph census required")
        families = [("pricing", pos["original_pricing"])]
        families += [
            (role, h[role])
            for h in pos["histories"]
            for role in ("physical", "pricing", "pricing_down", "pricing_up")
        ]
        for role, histories in families:
            for history in histories:
                _check(check)
                a, *_ = source(history["source"], role, check)
                count += 1
                work += a["joined_work_upper"]
                retained += a["joined_bytes_upper"] * (2 if role == "physical" else 1)
                if role == "physical":
                    physical.append(a)
        if ordinary_fees and pos.get('fees') is not None:
            from .commercial_fixed_fee_capacity_reference import graph_allowance
            fee_work, fee_bytes = graph_allowance(pos)
            work += fee_work
            retained += fee_bytes
    work += count * len(raw["reporting_ordinals"]) * 4096
    return work, retained, physical


def candidate(raw, check=lambda: False, *, ordinary_fees=False):
    _guard(raw, check)
    from dataclasses import fields
    from .commercial_reports_reference import normalize_specification
    from ._deposit_continuation_models import Account, Position, Scenario

    raw = _candidate_dto(raw)
    units = raw["commercial"]["units"]
    _require(0 < len(units) <= 32, "bounded complete unit grid required")
    gs = [graph(u["graph"], check, ordinary_fees=ordinary_fees) for u in units]
    spec = normalize_specification(raw["specification"])
    horizon = spec["horizon_days"]
    fee_groups = 0
    fee_records = 0
    for u, g in zip(units, gs):
        for a in g[2]:
            y = a["full_credit_end_year"]
            d = y * 365
            r = round(d)
            days = r if r / 365 == y else math.ceil(d)
            if y > days / 365:
                days += 1
            horizon = max(
                horizon, u["graph"]["as_of_ordinal"] - raw["as_of_ordinal"] + days
            )
        if ordinary_fees:
            from .commercial_fixed_fee_capacity_reference import histories, events
            for position in u['graph']['positions']:
                for history, _, physical, extra in histories(position):
                    if physical:
                        fee_records += len(history["policy"]["fees"])
                        horizon = max(horizon, history['policy']['complete_end_ordinal'] - raw['as_of_ordinal'])
                        fee_groups += events(history['policy']) + extra
    spec["horizon_days"] = horizon
    for key, cls in [
        ("accounts", Account),
        ("positions", Position),
        ("scenarios", Scenario),
    ]:
        for row in spec[key]:
            for field in fields(cls):
                if field.type is float:
                    row[field.name] = float(row[field.name])
    physical = [a for g in gs for a in g[2]]
    groups = sum(a["accounting_groups_upper"] + 8 for a in physical) + fee_groups
    bindings = len(physical) + fee_groups
    positions = len(spec["positions"])
    accounts = len(spec["accounts"])
    scenes = ["baseline"] + [s["name"] for s in spec["scenarios"]]
    scenarios = len(scenes)
    cells = len(units) * scenarios * len(raw["commercial"]["reporting_ordinals"]) * 11
    labels = (
        scenes
        + [a["id"] for a in spec["accounts"]]
        + [v for p in spec["positions"] for v in (p["id"], p["account"])]
        + [
            v
            for o in raw["owners"]
            for v in (o["unit_id"], o["position_id"], o["account_id"])
        ]
        + [
            v
            for m in raw["run_mapping"]
            for v in (m["source_scenario_id"], m["ledger_scenario_id"])
        ]
        + [u["unit_id"] for u in units]
        + [p["position_id"] for u in units for p in u["graph"]["positions"]]
    )
    label = max(len(v.encode()) for v in labels) + 128
    identities = len(units) + bindings + scenarios
    passes = (groups - 1).bit_length() if groups > 1 else 0
    sorting = groups * passes * 8 + (groups * (1 + passes) * 16 if groups > 1 else 0)
    construction = (
        sorting
        + groups * 128
        + (4 * identities**2 + bindings**2 * scenarios) * label
        + (bindings + len(units)) * (positions + accounts + scenarios + 1) * label
        + (positions + accounts + len(units) + bindings + scenarios) * 128
        + fee_records * label
        + cells * 128
        + length(spec) * 8
        + 128
    )
    graphbytes = sum(g[1] for g in gs)
    rawlen = length(raw)
    retained = (
        20 * rawlen
        + graphbytes
        + groups * (2 * max(len(s.encode()) for s in scenes) + 1024)
        + (bindings + positions) * 4096
        + cells * 512
    )
    unitwork = (
        8 * length(raw["commercial"])
        + sum(g[0] for g in gs)
        + len(units)
        * len(units[0]["graph"]["scenarios"])
        * (11 if ordinary_fees and any(p.get("fees") is not None for u in units for p in u["graph"]["positions"]) else 10)
        * (len(raw["commercial"]["reporting_ordinals"]) - 1)
        * 128
    )
    result = dict(
        work_upper=8 * rawlen + unitwork + construction,
        retained_bytes_upper=retained,
        full_end_ordinal=raw["as_of_ordinal"] + horizon,
    )
    _require(
        all(type(v) is int and 0 <= v < 1 << 64 for v in result.values()),
        "candidate census integer overflow",
    )
    _check(check)
    return result, spec


def verify(raw, preparation, check=lambda: False):
    """Verify only raw-derived preparation dimensions, never financial acceptance."""
    expected, specification = candidate(raw, check)
    _require(
        exact_json_identity(_candidate_dto(raw), preparation["raw_request"]),
        "raw-derived candidate DTO identity differs",
    )
    _require(
        exact_json_identity(specification, preparation["resolved_specification"]),
        "raw-derived normalized specification differs",
    )
    for field, observed in [
        ("work_upper", "admitted_work_upper"),
        ("retained_bytes_upper", "retained_bytes_upper"),
    ]:
        _require(
            exact_json_identity(expected[field], preparation[observed]),
            "raw-derived preparation admission differs/" + observed,
        )
    _check(check)
    return dict(
        schema="commercial-fixed-preparation-census-1",
        **expected,
        raw_derived_source_preparation=True,
        complete_resource_proof=False,
        publication_eligible=False,
    )
