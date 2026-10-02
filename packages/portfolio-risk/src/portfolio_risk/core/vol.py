"""LMM vol structure: factor loadings, Rebonato abcd, shifted-Black swaption
approximation, surface calibration, deterministic vol-feature paths."""
from __future__ import annotations

from functools import lru_cache
from ..core.runtime import run_cached
import numpy as np
from scipy.optimize import least_squares

from ..core.config import CC_VOL_POINTS, N_FACTORS, N_FWD, N_STEPS, DT, SHIFT, TENOR


def factor_loadings() -> np.ndarray:
    """PCA-reduced exponential-decay correlation, rows unit-normalized."""
    from .quant_native import enabled
    return _factor_loadings(enabled())


@lru_cache(maxsize=2)
def _factor_loadings(native_backend) -> np.ndarray:
    if native_backend:
        from .quant_native import call
        B=call(24,[N_FWD,N_FACTORS,TENOR,0.10],[(N_FWD,N_FACTORS)])[0]
        B.setflags(write=False)
        return B
    T = (np.arange(N_FWD) + 1) * TENOR
    rho = np.exp(-0.10 * np.abs(T[:, None] - T[None, :]))
    w, V = np.linalg.eigh(rho)
    idx = np.argsort(w)[::-1][:N_FACTORS]
    B = V[:, idx] * np.sqrt(w[idx])[None, :]
    # Canonical orientation preserves the existing built-in factor/draw pairing
    # while removing LAPACK-dependent eigenvector signs from the run contract.
    desired=np.array([-1.,-1.,1.])[:N_FACTORS]
    B *= np.where(B[0] >= 0,desired,-desired)
    B /= np.linalg.norm(B, axis=1, keepdims=True)
    B = np.ascontiguousarray(B)
    B.setflags(write=False)
    return B


factor_loadings.cache_clear = _factor_loadings.cache_clear
factor_loadings.cache_info = _factor_loadings.cache_info


def abcd(tau, p):
    a, b, c, d = p
    return (a + b * tau) * np.exp(-c * tau) + d


def model_swaption_vol(t, expiry, tenor, p, F0, dfs, B) -> float:
    """Market-lognormal-equivalent ATM vol under shifted dynamics; weights
    frozen at the t=0 curve (Rebonato approximation)."""
    return model_swaption_value_jac(t,expiry,tenor,p,F0,dfs,B)[0]


def model_swaption_value_jac(t, expiry, tenor, p, F0, dfs, B):
    """Rebonato value and analytic abcd derivative; avoids differencing noise."""
    from . import quant_native as native
    if native.enabled():
        query=np.array([[t,expiry,round((t+expiry)/TENOR),round(tenor/TENOR)]])
        row=native.call(17,[query,p,F0,dfs,B,TENOR,SHIFT],[(1,5)])[0][0]
        return row[0],row[1:]
    i0 = int(round((t + expiry) / TENOR))
    nq = int(round(tenor / TENOR))
    i1 = min(i0 + nq, N_FWD)
    idx = np.arange(i0, i1)
    P = dfs[idx + 1]
    ann = TENOR * P.sum()
    S0 = (dfs[i0] - dfs[i1]) / ann
    aF = (TENOR * P / ann) * (F0[idx] + SHIFT) / S0
    grid = np.linspace(t, t + expiry, 21)
    tau = np.maximum(idx[None, :] * TENOR - grid[:, None], 1e-6)
    V = np.einsum("gn,n,nk->gk", abcd(tau, p), aF, B[idx])
    value=np.sqrt(np.trapezoid((V * V).sum(axis=1), grid) / expiry)
    ex=np.exp(-p[2]*tau)
    ds=np.stack([ex,tau*ex,-(p[0]+p[1]*tau)*tau*ex,np.ones_like(tau)])
    deriv=np.einsum('jgn,n,nk->jgk',ds,aF,B[idx])
    jac=np.trapezoid(np.einsum('gk,jgk->jg',V,deriv),grid,axis=1)/expiry/(value if value else 1.)
    return value,jac


@run_cached
def calibrate_abcd(vol_pts, F0, dfs, B, x0=None, quiet=False) -> np.ndarray:
    """vol_pts rows: (expiry_y, tenor_y, lognormal ATM vol). Warm-startable."""
    from . import quant_native as native
    if native.enabled():
        params, stats = native.call(25, [vol_pts, F0, dfs, B,
            [0.05, 0.10, 0.50, 0.12] if x0 is None else x0,
            [TENOR, SHIFT]], [(4,), (3,)])
        if not quiet:
            print(f"[cal] abcd = {np.round(params, 4)}  RMSE = {stats[0]*1e4:.1f} bp vol")
        return params
    def resid(p):
        return np.array([model_swaption_vol(0.0, e, n, p, F0, dfs, B) - v
                         for e, n, v in vol_pts])
    def jac(p):
        return np.array([model_swaption_value_jac(0.,e,n,p,F0,dfs,B)[1] for e,n,_ in vol_pts])
    sol = least_squares(
        resid, x0=np.array([0.05, 0.10, 0.50, 0.12]) if x0 is None else x0,
        bounds=([-0.5, -0.5, 0.01, 0.0], [1.0, 1.0, 5.0, 1.0]), jac=jac)
    if not quiet:
        print(f"[cal] abcd = {np.round(sol.x, 4)}  RMSE = "
              f"{np.sqrt(np.mean(sol.fun**2))*1e4:.1f} bp vol")
    return sol.x


@run_cached
def vol_feature_paths(p, F0, dfs, B) -> np.ndarray:
    """Deterministic forward-vol features (6, N_STEPS) for the CC model.
    Limitation: a deterministic-vol LMM has no stochastic implied vol; these
    are time-decay paths off the t=0 curve. SV-LMM needed for vol dynamics."""
    from . import quant_native as native
    if native.enabled():
        queries=[]; masks=[]
        for e,nten in CC_VOL_POINTS:
            nq=int(round(nten/TENOR));tg=np.arange(N_STEPS)*DT
            first=np.round((tg+e)/TENOR).astype(int);masks.append(first+nq<N_FWD)
            capped=np.minimum(first,N_FWD-nq-1)
            queries.extend(np.column_stack([tg,np.full(N_STEPS,e),capped,np.full(N_STEPS,nq)]))
        values=native.call(17,[np.asarray(queries),p,F0,dfs,B,TENOR,SHIFT],[(len(queries),5)])[0][:,0].reshape(len(CC_VOL_POINTS),N_STEPS)
        for row,valid in zip(values,masks): row[~valid]=row[valid][-1]
        return values
    out = np.empty((len(CC_VOL_POINTS), N_STEPS))
    tg = np.arange(N_STEPS) * DT
    for j, (e, nten) in enumerate(CC_VOL_POINTS):
        nq = int(round(nten / TENOR))
        i0 = np.round((tg + e) / TENOR).astype(int)
        valid = i0 + nq < N_FWD
        i0c = np.minimum(i0, N_FWD - nq - 1)
        idx = i0c[:, None] + np.arange(nq)[None, :]
        P = dfs[idx + 1]
        ann = TENOR * P.sum(axis=1)
        S0 = (dfs[i0c] - dfs[i0c + nq]) / ann
        aF = TENOR * P * (F0[idx] + SHIFT) / (ann * S0)[:, None]
        g = tg[:, None] + np.linspace(0.0, e, 21)[None, :]
        tau = np.maximum(idx[:, None, :] * TENOR - g[:, :, None], 1e-6)
        V = np.einsum("tgn,tn,tnk->tgk", abcd(tau, p), aF, B[idx])
        integ = np.trapezoid((V * V).sum(-1), g[0] - g[0, 0], axis=1)
        v = np.sqrt(integ / e)
        v[~valid] = v[valid][-1]
        out[j] = v
    return out
