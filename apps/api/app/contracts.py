"""Storage and execution contracts; dictionaries keep the existing wire layout.

Snapshot money uses each book's declared monetary unit; rates are decimals,
shocks are basis points, and horizons/purchase dates are integer months. Arrow
frames and ndarray coefficients remain typed artifacts, not JSON row copies.
Historical snapshots may omit tapes/publications and name the Python backend.
Reading them never authorizes production execution or changes saved economics.
"""
from __future__ import annotations

from datetime import date
from typing import Any, NotRequired, Protocol, TypedDict, TYPE_CHECKING

if TYPE_CHECKING:
    import polars as pl
    from .schemas import RiskSettings, MarketScenario
    from portfolio_risk.strategy.contracts import PreparedLibrary


class QueueFull(RuntimeError):
    """The bounded compute queue cannot accept another job."""


class WorkspaceSnapshot(TypedDict):
    books: dict[str, pl.DataFrame]
    market: dict[str, Any]
    scenarios: dict[str, MarketScenario]
    settings: RiskSettings
    dep_hist: pl.DataFrame | None
    mbs_hists: Any
    asof: date | None
    equity: float
    hedges: pl.DataFrame | None
    programs: dict[str, dict]
    assumptions: dict[str, Any]
    revision: int
    tapes: NotRequired[dict[str, dict]]
    cohort_publications: NotRequired[dict[str, dict]]


class PreparedLibraryBundle(TypedDict, total=False):
    """Cold/invalidated cache is empty; publication writes all keys atomically."""
    revision: int
    library: PreparedLibrary
    base: dict[str, Any]
    compute_backend: str


class ModelIdentity(TypedDict):
    engine_version: str
    source_sha256: str
    python: str
    platform: str
    machine: str
    native_sha256: str | None
    risk_native_sha256: str | None
    balance_native_sha256: str | None
    workflow_native_sha256: str | None
    packages: dict[str, str]


class SavedLibrary(TypedDict):
    identity: ModelIdentity
    cache: PreparedLibraryBundle


class DurableRequest(TypedDict):
    operation: str
    args: tuple
    state: WorkspaceSnapshot
    identity: ModelIdentity


class WorkspacePort(Protocol):
    """Only materialization services needed by durable storage; no pricers."""
    _LOCK: Any
    STATE_META: dict[str, int]
    MAX_QUEUE: int

    def snapshot_local(self) -> WorkspaceSnapshot: ...
    def restore_snapshot(self, state: WorkspaceSnapshot) -> None: ...
    def seed_demo(self) -> None: ...
