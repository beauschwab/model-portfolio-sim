"""Hand competing-risk, censoring and chronological challenger diagnostics."""
from copy import deepcopy

import pytest

from portfolio_risk.analytics.consumer_challenger_reference import SCHEMA, evaluate


@pytest.fixture
def panel():
    rows = []
    for split, origin in [('train', 1000), ('holdout', 1101)]:
        for i, (kind, age) in enumerate([('default', 5), ('payoff', 10),
                                         (None, 20), ('default', 15)]):
            rows.append(dict(account_id=f'{split}-{i}', product='auto', stratum='group-a',
                vintage='supplied-vintage', cohort_id='cohort-a', origin_ordinal=origin,
                observed_through=origin + 20, exit_ordinal=origin + age if kind else None,
                exit_kind=kind))
    return dict(schema=SCHEMA, model_version=SCHEMA, source_id='synthetic-hand-panel',
        source_revision='1', data_kind='synthetic', training_split_id='through-1100',
        holdout_split_id='after-1100', training_end=1100, evaluation_as_of=1200,
        horizons_days=[10, 20], minimum_at_risk=2,
        assumptions=['independent_censoring_within_stratum',
                     'stable_censoring_across_training_and_holdout'], observations=rows)


@pytest.mark.parametrize('product', ['auto', 'personal', 'credit_card'])
def test_hand_competing_risks_and_time_dependent_scores(panel, product):
    for row in panel['observations']:
        row['product'] = product
    out = evaluate(panel)
    first, last = out['fit'][0]['periods']
    assert (first['default_probability'], first['payoff_probability'], first['survival']) == (.25, .25, .5)
    assert (last['cumulative_default'], last['cumulative_payoff'], last['survival']) == (.5, .25, .25)
    for period in out['fit'][0]['periods']:
        assert period['survival'] + period['cumulative_default'] + period['cumulative_payoff'] == 1.
    first, last = out['holdout_metrics']
    assert first['brier_score'] == .1875 and last['brier_score'] == .25
    assert first['calibration_error'] == last['calibration_error'] == 0.
    assert all(r['account_id'].startswith('holdout') for r in out['holdout_predictions'])
    assert out['adoption_eligible'] is False and out['accounting_standard_approval'] is False


def test_partial_censoring_uses_risk_sets_and_unrenormalized_ipcw(panel):
    for split, origin in [('train', 1000), ('holdout', 1101)]:
        rows = [r for r in panel['observations'] if r['account_id'].startswith(split)]
        for row, kind, age in zip(rows, [None, 'default', 'payoff', None], [5, 10, 15, 20]):
            row.update(observed_through=origin + (age if kind is None else 20),
                       exit_kind=kind, exit_ordinal=origin + age if kind else None)
    out = evaluate(panel)
    assert out['fit'][0]['periods'][0]['default_probability'] == pytest.approx(1 / 3)
    for metric in out['holdout_metrics']:
        assert metric['brier_score'] == pytest.approx(2 / 9)
        assert metric['ipcw_observed_default'] == pytest.approx(1 / 3)
        assert metric['censored_before_horizon'] == 1
    assert out['fit'][0]['censoring_km'][0]['survival_after'] == .75


def test_holdout_labels_and_post_cutoff_training_events_never_fit(panel):
    # A training account's later exit is unavailable at the training cutoff.
    panel['observations'][2].update(observed_through=1150, exit_ordinal=1120, exit_kind='default')
    first = evaluate(panel)
    changed = deepcopy(panel)
    changed['observations'][2].update(exit_ordinal=1130, exit_kind='payoff')
    for row in changed['observations'][4:]:
        row.update(exit_ordinal=None, exit_kind=None)
    later = evaluate(changed)
    assert first['fit'] == later['fit']
    assert first['holdout_metrics'] != later['holdout_metrics']


def test_observation_order_does_not_change_fit_or_score(panel):
    before = evaluate(panel)
    panel['observations'].reverse()
    assert evaluate(panel) == before


@pytest.mark.parametrize('defect', ['overlap', 'unknown', 'no_censoring_assumption', 'thin',
                                  'event_before_origin', 'wrong_split', 'unseen_stratum'])
def test_invalid_or_unidentified_panels_fail_explicitly(panel, defect):
    if defect == 'overlap':
        panel['observations'][4]['account_id'] = panel['observations'][0]['account_id']
    elif defect == 'unknown':
        panel['observations'][0]['unknown_covariate'] = 1
    elif defect == 'no_censoring_assumption':
        panel['assumptions'] = ['some-supplied-assumption']
    elif defect == 'thin':
        panel['minimum_at_risk'] = 3
    elif defect == 'event_before_origin':
        panel['observations'][0]['exit_ordinal'] = 999
    elif defect == 'wrong_split':
        panel['holdout_split_id'] = panel['training_split_id']
    else:
        panel['observations'][4]['stratum'] = 'unseen'
    with pytest.raises(ValueError, match='consumer challenger'):
        evaluate(panel)


def test_censoring_positivity_is_not_clipped(panel):
    panel['horizons_days'] = [20]
    for row in panel['observations'][:4]:
        row.update(observed_through=1005, exit_ordinal=None, exit_kind=None)
    with pytest.raises(ValueError, match='positivity'):
        evaluate(panel)


@pytest.mark.parametrize('stop', [1, 4, 15, 24, 'last'])
def test_cancellation_through_validation_fitting_and_holdout(panel, stop):
    if stop == 'last':
        checkpoints = []
        evaluate(panel, lambda: checkpoints.append(1))
        stop = len(checkpoints)
    calls = [0]
    def check():
        calls[0] += 1
        return calls[0] == stop
    with pytest.raises(InterruptedError):
        evaluate(panel, check)


def test_comparisons_use_same_holdout_rows_and_unrenormalized_censor_weights(panel):
    from portfolio_risk.analytics.consumer_challenger_reference import compare_predictions
    baseline = evaluate(panel)
    model = dict(model_id='perfect-hand', model_version='1', training_source_id=panel['source_id'],
        training_end=panel['training_end'], predictions=[dict(account_id=r['account_id'],
            horizon_days=r['horizon_days'], predicted_default=r['observed_default']) for r in baseline['holdout_predictions']])
    result = compare_predictions(panel, [model])
    assert all(m['brier_score'] == 0. for m in result['comparisons'][0]['holdout_metrics'])
    assert result['challenger'] == baseline
    for change in ('missing', 'duplicate', 'training_row', 'leak', 'domain'):
        bad = deepcopy(model)
        if change == 'missing': bad['predictions'].pop()
        elif change == 'duplicate': bad['predictions'][1] = bad['predictions'][0]
        elif change == 'training_row': bad['predictions'][0]['account_id'] = 'train-0'
        elif change == 'leak': bad['training_end'] += 1
        else: bad['predictions'][0]['predicted_default'] = True
        with pytest.raises(ValueError): compare_predictions(panel, [bad])


def test_tied_observed_events_precede_censoring_in_reverse_km(panel):
    panel['minimum_at_risk'] = 1
    for row in panel['observations']:
        index = int(row['account_id'][-1]); origin = row['origin_ordinal']
        kind, age = [('default',5),(None,5),('payoff',10),(None,20)][index]
        row.update(exit_kind=kind, exit_ordinal=origin+age if kind else None,
                   observed_through=origin+age)
    result = evaluate(panel)
    step = result['fit'][0]['censoring_km'][0]
    assert step['at_risk'] == 4 and step['censoring_at_risk'] == 3
    assert step['survival_after'] == pytest.approx(2/3)
    assert result['holdout_metrics'][0]['brier_score'] == pytest.approx(3/16)
