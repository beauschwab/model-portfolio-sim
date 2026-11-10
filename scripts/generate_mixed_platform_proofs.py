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

from portfolio_risk.analytics import consumer_mixed_reference, product_event_reference
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
    out = Path(sys.argv[1])
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result,separators=(",", ":")) + "\n", encoding="utf8")
    print("Independent mixed platform patch", hashlib.sha256(out.read_bytes()).hexdigest(),
          {name: len(case["changes"]) for name,case in result["cases"].items()})


if __name__ == "__main__":
    main()
