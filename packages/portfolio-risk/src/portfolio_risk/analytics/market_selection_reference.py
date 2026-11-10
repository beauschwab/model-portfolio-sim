"""Independent training-only stress-row selection verification.

Never returns a production scenario or calls the native selector. Publication
must also verify the supplied mean/covariance against the original training rows.
"""
import math

import numpy as np


def verify(raw, diagnostics, mean, covariance, check=lambda: False):
    def guard():
        if check():
            raise InterruptedError('market-driver selection verification cancelled')

    def require(condition, detail):
        if not condition:
            raise ValueError('market-driver selected scenarios ' + detail)

    guard()
    count = raw.get('scenario_count')
    require(count is None or type(count) is int and 1 <= count <= 8, 'count domain')
    selected = diagnostics.get('selected')
    require(type(selected) is list and len(selected) == (count or 0), 'count differs from request')
    dates, values = raw['dates'], raw['values']
    require(len(dates) == len(values) <= 1_000_000, 'source dimensions')
    n = sum(date <= raw['training_end'] for date in dates)
    require(n >= 30 and len(dates) - n >= 10, 'chronological split')
    require(all(type(i) is int and 0 <= i < n for i in selected)
            and len(set(selected)) == len(selected), 'unique training indices required')
    if count is None:
        guard()
        return
    center = np.asarray(mean, dtype=float)
    cov = np.asarray(covariance, dtype=float)
    require(center.shape == (4,) and cov.shape == (4, 4)
            and np.isfinite(center).all() and np.isfinite(cov).all(), 'moment domain')
    # Independent direct SVD; retain the native contract's relative singular
    # cutoff. No normal equations, regularization, or holdout observations.
    u, singular, vt = np.linalg.svd(cov)
    guard()
    cutoff = singular[0] * 1e-15
    inverse_values = np.array([1 / s if s > cutoff else 0. for s in singular])
    inverse = (vt.T * inverse_values) @ u.T
    scores = []
    for start in range(0, n, 1024):
        guard()
        block = np.asarray(values[start:min(start + 1024, n)], dtype=float)
        require(block.ndim == 2 and block.shape[1] == 4 and np.isfinite(block).all(),
                'training values')
        delta = block - center
        scores.extend(np.sum((delta @ inverse.T) * delta, axis=1).tolist())
    require(all(math.isfinite(score) for score in scores), 'distance domain')

    def ordered(left, right):
        a, b = scores[left], scores[right]
        # Independent decompositions can differ at floating point ties. Keep
        # the existing 3e-12 numerical comparison; exact duplicate observations
        # and the zero operator additionally retain native reverse-index ties.
        require(a >= b or math.isclose(a, b, rel_tol=3e-12, abs_tol=3e-12),
                'Mahalanobis ranking differs from training source')
        if (values[left] == values[right] or singular[0] == 0.) and a == b:
            require(left > right, 'equal-distance observation tie order differs')

    for left, right in zip(selected, selected[1:]):
        ordered(left, right)
    selected_set = set(selected)
    for i in range(n):
        if i % 1024 == 0:
            guard()
        if i not in selected_set:
            ordered(selected[-1], i)
    for i in selected:
        a, b, c, d = values[i]
        require(-.5 <= a <= .5 and -.5 <= b <= .5 and 0 <= c <= 1 and 0 <= d <= 20,
                'stress specification domain')
    guard()
