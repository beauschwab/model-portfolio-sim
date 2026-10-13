"""Declared Rust DTO JSON clones, never normalization of opaque Value subtrees.

Literal declarations are checked against Core request types, dated_store.rs,
aged_units::Weights, product_units/types.rs and mixed/candidate owner structs.
Dates, integer indices, bools and every Value specification retain exact types.
No returned financial receipt supplies this map.
"""
from copy import deepcopy
import math

_TYPES = {'AccrualPeriod': {'end_ordinal': 'i32', 'start_ordinal': 'i32'},
 'AccrualPlanInput': {'event_id': 'String',
                      'position_id': 'String',
                      'run': 'ScenarioSelector',
                      'segments': 'Vec<RecognitionSegment>'},
 'ActualDayCount': {},
 'ArmReset': {'input': 'MortgageIndexInput', 'ordinal': 'i32'},
 'ArmTerms': {'index': 'IndexDescriptor',
              'initial_down_cap': 'f64',
              'initial_up_cap': 'f64',
              'lifetime_down_cap': 'f64',
              'lifetime_up_cap': 'f64',
              'margin': 'f64',
              'note_floor': 'f64',
              'original_note_rate': 'f64',
              'payment_recast': 'PaymentRecast',
              'prior_reset_count': 'u32',
              'reference_calendar': 'RevisionIdentity',
              'subsequent_down_cap': 'f64',
              'subsequent_up_cap': 'f64'},
 'BalanceClass': {},
 'CardAction': {'amount': 'f64',
                'balance_class': 'BalanceClass',
                'rates': 'CardRates',
                'state': 'ConsumerState'},
 'CardRates': {'cash_advance': 'f64',
               'post_promotion': 'f64',
               'promotional': 'f64',
               'promotional_expiry_ordinal': 'i32',
               'purchase': 'f64'},
 'CardRequest': {'as_of_ordinal': 'i32',
                 'collection_cost': 'f64',
                 'collection_fraction': 'f64',
                 'compound_billed_interest': 'bool',
                 'cycles': 'Vec<StatementCycle>',
                 'day_count': 'ActualDayCount',
                 'horizon_days': 'u32',
                 'identity': 'ConsumerIdentity',
                 'initial_limit': 'f64',
                 'initial_purchase_grace': 'bool',
                 'initial_state': 'ConsumerState',
                 'minimum': 'MinimumPayment',
                 'opening_billed_interest': '[f64;3]',
                 'opening_contingent_purchase_interest': 'f64',
                 'opening_due_ordinal': 'Option<i32>',
                 'opening_fees': '[f64;3]',
                 'opening_minimum_remaining': 'f64',
                 'opening_principal': '[f64;3]',
                 'opening_statement_remaining': '[f64;3]',
                 'opening_unbilled_interest': '[f64;3]',
                 'opening_unrecognized_billed_interest': '[f64;3]',
                 'rates': 'CardRates',
                 'recovery_delay_days': 'u32',
                 'regain_grace_after_full_statement_payment': 'bool',
                 'schema': 'String',
                 'steps': 'Vec<CardStep>'},
 'CardStep': {'action': 'CardAction', 'ordinal': 'i32'},
 'CashflowAdjustment': {'rationale': 'String', 'surviving_fraction': 'f64'},
 'CashflowComponents': {'accrued_interest': 'f64',
                        'book_amortization': 'f64',
                        'cash_interest': 'f64',
                        'fees': 'f64',
                        'principal': 'f64'},
 'CashflowMeasure': {},
 'ConsumerIdentity': {'assumption_id': 'String',
                      'cohort_id': 'String',
                      'currency': 'String',
                      'measure': 'CashflowMeasure',
                      'model_version': 'String',
                      'original_loan_id': 'String',
                      'position_id': 'String',
                      'run_id': 'String',
                      'source_revision': 'String'},
 'ConsumerState': {},
 'CoverageStatus': {'rationale': 'String'},
 'DatedCashflowEvent': {'accrual_period': 'Option<AccrualPeriod>',
                        'adjustment': 'CashflowAdjustment',
                        'components': 'CashflowComponents',
                        'currency': 'String',
                        'day_offset': 'u32',
                        'event_id': 'String',
                        'measure': 'CashflowMeasure',
                        'payment_ordinal': 'i32',
                        'position_id': 'String',
                        'provenance': 'EventProvenance',
                        'scenario': 'ScenarioSelector'},
 'DatedSchedule': {'as_of_ordinal': 'i32',
                   'events': 'Vec<DatedCashflowEvent>',
                   'horizon_days': 'u32',
                   'scenarios': 'Vec<String>',
                   'schema': 'String'},
 'DatedStoreRequest': {'accrual_plans': 'Vec<AccrualPlanInput>',
                       'as_of_ordinal': 'i32',
                       'owners': 'Vec<OwnerMapping>',
                       'reporting_horizon_days': 'u32',
                       'runs': 'Vec<RunIdentity>',
                       'schema': 'String',
                       'sources': 'Vec<ScheduleSource>'},
 'DefaultCouponPolicy': {},
 'DepositDriver': {'competitor_rates': 'Vec<f64>',
                   'confidence_stress': 'Vec<f64>',
                   'external_inflows': 'Vec<f64>',
                   'offered_spread_stress': 'Vec<f64>',
                   'ordinal': 'i32',
                   'reference_rate': 'f64'},
 'DepositIdentity': {'assumption_id': 'String',
                     'assumption_rationale': 'String',
                     'cohort_id': 'String',
                     'currency': 'String',
                     'entity_id': 'String',
                     'measure': 'CashflowMeasure',
                     'model_version': 'String',
                     'run_id': 'String',
                     'source_revision': 'String'},
 'DepositSegment': {'characteristics': 'SegmentCharacteristics',
                    'interest_payment_ordinals': 'Vec<i32>',
                    'maturity_ordinal': 'Option<i32>',
                    'opening_accrued_interest': 'f64',
                    'opening_balance': 'f64',
                    'opening_paid_rate': 'f64',
                    'repricing': 'RepricingPolicy',
                    'retention': 'RetentionPolicy',
                    'segment_id': 'String'},
 'DepositStateRequest': {'as_of_ordinal': 'i32',
                         'day_count': 'ActualDayCount',
                         'drivers': 'Vec<DepositDriver>',
                         'horizon_days': 'u32',
                         'identity': 'DepositIdentity',
                         'migrations': 'Vec<MigrationPolicy>',
                         'schema': 'String',
                         'segments': 'Vec<DepositSegment>',
                         'terminal': 'TerminalPolicy'},
 'Direction': {},
 'EventProvenance': {'assumption_id': 'String',
                     'input_revision': 'String',
                     'model_id': 'String',
                     'model_version': 'String',
                     'rationale': 'String',
                     'source_id': 'String',
                     'source_revision': 'String'},
 'IndexDescriptor': {'average_calendar_days': 'Option<u32>',
                     'index_id': 'String',
                     'kind': 'IndexKind',
                     'tenor_months': 'Option<u32>'},
 'IndexKind': {},
 'InstallmentAction': {'amount': 'f64', 'state': 'ConsumerState'},
 'InstallmentProduct': {'annual_depreciation': 'f64',
                        'channel': 'String',
                        'collateral_value_as_of': 'f64',
                        'collection_cost': 'f64',
                        'collection_fraction': 'f64',
                        'dti': 'f64',
                        'dti_definition': 'String',
                        'new_vehicle': 'bool',
                        'original_ltv': 'f64',
                        'recovery_delay_days': 'u32',
                        'repossession_cost': 'f64',
                        'sale_haircut': 'f64',
                        'vintage_ordinal': 'i32'},
 'InstallmentRequest': {'accrue_contract_interest_in_nonaccrual': 'bool',
                        'as_of_ordinal': 'i32',
                        'day_count': 'ActualDayCount',
                        'disclosed_apr': 'f64',
                        'horizon_days': 'u32',
                        'identity': 'ConsumerIdentity',
                        'initial_state': 'ConsumerState',
                        'note_rate': 'f64',
                        'opening_accrued_interest': 'f64',
                        'opening_fees': 'f64',
                        'opening_principal': 'f64',
                        'opening_unrecognized_interest': 'f64',
                        'original_payment': 'f64',
                        'original_principal': 'f64',
                        'payment_dates': 'Vec<i32>',
                        'prepayment_penalty_fraction': 'f64',
                        'product': 'InstallmentProduct',
                        'schema': 'String',
                        'steps': 'Vec<InstallmentStep>'},
 'InstallmentStep': {'action': 'InstallmentAction', 'ordinal': 'i32'},
 'MigrationPolicy': {'base_per_year': 'f64',
                     'destination_rate_gap_sensitivity': 'f64',
                     'from_segment': 'String',
                     'to_segment': 'String'},
 'MinimumPayment': {'allocation_order': 'Vec<BalanceClass>',
                    'floor': 'f64',
                    'include_billed_interest': 'bool',
                    'include_fees': 'bool',
                    'principal_fraction': 'f64'},
 'MortgageIndexInput': {'available_unix_seconds': 'i64',
                        'fixing': 'SuppliedFixing',
                        'projection_source': 'RevisionIdentity',
                        'reset_cutoff_unix_seconds': 'i64'},
 'MortgageOwnership': {'default_buyout_lag_days': 'u32',
                       'default_coupon_policy': 'DefaultCouponPolicy',
                       'guarantee': 'RevisionIdentity',
                       'issuer': 'String',
                       'recognized_accrual_fraction': '[f64;3]',
                       'remittance_lag_days': 'u32',
                       'servicing_and_guarantee_spread': 'f64'},
 'MortgagePathPoint': {'credit_hazards_per_year': '[[f64;5];3]',
                       'refinancing_market_rate': 'f64',
                       'start_ordinal': 'i32',
                       'turnover_hazard_per_year': 'f64'},
 'MortgageRate': {'resets': 'Vec<ArmReset>', 'terms': 'ArmTerms'},
 'MortgageRecoveryWeight': {'lag_days': 'u32', 'weight': 'f64'},
 'MortgageRequest': {'as_of_ordinal': 'i32',
                     'borrower_collection_fraction': '[f64;3]',
                     'complete_recovery_tails': '[Vec<MortgageRecoveryWeight>;3]',
                     'current_note_rate': 'f64',
                     'current_payment': 'f64',
                     'day_count': 'ActualDayCount',
                     'horizon_days': 'u32',
                     'identity': 'ConsumerIdentity',
                     'loss_given_default': '[f64;3]',
                     'monetary_unit': 'String',
                     'opening_borrower_interest': '[f64;3]',
                     'opening_borrower_principal': '[f64;3]',
                     'opening_burnout_incentive_years': 'f64',
                     'opening_contractual_interest': 'f64',
                     'opening_contractual_principal': 'f64',
                     'opening_investor_interest': '[f64;3]',
                     'opening_investor_principal': '[f64;3]',
                     'opening_probability': '[f64;3]',
                     'original_payment': 'f64',
                     'original_principal': 'f64',
                     'origination_ordinal': 'i32',
                     'ownership': 'MortgageOwnership',
                     'path': 'Vec<MortgagePathPoint>',
                     'payment_dates': 'Vec<i32>',
                     'publication_cutoff_unix_seconds': 'i64',
                     'rate': 'MortgageRate',
                     'refinance': 'RefinanceCoefficients',
                     'schema': 'String'},
 'ObservedRange': {'history_revision': 'String',
                   'logistic_plateau_identified': 'bool',
                   'maximum_reference_rate': 'f64',
                   'minimum_reference_rate': 'f64'},
 'OwnerMapping': {'account_id': 'String',
                  'currency': 'String',
                  'direction': 'Direction',
                  'ownership': 'Ownership',
                  'position_id': 'String',
                  'source_id': 'String',
                  'source_revision': 'String'},
 'Ownership': {},
 'PaymentRecast': {},
 'PositionCoverage': {'position_id': 'String', 'run': 'ScenarioSelector', 'status': 'CoverageStatus'},
 'RecognitionSegment': {'accrued_interest': 'f64',
                        'book_amortization': 'f64',
                        'end_ordinal': 'i32',
                        'start_ordinal': 'i32'},
 'RefinanceCoefficients': {'burnout_sensitivity': 'f64',
                           'incentive_slope': 'f64',
                           'incentive_threshold': 'f64',
                           'prepay_eligibility': '[f64;3]',
                           'refinance_seasoning_days': 'u32',
                           'turnover_seasoning_days': 'u32'},
 'RepricingPolicy': {'ceiling': 'f64',
                     'falling_half_life_days': 'f64',
                     'floor': 'f64',
                     'observation_lag_days': 'u32',
                     'rising_half_life_days': 'f64',
                     'supported_observation_range': 'Option<ObservedRange>',
                     'target': 'TargetRule'},
 'RetentionPolicy': {'absolute_daily_velocity_sensitivity': 'f64',
                     'closure_per_year': 'f64',
                     'closure_stress_sensitivity': 'f64',
                     'concentration_stress_sensitivity': 'f64',
                     'digital_stress_sensitivity': 'f64',
                     'operational_protection': 'f64',
                     'opportunity_gap_sensitivity': 'f64',
                     'relationship_protection': 'f64',
                     'uninsured_stress_sensitivity': 'f64',
                     'withdrawal_per_year': 'f64'},
 'RevisionIdentity': {'id': 'String', 'revision': 'String'},
 'RunIdentity': {'input_revision': 'String', 'run_id': 'String', 'scenario_id': 'String', 'severity': 'f64'},
 'ScenarioSelector': {'scenario_id': 'String'},
 'ScheduleSource': {'coverage': 'Vec<PositionCoverage>',
                    'schedule': 'DatedSchedule',
                    'source_id': 'String',
                    'source_revision': 'String'},
 'SegmentCharacteristics': {'concentration': 'f64',
                            'digital_fraction': 'f64',
                            'operational_fraction': 'f64',
                            'relationship_fraction': 'f64',
                            'uninsured_fraction': 'f64'},
 'StatementCycle': {'due_ordinal': 'i32', 'statement_ordinal': 'i32'},
 'SuppliedFixing': {'fixing_id': 'String',
                    'index_id': 'String',
                    'publication_unix_seconds': 'i64',
                    'rate': 'f64',
                    'revision': 'String',
                    'value_ordinal': 'i32'},
 'TargetRule': {'beta': 'f64',
                'beta_max': 'f64',
                'beta_min': 'f64',
                'competitor_weight': 'f64',
                'intercept': 'f64',
                'midpoint': 'f64',
                'steepness': 'f64'},
 'TerminalPolicy': {'rationale': 'String', 'supplied_value': 'f64'},
 'Weights': {'asf': 'f64', 'hqla_l2a': 'f64', 'outflow30': 'f64', 'rsf': 'f64', 'rwa': 'f64'}}

def number(v):
    if type(v)not in(int,float)or not math.isfinite(v):raise ValueError('declared DTO f64 domain')
    return float(v)

def dto(value,typename):
    if value is None:return None
    if typename=='f64':return number(value)
    if typename.startswith(('Vec<','Box<','Option<')):
        inner=typename.split('<',1)[1][:-1]
        if typename.startswith('Vec<'):return [dto(v,inner)for v in value]
        return dto(value,inner)
    if typename.startswith('['):return[dto(v,typename[1:].rsplit(';',1)[0])for v in value]
    if typename not in _TYPES or type(value)is not dict:return deepcopy(value)
    return {k:dto(v,_TYPES[typename].get(k,'Value'))for k,v in value.items()}

def fields(value,names):
    out=deepcopy(value)
    for name in names:
        if name in out:out[name]=number(out[name])
    return out

def graph(value):
    out=deepcopy(value);r=out['request'];kind=out['kind'];r['bump_bp']=number(r['bump_bp'])
    for s in r['scenarios']:
        s['severity']=number(s['severity'])
        c=s['collateral_curve']
        for k in ('times_years','discount_factors'):c[k]=[number(v)for v in c[k]]
    for p in r['positions']:
        p['oas_target']=fields(p['oas_target'],('target_cash_price','base_oas'))
        product=p['product']if kind=='consumer_installment'else p
        if 'contract'in product:product['contract']=dto(product['contract'],{'consumer_installment':'InstallmentRequest','card':'CardRequest','mortgage':'MortgageRequest'}[kind])
        for branches in product['histories']:
            for role in ('physical','pricing','pricing_down','pricing_up'):
                for h in branches[role]:
                    h['weight']=number(h['weight'])
                    if kind=='deposit':h['request']=dto(h['request'],'DepositStateRequest')
                    elif kind=='mortgage':
                        h['path']=[dto(v,'MortgagePathPoint')for v in h['path']]
                        h['arm_resets']=[dto(v,'ArmReset')for v in h['arm_resets']]
                    else:h['steps']=[dto(v,'InstallmentStep'if kind=='consumer_installment'else'CardStep')for v in h['steps']]
    return out

def units(value):
    out=deepcopy(value)
    for u in out['units']:
        u.update(fields(u,('unit_face','quote_price_per_face_unit')))
        u['weights']=dto(u['weights'],'Weights')
        u['income_policy']=fields(u['income_policy'],('original_annual_effective_rate',))
        u['purchase_graph']=graph(u['purchase_graph'])
        for branch in u['age_branches']:branch['graph']=graph(branch['graph'])
    return out

def performing(value):
    out=deepcopy(value);c=out['candidate'];c['product']=units(c['product'])
    c['allocation']=[fields(v,('face',))for v in c['allocation']]
    c['owners']=[fields(v,('pre_purchase_principal','pre_purchase_accrued','pre_purchase_basis'))for v in c['owners']]
    c['run_mapping']=[fields(v,('severity',))for v in c['run_mapping']]
    out['backbook']['dated']=dto(out['backbook']['dated'],'DatedStoreRequest')
    # backbook.specification and combined_specification are raw serde_json::Value.
    return out

def primitive(value,kind):
    if kind=='card_v2':
        from .card_credit_v2_reference import _typed
        return _typed(value)
    if kind not in ('installment','card'):raise ValueError('explicit primitive DTO tag required')
    out=deepcopy(value);out['contract']=dto(out['contract'],'InstallmentRequest'if kind=='installment'else'CardRequest')
    out['acquisition']=fields(out['acquisition'],('principal','cost','original_annual_effective_rate'))
    out=fields(out,('opening_allowance',))
    out['allowance_reviews']=[fields(v,('target_amount',))for v in out['allowance_reviews']]
    out['chargeoffs']=[fields(v,('principal_amount','book_adjustment_amount','accrued_interest_reversal'))for v in out['chargeoffs']]
    return out

def owners(value):
    out=deepcopy(value)
    for o in out:
        o.update(fields(o,('unit_face','allocated_face')))
        if o['initial_allowance_review']is not None:o['initial_allowance_review']=fields(o['initial_allowance_review'],('amount',))
        for run in o['runs']:
            run['severity']=number(run['severity'])
            for p in run['paths']:
                p['source_weight']=number(p['source_weight'])
                p['request']['request']=primitive(p['request']['request'],p['request']['kind'])
    return out

def mixed(value):
    out=deepcopy(value);out['performing']=performing(out['performing']);out['product_owners']=owners(out['product_owners'])
    # Root combined_specification is Value, not the normalized Spec receipt.
    return out


def deposit_owner(value):
    """Ledger source1 Request: only runs/source is typed, specification is Value."""
    out=deepcopy(value)
    for run in out['runs']:
        run['severity']=number(run['severity'])
        c=run['source']['continuation']
        c['anchor']=fields(c['anchor'],('original_principal','original_price_per_100','original_gross_carrying_value','original_annual_effective_rate'))
        c['prefix']['drivers']=[dto(v,'DepositDriver')for v in c['prefix']['drivers']]
        for name in ('original','current'):c[name]=dto(c[name],'DepositStateRequest')
    return out


def deposit_portfolio(value):
    """Declared Ledger source2 Request: runs are typed; specification stays Value."""
    out=deepcopy(value)
    for run in out['runs']:run['severity']=number(run['severity'])
    for owner in out['positions']:
        for path in owner['paths']:
            c=path['source']['continuation']
            c['anchor']=fields(c['anchor'],('original_principal','original_price_per_100','original_gross_carrying_value','original_annual_effective_rate'))
            c['prefix']['drivers']=[dto(v,'DepositDriver')for v in c['prefix']['drivers']]
            for name in ('original','current'):c[name]=dto(c[name],'DepositStateRequest')
    return out
