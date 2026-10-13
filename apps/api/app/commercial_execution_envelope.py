"""Reconstruct immutable executed transport bytes with their integer clock."""
import math


def retained_command(schema,raw,timeout,milliseconds):
    if type(timeout)not in(int,float)or not math.isfinite(timeout)or not .001<=timeout<=1800.:
        raise ValueError('commercial original timeout domain differs')
    if type(milliseconds)is not int or not 1<=milliseconds<=int(timeout*1000):
        raise ValueError('commercial original native budget identity differs')
    if schema=='commercial-fee-journal-1':from portfolio_risk.core.commercial_fee_native import command
    elif schema=='commercial-holding-journal-1':from portfolio_risk.core.commercial_holding_native import command
    elif schema=='commercial-treasury-1':from portfolio_risk.core.commercial_treasury_native import command
    elif schema=='commercial-candidate-journal-1':from portfolio_risk.core.commercial_candidate_native import command
    else:raise ValueError('commercial integer-clock transport selection required')
    value=command(raw,timeout=timeout)
    # Keep the actual admitted integer. Floating seconds can floor1001ms to1000.
    value['execution_timeout_ms']=milliseconds
    return value
