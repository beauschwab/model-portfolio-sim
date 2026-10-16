"""Pure native DTO scalar/default identity; no financial source calculation."""
import math
from copy import deepcopy
from .modeled_json_identity import exact_json_identity

FLOAT_KEYS=set('opening_principal original_payment amount fee annual_rate margin floor cap rate cost original_annual_effective_rate annual_effective_rate end_year lag_years weight source_weight allocated_face opening_undrawn_commitment waiting_uniform destination_uniform principal_premium_fraction principal_clean_price_factor call_clean_price_factor put_clean_price_factor call_fee put_fee target_amount principal_amount book_adjustment_amount accrued_interest_reversal accrued_interest_amount'.split())
FLOAT_KEYS.update('loan_clean_cost transaction_cost purchased_accrued_cost'.split())
FLOAT_ARRAYS=set('principal_payments hazards_per_year loss_given_default interest_recovery_fraction credit_conversion_factor annual_effective_rates principal_fraction interest_fraction cancel_undrawn_fraction undrawn_fraction'.split())


# Native Request fields declared as serde_json::Value retain the submitted JSON
# scalar categories. They are normalized into a separate typed Spec later; raw
# receipt identity must not apply owner f64 key rules inside those opaque values.
RAW_LEDGER_FIELDS={
    'commercial-credit-ledger-1':frozenset({'spec'}),
    'commercial-candidate-journal-1':frozenset({'specification'}),
    'commercial-generated-candidate-journal-1':frozenset({'specification'}),
}


def typed(value,key=''):
    if type(value) is dict:
        opaque=RAW_LEDGER_FIELDS.get(value.get('schema'),())
        out={k:deepcopy(v)if k in opaque else typed(v,k)for k,v in value.items()}
        # SuppliedCurve owns Vec<f64> in both generated-law discounting and
        # graph collateral discounting. Arbitrary raw JSON remains untouched.
        if key in ('collateral_curve','discount_curve')and {'curve_id','source_id','source_revision','currency','role','times_years','discount_factors','right_tail'}<=out.keys():
            for name in ('times_years','discount_factors'):
                if type(out[name])is not list or any(type(x)not in(int,float)or not math.isfinite(x)for x in out[name]):
                    raise ArithmeticError('finite native supplied curve vector required')
                out[name]=[float(x)for x in out[name]]
        if out.get('schema')=='commercial-source-income-proof-1':
            out.setdefault('current_accounting_rate_reviews',[])
            if out.get('transfer') is None:out.pop('transfer',None)
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
GRAPH_FLOATS.update('discount_spread original_signed_fee_quote total_purchase_cost allocated_cost entitlement waive_unpaid_earned_fraction refund_prepaid_fraction refund_earned_fraction fraction'.split())


def graph_typed(value,key=''):
    value=typed(value,key)
    if type(value)is dict:
        opaque=RAW_LEDGER_FIELDS.get(value.get('schema'),())
        result={k:deepcopy(v)if k in opaque else graph_typed(v,k)for k,v in value.items()}
        if result.get('schema')=='commercial-generated-policy-1':
            for optional in ('oas_calibration','physical_path_inputs'):
                if result.get(optional) is None:result.pop(optional,None)
        if {'period_id','observation_index','driver_index','loading'} <= result.keys():
            if result.get('draw_tranche_id') is None:result.pop('draw_tranche_id',None)
        if {'path_key','review','source_recipe_index'} <= result.keys():
            if result.get('reviewed_credit_actions') is None:result.pop('reviewed_credit_actions',None)
        if result.get('schema')=='commercial-generated-physical-path-inputs-1' and result.get('source_work_policy') is None:result.pop('source_work_policy',None)
        if result.get('schema')=='commercial-product-graph-1':
            for position in result['positions']:
                if position.get('fees') is None:position.pop('fees',None)
        if result.get('schema')=='commercial-native-fees-1':
            if result.get('transfer') is None:result.pop('transfer',None)
            for name in ('default','prepayment','exercise','maturity'):
                if isinstance(result.get(name),dict)and result[name].get('refund_earned_fraction',0.)==0.:result[name].pop('refund_earned_fraction',None)
            for fee in result['fees']:
                if fee.get('earned_refunds_allowed') is False:fee.pop('earned_refunds_allowed')
        if 'origin_service'in result and 'valuation_release_policy'in result and result.get('purchase_cash_policy')is None:result.pop('purchase_cash_policy',None)
        if result.get('schema')=='commercial-fee-adoption-1'and not result.get('original_physical'):result.pop('original_physical',None)
        if 'policy' in result and 'path_id' in result and isinstance(result['policy'],dict) and result['policy'].get('schema')=='commercial-native-fees-1':
            if not result.get('acquisition_releases'):result.pop('acquisition_releases',None)
        return result
    if type(value)is list:return [graph_typed(v,key)for v in value]
    if key in GRAPH_FLOATS and value is not None:
        if type(value)not in (int,float)or not math.isfinite(value):raise ArithmeticError('finite commercial graph input required')
        return float(value)
    return value


def graph_identity(expected,actual):
    if not exact_json_identity(graph_typed(expected),actual):raise ArithmeticError('retained commercial raw/DTO identity differs')
