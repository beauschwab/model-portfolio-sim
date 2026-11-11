"""Independent raw covariance/CRN verification for the bounded Gaussian tape.

This is publication verification, never a selectable financial runtime. NumPy
owns the independent eigensolve and PCG64 replay; no native callback is used.
"""
import math

import numpy as np


def verify(raw, actual, check=lambda: False):
    def guard():
        if check():
            raise InterruptedError('joint market tape verification cancelled')

    def require(condition, name):
        if not condition:
            raise ValueError('joint market tape ' + name)

    def close(a, b, name):
        a, b = np.asarray(a, dtype=float), np.asarray(b, dtype=float)
        require(a.shape == b.shape and np.isfinite(a).all() and np.isfinite(b).all()
                and np.allclose(a, b, rtol=3e-12, atol=3e-12), name + ' differs from raw replay')

    def numbers(values, size, name):
        require(type(values) is list and len(values) == size, name + ' shape')
        require(all(type(v) in (int, float) and math.isfinite(v) for v in values), name + ' values')
        return np.asarray(values, dtype=float)

    guard()
    drivers, durations = raw['drivers'], raw['durations_years']
    n, steps, paths, k = len(drivers), len(durations), raw['paths'], raw['retained_factors']
    require(type(paths) is int and 1 <= paths <= 100_000 and 1 <= n <= 16
            and 1 <= steps <= 4096 and type(k) is int and 1 <= k <= n, 'dimensions')
    count = paths * steps * n
    require(count <= 1_000_000 and count * n <= 8_000_000, 'work admission')
    dt = numbers(durations, steps, 'durations')
    require(np.all((dt >= 1e-12) & (dt <= 100)) and sum(durations) <= 100, 'duration domain')
    require(type(actual.get('retained_factors')) is int and actual['retained_factors'] == k,
            'retained factor identity')
    covariance = numbers(raw['annual_covariance'], n * n, 'raw covariance').reshape(n, n)
    require(np.array_equal(covariance, covariance.T) and np.max(np.abs(covariance)) <= 4,
            'raw covariance symmetry/domain')
    variance = np.diag(covariance)
    require(np.all(variance >= 0), 'raw variance domain')
    scale = np.sqrt(variance)
    correlation = np.zeros((n, n))
    bound_adjustment = 0.
    for i in range(n):
        for j in range(n):
            if scale[i] == 0 or scale[j] == 0:
                require(covariance[i, j] == 0, 'zero variance covariance')
            else:
                rho = 1. if i == j else (covariance[i, j] / scale[i]) / scale[j]
                require(abs(rho) <= 1 + 4 * np.finfo(float).eps, 'correlation domain')
                bound_adjustment = max(bound_adjustment, abs(rho) - 1)
                correlation[i, j] = max(-1., min(1., rho))
    guard()
    eigenvalues, eigenvectors = np.linalg.eigh(correlation)
    tolerance = 8 * np.finfo(float).eps * n
    require(eigenvalues[0] >= -tolerance, 'raw covariance is not PSD')
    order = np.argsort(-eigenvalues, kind='stable')
    descending = eigenvalues[order]
    diagnostics = actual['diagnostics']
    close(numbers(diagnostics['correlation_eigenvalues_descending'], n, 'eigenvalues'),
          descending, 'raw covariance eigenvalues')
    close(diagnostics['roundoff_negative_eigenvalue'], min(0., eigenvalues[0]), 'negative eigenvalue')
    close(diagnostics['roundoff_correlation_bound_adjustment'], bound_adjustment, 'correlation adjustment')

    retained = numbers(diagnostics['retained_annual_covariance'], n * n,
                       'retained covariance').reshape(n, n)
    # At a tied truncation boundary, the historical contract does not prescribe
    # a platform-independent eigenspace basis. Certify the reported subspace
    # against the raw operator and top-k spectrum, then use its unique PSD root.
    tied = k < n and abs(descending[k - 1] - descending[k]) <= tolerance
    if tied:
        reduced = np.zeros((n, n))
        for i in range(n):
            for j in range(n):
                if scale[i] == 0 or scale[j] == 0:
                    require(retained[i, j] == 0, 'retained zero variance covariance')
                else:
                    reduced[i, j] = (retained[i, j] / scale[i]) / scale[j]
        close(reduced, reduced.T, 'retained symmetry')
        values, vectors = np.linalg.eigh(reduced)
        expected = np.sort(np.r_[np.maximum(descending[:k], 0.), np.zeros(n - k)])
        close(values, expected, 'top-k spectrum')
        close(correlation @ reduced, reduced @ reduced, 'raw invariant subspace')
        close(correlation @ reduced, reduced @ correlation, 'raw commuting subspace')
        root = (vectors * np.sqrt(np.maximum(values, 0.))) @ vectors.T
    else:
        selected = order[:k]
        root = (eigenvectors[:, selected] * np.sqrt(np.maximum(eigenvalues[selected], 0.))) @ eigenvectors[:, selected].T
    factor = scale[:, None] * root
    close(retained, factor @ factor.T, 'retained raw covariance')
    close(diagnostics['discarded_covariance_frobenius'],
          np.linalg.norm(covariance - retained), 'discarded covariance')
    guard()

    draws = raw['draws']
    if draws['kind'] == 'native_seed':
        seed = draws['seed']
        require(type(seed) is list and 1 <= len(seed) <= 16 and
                all(type(x) is int and 0 <= x <= 0xffffffff for x in seed), 'seed domain')
        # Native entropy words encode one little-endian unsigned integer. Its
        # rate tape consists of ceil(paths/2) draws followed by their negatives.
        entropy = sum(word << (32 * i) for i, word in enumerate(seed))
        half = (paths + 1) // 2
        first = np.random.Generator(np.random.PCG64(entropy)).standard_normal((half, steps, n))
        z = np.concatenate((first, -first[:paths - half]), axis=0)
    else:
        require(draws['kind'] == 'supplied_standard_normals', 'unknown draw source')
        z = numbers(draws['values'], count, 'supplied draws').reshape(paths, steps, n)
        require(np.max(np.abs(z)) <= 100, 'supplied draw domain')
    increments = actual.get('increments')
    require(type(increments) is list and len(increments) == count, 'increment shape')
    if np.min(np.abs(eigenvalues)) <= tolerance and np.any(scale > 0):
        # At a numerically null eigenvalue, independent eigensolvers can differ
        # by sqrt(epsilon) after taking the square root. Do not relax the final
        # cash/path comparison. Recover a candidate symmetric transformation
        # from <=n pivoted raw draw vectors, then independently certify its PSD
        # and squared-operator identities against the source covariance. Every
        # tape cell still has to reproduce from the caller's original draws.
        flat = z.reshape(paths * steps, n)
        residual = flat.copy()
        selected = []
        for _ in range(n):
            guard()
            norms = np.linalg.norm(residual, axis=1)
            pivot = int(np.argmax(norms))
            if norms[pivot] <= tolerance * max(1., float(np.linalg.norm(flat[pivot]))):
                break
            selected.append(pivot)
            unit = residual[pivot] / norms[pivot]
            residual -= (residual @ unit)[:, None] * unit
        if selected:
            x = flat[selected].T
            y = np.empty_like(x)
            for column, sample in enumerate(selected):
                guard()
                row = numbers(increments[sample * n:(sample + 1) * n], n, 'pivot increments')
                for driver in range(n):
                    if scale[driver] == 0:
                        require(row[driver] == 0, 'zero variance increment')
                        y[driver, column] = (root @ x[:, column])[driver]
                    else:
                        y[driver, column] = row[driver] / math.sqrt(durations[sample % steps]) / scale[driver]
            null = eigenvectors[:, np.abs(eigenvalues) <= tolerance]
            positive_root = root - null @ (null.T @ root @ null) @ null.T
            a, b = null.T @ x, null.T @ (y - positive_root @ x)
            u, singular, vt = np.linalg.svd(a, full_matrices=True)
            rank = int(np.sum(singular > tolerance * max(1., singular[0])))
            completion = np.zeros((null.shape[1], null.shape[1]))
            if rank:
                # Solve the small rectangular secant problem directly; forming
                # normal equations would amplify roundoff in these null modes.
                action = (b @ vt[:rank].T) / singular[:rank]
                leading = u[:, :rank].T @ action
                close(leading, leading.T, 'null-space secant symmetry')
                leading = (leading + leading.T) / 2
                cross = u[:, rank:].T @ action
                values, vectors = np.linalg.eigh(leading)
                require(values[0] >= -tolerance, 'null-space secant PSD')
                inverse_values = np.array([1 / v if v > tolerance else 0. for v in values])
                inverse = (vectors * inverse_values) @ vectors.T
                # Minimum PSD completion on directions absent from the draw
                # witnesses. Final source identities and all draws still gate it.
                tail = cross @ inverse @ cross.T
                completion = u @ np.block([[leading, cross.T], [cross, tail]]) @ u.T
            require(np.linalg.eigvalsh(completion)[0] >= -tolerance, 'null-space root PSD')
            small = np.maximum(eigenvalues[np.abs(eigenvalues) <= tolerance], 0.)
            require(np.linalg.norm(completion @ completion - np.diag(small), ord=2) <= tolerance,
                    'null-space covariance roundoff bound')
            candidate = positive_root + null @ completion @ null.T
            close(candidate, candidate.T, 'certified root symmetry')
            require(np.linalg.eigvalsh(candidate)[0] >= -tolerance, 'certified root PSD')
            close(candidate @ candidate, root @ root, 'certified raw square root')
            candidate_factor = scale[:, None] * candidate
            close(candidate_factor @ candidate_factor.T, retained, 'certified retained covariance')
            factor = candidate_factor
    # Check bounded blocks and every scalar before accepting downstream hazards.
    for path in range(paths):
        guard()
        for step in range(steps):
            if step % 128 == 0:
                guard()
            offset = (path * steps + step) * n
            expected = factor @ z[path, step] * math.sqrt(durations[step])
            close(numbers(increments[offset:offset + n], n, 'increments'), expected,
                  'source covariance/draw increments')
    guard()
    return dict(raw_covariance_verified=True, raw_draws_replayed=True,
                truncation_subspace_certified=True)
