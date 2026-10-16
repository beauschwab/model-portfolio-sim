"""Transport-only entrypoints for explicit shared native model analysis.

These contracts do not implicitly replace a saved product's economics. Product
adoption, empirical calibration and daily journal integration are separate gates.
There is no selectable Python financial implementation or fallback here.
"""
from ..core.quant_native import term_call


def analyze_commercial_fees(request: dict, **execution_options) -> dict:
    """Transport explicit loan and separate fee sources into one native journal."""
    from ..core.commercial_fee_native import run
    return run(request, **execution_options)[0]


def account_nonintegral_fees(request: dict, **execution_options) -> dict:
    """Transport reviewed physical fee services/settlements into the native owner."""
    from ..core.model_native import run
    return run("nonintegral-fee-accounting-1", request, **execution_options)[0]


def analyze_mixed_consumer_credit(request: dict, **execution_options) -> dict:
    """Transport exclusive acquired installment sources into the native mixed GL."""
    from ..core.consumer_mixed_native import run
    return run(request, **execution_options)[0]


def analyze_product_mixed_dated(request: dict, **execution_options) -> dict:
    """Transport bounded existing dated sources and exact-date candidate purchases."""
    from ..core.product_mixed_native import run
    return run(request, **execution_options)[0]


def analyze_product_candidates(request: dict, **execution_options) -> dict:
    """Transport bounded standalone acquisition and full-tail candidate journals."""
    from ..core.product_candidate_native import run
    return run(request, **execution_options)[0]


def fit_observed_credit(request: dict) -> dict:
    """Transport exact observed-state spells; Rust owns fit and holdout diagnostics."""
    return term_call("observed-credit-calibration-1", request)


def fit_observed_deposit_behavior(request: dict) -> dict:
    """Fit physical complete-account exits/migrations; never partial balance runoff."""
    return term_call("observed-deposit-calibration-1", request)


def generate_dated_term_flows(request: dict) -> dict:
    """Transport fixed contractual terms; Rust constructs disclosed actual-date flows."""
    return term_call("dated-term-1", request)


def evaluate_floating_coupons(request: dict) -> dict:
    """Transport complete historical fixings; Rust computes explicit coupon conventions."""
    return term_call("floating-rate-1", request)


def evaluate_credit(request: dict) -> dict:
    """Evaluate an explicit credit-model-1 input entirely in Rust."""
    return term_call("credit-model-1", request)


def project_consumer_installment(request: dict) -> dict:
    """Project explicit auto/personal terms and supplied path actions in Rust."""
    return term_call("consumer-installment-1", request)


def account_consumer_credit(request: dict) -> dict:
    """Transport one physical consumer source and reviewed original-yield/credit policies."""
    return term_call("consumer-credit-accounting-1", request)


def simulate_consumer_card(request: dict) -> dict:
    """Replay class balances, statement cycles and supplied card actions in Rust."""
    return term_call("consumer-card-1", request)


def generate_indexed_contract(request: dict) -> dict:
    """Build actual-date cashflows from explicitly observed or projected indices."""
    return term_call("indexed-contract-1", request)


def value_conditional_exercise(request: dict) -> dict:
    """Value issuer/holder exercise against supplied conditional state transitions."""
    return term_call("conditional-exercise-1", request)


def elect_cd(request: dict) -> dict:
    """Apply an explicit withdrawal, maturity, renewal or switching election."""
    return term_call("cd-election-1", request)


def evaluate_dated_income(request: dict) -> dict:
    """Apply supplied actual-date income/yield/review policies in Rust."""
    return term_call("dated-income-1", request)


def analyze_market_indexed(request: dict) -> dict:
    """Transport explicit supplied pricing curves into native indexed/swap analysis."""
    return term_call("market-indexed-adapter-1", request)


def analyze_model_market(request: dict) -> dict:
    """Analyze supplied collateral/index curves, smile quotes and joint covariance."""
    return term_call("model-market-1", request)


def fit_market_drivers(request: dict) -> dict:
    """Fit supplied market-driver moments with chronological holdout diagnostics."""
    return term_call("market-driver-calibration-1", request)


def bootstrap_market_quotes(request: dict) -> dict:
    """Bootstrap explicit source-linked par quotes with residual diagnostics."""
    return term_call("market-quote-bootstrap-1", request)


def prepare_aged_units(request: dict) -> dict:
    """Prepare actual-purchase/age native fixed-contract coefficient tensors."""
    return term_call("aged-units-1", request)


def analyze_commercial_graph(request: dict) -> dict:
    """Transport explicit commercial credit/continuation sources into Rust."""
    return term_call("commercial-product-graph-1", request)


def continue_commercial_holding(request: dict) -> dict:
    """Replay original acquisition and select a sealed prospective holding window."""
    return term_call("commercial-holding-continuation-1", request)


def journal_commercial_holding(request: dict, **execution_options) -> dict:
    """Transport original acquisition and prospective sources into native journal windows."""
    from ..core.commercial_holding_native import run
    return run(request, **execution_options)[0]


def prepare_commercial_units(request: dict) -> dict:
    """Prepare native commercial coefficients from reviewed Physical postings."""
    return term_call("commercial-units-1", request)


def evaluate_credit_paths(request: dict) -> dict:
    """Evaluate explicit event-time stochastic credit paths entirely in Rust."""
    return term_call("credit-path-1", request)


def evaluate_joint_state(request: dict) -> dict:
    """Evaluate supplied market, behavioral and credit drivers entirely in Rust.

    This coordinator preserves one caller-owned common-random-number stream
    across competing exits. It does not fit hazards, infer a pricing measure,
    or replace a saved product's behavioral/credit policy.
    """
    return term_call("joint-state-1", request)


def evaluate_joint_market_credit(request: dict) -> dict:
    """Generate one bounded market tape and feed its shocks into credit state paths.

    Rust owns the Gaussian path generation, hazard elasticity and competing
    transition arithmetic; this adapter only selects the versioned contract.
    """
    return term_call("joint-market-credit-1", request)


def account_credit_paths(request: dict) -> dict:
    """Transport explicit physical paths and reviewed daily accounting policies."""
    return term_call("credit-path-accounting-1", request)


def evaluate_derivative_contract(request: dict) -> dict:
    """Reprice mapped raw derivative trades and explicit CSA/default policies."""
    return term_call("derivative-contract-1",request)


def account_derivative_contract(request: dict) -> dict:
    """Transport a raw physical derivative path and reviewed journal policies."""
    return term_call("derivative-accounting-1",request)


def account_hedge_relationships(request: dict, **execution_options) -> dict:
    """Transport reviewed formal ASC 815 relationships over one Physical derivative source."""
    from portfolio_risk.core.model_native import run
    return run("hedge-accounting-1", request, **execution_options)[0]


def fit_credit_snapshots(request: dict) -> dict:
    """Fit declared interval-censored snapshot models with isolated holdouts."""
    return term_call("credit-snapshot-calibration-1", request)


def simulate_deposit_state(request: dict) -> dict:
    """Apply supplied daily deposit repricing and competing exit/migration policies."""
    return term_call("deposit-state-1", request)


def project_mortgage_state(request: dict) -> dict:
    """Project explicitly selected mortgage/agency ownership and supplied path inputs."""
    return term_call("mortgage-state-1", request)


def evaluate_funding_contract(request: dict) -> dict:
    """Evaluate actual-date funding, rollover and supplied collateral policies."""
    return term_call("funding-contract-1", request)


def resolve_dated_cashflows(schedule: dict, scenario: str) -> list[dict]:
    """Validate and select contractual/modelled events using actual dates."""
    return term_call("dated-cashflow-1", {"schedule": schedule, "scenario": scenario})


def fit_credit_covariates(request: dict) -> dict:
    """Transport exact physical spells and reviewed feature transforms for fitting."""
    return term_call("credit-covariate-1",request)


def project_credit_covariates(request: dict) -> dict:
    """Project an explicitly reviewed fitted physical model onto supplied drivers."""
    return term_call("credit-covariate-projection-1",request)


def account_card_credit(request: dict) -> dict:
    """Transport one raw supplied card path and reviewed original-yield policy."""
    return term_call("card-credit-accounting-1", request)


def account_card_credit_v2(request: dict) -> dict:
    """Transport explicitly selected whole-carrying and interest-basis ownership."""
    return term_call("card-credit-accounting-2", request)


def account_mortgage_credit(request: dict) -> dict:
    """Transport paired Physical whole-loan sources and original carrying policy."""
    return term_call("mortgage-credit-accounting-1", request)


def analyze_mixed_product_credit(request: dict) -> dict:
    """Transport tagged acquired product credit owners into the shared daily GL."""
    from ..core.product_credit_mixed_native import run
    return run(request)[0]


def analyze_advanced_graph(request: dict) -> dict:
    """Generate explicit native physical/pricing consumer sources and fixed-OAS risk."""
    return term_call("advanced-product-graph-1",request)


def analyze_card_graph(request: dict) -> dict:
    """Transport explicit P/Q revolving-card histories to the native graph."""
    return term_call("card-product-graph-1",request)


def analyze_deposit_graph(request: dict) -> dict:
    """Transport separately supplied physical and pricing deposit policies."""
    return term_call("deposit-product-graph-1",request)


def analyze_mortgage_graph(request: dict) -> dict:
    """Transport supplied borrower/investor histories and full remittance tails."""
    return term_call("mortgage-product-graph-1",request)


def reprice_mortgage_graph(base: dict,current: dict) -> dict:
    """Preserve the original mortgage graph calibration anchor."""
    return term_call("mortgage-product-reprice-1",dict(base=base,current=current))


def analyze_funding_graph(request: dict) -> dict:
    """Transport whole-book funding histories with exclusive collateral ownership."""
    return term_call("funding-product-graph-1",request)


def audit_graph_cohorts(request: dict) -> dict:
    """Transport explicit representative/member contracts to bounded native diagnostics."""
    return term_call("graph-cohort-audit-1",request)


def analyze_term_product_graph(request: dict) -> dict:
    """Transport explicit indexed and CD election histories with full cash tails."""
    return term_call("term-product-graph-1",request)


def prepare_product_units(request: dict) -> dict:
    """Transport actual-purchase performing products with reviewed original yields."""
    return term_call("product-units-1",request)


def reprice_term_product_graph(base: dict,current: dict) -> dict:
    """Preserve the original contractual product graph calibration anchor."""
    return term_call("term-product-reprice-1",dict(base=base,current=current))


def reprice_funding_graph(base: dict,current: dict) -> dict:
    """Preserve the original whole funding-book calibration anchor."""
    return term_call("funding-product-reprice-1",dict(base=base,current=current))


def reprice_deposit_graph(base: dict,current: dict) -> dict:
    """Preserve the original deposit graph liability calibration anchor."""
    return term_call("deposit-product-reprice-1",dict(base=base,current=current))


def reprice_card_graph(base: dict,current: dict) -> dict:
    """Preserve the original native card graph calibration anchor."""
    return term_call("card-product-reprice-1",dict(base=base,current=current))


def reprice_advanced_graph(base: dict,current: dict) -> dict:
    """Reprice changed scenarios against the complete original native base anchor."""
    return term_call("advanced-product-reprice-1",dict(base=base,current=current))


def capture_advanced_graph(graph: dict,owners: list,policy: dict) -> dict:
    """Prepare complete native scheduled sources under explicitly admitted policies."""
    return term_call("advanced-scheduled-capture-1",dict(graph=graph,owners=owners,policy=policy))


def analyze_term_sequence_graph(request: dict) -> dict:
    """Transport explicitly selected full CD sequence sources and original OAS."""
    return term_call("term-product-graph-2", request)


def reprice_term_sequence_graph(base: dict, current: dict) -> dict:
    """Hold the immutable original quote/Q/OAS receipt through current scenarios."""
    return term_call("term-product-reprice-2", dict(base=base,current=current))
