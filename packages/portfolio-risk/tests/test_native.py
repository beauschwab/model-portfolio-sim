"""Actual FFI parity, buffer ownership, thread determinism and no-fallback gates."""
import numba
import numpy as np
import pytest
from portfolio_risk.core.batch import CashflowBatch, NumpyDiscountBackend
from portfolio_risk.core.native import NumbaDiscountBackend, RustDiscountBackend, library_path


@pytest.mark.skipif(not library_path().exists(), reason='optional Rust backend: run scripts/build_native.py')
def test_native_random_ragged_batch_and_thread_parity():
    rng = np.random.default_rng(93)
    lengths = rng.integers(1, 121, size=1025)
    offsets = np.cumsum(np.r_[0, lengths]).astype(np.int64)
    times = rng.uniform(0, 40, offsets[-1]); values = rng.uniform(-.2, 5, offsets[-1])
    batch = CashflowBatch(offsets, times, values, 33)
    oas = rng.uniform(-.05, .3, 1025)
    expected = NumpyDiscountBackend().price(batch, oas)
    original = numba.get_num_threads()
    try:
        rust = RustDiscountBackend()
        results = []
        for count in (1, min(4, numba.config.NUMBA_NUM_THREADS)):
            numba.set_num_threads(count)
            for backend in (rust, NumbaDiscountBackend()):
                actual = backend.price(batch, oas)
                np.testing.assert_allclose(actual, expected, rtol=1e-12, atol=1e-12)
            results.append(rust.price(batch, oas))
        np.testing.assert_array_equal(*results)
        np.testing.assert_array_equal(batch.values, values)
        with pytest.raises(ValueError):
            rust.price(batch, oas[:-1])
        with pytest.raises(ValueError):
            rust.price(batch, np.full(1025, np.nan))
        overflow = CashflowBatch(np.array([0, 1]), np.array([1000.]), np.array([1.]), 1)
        with pytest.raises(ValueError, match='no fallback'):
            rust.price(overflow, np.array([-10.]))
        assert rust.price(CashflowBatch.from_rows([], 1), np.empty(0)).shape == (0,)
    finally:
        numba.set_num_threads(original)


def test_native_unavailable_fails_explicitly(monkeypatch, tmp_path):
    monkeypatch.setenv('PORTFOLIO_RISK_RUST_LIB', str(tmp_path / 'absent.dll'))
    with pytest.raises(RuntimeError, match='not built'):
        RustDiscountBackend()
