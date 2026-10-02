"""Auditable empirical joint-shock estimation with chronological holdout.

This estimates driver moments, not instrument PD/LGD or regulatory parameters.
Activation remains an explicit reviewed specification change.
"""
import hashlib
import numpy as np
import polars as pl

DRIVERS = ('rate_shift', 'spread_shift', 'deposit_flight', 'market_shock')


def _native_fit(history,training_end,count=None):
    from datetime import date
    from ..core.quant_native import term_call
    if set(history.columns)!={'date',*DRIVERS}:raise ValueError('dated driver observations are required')
    def ordinal(value):return value.toordinal() if isinstance(value,date) else date.fromisoformat(value).toordinal()
    return term_call('financial-controller-1',dict(op='driver_fit',dates=[ordinal(v) for v in history['date']],
        values=history.select(DRIVERS).rows(),training_end=ordinal(training_end),count=count))


def fit_joint_drivers(history: pl.DataFrame, training_end, source: str):
    if not source.strip() or set(history.columns) != {'date', *DRIVERS}:
        raise ValueError('dated driver observations and explicit source are required')
    from ..core.quant_native import enabled
    if enabled():
        output=_native_fit(history,training_end);output.pop('selected')
        return dict(version='empirical-joint-drivers-1',source=source,
            source_sha256=hashlib.sha256(history.write_json().encode()).hexdigest(),training_end=str(training_end),
            drivers=list(DRIVERS),**output,calibrated_parameters='joint driver first and second moments only',production_validated=False)
    if history['date'].n_unique() != history.height or not history['date'].is_sorted():
        raise ValueError('history must have unique increasing dates')
    train = history.filter(pl.col('date') <= training_end)
    holdout = history.filter(pl.col('date') > training_end)
    if train.height < 30 or holdout.height < 10:
        raise ValueError('at least 30 training and 10 subsequent holdout observations required')
    values = history.select(DRIVERS).to_numpy().astype(float)
    if not np.isfinite(values).all():
        raise ValueError('driver observations must be finite and complete')
    x, held = train.select(DRIVERS).to_numpy(), holdout.select(DRIVERS).to_numpy()
    mean, covariance = x.mean(0), np.cov(x, rowvar=False)
    scale = np.sqrt(np.maximum(np.diag(covariance), 1e-16))
    z = (held-mean)/scale
    return dict(version='empirical-joint-drivers-1', source=source,
        source_sha256=hashlib.sha256(history.write_json().encode()).hexdigest(),
        training_end=str(training_end), training_rows=train.height, holdout_rows=holdout.height,
        drivers=list(DRIVERS), mean=mean.tolist(), covariance=covariance.tolist(),
        holdout_bias=(held.mean(0)-mean).tolist(), holdout_rmse=np.sqrt(((held-mean)**2).mean(0)).tolist(),
        holdout_outside_three_sigma=(np.abs(z)>3).mean(0).tolist(),
        calibrated_parameters='joint driver first and second moments only', production_validated=False)


def empirical_joint_scenarios(history: pl.DataFrame, training_end, count=4):
    """Select observed joint rows; never independently combine marginal tails.

    Fit uses training rows only. Selection is by standardized joint distance,
    not a claimed risk probability or an optimized minimum-failure scenario.
    """
    if type(count) is not int or not 1 <= count <= 8:
        raise ValueError('scenario count must be in [1,8]')
    from ..core.quant_native import enabled
    if enabled():
        selected=_native_fit(history,training_end,count)['selected']
        return [dict(name='observed_'+str(history['date'][i]),**{k:float(history[k][i]) for k in DRIVERS}) for i in selected]
    fitted = fit_joint_drivers(history, training_end, 'supplied training history')
    train = history.filter(pl.col('date') <= training_end)
    x = train.select(DRIVERS).to_numpy()
    mean, cov = np.asarray(fitted['mean']), np.asarray(fitted['covariance'])
    inv = np.linalg.pinv(cov)
    distance = np.einsum('ni,ij,nj->n', x-mean, inv, x-mean)
    rows = []
    for i in np.argsort(distance,kind='stable')[-count:][::-1]:
        r = train.row(int(i), named=True)
        scenario = dict(name='observed_'+str(r['date']), **{k: float(r[k]) for k in DRIVERS})
        if not (-.5 <= scenario['rate_shift'] <= .5 and -.5 <= scenario['spread_shift'] <= .5
                and 0 <= scenario['deposit_flight'] <= 1 and 0 <= scenario['market_shock'] <= 20):
            raise ValueError('observed drivers exceed the stress specification domain')
        rows.append(scenario)
    return rows
