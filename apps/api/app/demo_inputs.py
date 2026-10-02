"""Demo inputs; the caller supplies the sole state owner.

Functions transport/format engine outputs. Financial calculations stay in Rust.
"""
from __future__ import annotations

import copy

def seed_demo(owner):
    """Load the WFC-proportional model balance sheet + demo market."""
    from portfolio_risk.demo import (demo_deposit_history, demo_market,
                               model_balance_sheet)
    owner.PRICING.clear()
    owner.TAPES.clear()
    owner.COHORT_PUBLICATIONS.clear()
    bs = model_balance_sheet(scale=0.01, basis="amortized_cost",
                             include_markets_bs=True)
    for k in ("mbs", "loans", "debt", "deposits", "cds", "mm"):
        if bs.get(k) is not None:
            owner.BOOKS[k] = bs[k]
    owner.MBS_HISTS = bs["mbs_hists"]
    owner.ASOF = bs["asof"]
    owner.EQUITY = bs.get("equity", 0.0)
    from portfolio_risk.demo import demo_hedge_book
    owner.HEDGES = demo_hedge_book(scale=0.01)
    owner.DEP_HIST = demo_deposit_history()
    from portfolio_risk.products.deposits import SEGMENTS
    from portfolio_risk.products.cds import CD_EW_PARAMS
    owner.ASSUMPTIONS.update(deposit_segments=copy.deepcopy(SEGMENTS), cd_ew_params=CD_EW_PARAMS.tolist())
    sr, vp = demo_market()
    owner.MARKET.clear()
    owner.MARKET["swap_rates"] = sr
    owner.MARKET["vol_pts"] = vp
    owner.MARKET["source"] = "Synthetic research market"
    owner.MARKET["provenance"] = {"curve": "assumed", "volatility": "assumed",
                            "warnings": ["Curve, volatility and behavioral histories are synthetic demo inputs."]}
