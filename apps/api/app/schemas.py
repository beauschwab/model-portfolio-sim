"""Pydantic surface for the rates workbench API."""
from __future__ import annotations

from typing import Any, Literal
from pydantic import BaseModel, ConfigDict, Field, FiniteFloat, field_validator, model_validator
from datetime import date

BookName = Literal["mbs", "loans", "debt", "deposits", "cds", "mm"]


class Market(BaseModel):
    """Par swap curve (10 pillars) + ATM vol surface rows [expiry, tenor, vol]."""
    swap_tenors: list[FiniteFloat] = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30]
    swap_rates: list[FiniteFloat]
    vol_pts: list[list[FiniteFloat]] = Field(min_length=1, max_length=64)

    @model_validator(mode="after")
    def valid_market(self):
        if self.swap_tenors != [1, 2, 3, 4, 5, 7, 10, 15, 20, 30] or len(self.swap_rates) != 10:
            raise ValueError("expect the standard 10 curve pillars")
        if any(r <= -0.019 or r > 1 for r in self.swap_rates):
            raise ValueError("rates are outside the shifted model domain")
        if any(len(p) != 3 or min(p) <= 0 or p[0] + p[1] > 40 or p[2] > 5 for p in self.vol_pts):
            raise ValueError("vol rows require supported positive expiry, tenor and volatility")
        if len({(p[0], p[1]) for p in self.vol_pts}) != len(self.vol_pts):
            raise ValueError("duplicate volatility points")
        return self



class PythonBackendDeprecated(ValueError):
    """A historical/reference backend cannot execute through the application."""


class RiskSettings(BaseModel):
    # Accept historical snapshots for inspection; execution rejects Python below.
    compute_backend: Literal['python', 'rust'] = 'rust'
    n_paths: int = Field(128, ge=32, le=2048)
    n_paths_base: int = Field(512, ge=32, le=2048)
    n_threads: int = Field(0, ge=0, le=256)  # 0 = all available cores
    seed: int = Field(7, ge=0, le=2**32 - 203)
    horizon_months: int = Field(27, ge=3, le=120)
    shocks_bp: list[FiniteFloat] = Field(default=[-100, 100, 200, 300], min_length=1, max_length=16)

    def require_production_backend(self):
        if self.compute_backend != 'rust':
            raise PythonBackendDeprecated('PYTHON_BACKEND_DEPRECATED: select Rust in settings and rebuild libraries/sessions; Python is retained only as an independent engine test reference')

    @field_validator("n_threads")
    @classmethod
    def threads_available(cls, value):
        import numba
        if value > numba.config.NUMBA_NUM_THREADS:
            raise ValueError(f"at most {numba.config.NUMBA_NUM_THREADS} threads are available")
        return value

    @field_validator("shocks_bp")
    @classmethod
    def bounded_shocks(cls, value):
        if any(abs(v) > 2000 for v in value):
            raise ValueError("shocks must be within +/-2000 bp")
        return value



class AssumptionPatch(BaseModel):
    """Targeted model-assumption overrides (catalog at GET /assumptions)."""
    prepay: dict[str, FiniteFloat] | None = None
    deposit_segments: dict[str, dict[str, FiniteFloat]] | None = None
    cd_ew_params: list[FiniteFloat] | None = None


class MarketScenario(BaseModel):
    """Named 9Q market-path scenario in trader terms; each leg is a
    per-quarter list (<=9 values; last value extends to Q9)."""
    name: str = Field(min_length=1, max_length=100, pattern=r"^[A-Za-z0-9_-]+$")
    ust10y_bp: list[FiniteFloat] = Field(default_factory=list, max_length=9)
    twos_tens_bp: list[FiniteFloat] = Field(default_factory=list, max_length=9)
    spread_bp: list[FiniteFloat] = Field(default_factory=list, max_length=9)
    vol_bp: list[FiniteFloat] = Field(default_factory=list, max_length=9)

    @field_validator("ust10y_bp", "twos_tens_bp", "spread_bp", "vol_bp")
    @classmethod
    def bounded_legs(cls, values):
        if any(abs(v) > 2000 for v in values):
            raise ValueError("scenario legs must be within +/-2000 bp")
        return values


class ForecastRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    snapshot_id: str = Field(pattern=r"^[a-f0-9]{64}$")
    scenario: Literal["baseline", "adverse", "median"]
    start_period: date
    horizon_months: int = Field(27, ge=3, le=120)
    alignment: Literal["relative_replay"] = "relative_replay"
    expected_revision: int = Field(ge=0)


class Program(BaseModel):
    name: str = Field(min_length=1, max_length=100)
    product: str = "custom"
    side: Literal["asset", "liability"]
    rate_ref: Literal["short", "s2", "s5", "s10", "s30"]
    is_float: bool = False
    spread_bp: FiniteFloat = Field(ge=-2000, le=2000)
    term_m: int = Field(ge=1, le=360, strict=True)
    amort: Literal["bullet", "annuity", "cpr"] = "bullet"
    cpr_annual: FiniteFloat = Field(0.06, ge=0, lt=1)
    start_m: int = Field(ge=0, le=359, strict=True)
    end_m: int = Field(ge=0, le=359, strict=True)
    monthly_notional: FiniteFloat | None = Field(None, ge=0)
    reinvest_frac: FiniteFloat | None = Field(None, ge=0, le=1)
    reinvest_source: BookName | None = None

    @model_validator(mode="after")
    def valid_window_and_size(self):
        if self.end_m < self.start_m:
            raise ValueError("end_m must not precede start_m")
        if (self.monthly_notional is None) == (self.reinvest_frac is None):
            raise ValueError("provide monthly_notional or reinvest_frac, exclusively")
        if self.reinvest_frac is not None and self.reinvest_source is None:
            raise ValueError("reinvestment requires a source book")
        return self


class RunRequest(BaseModel):
    kind: Literal["risk", "stress", "nii", "deposit_stress", "kpis", "strategy", "unitlib", "pricing", "whatif"]
    books: list[BookName] | None = None
    scenario: str | None = None
    spread_overrides_bp: dict[BookName, dict[str, FiniteFloat]] = Field(default_factory=dict)
    assumption_overrides: dict[BookName, dict[str, dict[str, FiniteFloat]]] = Field(default_factory=dict)
    calibration_mode: Literal['hold', 'recalibrate'] = 'hold'
    include_analytics: bool = False
    backend: Literal['rust'] = 'rust'
    expected_revision: int | None = Field(None, ge=0)

    @model_validator(mode="after")
    def pricing_overrides(self):
        if self.spread_overrides_bp and self.kind not in ("pricing", "whatif"):
            raise ValueError("spread_overrides_bp is supported only for pricing/whatif runs")
        if (self.assumption_overrides or self.calibration_mode != 'hold') and self.kind != 'whatif':
            raise ValueError('temporary assumptions and recalibration require a whatif run')
        if self.include_analytics and self.kind not in ('pricing', 'whatif'):
            raise ValueError('pricing analytics/backend options require pricing or whatif')
        if any(abs(v) > 2000 for rows in self.spread_overrides_bp.values() for v in rows.values()):
            raise ValueError("instrument spread shifts must be within +/-2000 bp")
        if self.books is not None and len(set(self.books)) != len(self.books):
            raise ValueError("books must not contain duplicates")
        return self


class JobStatus(BaseModel):
    """Status/progress only -- the computed result is fetched separately as an
    Arrow envelope from GET /jobs/{id}/result. Extra keys in the JOBS dict
    (notably the raw `result`) are ignored by pydantic, so JobStatus(**job)
    never tries to JSON-encode the held DataFrames."""
    id: str
    kind: str
    status: Literal["queued", "running", "done", "error"]
    detail: str | None = None
    progress: dict[str, Any] | None = None
    revision: int = 0
    market_provenance: dict[str, Any] | None = None


class Allocation(BaseModel):
    template: str
    purchase_m: int = Field(ge=0, le=359, strict=True)
    notional: FiniteFloat = Field(ge=0)

    @field_validator("template")
    @classmethod
    def known_template(cls, value):
        from portfolio_risk.strategy.unitlib import TEMPLATES
        if value not in TEMPLATES:
            raise ValueError("unknown template")
        return value


class CommercialConstraint(BaseModel):
    label: str = Field(min_length=1, max_length=100)
    template: str
    sense: Literal[">=", "<="]
    rhs: FiniteFloat = Field(ge=0)

    @field_validator("template")
    @classmethod
    def known_template(cls, value):
        if value in ("ALL_ASSET", "ALL_LIAB"):
            return value
        return Allocation.known_template(value)


class OptimizeRequest(BaseModel):
    lcr_min: FiniteFloat = Field(1.10, gt=0, le=10)
    nsfr_min: FiniteFloat = Field(1.05, gt=0, le=10)
    cet1_min: FiniteFloat = Field(0.10, gt=0, lt=1)
    eve_limit: FiniteFloat = Field(0.15, gt=0, le=1)
    max_total_assets: FiniteFloat = Field(3e10, gt=0)
    cash_budget: FiniteFloat = Field(0, ge=0)
    commercial: list[CommercialConstraint] = Field(default_factory=list, max_length=100)
    capital_limits: list[dict] = Field(default_factory=list, max_length=2048)
    scenarios: list[str] = Field(default_factory=list, max_length=12)


class DecisionBuildRequest(BaseModel):
    expected_revision: int = Field(ge=0)
    options: OptimizeRequest = Field(default_factory=OptimizeRequest)


class DecisionUpdateRequest(BaseModel):
    expected_revision: int = Field(ge=0)
    version: int = Field(ge=0)
    edits: dict[str, dict[str, FiniteFloat | None]] = Field(default_factory=dict, max_length=10000)
    templates: dict[str, dict[str, FiniteFloat | None]] = Field(default_factory=dict, max_length=7)
    constraints: OptimizeRequest | None = None


class DecisionEvalRequest(BaseModel):
    expected_revision: int = Field(ge=0)
    version: int = Field(ge=0)
    allocation: list[Allocation] = Field(max_length=4096)


class BalanceStressRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: int = Field(ge=0)
    specification: dict[str, Any]

    @field_validator("specification")
    @classmethod
    def valid_stress_specification(cls, value):
        from portfolio_risk.analytics.balance_stress import validate
        validate(value)
        return value


class SavedBalanceStressRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    expected_revision: int = Field(ge=0)
    specification: dict[str, Any]
    position_mapping: dict[str, dict[str, Any]] = Field(max_length=2000)
    amount_scale: FiniteFloat = Field(gt=0, le=1e9)
    allocation: list[Allocation] = Field(default_factory=list, max_length=2000)
    template_mapping: dict[str, dict[str, Any]] = Field(default_factory=dict, max_length=100)
    observations: list[dict[str, Any]] = Field(default_factory=list, max_length=10000)
    calibration: dict[str, Any] | None = None


class StreamedBalanceStressRequest(BaseModel):
    model_config = ConfigDict(extra='forbid')
    expected_revision: int = Field(ge=0)
    specification: dict[str, Any]
    backend: Literal['rust'] = 'rust'
    large_book: bool = False

    @model_validator(mode='after')
    def valid_specification(self):
        from portfolio_risk.analytics.balance_stress import validate
        validate(self.specification, max_positions=60000 if self.large_book else 2000,
                 max_work=90_000_000 if self.large_book else 3_000_000)
        return self
