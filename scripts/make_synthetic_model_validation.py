"""Offline synthetic histories and known-truth checks; never a production model.

Generate with the standard library, then optionally send the raw observations to
the existing Rust calibration entrypoints. No fitted coefficient generates its
own validation data, and no synthetic policy is represented as reviewed.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import datetime as dt
import hashlib
import json
import math
from pathlib import Path
import random

YEAR_SECONDS = 365 * 86400
START = dt.date(2021, 1, 1).toordinal() * 86400
SPLIT = dt.date(2023, 1, 1).toordinal() * 86400
END = dt.date(2024, 1, 1).toordinal() * 86400
CREDIT_STATES = ["performing", "watch", "delinquent", "default", "prepaid"]
DEPOSIT_STATES = ["retail", "commercial", "withdrawal", "closure"]
# Explicit hypothetical laws, not estimates for these products or institutions.
CREDIT_BASE = [[0., .18, .06, .03, .14], [.32, 0., .22, .08, .10],
               [.20, .25, 0., .40, .08]]
PRODUCT_SCALES = {"auto": 1., "personal": 1.4, "card": 2.,
                  "mortgage": .65, "commercial": .85}
DEPOSIT_BASE = [[0., .12, .15, .04], [.08, 0., .22, .06]]


def clock(seconds: int) -> dict:
    ordinal, seconds_of_day = divmod(seconds, 86400)
    return dict(ordinal=ordinal, seconds_of_day=seconds_of_day)


def seconds(value: dict) -> int:
    return value["ordinal"] * 86400 + value["seconds_of_day"]


def _walks(rng, rows, states, live, count, start, end, partition):
    """Independent exponential waits/competing risks, rounded UP to seconds.

    Every live sojourn is recorded. A boundary censor does not fabricate an
    event, and a default/exit ends the account. The maximum rounding error is
    one second per waiting time, disclosed in the truth file.
    """
    for origin in range(live):
        for account in range(count):
            account_id = f"{partition}-{origin}-{account}"
            state, now, index = origin, start, 0
            while now < end:
                rates = rows[state]
                total = sum(rates)
                wait = max(1, math.ceil(rng.expovariate(total) * YEAR_SECONDS))
                stop = min(end, now + wait)
                destination = None
                if now + wait <= end:
                    draw = rng.random() * total
                    cumulative = 0.
                    for candidate, rate in enumerate(rates):
                        cumulative += rate
                        if draw < cumulative:
                            destination = candidate
                            break
                    assert destination is not None
                yield dict(spell_id=f"{account_id}-{index}", account_id=account_id,
                           start=clock(now), end=clock(stop), state=states[state],
                           destination=None if destination is None else states[destination])
                if destination is None or destination >= live:
                    break
                state, now, index = destination, stop, index + 1


def _header(schema, product):
    return dict(schema=schema, source_id=f"synthetic-{product}", source_revision="synthetic-v1",
                population_id=f"hypothetical-{product}-accounts", model_policy_id="example-only-v1",
                training_split_id="2021-2022", holdout_split_id="2023",
                observation_mode="exact_transition_times", data_kind="synthetic",
                train_start=clock(START), train_end=clock(SPLIT), holdout_end=clock(END))


def credit_history(product, count, seed, shift=False):
    rows = [[v * PRODUCT_SCALES[product] for v in row] for row in CREDIT_BASE]
    future = deepcopy(rows)
    if shift:
        for row in future:
            row[3] *= 4.  # Deliberate, disclosed out-of-time default regime change.
    rng = random.Random(seed)
    raw = _header("observed-credit-calibration-1", product)
    raw["unobserved_row_policy"] = [{"kind": "reject"} for _ in rows]
    raw["spells"] = []
    for partition, a, b, law in [("train", START, SPLIT, rows), ("holdout", SPLIT, END, future)]:
        for spell in _walks(rng, law, CREDIT_STATES, 3, count, a, b, partition):
            origin, destination = spell.pop("state"), spell.pop("destination")
            spell.update(origin=origin, ending=({"kind": "right_censored"} if destination is None
                         else {"kind": "transition", "destination": destination}))
            raw["spells"].append(spell)
    return raw, dict(training_hazards=rows, holdout_hazards=future)


def deposit_history(count, seed, shift=False):
    future = deepcopy(DEPOSIT_BASE)
    if shift:
        for row in future:
            row[2] *= 4.
    raw = _header("observed-deposit-calibration-1", "deposits")
    raw.update(sampling_unit="account_full_exit", segment_ids=DEPOSIT_STATES[:2],
               unobserved_row_policy=[{"kind": "reject"} for _ in range(2)], spells=[])
    rng = random.Random(seed)
    for partition, a, b, law in [("train", START, SPLIT, DEPOSIT_BASE), ("holdout", SPLIT, END, future)]:
        for spell in _walks(rng, law, DEPOSIT_STATES, 2, count, a, b, partition):
            segment, destination = spell.pop("state"), spell.pop("destination")
            ending = ({"kind": "right_censored"} if destination is None else
                      {"kind": "migration", "destination": destination} if destination in DEPOSIT_STATES[:2]
                      else {"kind": destination})
            spell.update(segment_id=segment, ending=ending)
            raw["spells"].append(spell)
    return raw, dict(training_hazards=DEPOSIT_BASE, holdout_hazards=future)


def snapshot_history(count, seed, shift=False):
    """Endpoint Bernoulli observations: no inferred within-interval transition time."""
    rng = random.Random(seed)
    raw = _header("credit-snapshot-calibration-1", "panel-credit")
    raw.pop("observation_mode")
    # The native panel likelihood admits at most one ACT/365 year per interval.
    # Use the final training year rather than inventing intermediate snapshots.
    panel_start = SPLIT - YEAR_SECONDS
    raw["train_start"] = clock(panel_start)
    fixed = lambda name: dict(kind="fixed_row", hazards_per_year=[0.] * 5,
                             assumption_id=f"example-{name}", rationale="Explicit hypothetical absorbing row")
    raw.update(monetary_unit="USD currency_units", crossing_split_policy="reject",
               censored_outcome_policy="reject", covariate_names=[], term_levels=["reference"],
               rows=[dict(kind="log_linear", edges=[dict(destination="default",
                    intercept=dict(initial=-1., lower=-8., upper=2.),
                    covariate_coefficients=[], term_coefficients=[])]), fixed("watch"), fixed("delinquent")],
               optimizer=dict(max_search_evaluations=1800, initial_mesh=.5,
                              final_mesh=1e-6, objective_improvement_tolerance=1e-12),
               snapshots=[], default_outcomes=[])
    for partition, a, b, rate in [("train", panel_start, SPLIT, .4), ("holdout", SPLIT, END, 1.6 if shift else .4)]:
        for i in range(count * 3):
            destination = "default" if rng.random() < -math.expm1(-rate * (b-a)/YEAR_SECONDS) else "performing"
            raw["snapshots"].append(dict(snapshot_id=f"{partition}-{i}", account_id=f"{partition}-{i}",
                source_id="synthetic-panel", start=clock(a), end=clock(b), origin="performing",
                destination=destination, covariates=[], term_level="reference"))
        # Independent complete default outcomes, not fabricated from the snapshots.
        for i in range(12):
            default = a + 90 * 86400
            raw["default_outcomes"].append(dict(outcome_id=f"{partition}-outcome-{i}",
                account_id=f"{partition}-outcome-{i}", source_id="synthetic-complete-outcomes",
                default_origin="performing", default_time=clock(default),
                pre_default_drawn=10000., pre_default_undrawn=2000., realized_ead=11200.,
                recoveries=[dict(time=clock(default+30*86400), principal=2240.),
                            dict(time=clock(default+90*86400), principal=3360.)],
                resolution=dict(kind="complete", resolved_time=clock(default+120*86400))))
    return raw, dict(training_hazard=.4, holdout_hazard=1.6 if shift else .4,
                     realized_lgd=.5, realized_ccf=.6)


def build_pack(count=600, seed=20261019):
    if not 30 <= count <= 2000:
        raise ValueError("accounts per live origin must be in 30..2000")
    requests, truth = {}, {}
    for index, product in enumerate(PRODUCT_SCALES):
        name = f"{product}-credit"
        requests[name], truth[name] = credit_history(product, count, seed+index)
    for name, builder, offset in [("commercial-credit-shift", lambda n,s: credit_history("commercial", n,s,True), 4),
                                  ("deposits", deposit_history, 10),
                                  ("deposits-shift", lambda n,s: deposit_history(n,s,True), 10),
                                  ("panel-credit", snapshot_history, 20),
                                  ("panel-credit-shift", lambda n,s: snapshot_history(n,s,True), 20)]:
        requests[name], truth[name] = builder(count, seed+offset)
    return requests, dict(schema="synthetic-model-truth-1", data_kind="synthetic", seed=seed,
        accounts_per_live_origin_per_window=count, year_seconds=YEAR_SECONDS,
        waiting_time_rounding="up to integer seconds; <1 second per sojourn",
        cases=truth)


def write_pack(output, count=600, seed=20261019):
    output = Path(output)
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise ValueError("output directory must be empty; previous evidence is never overwritten")
    requests, truth = build_pack(count, seed)
    policies = dict(schema="synthetic-example-policies-1", data_kind="synthetic",
        reviewed=False, approved_for_production=False,
        purpose="Hypothetical validation inputs; no bank, regulator or accounting review is asserted",
        observation_policy="exact complete-account spells; snapshots retain endpoint-only observation",
        population_policy="independent training and later holdout cohorts; all live origins explicitly sampled",
        unused_origins="reject rather than silently infer a hazard row",
        recovery_policy="separate complete outcomes; undiscounted realized principal LGD and CCF diagnostics",
        unsupported=["partial deposit withdrawals", "deposit beta fitting", "mortgage prepayment incentive fitting",
                     "card utilization/payment allocation fitting", "borrower or macro covariate validation",
                     "institution-specific accounting approval", "regulatory methodology approval"])
    files = {}
    for name, value in {**requests, "truth": truth, "example-policies": policies}.items():
        data = (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)+"\n").encode()
        path = output / f"{name}.json"
        path.write_bytes(data)
        files[path.name] = dict(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    manifest = dict(schema="synthetic-model-validation-pack-1", data_kind="synthetic",
                    generator="scripts/make_synthetic_model_validation.py", generator_version=1,
                    seed=seed, accounts_per_live_origin_per_window=count, files=files)
    (output/"manifest.json").write_text(json.dumps(manifest, indent=2)+"\n", encoding="utf-8")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--accounts-per-origin", type=int, default=600)
    parser.add_argument("--seed", type=int, default=20261019)
    args = parser.parse_args()
    manifest = write_pack(args.output, args.accounts_per_origin, args.seed)
    print(f"Wrote {len(manifest['files'])} synthetic input/truth/policy files to {args.output.resolve()}")


if __name__ == "__main__":
    main()
