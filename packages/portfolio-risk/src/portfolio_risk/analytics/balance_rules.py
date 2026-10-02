"""Versioned internal research limits shared by daily reporting and replay.

These are explicitly supplied weights, not a jurisdiction's regulatory rulebook.
Never label a pass under this ruleset as regulatory compliance.
"""
RULESET = 'research-weights-v2'
L2_CAP = .40
INFLOW_CAP = .75


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
