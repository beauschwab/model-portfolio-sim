"""Independent complete-path economic loss replay; never a production backend."""
from copy import deepcopy
import math

from .credit_path_reference import simulate_path
from ._deposit_continuation_income_reference import native_json_length
from .modeled_json_identity import exact_json_identity
from .lifetime_loss_validation import validate, typed, source_for, ordered_sum

SCHEMA = 'lifetime-credit-loss-1'


def equal(actual, expected, check, location='lifetime loss'):
    check()
    if type(expected) is dict:
        if type(actual) is not dict or actual.keys() != expected.keys():
            raise ArithmeticError(location + ' fields')
        for key, value in expected.items():
            equal(actual[key], value, check, location + '/' + key)
    elif type(expected) is list:
        if type(actual) is not list or len(actual) != len(expected):
            raise ArithmeticError(location + ' length')
        for index, (a, b) in enumerate(zip(actual, expected)):
            equal(a, b, check, location + '/' + str(index))
    elif type(expected) is float:
        if type(actual) not in (int, float) or not math.isfinite(actual) or not math.isclose(
                actual, expected, rel_tol=3e-12, abs_tol=3e-12):
            raise ArithmeticError(location + ' value')
    elif type(actual) is not type(expected) or actual != expected:
        raise ArithmeticError(location + ' identity')


def reconstruct(raw, check=lambda: False):
    def guard():
        if check():
            raise InterruptedError('lifetime loss verification cancelled')
    guard()
    r = typed(raw)
    work, size = validate(r, guard)
    # Validate the entire scenario/model cross-product before replaying any path.
    for cohort in r['cohorts']:
        for model in cohort['models']:
            for scenario in r['scenarios']:
                source_for(cohort, model, scenario, r['drivers'], guard)
    cells = []
    for cohort in r['cohorts']:
        for model in cohort['models']:
            for scenario in r['scenarios']:
                source = source_for(cohort, model, scenario, r['drivers'], guard)
                cell = dict(cohort_id=cohort['cohort_id'], product=cohort['product'],
                    vintage=cohort['vintage'], model_id=model['model_id'],
                    model_version=model['model_version'], model_kind=model['kind'],
                    scenario_id=scenario['scenario_id'], expected_principal_loss=0.,
                    expected_signed_interest_loss=0., expected_total_loss=0.,
                    expected_default_probability=0., full_paths=[])
                for opening, weight in zip(source['paths'], cohort['probability_weights']):
                    guard()
                    result, _ = simulate_path(source, opening, guard)
                    if result['outcome'] not in ('default', 'prepaid', 'matured'):
                        raise ValueError('lifetime loss incomplete contractual tail')
                    default = result['default']
                    if default is not None:
                        interest = default['accrued_interest'] - ordered_sum(p['interest'] for p in default['recoveries'])
                        cell['expected_principal_loss'] += weight * default['economic_principal_loss'] * cohort['monetary_multiplier']
                        cell['expected_signed_interest_loss'] += weight * interest * cohort['monetary_multiplier']
                        cell['expected_default_probability'] += weight
                    cell['full_paths'].append(result)
                cell['expected_total_loss'] = cell['expected_principal_loss'] + cell['expected_signed_interest_loss']
                for key in ('expected_principal_loss', 'expected_signed_interest_loss', 'expected_total_loss'):
                    if not math.isfinite(cell[key]) or abs(cell[key]) > 1e15:
                        raise ValueError('lifetime loss aggregate monetary domain')
                cells.append(cell)
    bands = []
    for cohort in r['cohorts']:
        selections = [('macro', m['model_id']) for m in cohort['models']]
        selections += [('model', s['scenario_id']) for s in r['scenarios']]
        for dimension, selection in selections:
            values = [c['expected_total_loss'] for c in cells if c['cohort_id'] == cohort['cohort_id']
                      and c['model_id' if dimension == 'macro' else 'scenario_id'] == selection]
            bands.append(dict(cohort_id=cohort['cohort_id'], dimension=dimension,
                fixed_selection=selection, minimum_loss=min(values), maximum_loss=max(values),
                spread=max(values)-min(values)))
    totals, backtests = {}, []
    for cell in cells:
        guard()
        key = (cell['product'], cell['model_kind'], cell['scenario_id'])
        total = totals.setdefault(key, dict(product=key[0], model_kind=key[1], scenario_id=key[2],
            expected_principal_loss=0., expected_signed_interest_loss=0., expected_total_loss=0., cohorts=[]))
        for field in ('expected_principal_loss', 'expected_signed_interest_loss'):
            total[field] += cell[field]
        total['expected_total_loss'] = total['expected_principal_loss'] + total['expected_signed_interest_loss']
        total['cohorts'].append(cell['cohort_id'])
        for field in ('expected_principal_loss', 'expected_signed_interest_loss', 'expected_total_loss'):
            if not math.isfinite(total[field]) or abs(total[field]) > 1e15:
                raise ValueError('lifetime loss product monetary domain')
        if cell['scenario_id'] == r['base_scenario_id']:
            for outcome in r['outcomes']:
                if outcome['cohort_id'] != cell['cohort_id']:
                    continue
                reported = outcome['reported_principal_loss'] + outcome['reported_signed_interest_loss']
                if abs(reported) > 1e15:
                    raise ValueError('lifetime loss reported monetary domain')
                backtests.append(dict(cohort_id=cell['cohort_id'], vintage=cell['vintage'],
                    model_id=cell['model_id'], outcome_source_id=outcome['source_id'],
                    outcome_source_revision=outcome['source_revision'], projected_total_loss=cell['expected_total_loss'],
                    reported_total_loss=reported, reported_minus_projected=reported-cell['expected_total_loss']))
    result = dict(schema=SCHEMA, raw_source=r, cells=cells, sensitivity=bands,
        product_totals=list(totals.values()), holdout_outcomes=backtests,
        admitted_work_upper=work, admitted_output_bytes_upper=size,
        complete_unweighted_paths_replayed=True, common_random_numbers_retained=True,
        observed_accuracy_validated=False, allowance_or_accounting_approval=False, adoption_eligible=False)
    guard()
    if native_json_length(result) > size:
        raise ValueError('lifetime loss prospective output bound')
    return result


def verify(raw, result, check=lambda: False):
    expected = reconstruct(raw, check)
    if not exact_json_identity(result.get('raw_source'), expected['raw_source']):
        raise ArithmeticError('lifetime loss raw source identity')
    def guard():
        if check():
            raise InterruptedError('lifetime loss verification cancelled')
    equal(result, expected, guard)
    guard()
    return dict(verified=True, complete_unweighted_paths_recomputed=True,
        scenario_model_sensitivity_recomputed=True, product_outcomes_recomputed=True,
        empirical_validation=False, accounting_approval=False, adoption_eligible=False)
