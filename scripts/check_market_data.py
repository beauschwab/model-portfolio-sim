"""Read-only live source smoke test; stores snapshots but never changes a book.

uv run --project apps/api python scripts/check_market_data.py --as-of 2026-09-25
"""
import argparse
from datetime import date, timedelta
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps" / "api"))
from app.market_data import FetchRequest, fetch_snapshot

parser = argparse.ArgumentParser()
parser.add_argument("--as-of", type=date.fromisoformat, default=date.today())
parser.add_argument("--sources", nargs="+", default=["eris_sofr", "eris_options", "nyfed", "treasury",
                    "fed_zero", "pmms", "fhfa", "fdic", "fdic_rates", "fed_stress"])
parser.add_argument("--fdic-cert", default="3511")
parser.add_argument("--sec-cik", default="72971")
args = parser.parse_args()
failed = False
for source in args.sources:
    try:
        snap = fetch_snapshot(FetchRequest(dataset=source, as_of=args.as_of,
            start=args.as_of - timedelta(days=365), identifier=(args.fdic_cert if source == "fdic"
                                                               else args.sec_cik if source == "sec" else "")))
        print(json.dumps({"source": source, "status": "ok", "rows": snap["observation_count"],
                          "id": snap["id"], "curve": snap["curve"], "sample": snap["observations"][:1]}), flush=True)
    except Exception as exc:
        failed = True
        print(json.dumps({"source": source, "status": "error", "detail": str(exc)}), flush=True)
sys.exit(1 if failed else 0)
