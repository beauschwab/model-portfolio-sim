"""Pydantic surface for the rates workbench API."""
from __future__ import annotations

from typing import Any, Literal
from pydantic import BaseModel, Field

BookName = Literal["mbs", "loans", "debt", "deposits", "cds", "mm"]


class Market(BaseModel):
    """Par swap curve (10 pillars) + ATM vol surface rows [expiry, tenor, vol]."""
    swap_tenors: list[float] = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30]
    swap_rates: list[float]
    vol_pts: list[list[float]]


class RiskSettings(BaseModel):
    n_paths: int = Field(128, ge=32, le=2048)
    n_threads: int = Field(0, ge=0, le=256)  # 0 = all available cores
    seed: int = 7
    horizon_months: int = Field(27, ge=3, le=120)
    shocks_bp: list[float] = [-100, 100, 200, 300]


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
    id: str
    kind: str
    status: Literal["queued", "running", "done", "error"]
    detail: str | None = None
    result: Any | None = None
    progress: dict[str, Any] | None = None
