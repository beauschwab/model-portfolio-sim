"""Supported v1 scope and the explicit experimental capability flag.

One registry decides which explicit model contracts a default install admits. It gates
admission of new work only: historical jobs, manifests and tables stay readable, and no
economics, tolerance or schema changes with the flag. See docs/supported-scope.md.
"""
import os

from fastapi import HTTPException

SETTING = 'WORKBENCH_EXPERIMENTAL_MODELS'

# v1 ALM core: mortgages/MBS, securities, non-maturity deposits, CDs, wholesale funding and
# swap hedges; NII/EVE, stress, optimizer and Strategy Lab. Everything else is experimental.
V1_MODEL_SCHEMAS = frozenset({
    # Deposits and CDs
    'deposit-state-1', 'deposit-product-graph-1', 'observed-deposit-calibration-1',
    'cd-election-1', 'term-product-graph-1', 'term-product-graph-2', 'term-product-reprice-2',
    # Mortgages/MBS and securities
    'mortgage-state-1', 'mortgage-product-graph-1', 'indexed-contract-1',
    'market-indexed-adapter-1', 'market-indexed-books-1',
    # Wholesale funding
    'funding-contract-1', 'funding-product-graph-1',
    # Swap hedges (economic risk; formal ASC 815 designation is experimental)
    'derivative-contract-1', 'saved-derivative-risk-1',
    # Markets, income, cohorts, units and Strategy Lab
    'model-market-1', 'market-driver-calibration-1', 'market-quote-bootstrap-1',
    'dated-income-1', 'graph-cohort-audit-1', 'aged-units-1', 'product-units-1',
    'strategy-product-1', 'product-candidate-journal-1', 'product-mixed-dated-1',
})

V1_LEDGER_SCHEMAS = frozenset({
    'graph-dated-ledger-1', 'deposit-graph-dated-ledger-1', 'deposit-graph-income-ledger-1',
    'deposit-revised-income-ledger-1', 'deposit-performing-income-ledger-1',
    'deposit-performing-portfolio-ledger-2',
})


def enabled():
    """Read per request so a restart with the setting is the only way to change it."""
    return os.getenv(SETTING, '0') == '1'


def refuse(surface):
    raise HTTPException(403, f'{surface} is experimental and outside the supported v1 scope; '
                             f'set {SETTING}=1 on the API to enable it')


def experimental(surface):
    """Route dependency: refuse before the body is validated, so the refusal never depends on its shape."""
    def check():
        if not enabled():
            refuse(surface)
    return check


def model_supported(schema, request=None):
    if schema not in V1_MODEL_SCHEMAS:
        return False
    # Commercial cohorts share the cohort-audit contract but not its v1 products.
    return not (schema == 'graph-cohort-audit-1' and isinstance(request, dict) and request.get('kind') == 'commercial')


def require_model(schema, request=None):
    if not enabled() and not model_supported(schema, request):
        refuse(f'Model contract `{schema}`')


def require_ledger(schema):
    if not enabled() and schema not in V1_LEDGER_SCHEMAS:
        refuse(f'Ledger contract `{schema}`')


def describe(schemas):
    """Split a route's known contracts into the v1 and experimental sets, in their original order."""
    supported = [s for s in schemas if s in V1_MODEL_SCHEMAS]
    experimental = [s for s in schemas if s not in V1_MODEL_SCHEMAS]
    return dict(schemas=list(schemas) if enabled() else supported, supported=supported,
                experimental=experimental, experimental_enabled=enabled())
