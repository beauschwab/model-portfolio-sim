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


class RiskSettings(BaseModel):
    n_paths: int = Field(128, ge=32, le=2048)
    n_paths_base: int = Field(512, ge=32, le=2048)
    n_threads: int = Field(0, ge=0, le=256)  # 0 = all available cores
    seed: int = Field(7, ge=0, le=2**32 - 203)
    horizon_months: int = Field(27, ge=3, le=120)
    shocks_bp: list[FiniteFloat] = Field(default=[-100, 100, 200, 300], min_length=1, max_length=16)

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
    prepay: dict[str, float] | None = None
    deposit_segments: dict[str, dict[str, float]] | None = None
    cd_ew_params: list[float] | None = None


class MarketScenario(BaseModel):
    """Named 9Q market-path scenario in trader terms; each leg is a
    per-quarter list (<=9 values; last value extends to Q9)."""
    name: str
    ust10y_bp: list[float] = []
    twos_tens_bp: list[float] = []
    spread_bp: list[float] = []
    vol_bp: list[float] = []


class RunRequest(BaseModel):
    kind: Literal["risk", "stress", "nii", "deposit_stress", "kpis", "strategy", "unitlib"]
    books: list[BookName] | None = None
    scenario: str | None = None


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


class ForecastRequest(BaseModel):
    model_config = ConfigDict(extra="forbid")
    snapshot_id: str = Field(pattern=r"^[a-f0-9]{64}$")
    scenario: Literal["baseline", "adverse", "median"]
    start_period: date
    horizon_months: int = Field(27, ge=3, le=120)
    alignment: Literal["relative_replay"] = "relative_replay"
    expected_revision: int = Field(ge=0)
