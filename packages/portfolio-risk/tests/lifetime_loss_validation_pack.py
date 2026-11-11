"""Reproducible synthetic #95 analysis pack; not observed/model adoption evidence.

Run from the repository root with the engine on PYTHONPATH and matching native
artifacts: python packages/portfolio-risk/tests/lifetime_loss_validation_pack.py OUTPUT.json
"""
from copy import deepcopy
import hashlib
import json
import math
from pathlib import Path
import sys

from portfolio_risk.analytics import model_contracts as native
from portfolio_risk.analytics.consumer_challenger_reference import compare_predictions
from portfolio_risk.analytics.lifetime_loss_reference import verify

ROOT = Path(__file__).resolve().parents[3]


def build():
    fit_raw = json.loads((ROOT/'examples/models/credit-covariate-1.json').read_text())
    train_end = fit_raw['train_end']['ordinal']
    fit_raw['holdout_end']['ordinal'] += 1
    training = deepcopy(fit_raw['segments'])
    for row in training:
        held = deepcopy(row)
        held.update(account_id='held-'+row['account_id'], segment_id='held-'+row['segment_id'],
            start=dict(ordinal=train_end+1, seconds_of_day=0),
            end=dict(ordinal=train_end+366, seconds_of_day=0),
            available_time=dict(ordinal=train_end+1, seconds_of_day=0), source=fit_raw['holdout_source'])
        fit_raw['segments'].append(held)
    constant_raw = dict(schema='observed-credit-calibration-1', source_id='same-synthetic-panel',
        source_revision='1', population_id='synthetic-population', model_policy_id='constant-hazard-mle',
        training_split_id='train', holdout_split_id='holdout', observation_mode='exact_transition_times',
        data_kind='synthetic', train_start=fit_raw['train_start'], train_end=fit_raw['train_end'],
        holdout_end=fit_raw['holdout_end'], unobserved_row_policy=[dict(kind='reject')]+[
            dict(kind='supplied_row', transition_hazards_per_year=[0.]*5,
                 assumption_id=state+'-off', rationale='Explicit unobserved-origin assumption')
            for state in ('watch', 'delinquent')], spells=[dict(spell_id=r['segment_id'],
                account_id=r['account_id'], start=r['start'], end=r['end'], origin=r['origin'],
                ending=r['ending']) for r in fit_raw['segments']])
    cov_fit = native.fit_credit_covariates(fit_raw)
    constant_fit = native.fit_observed_credit(constant_raw)
    edge = cov_fit['model']['rows'][0]['edges'][0]
    baseline, beta = math.exp(edge['intercept']['value']), edge['covariate_coefficients'][0]['value']
    constant = constant_fit['transition_hazards_per_year'][0][3]
    # Independent exact-spell MLE: three defaults/eight exposure years;
    # grouped hazards one/four and two/four. These assertions bind the fit.
    assert abs(constant-3/8) < 1e-12 and abs(baseline-.25) < 1e-9 and abs(math.exp(beta)-2.) < 1e-9
    observations, groups = [], {}
    for row in fit_raw['segments']:
        group = int(row['covariates'][0]); groups[row['account_id']] = group
        event = row['ending']['kind'] == 'transition'
        observations.append(dict(account_id=row['account_id'], product='auto', stratum=f'group-{group}',
            vintage='synthetic-holdout' if row['account_id'].startswith('held-') else 'synthetic-training',
            cohort_id=f'group-{group}', origin_ordinal=row['start']['ordinal'], observed_through=row['end']['ordinal'],
            exit_ordinal=row['end']['ordinal'] if event else None, exit_kind='default' if event else None))
    panel = dict(schema='consumer-survival-challenger-1', model_version='consumer-survival-challenger-1',
        source_id='same-synthetic-panel', source_revision='1', data_kind='synthetic',
        training_split_id='train', holdout_split_id='holdout', training_end=train_end,
        evaluation_as_of=train_end+366, horizons_days=[180,365], minimum_at_risk=1,
        assumptions=['independent_censoring_within_stratum','stable_censoring_across_training_and_holdout'],
        observations=observations)
    comparisons = []
    for name in ('constant_row','supplied_covariate','fixed_row_assumption'):
        predictions = []
        for row in observations:
            if row['origin_ordinal'] <= train_end: continue
            hazard = constant if name == 'constant_row' else baseline*math.exp(beta*groups[row['account_id']]) if name == 'supplied_covariate' else .1
            for h in panel['horizons_days']:
                # Independent analytical CTMC score, not a production callback.
                predictions.append(dict(account_id=row['account_id'], horizon_days=h,
                    predicted_default=-math.expm1(-hazard*h/365.)))
        comparisons.append(dict(model_id=name, model_version='same-panel-1', training_source_id=panel['source_id'],
            training_end=train_end, predictions=predictions))
    diagnostics = compare_predictions(panel, comparisons)
    request = json.loads((ROOT/'examples/models/lifetime-credit-loss-1.json').read_text())
    request.update(source_id='synthetic-same-panel-loss', training_end=train_end,
        training_split_id='train', holdout_split_id='holdout', outcomes=[], cohorts=[])
    request['drivers'][0].update(driver_id='unemployment', scaling_training_end=train_end,
        scaling_source_id=panel['source_id'], minimum=-2., maximum=3., availability_lag_days=0)
    for s in request['scenarios']: s['forecast_as_of_ordinal']=train_end+1
    template = json.loads((ROOT/'examples/models/lifetime-credit-loss-1.json').read_text())['cohorts'][0]
    for group in (0,1):
        c = deepcopy(template); c.update(cohort_id=f'group-{group}', vintage='synthetic-holdout')
        c['source']['origin_time']['ordinal'] = train_end+1
        c['source']['paths'] = c['source']['paths'][:16]; c['probability_weights'] = [1/16]*16
        c['source']['intervals'][0]['hazards_per_year'] = [[0.,0.,0.,baseline*math.exp(beta*group),0.],[0.]*5,[0.]*5]
        c['models'][0]['hazards_per_year'] = constant_fit['transition_hazards_per_year']
        c['models'][1].update(training_end=train_end, coefficient_source_id=panel['source_id'],
            coefficient_source_revision=cov_fit['model']['coefficient_identity_sha256'])
        c['models'][1]['beta'][3][0] = beta
        c['models'][2]['hazards_per_year'] = [[[0.,0.,0.,.1,0.],[0.]*5,[0.]*5]]
        request['cohorts'].append(c)
        request['outcomes'].append(dict(cohort_id=c['cohort_id'], vintage=c['vintage'],
            source_id='synthetic-completed-recoveries', source_revision='1', monetary_unit=c['source']['monetary_unit'],
            observed_through_year=3., reported_principal_loss=2.5*(group+1), reported_signed_interest_loss=0.))
    result = native.evaluate_lifetime_credit_loss(request); receipt = verify(request,result)
    zero = deepcopy(request)
    for scenario in zero['scenarios']: scenario['lag_aligned_values'] = [[0.]]
    collapsed = native.evaluate_lifetime_credit_loss(zero); verify(zero,collapsed)
    assert all(b['spread'] == 0. for b in collapsed['sensitivity'] if b['dimension']=='macro')
    assert all(b['spread'] > 0. for b in result['sensitivity'] if b['dimension']=='macro' and b['fixed_selection']=='covariate')
    wide = deepcopy(request)
    for scenario in wide['scenarios']:
        scenario['lag_aligned_values'][0][0] *= 2.
    wider = native.evaluate_lifetime_credit_loss(wide); verify(wide,wider)
    widths = lambda value:[b['spread'] for b in value['sensitivity'] if b['dimension']=='macro' and b['fixed_selection']=='covariate']
    assert all(b >= a for a,b in zip(widths(result), widths(wider)))
    canonical=lambda x:json.dumps(x,sort_keys=True,separators=(',',':'),allow_nan=False).encode()
    return dict(schema='synthetic-lifetime-validation-pack-1', synthetic=True, empirical_validation=False,
        policy_approval=False, native_constant_fit=constant_fit, native_covariate_fit=cov_fit,
        fit_request=fit_raw, constant_fit_request=constant_raw, panel=panel,
        panel_sha256=hashlib.sha256(canonical(panel)).hexdigest(), diagnostics=diagnostics,
        loss_request=request, loss_result=result, loss_verification=receipt,
        zero_macro_spread_verified=True, positive_macro_spread_verified=True,
        wider_declared_macro_spread_verified=True, macro_widths=dict(base=widths(result), double_deviation=widths(wider)),
        modeled_recovery_outcomes_source='synthetic-completed-recoveries',
        validation_scope='same exact-event panel; independent analytical CTMC and discrete-time competing-risk holdout scores; native full-tail Monte Carlo economic loss')


if __name__ == '__main__':
    destination=Path(sys.argv[1]); destination.parent.mkdir(parents=True,exist_ok=True)
    result=build(); destination.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    print(destination, hashlib.sha256(destination.read_bytes()).hexdigest())
