"""Explicit independent reference evidence for platform libm fixture diagnostics.

Does not import a native runtime or modify checked-in fixtures. Frozen expected
Windows results and the original source cases remain intact.
"""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import platform
import sys

from portfolio_risk.analytics import card_credit_v2_reference as oracle


def expand(result):
    return dict(
        economic=result["economic"],
        trace=[dict(
            atomic_action=a["atomic_action"], ordinal=a["ordinal"],
            source_kind=a["source"]["kind"],
            recognition_indices=a["source"]["recognition_indices"],
            cash_event_id=a["source"]["cash_event_id"],
            interest_recovery_index=a["source"]["interest_recovery_index"],
        ) for a in result["actions"]],
        daily=[dict(ordinal=d["ordinal"], snapshot=d["snapshot"]) for d in result["daily"]],
        **{k: result[k] for k in ("events", "shared_intents", "income_steps", "reporting", "closing", "complete")},
    )


def differences(original, current, path=""):
    if isinstance(original, dict):
        assert original.keys() == current.keys(), path
        for key in original:
            yield from differences(original[key], current[key], path + "/" + key.replace("~", "~0").replace("/", "~1"))
    elif isinstance(original, list):
        assert len(original) == len(current), path
        for i, (left, right) in enumerate(zip(original, current)):
            yield from differences(left, right, path + "/" + str(i))
    elif original != current:
        assert type(original) is float and type(current) is float, path
        yield dict(path=path, original=original, expected=current)


def main():
    root = Path(__file__).resolve().parents[1]
    path = root / "packages/portfolio-model-core/tests/fixtures/card_credit_v2_proof.json"
    original = json.loads(path.read_text())
    result = deepcopy(original)
    for case in result["cases"]:
        if case["expected"] is None:
            try:
                oracle.reconstruct(deepcopy(case["raw"]))
            except ArithmeticError:
                continue
            raise AssertionError("independent oracle accepted rejected case " + case["name"])
        expected = expand(oracle.reconstruct(deepcopy(case["raw"])))
        # Existing independent numerical/structural gate is unchanged.
        oracle._equal(expected, case["expected"])
        case["expected"] = expected
    result["platform_provenance"] = {
        "platform": platform.platform(), "python": sys.version,
        "original_fixture_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "reference": "portfolio_risk.analytics.card_credit_v2_reference.reconstruct",
        "native_callback": False,
    }
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, separators=(",", ":")) + "\n", encoding="utf8")
    changes = list(differences(original["cases"], result["cases"], "/cases"))
    if sys.platform == "win32":
        assert not changes, "independent Windows proof changed the frozen expectations"
    elif sys.platform == "linux" and platform.machine() == "x86_64" and platform.libc_ver()[0] == "glibc":
        frozen = json.loads((path.parent / "card_credit_v2_linux_gnu_patch.json").read_text())
        assert changes == frozen["changes"], "independent Linux proof differs from the checked-in exact expectations"
    patch = dict(provenance=result["platform_provenance"], changes=changes)
    out.with_name("platform-patch.json").write_text(json.dumps(patch, separators=(",", ":")) + "\n", encoding="utf8")
    print("Independent platform proof", hashlib.sha256(out.read_bytes()).hexdigest())


if __name__ == "__main__":
    main()
