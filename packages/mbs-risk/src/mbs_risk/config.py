"""Global configuration. Everything tunable lives here.

Note: numba kernels freeze module-level constants at first compile. Changing
values at runtime requires re-import (or pass-through params, which is why
prepay parameters travel as a vector, not constants).
"""
from __future__ import annotations

import numpy as np

# --- simulation -------------------------------------------------------------
N_PATHS_BASE = 512          # base run (OAS solve)
N_PATHS_SENS = 128          # scenario/stress runs (CRN differences)
N_STEPS   = 360             # monthly, 30y
DT        = 1.0 / 12.0
TENOR     = 0.25            # quarterly forwards
N_FWD     = 161             # to 40.25y (10y swap defined at t=30y)
N_FACTORS = 3
SHIFT     = 0.02            # shifted-lognormal displacement
INC_LAG   = 2               # months, incentive lag
SEED      = 7
SETTLE_MONTH = 6

# --- numerics ----------------------------------------------------------------
USE_FLOAT32      = True     # path-array storage dtype (scalar math stays f64)
RATIONAL_SIGMOID = True     # Pade(7,6) logistics; False -> exact exp
ADT = np.float32 if USE_FLOAT32 else np.float64

# --- risk --------------------------------------------------------------------
CURVE_BUMP = 0.0001         # +/- 1bp per par pillar
VOL_BUMP   = 0.0025         # +/- 25 lognormal vol bp per surface point
SWAP_TENORS = np.array([1, 2, 3, 4, 5, 7, 10, 15, 20, 30], dtype=float)
CAL_EXPIRIES = (1.0, 3.0, 5.0)
CAL_TENORS   = (2.0, 5.0, 10.0)

# --- stress capital (9Q, monthly horizons, instantaneous parallel shocks) ---
STRESS_HORIZONS_M = np.arange(1, 28, dtype=np.int64)
STRESS_SHOCKS_BP  = (-100.0, 100.0, 200.0, 300.0)

# --- current coupon feature set ----------------------------------------------
CC_VOL_POINTS = ((1.0, 10.0), (2.0, 10.0), (5.0, 10.0),
                 (1.0, 5.0),  (3.0, 7.0),  (5.0, 5.0))

# --- prepay parameter vector (tunable without numba recompile) ---------------
# [refi_max, refi_a, refi_b, burn_k, turnover_cpr, cpr_cap,
#  hpa_beta, lock_floor, lock_slope]
PREPAY_PARAMS = np.array([0.45, -2.2, 600.0, 4.0, 0.06, 0.95,
                          1.5, 0.55, 250.0])
SEASONALITY = np.array([0.75, 0.78, 0.92, 1.05, 1.15, 1.22,
                        1.25, 1.20, 1.08, 0.98, 0.85, 0.77])

# --- HPI process ---------------------------------------------------------------
HPI_MU, HPI_BETA, HPI_SIG = 0.035, -1.2, 0.05

# --- derived grids -------------------------------------------------------------
MOY = ((SETTLE_MONTH - 1 + np.arange(N_STEPS)) % 12).astype(np.int64)
TGRID = (np.arange(N_STEPS) + 1.0) / 12.0
