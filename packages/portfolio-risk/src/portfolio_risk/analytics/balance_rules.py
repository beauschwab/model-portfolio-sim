"""Versioned internal research limits shared by daily reporting and replay.

These are explicitly supplied weights, not a jurisdiction's regulatory rulebook.
Never label a pass under this ruleset as regulatory compliance.
"""
RULESET = 'research-weights-v2'
L2_CAP = .40
INFLOW_CAP = .75
# The only HTM accounting policy implemented: US GAAP ASC 320-10-25 sale
# exceptions with a portfolio reclassification above a stated materiality.
HTM_POLICY = 'asc320-htm-v1'
# Sale reasons ASC 320-10-25-6 and 25-14 treat as consistent with HTM intent.
HTM_EXCEPTIONS = ('credit_deterioration', 'tax_law_change', 'business_combination', 'regulatory_change', 'near_maturity')
SALE_REASONS = (*HTM_EXCEPTIONS, 'liquidity', 'other')
LIQUIDITY_STATUS = ('within_limit', 'unavailable', 'breach')


def liquidity_basis(applicability, floor):
    """How a liquidity floor applies. Empty resolves from the floor, so older
    specifications keep their limits as internal limits."""
    if applicability:
        return applicability
    return 'internal_limit' if floor > 0 else 'unconfigured'


def liquidity_status(basis, ratio, margin):
    """(status, rank): a breach outranks unavailable inputs, which outrank a pass."""
    if basis != 'internal_limit':
        return ('not_applicable' if basis == 'not_applicable' else 'unconfigured'), 0
    if margin < -1e-9:
        return 'breach', 2
    if ratio is None:
        return 'unavailable', 1
    return 'within_limit', 0


def liquidity_components(level1, level2a, outflow, inflow):
    """Shared composition/inflow arithmetic; classifications remain explicit."""
    level2a = min(level2a, level1*L2_CAP/(1-L2_CAP))
    capped_inflow = min(inflow, INFLOW_CAP*outflow)
    return level1+level2a, capped_inflow, max(0., outflow-capped_inflow)


def headroom(account, *, cash, cet1, rwa, exposure, hqla, outflow, asf, rsf, htm, assets):
    return dict(cash=cash-account.cash_floor,
                cet1=cet1-account.cet1_floor*rwa,
                leverage=cet1-account.leverage_floor*exposure,
                lcr=hqla-account.lcr_floor*outflow,
                nsfr=asf-account.nsfr_floor*rsf,
                htm=account.htm_asset_limit*max(assets, 0.)-htm)


def validation_status(result):
    """One authority for full daily candidate acceptance, including day zero."""
    return dict(dynamic_validated=result['breaches'].is_empty(), ruleset=RULESET,
                validation_scope='daily research limits and independent journal replay',
                regulatory_validated=False, calibration_validated=False,
                breach_count=result['breaches'].height)
