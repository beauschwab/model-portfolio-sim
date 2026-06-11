"""mbs_risk: shifted-lognormal LMM OAS/risk/stress engine for agency MBS.

Public API:
    run_risk(port, swap_rates, vol_pts, cc_hist, ps_hist)   -> KRDs + vegas
    run_stress(port, swap_rates, vol_pts, cc_hist, ps_hist) -> 9Q stress pack
    demo.demo_portfolio / demo_market / demo_histories
"""
from .risk import run_risk
from .corp import CorpDeck, run_corp_risk
from .deposits import DepositDeck, run_deposit_risk, run_deposit_stress
from .cds import CDDeck, run_cd_risk
from .stress import run_stress
from . import config, demo

__version__ = "0.15.0"
__all__ = ["run_risk", "run_stress", "run_corp_risk", "CorpDeck",
           "run_deposit_risk", "run_deposit_stress", "DepositDeck",
           "run_cd_risk", "CDDeck",
           "config", "demo", "__version__"]
