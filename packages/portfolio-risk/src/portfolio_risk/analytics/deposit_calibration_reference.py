"""Independent exact-account-history replay; never a production fitting backend."""
import math

SCHEMA = "observed-deposit-calibration-1"
YEAR = 365 * 86400


def verify(raw, result, check=lambda: False):
    """Recompute every published field without calling Rust or using fitted output."""
    def checkpoint():
        if check():
            raise InterruptedError("deposit calibration verification cancelled")

    def timestamp(value):
        day, second = value["ordinal"], value["seconds_of_day"]
        if (type(day) is not int or not 1 <= day <= 3652059
                or type(second) is not int or not 0 <= second < 86400):
            raise ValueError("invalid deposit observation timestamp")
        return day * 86400 + second

    def compare(actual, expected, path="result"):
        checkpoint()
        if isinstance(expected, dict):
            if not isinstance(actual, dict) or actual.keys() != expected.keys():
                raise ValueError(f"deposit calibration {path} fields differ")
            for key in expected:
                compare(actual[key], expected[key], f"{path}.{key}")
        elif isinstance(expected, list):
            if not isinstance(actual, list) or len(actual) != len(expected):
                raise ValueError(f"deposit calibration {path} shape differs")
            for i, item in enumerate(expected):
                compare(actual[i], item, f"{path}[{i}]")
        elif isinstance(expected, float):
            if (type(actual) not in (int, float) or not math.isfinite(actual)
                    or not math.isclose(actual, expected, rel_tol=3e-12, abs_tol=3e-12)):
                raise ValueError(f"deposit calibration {path} amount differs")
        elif type(actual) is not type(expected) or actual != expected:
            raise ValueError(f"deposit calibration {path} identity/count differs")

    checkpoint()
    if (raw["schema"] != SCHEMA or raw["sampling_unit"] != "account_full_exit"
            or raw["observation_mode"] != "exact_transition_times"
            or raw["data_kind"] not in ("observed", "synthetic")):
        raise ValueError("unsupported deposit observation contract")
    segments = raw["segment_ids"]
    n = len(segments)
    if not 1 <= n <= 32 or len(set(segments)) != n:
        raise ValueError("invalid deposit segment identities")
    index = {segment: i for i, segment in enumerate(segments)}
    policies = raw["unobserved_row_policy"]
    if len(policies) != n:
        raise ValueError("deposit segment policy shape differs")
    start, split, end = (timestamp(raw[key]) for key in ("train_start", "train_end", "holdout_end"))
    if not start < split < end or end - start > 100 * YEAR:
        raise ValueError("invalid deposit chronological windows")
    if raw["training_split_id"] == raw["holdout_split_id"]:
        raise ValueError("deposit split identities overlap")
    if not 1 <= len(raw["spells"]) <= 200000:
        raise ValueError("deposit spell admission exceeded")
    spells = []
    identities, accounts = set(), set()
    for spell in raw["spells"]:
        checkpoint()
        a, b = timestamp(spell["start"]), timestamp(spell["end"])
        kind = spell["ending"]["kind"]
        if (not start <= a < b <= end or spell["segment_id"] not in index
                or spell["spell_id"] in identities
                or kind not in ("migration", "withdrawal", "closure", "right_censored")):
            raise ValueError("invalid deposit observed spell")
        if kind == "migration":
            destination = spell["ending"]["destination"]
            if destination not in index or destination == spell["segment_id"]:
                raise ValueError("invalid deposit migration destination")
        identities.add(spell["spell_id"])
        accounts.add(spell["account_id"])
        spells.append((spell, a, b))
    if len(accounts) > 100000:
        raise ValueError("deposit account admission exceeded")
    checkpoint()
    spells.sort(key=lambda row: (row[0]["account_id"], row[1], row[0]["spell_id"]))
    checkpoint()
    for i, (spell, a, b) in enumerate(spells):
        checkpoint()
        if i and spells[i-1][0]["account_id"] == spell["account_id"]:
            prior, _, prior_end = spells[i-1]
            if (prior_end != a or prior["ending"]["kind"] != "migration"
                    or prior["ending"]["destination"] != spell["segment_id"]):
                raise ValueError("deposit account continuity differs")
        if (spell["ending"]["kind"] == "migration" and b < end
                and (i+1 == len(spells) or spells[i+1][0]["account_id"] != spell["account_id"])):
            raise ValueError("deposit migration lacks destination continuation")

    def evidence(left, right):
        exposure = [0] * n
        migration = [[0] * n for _ in segments]
        withdrawal, closure = [0] * n, [0] * n
        censored = 0
        for spell, a, b in spells:
            checkpoint()
            origin = index[spell["segment_id"]]
            exposure[origin] += max(0, min(b, right) - max(a, left))
            if left < b <= right:
                kind = spell["ending"]["kind"]
                if kind == "migration":
                    migration[origin][index[spell["ending"]["destination"]]] += 1
                elif kind == "withdrawal":
                    withdrawal[origin] += 1
                elif kind == "closure":
                    closure[origin] += 1
                else:
                    censored += 1
        return exposure, migration, withdrawal, closure, censored

    training, holdout = evidence(start, split), evidence(split, end)
    if not sum(training[0]) or not sum(holdout[0]):
        raise ValueError("deposit fit requires both windows' exposure")
    hazards = []
    for i, segment in enumerate(segments):
        checkpoint()
        policy = policies[i]
        if policy["kind"] == "supplied_row":
            supplied = [policy["withdrawal_per_year"], policy["closure_per_year"]] + policy["migration_per_year"]
            if (len(policy["migration_per_year"]) != n or policy["migration_per_year"][i] != 0
                    or any(type(x) not in (int, float) or not math.isfinite(x) or x < 0 for x in supplied)
                    or sum(supplied) > 100):
                raise ValueError("invalid supplied deposit hazard row")
        elif policy["kind"] != "reject":
            raise ValueError("invalid deposit unobserved row policy")
        years = training[0][i] / YEAR
        if not years:
            if policy["kind"] == "reject":
                raise ValueError("deposit segment has no training exposure")
            row = dict(segment_id=segment, withdrawal_per_year=float(policy["withdrawal_per_year"]),
                       closure_per_year=float(policy["closure_per_year"]),
                       migration_per_year=[float(x) for x in policy["migration_per_year"]],
                       standard_error_withdrawal=None, standard_error_closure=None,
                       standard_error_migration=[None] * n,
                       estimate_origin=dict(kind="supplied_assumption", assumption_id=policy["assumption_id"], rationale=policy["rationale"]))
        else:
            migration, withdrawal, closure = training[1][i], training[2][i], training[3][i]
            row = dict(segment_id=segment, withdrawal_per_year=withdrawal/years,
                       closure_per_year=closure/years, migration_per_year=[x/years for x in migration],
                       standard_error_withdrawal=math.sqrt(withdrawal)/years if withdrawal else None,
                       standard_error_closure=math.sqrt(closure)/years if closure else None,
                       standard_error_migration=[0.0 if j == i else (math.sqrt(x)/years if x else None) for j, x in enumerate(migration)],
                       estimate_origin=dict(kind="observed_mle" if withdrawal+closure+sum(migration) else "observed_no_events"))
        if row["withdrawal_per_year"] + row["closure_per_year"] + sum(row["migration_per_year"]) > 100:
            raise ValueError("deposit fitted row intensity exceeded")
        hazards.append(row)

    def diagnostics(window):
        exposure, migration, withdrawal, closure, censored = window
        years = [x/YEAR for x in exposure]
        em = [[hazards[i]["migration_per_year"][j]*years[i] for j in range(n)] for i in range(n)]
        ew = [hazards[i]["withdrawal_per_year"]*years[i] for i in range(n)]
        ec = [hazards[i]["closure_per_year"]*years[i] for i in range(n)]
        likelihood, impossible = 0.0, 0
        for i in range(n):
            checkpoint()
            h = hazards[i]
            likelihood -= (h["withdrawal_per_year"]+h["closure_per_year"]+sum(h["migration_per_year"]))*years[i]
            pairs = list(zip(migration[i], h["migration_per_year"])) + [(withdrawal[i], h["withdrawal_per_year"]), (closure[i], h["closure_per_year"])]
            for count, rate in pairs:
                if count and rate == 0:
                    impossible += count
                elif count:
                    likelihood += count * math.log(rate)
        return dict(exposure_years_by_segment=years, migration_counts=migration,
                    withdrawal_counts=withdrawal, closure_counts=closure,
                    expected_migration_counts=em, expected_withdrawal_counts=ew,
                    expected_closure_counts=ec, log_likelihood=likelihood if not impossible else None,
                    impossible_event_count=impossible, right_censored_spell_count=censored)

    expected = {key: raw[key] for key in (
        "schema", "source_id", "source_revision", "population_id", "model_policy_id",
        "training_split_id", "holdout_split_id", "observation_mode", "sampling_unit",
        "data_kind", "segment_ids", "train_start", "train_end", "holdout_end")}
    expected.update(measure="physical", account_count=len(accounts), spell_count=len(spells),
                    segment_hazards=hazards, training=diagnostics(training), holdout=diagnostics(holdout),
                    adoption_eligible=False)
    compare(result, expected)
    return dict(verified=True, deposit_behavior_recomputed=True,
                chronological_holdout_recomputed=True, adoption_eligible=False)
