"""Intraday settlement replay (`intraday-settlement-1`); Rust owns the replay.

Under the explicit Python reference context the independent reference runs instead.
See docs/models/intraday-settlement.md.
"""
SCHEMA = 'intraday-settlement-1'


def run_intraday_settlement(request):
    from ..core.quant_native import enabled, term_call
    if enabled():
        return term_call('financial-controller-1', dict(op='intraday_settlement', request=request))
    from .intraday_settlement_reference import replay
    return replay(request)
