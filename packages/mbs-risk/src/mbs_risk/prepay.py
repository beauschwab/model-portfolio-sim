"""Prepay model DATA: spline anchors, LUTs, categorical multipliers.
All anchors here are stylized shapes -- replace with loan-level fits; they
are plain (x, y) anchors -> natural cubic, so any offline fit drops in.
Kernel-side evaluation lives in kernels.py."""
from __future__ import annotations

import numpy as np
import polars as pl
from scipy.interpolate import CubicSpline

from .config import PREPAY_PARAMS


def nat_spline(x: np.ndarray, y: np.ndarray):
    """-> (knots, flat interval-major poly coefs) for in-kernel evaluation."""
    cs = CubicSpline(x, y, bc_type="natural")
    return x.astype(np.float64), cs.c.T.reshape(-1).astype(np.float64)


# --- in-kernel LTV refi-availability spline ------------------------------------
LTV_KNOTS, LTV_COEFS = nat_spline(
    np.array([0.30, 0.50, 0.70, 0.80, 0.90, 1.00, 1.20]),
    np.array([1.05, 1.05, 1.00, 0.95, 0.80, 0.55, 0.30]))

# --- static (per-security) multiplier anchors -----------------------------------
FICO_X = np.array([580.0, 640.0, 680.0, 720.0, 760.0, 800.0])
FICO_Y = np.array([0.60, 0.75, 0.90, 1.00, 1.10, 1.15])
SIZE_X = np.array([50e3, 100e3, 175e3, 250e3, 400e3, 600e3])
SIZE_Y = np.array([0.55, 0.75, 0.92, 1.00, 1.12, 1.18])
STATE_MULT = {"CA": 1.10, "NY": 0.85, "TX": 0.95, "FL": 1.00, "PR": 0.70}
CHANNEL_MULT = {"R": 1.00, "B": 1.15, "C": 1.05}

# --- LUTs: exact-to-interp replacements for in-loop transcendentals ------------
_SMM_N, _SMM_MAX = 4097, 0.96
SMM_LUT = 1.0 - (1.0 - np.linspace(0.0, _SMM_MAX, _SMM_N)) ** (1.0 / 12.0)
SMM_SCALE = (_SMM_N - 1) / _SMM_MAX

_BRN_N, _BRN_MAX = 2049, 0.08
BURN_LUT = np.exp(-PREPAY_PARAMS[3] * np.linspace(0.0, _BRN_MAX, _BRN_N))
BURN_SCALE = (_BRN_N - 1) / _BRN_MAX


def static_multipliers(port: pl.DataFrame) -> np.ndarray:
    """FICO spline x loan-size spline x state x channel, per security."""
    fico_s = CubicSpline(FICO_X, FICO_Y, bc_type="natural")
    size_s = CubicSpline(SIZE_X, SIZE_Y, bc_type="natural")
    f = np.clip(port["fico"].to_numpy(), FICO_X[0], FICO_X[-1])
    s = np.clip(port["avg_loan_size"].to_numpy(), SIZE_X[0], SIZE_X[-1])
    st = np.array([STATE_MULT.get(x, 1.0) for x in port["state"].to_list()])
    ch = np.array([CHANNEL_MULT.get(x, 1.0) for x in port["channel"].to_list()])
    return fico_s(f) * size_s(s) * st * ch
