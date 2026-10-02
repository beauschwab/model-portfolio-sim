"""Transport-only entrypoints for explicit shared native model analysis.

These contracts do not implicitly replace a saved product's economics. Product
adoption, empirical calibration and daily journal integration are separate gates.
There is no selectable Python financial implementation or fallback here.
"""
from ..core.quant_native import term_call


def fit_observed_credit(request: dict) -> dict:
    """Transport exact observed-state spells; Rust owns fit and holdout diagnostics."""
    return term_call("observed-credit-calibration-1", request)


def generate_dated_term_flows(request: dict) -> dict:
    """Transport fixed contractual terms; Rust constructs disclosed actual-date flows."""
    return term_call("dated-term-1", request)


def evaluate_floating_coupons(request: dict) -> dict:
    """Transport complete historical fixings; Rust computes explicit coupon conventions."""
    return term_call("floating-rate-1", request)


def evaluate_credit(request: dict) -> dict:
    """Evaluate an explicit credit-model-1 input entirely in Rust."""
    return term_call("credit-model-1", request)


def resolve_dated_cashflows(schedule: dict, scenario: str) -> list[dict]:
    """Validate and select contractual/modelled events using actual dates."""
    return term_call("dated-cashflow-1", {"schedule": schedule, "scenario": scenario})
