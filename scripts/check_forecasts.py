"""Live public-source smoke; optional conditional runs use disposable demo books.

Usage: uv run --project apps/api python scripts/check_forecasts.py --run
Never modifies active API inputs. Downloads are retained as research snapshots.
"""
import argparse
from datetime import date
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "apps/api"))
from app import market_data as md
from app.forecast_data import metadata, normalized_rows
from portfolio_risk.analytics.forecast import compile_forecast, run_forecast_nii
from portfolio_risk.core.runtime import RunConfig, run_context


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--as-of", type=date.fromisoformat, default=date.today())
    parser.add_argument("--sources", nargs="+", choices=["fed_stress", "fed_sep", "philly_spf", "nyfed_sme"],
                        default=["fed_stress", "fed_sep", "philly_spf", "nyfed_sme"])
    parser.add_argument("--run", action="store_true")
    args = parser.parse_args()
    failed = False
    for name in args.sources:
        try:
            source = md.fetch_snapshot(md.FetchRequest(dataset=name, as_of=args.as_of))
            info = metadata(source)
            for scenario in info["runnable"]:
                plan = compile_forecast(normalized_rows(source), scenario, info["periods"][0], 27)
                if args.run:
                    import numba
                    from portfolio_risk.demo import model_balance_sheet, demo_market, demo_deposit_history
                    numba.set_num_threads(min(2, numba.config.NUMBA_NUM_THREADS))
                    bs = model_balance_sheet(scale=.001, basis="amortized_cost", include_markets_bs=True)
                    for k in ("mbs", "loans", "debt", "deposits", "cds", "mm"):
                        if k in bs:
                            bs[k] = bs[k].head(3)
                    sr, vp = demo_market()
                    with run_context(RunConfig(32, 32)):
                        result = run_forecast_nii(bs, sr, vp, demo_deposit_history(), plan)
                    assert result["monthly"].height == 27
                    print(name, scenario, "conditional run passed", flush=True)
            print(name, source["id"], source["observation_count"], "observations", flush=True)
        except Exception as exc:
            failed = True
            print(name, type(exc).__name__, str(exc), flush=True)
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
