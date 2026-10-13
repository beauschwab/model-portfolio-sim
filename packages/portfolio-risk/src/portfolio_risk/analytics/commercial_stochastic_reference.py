"""Independent OU/SciPy regression, raw credit and stopped-cash verification.

Validation only. No native paths, monetary checkpoints or coefficients feed the
reference. Training and evaluation use separate declared NumPy PCG64 streams.
"""

import itertools
import math
from copy import deepcopy

import numpy as np

from .commercial_credit_reference import replay, typed
from .commercial_exercise_reference import _coupon_observed, _state, _surviving_rights
from .commercial_ledger_reference import identity
from .commercial_reference_math import _cash, _df
from .commercial_source_reference import compare

SCHEMA = "commercial-generated-exercise-1"
FLOATS = {
    "initial_level",
    "long_run_mean",
    "mean_reversion",
    "loading",
    "fixed_base_oas",
    "parallel_bump",
}


def _typed(value, key=""):
    value = typed(value, key)
    if type(value) is dict:
        out = {k: _typed(v, k) for k, v in value.items()}
        if out.get("rights_only_termination") is None:
            out.pop("rights_only_termination", None)
        return out
    if type(value) is list:
        return [_typed(v, key) for v in value]
    if key in FLOATS | {"annual_covariance", "betas"}:
        if type(value) not in (int, float) or not math.isfinite(value):
            raise ArithmeticError("finite stochastic input required")
        return float(value)
    return value


def _admit(r, measure="pricing"):
    required = {
        "schema",
        "admission_tier",
        "identity",
        "source",
        "policy",
        "discount_curve",
        "fixed_base_oas",
        "parallel_bump",
        "drivers",
        "annual_covariance",
        "time_ordinals",
        "coupon_loadings",
        "hazard_loadings",
        "training",
        "evaluation",
        "regression_policy",
    }
    if set(r) != required:
        raise ArithmeticError("generated request fields differ")
    if (
        r.get("schema") != SCHEMA
        or r.get("admission_tier") != "bounded_generated_1024_samples-1"
        or r.get("regression_policy")
        != "native_observable_quadratic_svd_train_evaluate-1"
    ):
        raise ArithmeticError("explicit generated exercise contract required")
    if r["policy"]["information_policy"] != "native_observable_markov_state-1":
        raise ArithmeticError("observable Markov policy required")
    source = r["source"]
    asof = source["source"]["current"]["asof_ordinal"]
    grid = r["time_ordinals"]
    drivers = r["drivers"]
    if (
        r["discount_curve"]["currency"] != source["source"]["current"]["currency"]
        or not -5.0 <= r["fixed_base_oas"] <= 5.0
        or not -0.1 <= r["parallel_bump"] <= 0.1
    ):
        raise ArithmeticError("generated discount currency/OAS/bump differs")
    for v in (
        r["identity"],
        r["training"]["randomness_domain"],
        r["evaluation"]["randomness_domain"],
    ):
        if (
            type(v) is not dict
            or set(v) != {"id", "revision"}
            or any(type(s) is not str or not s.strip() for s in v.values())
        ):
            raise ArithmeticError("generated identity required")
    if (
        not 1 <= len(drivers) <= 4
        or not 2 <= len(grid) <= 129
        or grid[0] != asof
        or any(type(o) is not int for o in grid)
        or any(a >= b for a, b in itertools.pairwise(grid))
        or grid[-1] > source["source"]["current"]["tail_end_ordinal"]
    ):
        raise ArithmeticError("bounded chronological generated grid required")
    if (
        source.get("performing_draw_tranches")
        or (measure == "pricing" and source.get("reviewed_credit_actions") is not None)
        or len(source["credit"]["paths"]) != 1
        or any(
            source["source"][k]["measure"] != measure for k in ("original", "current")
        )
    ):
        raise ArithmeticError("explicit undrawn-tranche-free supplied measure required")
    for s in (r["training"], r["evaluation"]):
        if (
            set(s) != {"paths", "market_seed", "credit_seed", "randomness_domain"}
            or type(s["paths"]) is not int
            or not 128 <= s["paths"] <= 512
            or s["paths"] % 2
            or not 1 <= len(s["market_seed"]) <= 16
            or any(type(v) is not int or not 0 <= v < 2**32 for v in s["market_seed"])
            or type(s["credit_seed"]) is not int
            or not 0 <= s["credit_seed"] < 2**64
        ):
            raise ArithmeticError("native seeded sample identity required")
    if (
        r["training"]["market_seed"] == r["evaluation"]["market_seed"]
        or r["training"]["credit_seed"] == r["evaluation"]["credit_seed"]
        or r["training"]["randomness_domain"] == r["evaluation"]["randomness_domain"]
    ):
        raise ArithmeticError("training/evaluation domains must differ")
    if any(
        set(d)
        != {
            "driver_id",
            "initial_level",
            "long_run_mean",
            "mean_reversion",
            "observation_policy",
        }
        or type(d["driver_id"]) is not str
        or not d["driver_id"]
        or len(d["driver_id"]) > 128
        or any(ord(c) < 32 for c in d["driver_id"])
        or not -100.0 <= d["initial_level"] <= 100.0
        or not -100.0 <= d["long_run_mean"] <= 100.0
        or d["observation_policy"] != "observable_date_midnight_utc-1"
        or not 0 <= d["mean_reversion"] <= 100
        for d in drivers
    ) or len({d["driver_id"] for d in drivers}) != len(drivers):
        raise ArithmeticError("unique observable OU drivers required")
    covariance = np.array(r["annual_covariance"]).reshape(len(drivers), len(drivers))
    if (
        np.any(np.diag(covariance) < 0.0)
        or not np.allclose(covariance, covariance.T, rtol=0.0, atol=1e-12)
        or np.linalg.eigvalsh(covariance)[0]
        < -8
        * np.finfo(float).eps
        * len(drivers)
        * max(float(np.max(np.abs(covariance))), 1.0)
    ):
        raise ArithmeticError("supplied Gaussian covariance is not symmetric PSD")
    keys = {(l["period_id"], l["observation_index"]) for l in r["coupon_loadings"]}
    if len(keys) != len(r["coupon_loadings"]):
        raise ArithmeticError("duplicate coupon loading")
    for l in r["coupon_loadings"]:
        if (
            set(l) != {"period_id", "observation_index", "driver_index", "loading"}
            or type(l["driver_index"]) is not int
            or not 0 <= l["driver_index"] < len(drivers)
            or type(l["observation_index"]) is not int
            or l["observation_index"] < 0
            or abs(l["loading"]) > 100.0
        ):
            raise ArithmeticError("unique bounded coupon driver mapping required")
        period = next(
            (
                p
                for p in source["source"]["current"]["periods"]
                if p["period_id"] == l["period_id"]
            ),
            None,
        )
        if (
            period is None
            or period["coupon"]["kind"] != "indexed"
            or l["observation_index"] >= len(period["coupon"]["observations"])
        ):
            raise ArithmeticError("generated coupon loading source absent")
        observation = period["coupon"]["observations"][l["observation_index"]]
        if observation["fixing_ordinal"] <= asof:
            raise ArithmeticError("observed past fixing cannot be generated")
    for p in source["source"]["current"]["periods"]:
        for i, o in enumerate(p["coupon"].get("observations", [])):
            if o["fixing_ordinal"] > asof and (
                (p["period_id"], i) not in keys
                or o["fixing_ordinal"] not in grid
                or o["source"]["kind"] != "conditional_projection"
                or o["selection_cutoff_unix_seconds"]
                != (o["fixing_ordinal"] - 719163) * 86400
            ):
                raise ArithmeticError("complete generated fixing mapping required")
    for date in r["policy"]["exercise_dates"]:
        if date["ordinal"] not in grid:
            raise ArithmeticError("exercise state grid uncovered")
    hazards = set()
    for h in r["hazard_loadings"]:
        if (
            set(h) != {"interval_index", "origin_index", "destination_index", "betas"}
            or any(
                type(h[k]) is not int
                for k in ("interval_index", "origin_index", "destination_index")
            )
            or not 0 <= h["interval_index"] < len(source["credit"]["intervals"])
            or not 0 <= h["origin_index"] < 3
            or not 0 <= h["destination_index"] < 5
            or h["origin_index"] == h["destination_index"]
            or len(h["betas"]) != len(drivers)
            or any(abs(b) > 100.0 for b in h["betas"])
        ):
            raise ArithmeticError("explicit bounded joint hazard loading required")
        key = (h["interval_index"], h["origin_index"], h["destination_index"])
        if key in hazards:
            raise ArithmeticError("duplicate generated hazard mapping")
        hazards.add(key)
        start = (
            0.0
            if h["interval_index"] == 0
            else source["credit"]["intervals"][h["interval_index"] - 1]["end_year"]
        )
        day = round(start * 365.0)
        if day / 365.0 != start or asof + day not in grid:
            raise ArithmeticError("generated credit interval start unobserved")
        base = source["credit"]["intervals"][h["interval_index"]]["hazards_per_year"][
            h["origin_index"]
        ][h["destination_index"]]
        if base == 0.0 and any(h["betas"]):
            raise ArithmeticError("zero hazard cannot acquire implicit modeled credit")
    feature_upper = (
        2 * len(drivers)
        + 4
        + sum(
            len(p["coupon"].get("observations", []))
            for p in source["source"]["current"]["periods"]
        )
        + 2 * len(source["source"]["current"]["periods"])
    )
    if feature_upper > 64:
        raise ArithmeticError(
            "generated observable contractual state exceeds 64 features"
        )
    from .commercial_exercise_reference import compile_policy

    policy = deepcopy(r["policy"])
    policy["information_policy"] = "native_full_observed_prefix-1"
    compile_policy(
        policy,
        [{"source": source, "weight": 1.0}],
        r["discount_curve"],
        r["fixed_base_oas"],
        r["parallel_bump"],
    )


def _tape(r, sample, check):
    d = len(r["drivers"])
    steps = len(r["time_ordinals"])
    half = sample["paths"] // 2
    z = np.random.Generator(np.random.PCG64(sample["market_seed"])).standard_normal(
        (half, steps - 1, d)
    )
    z = np.concatenate((z, -z), axis=0)
    covariance = np.array(r["annual_covariance"], dtype=float).reshape(d, d)
    means = np.array([x["long_run_mean"] for x in r["drivers"]])
    a = np.array([x["mean_reversion"] for x in r["drivers"]])
    levels = np.zeros((sample["paths"], steps, d))
    levels[:, 0, :] = [x["initial_level"] for x in r["drivers"]]
    for step in range(1, steps):
        check()
        dt = (r["time_ordinals"][step] - r["time_ordinals"][step - 1]) / 365.0
        decay = a[:, None] + a[None, :]
        integral = np.full((d, d), dt)
        nonzero = decay != 0.0
        integral[nonzero] = -np.expm1(-decay[nonzero] * dt) / decay[nonzero]
        v = covariance * integral
        scale = np.sqrt(np.diag(v))
        correlation = np.divide(
            v,
            scale[:, None] * scale[None, :],
            out=np.zeros_like(v),
            where=scale[:, None] * scale[None, :] != 0.0,
        )
        eigen, rotation = np.linalg.eigh(correlation)
        if eigen[0] < -8 * np.finfo(float).eps * d:
            raise ArithmeticError("generated OU covariance not PSD")
        root = scale[:, None] * (
            rotation @ np.diag(np.sqrt(np.maximum(eigen, 0.0))) @ rotation.T
        )
        levels[:, step, :] = (
            means
            + (levels[:, step - 1, :] - means) * np.exp(-a * dt)
            + z[:, step - 1, :] @ root.T
        )
    return levels


def _raw_path(r, sample, levels, i):
    raw = deepcopy(r["source"])
    raw["credit"]["paths"][0].update(path_key=i + 1, path_id=f"generated-{i}")
    domain = sample["randomness_domain"]
    raw["credit"]["draw_stream"] = {
        "kind": "counter_splitmix64_v1",
        "stream_id": domain["id"],
        "stream_revision": domain["revision"],
        "seed": sample["credit_seed"],
    }
    for loading in r["coupon_loadings"]:
        p = next(
            p
            for p in raw["source"]["current"]["periods"]
            if p["period_id"] == loading["period_id"]
        )
        o = p["coupon"]["observations"][loading["observation_index"]]
        j = loading["driver_index"]
        step = r["time_ordinals"].index(o["fixing_ordinal"])
        o["rate"] += loading["loading"] * (
            float(levels[i, step, j]) - r["drivers"][j]["initial_level"]
        )
        o["source"] = {
            "kind": "conditional_projection",
            "model": deepcopy(r["identity"]),
            "path": deepcopy(raw["source"]["current"]["path"]),
            "information_unix_seconds": (o["fixing_ordinal"] - 719163) * 86400,
            "measure": raw["source"]["current"]["measure"],
        }
    for h in r["hazard_loadings"]:
        index = h["interval_index"]
        start = 0.0 if index == 0 else raw["credit"]["intervals"][index - 1]["end_year"]
        day = round(start * 365.0)
        if day / 365.0 != start:
            raise ArithmeticError("exact native credit-start date required")
        step = r["time_ordinals"].index(raw["source"]["current"]["asof_ordinal"] + day)
        base = raw["credit"]["intervals"][index]["hazards_per_year"][h["origin_index"]][
            h["destination_index"]
        ]
        exponent = sum(
            b * (float(levels[i, step, j]) - r["drivers"][j]["initial_level"])
            for j, b in enumerate(h["betas"])
        )
        value = 0.0 if base == 0 else math.exp(math.log(base) + exponent)
        if not math.isfinite(value) or not 0 <= value <= 100:
            raise ArithmeticError("generated credit hazard outside domain")
        raw["credit"]["intervals"][index]["hazards_per_year"][h["origin_index"]][
            h["destination_index"]
        ] = value
    return raw


def _paths(r, sample, levels, check):
    paths = []
    asof = r["source"]["source"]["current"]["asof_ordinal"]
    opening = r["source"]["source"]["current"]["opening_principal"]
    for i in range(sample["paths"]):
        check()
        raw = _raw_path(r, sample, levels, i)
        projection = replay(raw, check)
        points = []
        for date in r["policy"]["exercise_dates"]:
            state, principal, legal = _state(
                raw, projection, (date["ordinal"] - asof) / 365.0, date["timing"]
            )
            step = r["time_ordinals"].index(date["ordinal"])
            features = list(map(float, levels[i, step, :])) + [
                principal / opening,
                legal / opening,
                float(state == "watch"),
                float(state == "delinquent"),
            ]
            if r["hazard_loadings"]:
                interval = next(
                    (
                        j
                        for j, v in enumerate(raw["credit"]["intervals"])
                        if (date["ordinal"] - asof) / 365.0 < v["end_year"]
                    ),
                    len(raw["credit"]["intervals"]) - 1,
                )
                stochastic = any(
                    h["interval_index"] == interval and any(h["betas"])
                    for h in r["hazard_loadings"]
                )
                start = (
                    0.0
                    if interval == 0
                    else raw["credit"]["intervals"][interval - 1]["end_year"]
                )
                anchor = (
                    r["time_ordinals"].index(asof + round(start * 365.0))
                    if stochastic
                    else None
                )
                features.extend(
                    list(map(float, levels[i, anchor, :]))
                    if stochastic
                    else [0.0] * len(r["drivers"])
                )
            for period in raw["source"]["current"]["periods"]:
                features.extend(
                    o["rate"]
                    if o["fixing_ordinal"] <= date["ordinal"]
                    and o["selection_cutoff_unix_seconds"]
                    <= (date["ordinal"] - 719163) * 86400
                    else 0.0
                    for o in period["coupon"].get("observations", [])
                )
            points_at_date = projection["carry_points"]
            if date["timing"] == "before_scheduled_cash":
                points_at_date = points_at_date[
                    : next(
                        (
                            j
                            for j, p in enumerate(points_at_date)
                            if p["year"] == (date["ordinal"] - asof) / 365.0
                            and p["kind"]
                            in ("contractual_cash", "funded_draw", "full_live_exit")
                        ),
                        len(points_at_date),
                    )
                ]
            point = next(
                p
                for p in reversed(points_at_date)
                if p["year"] <= (date["ordinal"] - asof) / 365.0
            )
            legal_coupons = {
                p["coupon_id"]: p for p in point.get("canonical_legal_coupons", [])
            }
            for period in raw["source"]["current"]["periods"]:
                coupon = (
                    legal_coupons.get(period["period_id"])
                    if _coupon_observed(period, date["ordinal"], date["timing"])
                    else None
                )
                features.extend(
                    [
                        coupon["opening_entitlement_principal"] / opening,
                        coupon["legal_remaining"] / opening,
                    ]
                    if coupon
                    else [0.0, 0.0]
                )
            points.append(
                {
                    "state": state,
                    "principal": principal,
                    "legal": legal,
                    "surviving_rights": _surviving_rights(
                        raw, projection, date["ordinal"], date["timing"]
                    ),
                    "features": features,
                }
            )
        paths.append(
            {"raw": raw, "cash": _cash(projection["credit"]), "points": points}
        )
    return paths


def _basis(features, means, scales, active):
    x = [(features[i] - means[i]) / scales[i] for i in active]
    return [1.0] + x + [x[i] * x[j] for i in range(len(x)) for j in range(i, len(x))]


def _fit(date, rows, targets):
    n = len(rows)
    d = len(rows[0]["features"])
    means = [sum(p["features"][i] / n for p in rows) for i in range(d)]
    scales = [
        math.sqrt(sum((p["features"][i] - means[i]) ** 2 / n for p in rows))
        for i in range(d)
    ]
    active = [
        i
        for i in range(d)
        if scales[i] > 8 * np.finfo(float).eps * max(abs(means[i]), 1.0)
    ]
    scales = [s if i in active else 1.0 for i, s in enumerate(scales)]
    columns = 1 + len(active) + len(active) * (len(active) + 1) // 2
    if n < 2 * columns:
        raise ArithmeticError("insufficient live training sample")
    matrix = np.array([_basis(p["features"], means, scales, active) for p in rows])
    coefficients, _, rank, singular = np.linalg.lstsq(
        matrix, np.array(targets), rcond=np.finfo(float).eps * max(matrix.shape)
    )
    states = list(dict.fromkeys(p["state"] for p in rows))
    return {
        "ordinal": date["ordinal"],
        "means": means,
        "scales": scales,
        "active_features": active,
        "coefficients": list(map(float, coefficients)),
        "observations": n,
        "rank": int(rank),
        "singular_values": list(map(float, singular)),
        "observed_live_states": states,
    }


def _forecast(regression, point):
    if point["state"] not in regression["observed_live_states"]:
        raise ArithmeticError("evaluation live credit state lacks training coverage")
    for i, value in enumerate(point["features"]):
        if i not in regression["active_features"] and abs(
            value - regression["means"][i]
        ) > 1e-9 + 1e-12 * abs(regression["means"][i]):
            raise ArithmeticError(
                "evaluation contractual state lacks varying training coverage"
            )
    return sum(
        x * c
        for x, c in zip(
            _basis(
                point["features"],
                regression["means"],
                regression["scales"],
                regression["active_features"],
            ),
            regression["coefficients"],
        )
    )


def _after(t, date):
    return t > date[0] or t == date[0] and date[1] == "before_scheduled_cash"


def _before(t, date):
    return t < date[0] or t == date[0] and date[1] == "after_scheduled_cash"


def _live(p):
    return p["state"] in ("performing", "watch", "delinquent")


def evaluate(raw, check=lambda: False, *, _training_only=False, _retained_samples=None):
    r = _typed(raw)
    _admit(r)
    asof = r["source"]["source"]["current"]["asof_ordinal"]
    dates = r["policy"]["exercise_dates"]

    def discount(t):
        return _df(r["discount_curve"], t) * math.exp(
            -(r["fixed_base_oas"] + r["parallel_bump"]) * t
        )

    def clock(d):
        return ((d["ordinal"] - asof) / 365.0, d["timing"])

    def cash(c):
        return c["principal"] + c["interest"] + c["fees"]

    def targets(paths, index, next_values):
        now = clock(dates[index])
        end = clock(dates[index + 1]) if index + 1 < len(dates) else None
        values = []
        for i, p in enumerate(paths):
            value = next_values[i] * discount(end[0]) / discount(now[0]) if end else 0.0
            for c in p["cash"]:
                if _after(c["year"], now) and (end is None or _before(c["year"], end)):
                    value += cash(c) * discount(c["year"]) / discount(now[0])
            values.append(value)
        return values

    def choose(index, point, continuation, path):
        if not _live(point):
            return "continue", continuation
        d = dates[index]
        rights = point["principal"] == 0.0 and point["surviving_rights"]
        if rights and r["policy"].get("rights_only_termination") is None:
            raise ArithmeticError("reviewed rights-only termination required")
        if point["principal"] == 0.0 and not rights:
            return "continue", continuation
        value = continuation
        decision = "continue"
        for action in (
            ("put", "call")
            if r["policy"]["simultaneous_priority"] == "issuer"
            else ("call", "put")
        ):
            f = d[action + "_clean_price_factor"]
            if f is None:
                continue
            exercise = point["principal"] * f + point["legal"] + d[action + "_fee"]
            if (
                action == "call"
                and exercise < value
                or action == "put"
                and exercise > value
            ):
                decision = action
                value = exercise
        return decision, value

    def root(paths, next_values):
        first = clock(dates[0]) if dates else None
        values = []
        for i, p in enumerate(paths):
            value = next_values[i] * discount(first[0]) if first else 0.0
            for c in p["cash"]:
                if c["year"] > 0.0 and (first is None or _before(c["year"], first)):
                    value += cash(c) * discount(c["year"])
            values.append(value)
        return values

    training = (_retained_samples["training"] if _retained_samples is not None
                else _paths(r, r["training"], _tape(r, r["training"], check), check))
    values = [0.0] * len(training)
    fits = []
    for index in reversed(range(len(dates))):
        check()
        target = targets(training, index, values)
        ids = [i for i, p in enumerate(training) if _live(p["points"][index])]
        if not ids:
            raise ArithmeticError("live training coverage absent")
        regression = _fit(
            dates[index],
            [training[i]["points"][index] for i in ids],
            [target[i] for i in ids],
        )
        fits.append(regression)
        for i, p in enumerate(training):
            point = p["points"][index]
            prediction = _forecast(regression, point) if _live(point) else target[i]
            decision, value = choose(index, point, prediction, p)
            values[i] = target[i] if decision == "continue" else value
    training_value = sum(root(training, values)) / len(training)
    fits.reverse()
    del training
    if _training_only:
        return {
            "request": r,
            "regressions": fits,
            "training_value": float(training_value),
        }
    levels = (_retained_samples["evaluation_levels"] if _retained_samples is not None
              else _tape(r, r["evaluation"], check))
    evaluation = (_retained_samples["evaluation"] if _retained_samples is not None
                  else _paths(r, r["evaluation"], levels, check))
    values = [0.0] * len(evaluation)
    decisions = [["continue"] * len(dates) for _ in evaluation]
    for index in reversed(range(len(dates))):
        target = targets(evaluation, index, values)
        for i, p in enumerate(evaluation):
            point = p["points"][index]
            prediction = _forecast(fits[index], point) if _live(point) else target[i]
            decision, value = choose(index, point, prediction, p)
            decisions[i][index] = decision
            values[i] = target[i] if decision == "continue" else value
    values = root(evaluation, values)
    mean = sum(values) / len(values)
    half = len(values) // 2
    clusters = [(values[i] + values[i + half]) * 0.5 for i in range(half)]
    variance = sum((v - mean) ** 2 for v in clusters) / (half - 1)
    outcomes = []
    for i, p in enumerate(evaluation):
        check()
        first = next((j for j, d in enumerate(decisions[i]) if d != "continue"), None)
        stops = []
        if first is not None:
            date = dates[first]
            action = decisions[i][first]
            stop = {
                "path_key": i + 1,
                "event_id": f"generated-exercise:{r['identity']['id']}:{date['ordinal']}",
                "ordinal": date["ordinal"],
                "action": action,
                "principal_clean_price_factor": date[action + "_clean_price_factor"],
                "fee": date[action + "_fee"],
                "review": deepcopy(r["policy"]["identity"]),
            }
            if date["timing"] == "before_scheduled_cash":
                stop["timing"] = "before_scheduled_cash"
            if p["points"][first]["principal"] == 0.0:
                stop["rights_only_review"] = deepcopy(
                    r["policy"]["rights_only_termination"]["review"]
                )
            stops = [stop]
        stopped = replay(p["raw"], check, stops=stops)
        actual = sum(
            cash(c) * discount(c["year"])
            for c in _cash(stopped["credit"])
            if c["year"] > 0.0
        )
        compare(float(values[i]), float(actual), check=check)
        outcomes.append(
            {
                "path_key": i + 1,
                "value": float(values[i]),
                "first_exercise_ordinal": dates[first]["ordinal"]
                if first is not None
                else None,
                "first_exercise_decision": decisions[i][first]
                if first is not None
                else "continue",
                "full_native_cash_replayed": True,
            }
        )
    return {
        "schema": SCHEMA,
        "raw_source": r,
        "regressions": fits,
        "training_value": float(training_value),
        "evaluation_value": float(mean),
        "evaluation_standard_error": math.sqrt(variance / half),
        "evaluation_clusters": half,
        "evaluation_paths": outcomes,
        "generated_joint_market_credit": True,
        "evaluation_outcomes_used_for_fit": False,
        "finite_distribution": False,
        "convergence_established": False,
        "physical_journal_eligible": False,
        "empirical_validation": False,
        "disclosure": "Supplied observable Gaussian OU law and piecewise-start loglinear credit hazards; deterministic supplied collateral discount. Quadratic LSM fit uses training only, fresh evaluation native cash is replayed. Sampling diagnostics do not establish model/basis convergence, observed accuracy, no-arbitrage calibration, or journal adoption.",
    }


def fit_pricing_law(raw, check=lambda: False):
    """Fit only independently generated Q training observations, never P outcomes."""
    return evaluate(raw, check, _training_only=True)


def generated_policy_sources(
    raw, fitted_policy, check=lambda: False, *, original=False
):
    """Replay a supplied law using an independently fitted, retained Q policy.

    Returns raw generated paths and derived stops so downstream proof/journal
    verification can reconstruct ownership without native prepared money.
    """
    r = _typed(raw)
    if original:
        r["source"]["source"]["current"] = deepcopy(r["source"]["source"]["original"])
    measure = r["source"]["source"]["current"]["measure"]
    if measure not in ("pricing", "physical"):
        raise ArithmeticError("separately supplied generated measure required")
    _admit(r, measure)
    q = fitted_policy["request"]
    identity(q["policy"], r["policy"])
    if len(q["policy"]["exercise_dates"]) != len(fitted_policy["regressions"]):
        raise ArithmeticError("complete fitted Q policy required")
    levels = _tape(r, r["evaluation"], check)
    paths = _paths(r, r["evaluation"], levels, check)
    sources = []
    for i, path in enumerate(paths):
        check()
        stops = []
        for j, date in enumerate(q["policy"]["exercise_dates"]):
            point = path["points"][j]
            if not _live(point):
                continue
            rights = point["principal"] == 0.0 and point["surviving_rights"]
            if rights and q["policy"].get("rights_only_termination") is None:
                raise ArithmeticError("reviewed rights-only termination required")
            if point["principal"] == 0.0 and not rights:
                continue
            value = _forecast(fitted_policy["regressions"][j], point)
            decision = "continue"
            for action in (
                ("put", "call")
                if q["policy"]["simultaneous_priority"] == "issuer"
                else ("call", "put")
            ):
                factor = date[action + "_clean_price_factor"]
                if factor is None:
                    continue
                exercise = (
                    point["principal"] * factor + point["legal"] + date[action + "_fee"]
                )
                if (
                    action == "call"
                    and exercise < value
                    or action == "put"
                    and exercise > value
                ):
                    decision, value = action, exercise
            if decision == "continue":
                continue
            stop = {
                "path_key": i + 1,
                "event_id": f"generated-policy:{q['identity']['id']}:{date['ordinal']}",
                "ordinal": date["ordinal"],
                "action": decision,
                "principal_clean_price_factor": date[decision + "_clean_price_factor"],
                "fee": date[decision + "_fee"],
                "review": deepcopy(q["policy"]["identity"]),
                "timing": date["timing"],
            }
            if point["principal"] == 0.0:
                stop["rights_only_review"] = deepcopy(
                    q["policy"]["rights_only_termination"]["review"]
                )
            stops = [stop]
            break
        generated = deepcopy(path["raw"])
        if original:
            generated["source"]["original"] = deepcopy(generated["source"]["current"])
        projection = replay(generated, check, stops=stops)
        sources.append(
            {
                "raw_request": generated,
                "native_stops": stops,
                "cash": _cash(projection["credit"]),
                "weight": 1.0 / len(paths),
                "projection": projection,
            }
        )
    return sources


def verify(raw, actual, check=lambda: False):
    expected = evaluate(raw, check)
    identity(expected["raw_source"], actual["raw_source"])
    received = deepcopy(actual)
    for name, limit in (
        ("admitted_work_upper", 4_000_000_000),
        ("retained_bytes_upper", 128 << 20),
    ):
        value = received.pop(name, None)
        if type(value) is not int or not 0 < value <= limit:
            raise ArithmeticError("generated admission integer/bound differs")
    compare(expected, received, check=check)
    return expected
