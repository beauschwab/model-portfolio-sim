"""Market DF projection: independent par math and invalid-curve rejection."""
import numpy as np
import pytest
from portfolio_risk.core.curve import market_discount_factors_to_par, bootstrap_curve


@pytest.mark.parametrize("zero", [.04, -.005])
def test_flat_market_curve_preserves_discount_factors(zero):
    t = np.arange(0, 51, .25)
    out = market_discount_factors_to_par(t, np.exp(-zero * t))
    assert out["swap_rates"] == pytest.approx([np.expm1(zero)] * 10, abs=1e-12)
    actual = bootstrap_curve(out["swap_tenors"], out["swap_rates"])
    assert actual[:121] == pytest.approx(np.exp(-zero * t[:121]), abs=1e-11)
    assert out["max_zero_error_bp_30y"] < 1e-7


def test_nonflat_curve_par_cashflows_and_reported_error():
    t = np.arange(0, 51, .25)
    d = np.exp(-(.03 + .012 * (1 - np.exp(-t / 3))) * t)
    out = market_discount_factors_to_par(t, d)
    annual = d[4:121:4]
    for tenor, rate in zip(out["swap_tenors"], out["swap_rates"]):
        n = int(tenor)
        assert rate * sum(annual[:n]) + annual[n - 1] == pytest.approx(1, abs=1e-12)
    assert out["max_zero_error_bp_30y"] > 0  # sparse reconstruction is disclosed


@pytest.mark.parametrize("t,d", [([0, 30], [1, 0]), ([0, 30], [1, float('nan')]),
    ([0, 29], [1, .3]), ([1, 30], [1, .3]), ([0, 1, 1, 30], [1, .99, .98, .3]),
    ([0, 30], [.99, .3]), ([0, 30], [1, 3])])
def test_invalid_market_curve_rejected(t, d):
    with pytest.raises(ValueError):
        market_discount_factors_to_par(t, d)
