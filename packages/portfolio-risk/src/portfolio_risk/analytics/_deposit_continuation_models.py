from dataclasses import dataclass,asdict
from functools import lru_cache
from typing import get_type_hints
import math
@dataclass
class Account:
    id: str
    entity: str
    currency: str
    cash: float
    equity: float
    cash_floor: float = 0.0
    cet1_floor: float = 0.07
    leverage_floor: float = 0.04
    capital_deductions: float = 0.0
    include_aoci: bool = True
    annual_fees: float = 0.0
    annual_costs: float = 0.0
    tax_rate: float = 0.0
    annual_dividends: float = 0.0
    lcr_floor: float = 0.0
    nsfr_floor: float = 0.0
    htm_asset_limit: float = 1.0
    lcr_applicability: str = ""
    nsfr_applicability: str = ""
    htm_accounting: str = ""
    htm_taint_materiality: float = 0.0

@dataclass
class Position:
    id: str
    account: str
    kind: str  # loan, security, deposit, funding, repo, reverse_repo
    balance: float
    rate: float = 0.0
    payment_interval_days: int = 30
    floating_beta: float = 0.0
    maturity_day: int = 0  # zero = beyond horizon; ACT/365, no calendar adjustment
    classification: str = "ac"  # ac, afs, htm, trading
    risk_weight: float = 1.0
    asf_weight: float = 0.0
    rsf_weight: float = 1.0
    lcr_outflow_weight: float = 0.0
    hqla_weight: float = 0.0
    duration: float = 0.0
    allowance: float = 0.0
    annual_pd: float = 0.0
    lgd: float = 0.45
    watch_fraction: float = 0.0
    watch_pd_multiplier: float = 3.0
    migration_rate: float = 0.0
    recovery_days: int = 90
    commitment: float = 0.0  # undrawn, not total facility
    draw_fraction: float = 0.0  # fraction of original undrawn per stress unit, over 30 days
    monthly_runoff: float = 0.0
    uninsured_fraction: float = 0.0
    concentration: float = 0.0
    operational_fraction: float = 0.0
    digital_fraction: float = 0.0
    rollover: float = 0.0
    collateral_pool: str = ""
    eligible_fraction: float = 0.0
    encumbered_fraction: float = 0.0
    haircut: float = 0.0
    source_id: str = ""
    book_adjustment: float = 0.0
    opening_accrued: float = 0.0
    collateral_position: str = ""
    pledged_face: float = 0.0
    start_day: int = 0  # forward origination exchanges cash for carrying value
    hqla_level: str = 'level1'
    lcr_inflow_weight: float = 0.0
    opening_market_price: float = 1.0  # clean value per unit principal, separate from cost basis

@dataclass
class Scenario:
    name: str
    start_day: int = 1
    rate_shift: float = 0.0
    spread_shift: float = 0.0
    deposit_flight: float = 0.0  # additional monthly hazard input
    pd_multiplier: float = 1.0
    migration_multiplier: float = 1.0
    lgd_add: float = 0.0
    draw_multiplier: float = 0.0
    rollover_loss: float = 0.0
    haircut_add: float = 0.0
    market_shock: float = 0.0
    outage_days: int = 0  # delays policy execution, not contractual outflows

@lru_cache(maxsize=16)
def _field_types(cls):
    """Dataclass schemas are process-static; resolve postponed annotations once."""
    return get_type_hints(cls)

def _decode(cls, raw):
    if not isinstance(raw, dict):
        raise ValueError(f"{cls.__name__} must be an object")
    types = _field_types(cls)
    if set(raw) - set(types):
        raise ValueError(f"unknown {cls.__name__} fields: {sorted(set(raw)-set(types))}")
    try:
        obj = cls(**raw)
    except TypeError as exc:
        raise ValueError(str(exc)) from exc
    for name, typ in types.items():
        v = getattr(obj, name)
        if typ is float:
            if isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or abs(v) > 1e15:
                raise ValueError(f"{cls.__name__}.{name} must be a finite number of magnitude <= 1e15")
        elif typ is int:
            if type(v) is not int or not 0 <= v <= 36500:
                raise ValueError(f"{name} must be an integer in [0, 36500]")
        elif typ is bool and type(v) is not bool:
            raise ValueError(f"{name} must be boolean")
        elif typ is str and (not isinstance(v, str) or len(v) > 120):
            raise ValueError(f"{name} must be a short string")
    return obj
