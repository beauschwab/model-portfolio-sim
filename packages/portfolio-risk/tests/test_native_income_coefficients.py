"""Separate retained service-income aggregation and no-repricing evaluation."""
import numpy as np
import pytest

from portfolio_risk.core import quant_native


def test_signed_separate_fee_coefficients_and_combined_income():
    fees, combined, fee_total, combined_total = quant_native.income_coefficients(
        [[.5, -.2], [-.3, .6]], [100., 50.], [2., -1.])
    np.testing.assert_array_equal(fees, [35., 10.])
    np.testing.assert_array_equal(combined, [37., 9.])
    assert fee_total == 45. and combined_total == 46.


def test_empty_separate_fee_allocation_retains_interest():
    fees, combined, fee_total, combined_total = quant_native.income_coefficients(
        np.empty((0, 2)), [], [2., -1.])
    np.testing.assert_array_equal(fees, [0., 0.])
    np.testing.assert_array_equal(combined, [2., -1.])
    assert fee_total == 0. and combined_total == 1.


@pytest.mark.parametrize('coefficients,amounts,interest', [
    ([[float('nan')]], [1.], [0.]), ([[1.]], [-1.], [0.]),
    ([[1e15]], [2.], [0.]), ([[1.]], [1.], [float('inf')]),
    ([[1.]], [1., 2.], [0.]), ([[1.]], [1.], [0., 0.]),
])
def test_invalid_separate_income_refuses(coefficients, amounts, interest):
    with pytest.raises(ValueError):
        quant_native.income_coefficients(coefficients, amounts, interest)


def test_missing_separate_income_export_requires_rebuild(monkeypatch):
    monkeypatch.setattr(quant_native, '_load', lambda path: (object(), None))
    with pytest.raises(RuntimeError, match='rebuild and restart'):
        quant_native.income_coefficients([[1.]], [1.], [0.])
