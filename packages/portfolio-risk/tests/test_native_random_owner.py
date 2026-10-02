"""Native CRN ownership: seed compatibility, exact tapes and final-output gates."""
import numpy as np
import pytest

from portfolio_risk.core import quant_native
from portfolio_risk.core.native import library_path
from portfolio_risk.core.runtime import run_context, RunConfig
from portfolio_risk.core.scenarios import CRN

pytestmark = pytest.mark.skipif(not library_path().is_file(), reason='build native product backend')


def oracle(n, seed, months, factors):
    half = (n + 1) // 2
    raw = np.random.default_rng(seed).standard_normal((half, months, factors))
    return (np.concatenate([raw, -raw])[:n],
            np.random.default_rng(seed + 101).standard_normal((n, months)),
            np.random.default_rng(seed + 202).standard_normal((n, months)))


@pytest.mark.parametrize('seed', [0, 19, 2**32-1, 2**64-1, 2**128+73, 2**300-1])
@pytest.mark.parametrize('paths', [1, 2, 7, 32])
def test_numpy_seed_sequence_pcg_and_normals(seed, paths):
    expected = oracle(paths, seed, 997, 3)
    actual = quant_native.shared_draws(paths, seed, 997, 3)
    for a, b in zip(actual, expected):
        # Integers and regular Ziggurat branches are exact. A platform libm may
        # differ by an ulp on log1p in the rare normal tail; no stream drift allowed.
        np.testing.assert_allclose(a, b, rtol=0, atol=2e-15)


def test_native_crn_does_not_call_numpy_rng(monkeypatch):
    expected = CRN(7, 19)
    def forbidden(*args, **kwargs):
        raise AssertionError('Python random generation inside native CRN')
    for name in ('default_rng', 'SeedSequence', 'PCG64'):
        monkeypatch.setattr(np.random, name, forbidden)
    with run_context(RunConfig(compute_backend='rust')):
        actual = CRN(7, 19)
    for name in ('Z', 'eps_ps', 'eps_h'):
        np.testing.assert_allclose(getattr(actual, name), getattr(expected, name), rtol=0, atol=2e-15)
    assert actual.n == 7
    np.testing.assert_array_equal(actual.Z[4:], -actual.Z[:3])


def test_streams_reproducible_independent_and_thread_invariant():
    from numba import get_num_threads, set_num_threads
    before = get_num_threads()
    try:
        tapes = []
        for threads in (1, 2):
            set_num_threads(threads)
            tapes.append(quant_native.shared_draws(8, 1701, 400, 3))
        for a, b in zip(*tapes): np.testing.assert_array_equal(a, b)
        for a, b in zip(tapes[0], quant_native.shared_draws(8, 1702, 400, 3)):
            assert not np.array_equal(a, b)
        assert not np.array_equal(tapes[0][1], tapes[0][2])
    finally:
        set_num_threads(before)


@pytest.mark.parametrize('args', [(0,19,10,3), (1,-1,10,3), (1,2**32768,10,3),
                                (1,19,0,3), (1,19,10,0), (10**12,19,360,3), (1,1.2,10,3)])
def test_invalid_inputs_rejected_before_output_allocation(monkeypatch, args):
    def forbidden(*args, **kwargs):
        raise AssertionError('invalid tape input reached allocation/FFI')
    monkeypatch.setattr(quant_native, 'call', forbidden)
    with pytest.raises((ValueError, TypeError)):
        quant_native.shared_draws(*args)


@pytest.mark.parametrize('inputs', [([-.1],[1,1,1]), ([2**32],[1,1,1]), ([],[1,1,1]),
                                    ([1],[0,1,1]), ([1],[2**40,1,1])])
def test_native_boundary_independently_checks_inputs(inputs):
    with pytest.raises(ValueError, match='no fallback'):
        quant_native.call(27, inputs, [(1,1,1), (1,1), (1,1)])


def test_long_gaussian_stream_matches_including_tail_rejections():
    # One million independent normals exercises tail and rejection branches.
    actual = quant_native.shared_draws(2, 42, 200000, 1)
    expected = oracle(2, 42, 200000, 1)
    for a, b in zip(actual, expected):
        np.testing.assert_allclose(a, b, rtol=0, atol=2e-15)
    assert max(np.max(np.abs(x)) for x in actual) > 4.5
