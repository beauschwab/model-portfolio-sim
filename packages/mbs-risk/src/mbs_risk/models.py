"""Econometric models: trending current coupon (partial adjustment),
primary/secondary OU spread, HPI process. Fitters take Polars frames of
monthly history; path builders consume CRN shocks."""
from __future__ import annotations

import numpy as np
import polars as pl

from .config import DT, HPI_BETA, HPI_MU, HPI_SIG

CC_FEATURES = ["s2", "s5", "s10", "s30", "v0", "v1", "v2", "v3", "v4", "v5"]


# --- trending current coupon -------------------------------------------------
def fit_current_coupon(hist: pl.DataFrame) -> dict:
    """Fair value: OLS cc ~ [4 swaps, 6 swaption vols, 1].
    Trend speed lambda from Delta(cc) ~ lambda * (fair - cc_lag)."""
    X = np.column_stack([hist[c].to_numpy() for c in CC_FEATURES]
                        + [np.ones(len(hist))])
    y = hist["cc"].to_numpy()
    beta, *_ = np.linalg.lstsq(X, y, rcond=None)
    fair = X @ beta
    dcc, gap = np.diff(y), fair[1:] - y[:-1]
    lam = float(np.clip((gap @ dcc) / (gap @ gap), 0.02, 1.0))
    print(f"[cc] lambda = {lam:.3f}  R2 = {1 - (y-fair).var()/y.var():.3f}")
    return {"beta": beta, "lam": lam}


def cc_paths(swaps: np.ndarray, volfeat: np.ndarray, cc_model: dict
             ) -> np.ndarray:
    """swaps (P,4,T), volfeat (6,T) -> secondary CC (P,T) via
    cc_t = cc_{t-1} + lambda (fair_t - cc_{t-1})."""
    P, _, T = swaps.shape
    beta, lam = cc_model["beta"], cc_model["lam"]
    fair = (np.einsum("pft,f->pt", swaps, beta[:4])
            + volfeat.T @ beta[4:10] + beta[10])
    cc = np.empty((P, T))
    cc[:, 0] = fair[:, 0]
    for m in range(1, T):
        cc[:, m] = cc[:, m - 1] + lam * (fair[:, m] - cc[:, m - 1])
    return cc


# --- primary/secondary spread (OU via AR(1)) ----------------------------------
def fit_ps_spread(hist: pl.DataFrame) -> dict:
    x = hist["ps"].to_numpy()
    X = np.column_stack([x[:-1], np.ones(len(x) - 1)])
    (phi, c), *_ = np.linalg.lstsq(X, x[1:], rcond=None)
    phi = float(np.clip(phi, 0.0, 0.9995))
    out = {"kappa": (1 - phi) / DT, "theta": float(c / (1 - phi)),
           "sigma": float(np.std(x[1:] - (phi * x[:-1] + c)) / np.sqrt(DT))}
    print(f"[ps] kappa = {out['kappa']:.2f}  theta = {out['theta']*1e4:.0f}bp"
          f"  sigma = {out['sigma']*1e4:.0f}bp/sqrt(y)")
    return out


def ps_paths(ps_model: dict, ps_spot: float, eps: np.ndarray) -> np.ndarray:
    k, th, sg = ps_model["kappa"], ps_model["theta"], ps_model["sigma"]
    P, T = eps.shape
    ps = np.empty((P, T))
    x = np.full(P, ps_spot)
    for m in range(T):
        x = x + k * (th - x) * DT + sg * np.sqrt(DT) * eps[:, m]
        ps[:, m] = np.maximum(x, 0.0)
    return ps


# --- HPI ------------------------------------------------------------------------
def hpi_paths(s10: np.ndarray, eps: np.ndarray) -> np.ndarray:
    """Lognormal HPI, drift linked to the 10y rate (clipped sensitivity)."""
    dr = np.clip(s10 - s10[:, :1], -0.05, 0.05)
    dlog = (HPI_MU + HPI_BETA * dr) * DT + HPI_SIG * np.sqrt(DT) * eps
    return np.exp(np.clip(np.cumsum(dlog, axis=1), -3.0, 3.0))


def yoy_from_hpi(H: np.ndarray) -> np.ndarray:
    P, T = H.shape
    Hpad = np.concatenate([np.ones((P, 12)), H], axis=1)
    return H / Hpad[:, :T] - 1.0


# --- registered class wrappers (interfaces.ModelSuite components) -------------
from .interfaces import register


@register("cc", "trending_cc")
class TrendingCC:
    """Partial-adjustment CC on [4 swaps, 6 vols]. Default."""
    def fit(self, hist):
        return fit_current_coupon(hist)

    def paths(self, swaps, volfeat, params):
        return cc_paths(swaps, volfeat, params)

    def shock_response(self, params, d, k):
        lam = params["lam"]
        K = d * params["beta"][:4].sum()
        return np.where(k > 0, K * (1.0 - (1.0 - lam) ** k), 0.0)


@register("ps", "ou_ps")
class OUSpread:
    """OU primary/secondary spread, AR(1) fit. Default."""
    def fit(self, hist):
        return fit_ps_spread(hist)

    def paths(self, params, spot, eps):
        return ps_paths(params, spot, eps)


@register("hpi", "rate_linked_hpi")
class RateLinkedHPI:
    """Lognormal HPI with 10y-rate-linked drift. Default."""
    def paths(self, s10, eps):
        return hpi_paths(s10, eps)

    def shock_multiplier(self, d, k):
        return np.where(k > 0, np.exp(HPI_BETA * d * DT * k), 1.0)
