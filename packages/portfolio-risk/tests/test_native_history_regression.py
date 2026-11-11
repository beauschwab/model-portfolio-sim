"""Independent public ABI regression and original numerical-tolerance controls."""
import numpy as np
import pytest

from portfolio_risk.core.quant_native import call


def coupon_history(n, shape):
    x = np.arange(n, dtype=np.float64) - (n - 1) / 2
    x /= np.sqrt(np.sum(x*x) / n)
    features = np.zeros((n, 10))
    if shape == 0:
        features[:, :2] = x[:, None]
    elif shape == 1:
        features[:, 0] = x
        features[:, 1] = 7*x
    elif shape == 2:
        for j in range(10):
            features[:, j] = .02+.001*(j+1)*x if j < 4 else .1+.01*(j-3)*x
    else:
        features[:, :4] = (.02+.001*x)[:, None]
        features[:, 4:] = (.1+.01*x)[:, None]
    return np.column_stack([features, .03+.001*x])


@pytest.mark.parametrize('n', [2, 3, 10, 11, 12, 16, 21, 22, 32, 64, 128, 256, 512])
@pytest.mark.parametrize('shape', range(4))
def test_public_coupon_collinear_history_matches_independent_lstsq(n, shape):
    history = coupon_history(n, shape)
    design = np.column_stack([history[:, :10], np.ones(n)])
    target = history[:, 10]
    expected = np.linalg.lstsq(design, target, rcond=None)[0]
    beta, metrics = call(22, [history], [(11,), (2,)])
    fair = design @ expected
    gap = fair[1:] - target[:-1]
    lam = np.clip(np.sum(gap*np.diff(target))/np.sum(gap*gap), .02, 1.)
    # These are the existing calibration-owner tolerances, unchanged.
    np.testing.assert_allclose(design @ beta, fair, rtol=1e-11, atol=1e-13)
    np.testing.assert_allclose(beta, expected, rtol=1e-10, atol=1e-12)
    assert metrics[0] == pytest.approx(lam, rel=1e-10, abs=1e-12)
    assert metrics[1] == pytest.approx(1., rel=1e-10, abs=1e-12)


@pytest.mark.parametrize('n', [118_438, 118_439])
def test_history_work_refuses_before_design_and_does_not_write_outputs(n):
    # Finite input with a genuine over-budget shape; no monetary output is valid.
    import ctypes
    from portfolio_risk.core.quant_native import Buffer, _load
    from portfolio_risk.core.native import library_path
    history = np.zeros((n, 11), dtype=np.float64)
    beta = np.full(11, 12345., dtype=np.float64)
    metrics = np.full(2, 67890., dtype=np.float64)
    def descriptor(a):
        return Buffer(a.ctypes.data_as(ctypes.POINTER(ctypes.c_double)), a.size,
                      (ctypes.c_size_t*3)(*a.shape, *([1]*(3-a.ndim))))
    _, invoke = _load(str(library_path()))
    status = invoke(22, (Buffer*1)(descriptor(history)), 1,
                    (Buffer*2)(descriptor(beta), descriptor(metrics)), 2, 1)
    assert status != 0
    assert np.all(beta == 12345.)
    assert np.all(metrics == 67890.)


def test_native_history_has_no_python_solver_callback(monkeypatch):
    history = coupon_history(64, 0)
    def forbidden(*args, **kwargs):
        raise AssertionError('Python numerical callback entered native history fit')
    monkeypatch.setattr(np.linalg, 'lstsq', forbidden)
    beta, _ = call(22, [history], [(11,), (2,)])
    design = np.column_stack([history[:, :10], np.ones(64)])
    np.testing.assert_allclose(design @ beta, history[:, 10], rtol=1e-11, atol=1e-13)


def test_large_history_remains_accepted_beyond_stochastic_domain():
    history = coupon_history(4096, 0)
    beta, metrics = call(22, [history], [(11,), (2,)])
    design = np.column_stack([history[:, :10], np.ones(4096)])
    np.testing.assert_allclose(design @ beta, history[:, 10], rtol=1e-11, atol=1e-13)
    assert metrics[0] == pytest.approx(1., rel=1e-10, abs=1e-12)


@pytest.mark.parametrize('n', [2, 3, 10])
def test_wide_full_row_rank_public_fit_matches_minimum_norm(n):
    rng = np.random.default_rng(311+n)
    history = np.column_stack([rng.normal(size=(n, 10)), .03+.001*np.arange(n)])
    design = np.column_stack([history[:, :10], np.ones(n)])
    expected = np.linalg.lstsq(design, history[:, 10], rcond=None)[0]
    beta, _ = call(22, [history], [(11,), (2,)])
    np.testing.assert_allclose(beta, expected, rtol=1e-10, atol=1e-12)
    np.testing.assert_allclose(design @ beta, history[:, 10], rtol=1e-11, atol=1e-13)
