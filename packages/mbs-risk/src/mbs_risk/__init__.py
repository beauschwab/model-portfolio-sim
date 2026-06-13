"""mbs-risk: layered package (v0.17 reorg).
core/ paths+pricing+kernels | models/ behavioral fits | products/ decks+
engines | analytics/ risk/stress/accounting/kpis | strategy/ programs+
unitlib+optimizer. Old flat paths (mbs_risk.kernels, mbs_risk.corp, ...)
remain importable via module aliases below -- tests, apps, and the skill
keep working unchanged."""
import sys as _sys
from .core import (config, conventions, curve, interfaces, kernels, lmm,
                   pricing, scenarios, vol)
from .models import models, prepay
from .products import cds, corp, deposits, hedges, mm
from .analytics import accounting, kpis, risk, stress
from .strategy import optimizer, strategies, unitlib
for _m in (config, conventions, curve, interfaces, kernels, lmm, pricing,
           scenarios, vol, models, prepay, cds, corp, deposits, hedges,
           mm, accounting, kpis, risk, stress, optimizer, strategies,
           unitlib):
    _sys.modules["mbs_risk." + _m.__name__.rsplit(".", 1)[-1]] = _m

from .analytics.risk import run_risk
from .products.corp import CorpDeck, run_corp_risk
from .products.deposits import DepositDeck, run_deposit_risk, run_deposit_stress
from .products.cds import CDDeck, run_cd_risk
from .analytics.stress import run_stress
from . import demo

__version__ = "0.17.3"
__all__ = ["run_risk", "run_stress", "run_corp_risk", "CorpDeck",
           "run_deposit_risk", "run_deposit_stress", "DepositDeck",
           "run_cd_risk", "CDDeck",
           "config", "demo", "__version__"]
