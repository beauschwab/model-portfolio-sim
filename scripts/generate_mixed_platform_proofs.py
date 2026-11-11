"""Independent platform expectations for two historical mixed-journal fixtures.

Retains existing reference-versus-native rounding differences as frozen original
values. A platform change in one of those excluded fields rejects, rather than
silently altering its baseline. No native financial callback is used.
"""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import platform
import sys

from portfolio_risk.analytics import consumer_mixed_reference, product_event_reference, product_strategy_reference
from generate_card2_platform_proof import differences

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "packages/portfolio-decision-native/tests/fixtures"


def inputs(name):
    filename, source, oracle = {
        "consumer": ("product_mixed_consumer_credit_baseline.json", "product-mixed-consumer-credit-1", consumer_mixed_reference),
        "card1": ("product_mixed_card1_baseline.json", "product-mixed-product-credit-1", product_event_reference),
    }[name]
    path = FIXTURES / filename
    baseline = json.loads(path.read_text())
    for table in ("daily_balance", "reported_daily_balance"):
        for row in baseline["ok"][table]:
            row.update(lcr_headroom=None, nsfr_headroom=None, lcr_status="unconfigured", nsfr_status="unconfigured")
    raw = json.loads((ROOT / f"examples/models/{source}.json").read_text())
    prepared = deepcopy(baseline["ok"]["preparation"]["performing_preparation"]["native_preparation"])
    # The mixed oracle independently checks preparation, but retains its nested
    # receipts. Reconstruct their income too, instead of copying Windows libm
    # results into a Linux expected output.
    preparation = prepared["preparation"]
    product_raw = preparation["raw_source"]
    product_strategy_reference.verify_preparation(product_raw, preparation)
    budget = product_strategy_reference._Budget(lambda: False)
    for unit, receipt in zip(product_raw["units"], preparation["units"], strict=True):
        for income in receipt["income"]:
            native, states = product_strategy_reference._income(
                unit, income["native_income"]["complete_source_projection"],
                unit["purchase_graph"]["kind"], product_raw["reporting_ordinals"], budget,
            )
            income["native_income"] = native
            income["opening_states"] = states
    product_strategy_reference.verify_preparation(product_raw, preparation)
    expected = {"ok": oracle.reconstruct(raw, prepared)}
    oracle.verify(raw, expected["ok"])
    return path, baseline, expected


def main():
    exclusions_path = FIXTURES / "mixed_card1_independent_windows_differences.json"
    exclusions = json.loads(exclusions_path.read_text())
    result = dict(platform=platform.platform(), python=sys.version, native_callback=False, cases={})
    for name in ("consumer", "card1"):
        path, baseline, expected = inputs(name)
        # Git checkout line endings are not part of the numeric fixture proof.
        digest = hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest()
        if name == "card1":
            assert digest == exclusions["original_fixture_lf_sha256"]
        diffs = list(differences(baseline, expected))
        excluded = {d["path"]: d for d in exclusions["differences"]} if name == "card1" else {}
        actual_exclusions = {d["path"]: d for d in diffs if d["path"] in excluded}
        assert actual_exclusions == excluded, "platform changed a frozen reference-rounding difference"
        changes = [d for d in diffs if d["path"] not in excluded]
        result["cases"][name] = dict(
            original_fixture_lf_sha256=digest,
            independent_reference=("consumer_mixed_reference" if name == "consumer" else "product_event_reference") + ".reconstruct",
            frozen_exclusions_sha256=hashlib.sha256(exclusions_path.read_bytes()).hexdigest(),
            frozen_reference_rounding_exclusions=len(excluded), changes=changes,
        )
    if sys.platform == "win32":
        assert all(not case["changes"] for case in result["cases"].values()), "independent Windows proof changed the frozen expectations"
    elif sys.platform == "linux" and platform.machine() == "x86_64" and platform.libc_ver()[0] == "glibc":
        frozen = json.loads((FIXTURES / "mixed_linux_gnu_patch.json").read_text())
        for name, case in result["cases"].items():
            for key in ("changes", "original_fixture_lf_sha256", "frozen_reference_rounding_exclusions"):
                assert case[key] == frozen["cases"][name][key], f"independent Linux {name} proof changed {key}"
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result,separators=(",", ":")) + "\n", encoding="utf8")
    print("Independent mixed platform patch", hashlib.sha256(out.read_bytes()).hexdigest(),
          {name: len(case["changes"]) for name,case in result["cases"].items()})


if __name__ == "__main__":
    main()
