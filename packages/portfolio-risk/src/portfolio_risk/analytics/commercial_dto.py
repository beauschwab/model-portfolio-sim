"""Pure native DTO scalar/default identity; no financial source calculation."""
import math
from .modeled_json_identity import exact_json_identity

FLOAT_KEYS=set('opening_principal original_payment amount fee annual_rate margin floor cap rate cost original_annual_effective_rate annual_effective_rate end_year lag_years weight source_weight allocated_face opening_undrawn_commitment waiting_uniform destination_uniform principal_premium_fraction principal_clean_price_factor call_clean_price_factor put_clean_price_factor call_fee put_fee target_amount principal_amount book_adjustment_amount accrued_interest_reversal accrued_interest_amount'.split())
FLOAT_ARRAYS=set('principal_payments hazards_per_year loss_given_default interest_recovery_fraction credit_conversion_factor annual_effective_rates principal_fraction interest_fraction cancel_undrawn_fraction undrawn_fraction'.split())


def typed(value,key=''):
    if type(value) is dict:
        out={k:typed(v,k) for k,v in value.items()}
        if out.get('schema')=='commercial-source-income-proof-1':out.setdefault('current_accounting_rate_reviews',[])
        if out.get('schema')=='commercial-source-credit-accounting-proof-1':
            for optional in ('prepayment_terms','maturity_basis_policy','reviewed_credit_actions'):
                if out.get(optional) is None:out.pop(optional,None)
            if not out.get('performing_draw_tranches'):out.pop('performing_draw_tranches',None)
            if 'reviewed_credit_actions' in out:
                for name in ('allowance_reviews','chargeoffs','legal_waivers'):out['reviewed_credit_actions'].setdefault(name,[])
        if 'max_jumps_per_path' in out and 'intervals' in out and 'identity' in out:
            out.setdefault('opening_undrawn_commitment',0.);out.setdefault('instrument_kind','commercial_loan')
        return out
    if type(value) is list:return [typed(v,key) for v in value]
    if value is None:return None
    if key in FLOAT_KEYS|FLOAT_ARRAYS:
        if type(value) not in (int,float) or not math.isfinite(value):raise ArithmeticError('finite financial input required')
        return float(value)
    return value


GRAPH_FLOATS=set('initial_level long_run_mean mean_reversion loading fixed_base_oas parallel_bump annual_covariance weight bump_bp severity base_oas lower_oas upper_oas target_cash_price original_signed_cash_quote unit_face hqla_l2a outflow30 asf rsf rwa call_clean_price_factor put_clean_price_factor call_fee put_fee target_amount principal_amount book_adjustment_amount accrued_interest_reversal accrued_interest_amount'.split())


def graph_typed(value,key=''):
    value=typed(value,key)
    if type(value)is dict:
        result={k:graph_typed(v,k)for k,v in value.items()}
        if result.get('schema')=='commercial-generated-policy-1' and result.get('oas_calibration') is None:result.pop('oas_calibration',None)
        return result
    if type(value)is list:return [graph_typed(v,key)for v in value]
    if key in GRAPH_FLOATS and value is not None:
        if type(value)not in (int,float)or not math.isfinite(value):raise ArithmeticError('finite commercial graph input required')
        return float(value)
    return value


def graph_identity(expected,actual):
    if not exact_json_identity(graph_typed(expected),actual):raise ArithmeticError('retained commercial raw/DTO identity differs')
