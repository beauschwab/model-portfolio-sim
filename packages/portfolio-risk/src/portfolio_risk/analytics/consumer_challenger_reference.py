"""Analysis-only discrete-time competing-risk challenger and holdout scoring.

This module is deliberately absent from production dispatch. Exact observed
default/payoff dates define empirical competing-risk transitions on each supplied
time bin. Product-limit risk sets retain partial censored observations. Training-only
stratum-specific censoring Kaplan-Meier weights score holdout cumulative default
incidence. Neither a fitted challenger nor a good score grants adoption.
"""
import math
import unicodedata
from bisect import bisect_left, bisect_right
from collections import Counter


SCHEMA = 'consumer-survival-challenger-1'


def evaluate(raw, check=lambda: False):
    def guard():
        if check():
            raise InterruptedError('consumer challenger cancelled')

    def require(condition, text):
        if not condition:
            raise ValueError('consumer challenger ' + text)

    def label(value):
        require(type(value) is str and 0 < len(value.encode()) <= 256
                and value.strip() and not any(unicodedata.category(c) == 'Cc' for c in value), 'identity')

    def day(value):
        require(type(value) is int and 1 <= value <= 3652059, 'ordinal domain')

    guard()
    require(type(raw) is dict and set(raw) == {
        'schema', 'model_version', 'source_id', 'source_revision', 'data_kind',
        'training_split_id', 'holdout_split_id', 'training_end', 'evaluation_as_of',
        'horizons_days', 'minimum_at_risk', 'assumptions', 'observations'}, 'request fields')
    require(raw['schema'] == SCHEMA and raw['model_version'] == SCHEMA, 'schema/version')
    for name in ('source_id', 'source_revision', 'training_split_id', 'holdout_split_id'):
        label(raw[name])
    require(raw['training_split_id'] != raw['holdout_split_id'], 'split identities differ')
    require(raw['data_kind'] in ('synthetic', 'observed'), 'data kind')
    day(raw['training_end']); day(raw['evaluation_as_of'])
    require(raw['evaluation_as_of'] > raw['training_end'], 'chronological evaluation')
    horizons = raw['horizons_days']
    require(type(horizons) is list and 1 <= len(horizons) <= 120
            and all(type(h) is int and 0 < h <= 36500 for h in horizons)
            and all(a < b for a, b in zip(horizons, horizons[1:])), 'horizon grid')
    minimum = raw['minimum_at_risk']
    require(type(minimum) is int and 1 <= minimum <= 100_000, 'minimum exposure')
    assumptions = raw['assumptions']
    require(type(assumptions) is list and 1 <= len(assumptions) <= 64, 'assumption list')
    for assumption in assumptions:
        label(assumption)
    require({'independent_censoring_within_stratum',
             'stable_censoring_across_training_and_holdout'} <= set(assumptions),
            'explicit censoring assumptions required')
    observations = raw['observations']
    require(type(observations) is list and 2 <= len(observations) <= 20_000
            and len(observations) * len(horizons) <= 100_000, 'work admission')
    groups, seen = {}, set()
    for row in observations:
        guard()
        require(type(row) is dict and set(row) == {
            'account_id', 'product', 'stratum', 'vintage', 'cohort_id', 'origin_ordinal',
            'observed_through', 'exit_ordinal', 'exit_kind'}, 'observation fields')
        for name in ('account_id', 'stratum', 'vintage', 'cohort_id'):
            label(row[name])
        require(row['account_id'] not in seen, 'account overlaps within or between splits')
        seen.add(row['account_id'])
        require(row['product'] in ('auto', 'personal', 'credit_card'), 'product')
        origin, through, event = row['origin_ordinal'], row['observed_through'], row['exit_ordinal']
        day(origin); day(through)
        require(origin <= through <= raw['evaluation_as_of'], 'observation chronology')
        if event is None:
            require(row['exit_kind'] is None, 'absent event kind')
        else:
            day(event)
            require(origin < event <= through and row['exit_kind'] in ('default', 'payoff'),
                    'exact event chronology/kind')
        training = origin <= raw['training_end']
        cutoff = min(through, raw['training_end']) if training else through
        observed_exit = event is not None and event <= cutoff
        terminal = event if observed_exit else cutoff
        sample = dict(row, age=terminal - origin,
                      outcome=row['exit_kind'] if observed_exit else 'censored')
        key = row['product'], row['stratum']
        group = groups.setdefault(key, dict(training=[], holdout=[]))
        group['training' if training else 'holdout'].append(sample)
    require(len(groups) <= 256, 'stratum admission')
    fits, metrics, predictions = [], [], []
    for (product, stratum), group in sorted(groups.items()):
        guard()
        train = sorted(group['training'], key=lambda r: r['account_id'])
        held = sorted(group['holdout'], key=lambda r: r['account_id'])
        require(train and held, 'every stratum needs training and holdout observations')
        terminal = {}
        for row in train:
            terminal.setdefault(row['age'], Counter())[row['outcome']] += 1
        ages_sorted = sorted(terminal)
        risk = sum(row['age'] > 0 for row in train)
        survival, default_cif, payoff_cif, start = 1., 0., 0., 0
        fitted = []
        for horizon in horizons:
            guard()
            at_risk, outcomes = risk, Counter()
            require(at_risk >= minimum, 'unidentified training period; no implicit fixed-row fallback')
            local_survival, default_hazard, payoff_hazard = 1., 0., 0.
            for age in ages_sorted[bisect_right(ages_sorted, start):bisect_right(ages_sorted, horizon)]:
                guard()
                events = terminal[age]
                require(risk > 0, 'empty event risk set')
                default_hazard += local_survival * events['default'] / risk
                payoff_hazard += local_survival * events['payoff'] / risk
                local_survival *= (risk - events['default'] - events['payoff']) / risk
                risk -= sum(events.values())
                outcomes.update(events)
            default_cif += survival * default_hazard
            payoff_cif += survival * payoff_hazard
            survival *= local_survival
            fitted.append(dict(horizon_days=horizon, at_risk=at_risk,
                defaults=outcomes['default'], payoffs=outcomes['payoff'],
                censored_observations=outcomes['censored'], default_probability=default_hazard,
                payoff_probability=payoff_hazard, cumulative_default=default_cif,
                cumulative_payoff=payoff_cif, survival=survival))
            start = horizon

        # KM for censoring: default/payoff are observed competing exits, not
        # censoring. Observed events precede censoring at a tied exact day;
        # remove those events from the reverse-KM censoring denominator.
        deaths, censored = Counter(), Counter()
        for row in train:
            deaths[row['age']] += 1
            censored[row['age']] += row['outcome'] == 'censored'
        risk, g, km = len(train), 1., []
        for age in sorted(deaths):
            prior = g
            censor_risk = risk - (deaths[age] - censored[age])
            if censored[age]:
                g *= 1 - censored[age] / censor_risk
            km.append(dict(age_days=age, at_risk=risk, censoring_at_risk=censor_risk, censored=censored[age],
                           survival_before=prior, survival_after=g))
            risk -= deaths[age]
        ages = [step['age_days'] for step in km]

        def censor_survival(age, before=False):
            index = (bisect_left if before else bisect_right)(ages, age)
            value = km[index - 1]['survival_after'] if index else 1.
            require(value > 0, 'censoring positivity absent at requested horizon')
            return value

        fits.append(dict(product=product, stratum=stratum, training_rows=len(train),
                         periods=fitted, censoring_km=km))
        for period in fitted:
            guard()
            h, predicted = period['horizon_days'], period['cumulative_default']
            censor_survival(h, before=True)
            bins = {}
            for row in held:
                guard()
                exited = row['outcome'] != 'censored' and row['age'] <= h
                if exited:
                    weight = 1 / censor_survival(row['age'], before=True)
                    observed = float(row['outcome'] == 'default')
                elif row['age'] >= h:
                    # observed_through is inclusive; a censor at h still
                    # establishes the no-default outcome through day h.
                    weight = 1 / censor_survival(h, before=True)
                    observed = 0.
                else:
                    weight, observed = 0., 0.
                record = dict(account_id=row['account_id'], product=product,
                    stratum=stratum, vintage=row['vintage'], cohort_id=row['cohort_id'],
                    horizon_days=h, predicted_default=predicted, observed_default=observed,
                    evaluation_weight=weight, censored_before_horizon=weight == 0.)
                predictions.append(record)
                bins.setdefault((row['vintage'], row['cohort_id']), []).append(record)
            for (vintage, cohort), rows in sorted(bins.items()):
                n = len(rows)
                observed = math.fsum(r['evaluation_weight'] * r['observed_default'] for r in rows) / n
                brier = math.fsum(r['evaluation_weight'] * (r['observed_default'] - predicted) ** 2
                                  for r in rows) / n
                require(math.isfinite(brier) and math.isfinite(observed), 'score overflow')
                metrics.append(dict(product=product, stratum=stratum, vintage=vintage,
                    cohort_id=cohort, horizon_days=h, holdout_rows=n,
                    predicted_default=predicted, ipcw_observed_default=observed,
                    calibration_error=observed - predicted, brier_score=brier,
                    censored_before_horizon=sum(r['censored_before_horizon'] for r in rows)))
    guard()
    return dict(schema=SCHEMA, model_version=SCHEMA, source_id=raw['source_id'],
        source_revision=raw['source_revision'], data_kind=raw['data_kind'],
        training_split_id=raw['training_split_id'], holdout_split_id=raw['holdout_split_id'],
        training_end=raw['training_end'], evaluation_as_of=raw['evaluation_as_of'],
        assumptions=list(assumptions), fit=fits, holdout_metrics=metrics,
        holdout_predictions=predictions, deterministic=True, adoption_eligible=False,
        accounting_standard_approval=False, scope='independent_analysis_only')


def compare_predictions(raw, comparisons, check=lambda: False):
    """Score declared model predictions on exactly the challenger's holdout rows.

    This is independent analysis tooling. Prediction lineage is retained, not
    certified as an approved fit. A validation pack must bind each fit to its panel.
    """
    base = evaluate(raw, check)
    if type(comparisons) is not list or not 1 <= len(comparisons) <= 8:
        raise ValueError('consumer challenger comparison count')
    expected = {(r['account_id'], r['horizon_days']): r for r in base['holdout_predictions']}
    models, reports = set(), []
    for model in comparisons:
        if check(): raise InterruptedError('consumer challenger comparison cancelled')
        if type(model) is not dict or set(model) != {'model_id', 'model_version', 'training_source_id',
                'training_end', 'predictions'}:
            raise ValueError('consumer challenger comparison fields')
        for key in ('model_id', 'model_version', 'training_source_id'):
            value = model[key]
            if type(value) is not str or not value.strip() or len(value.encode()) > 256 or any(unicodedata.category(c) == 'Cc' for c in value):
                raise ValueError('consumer challenger comparison identity')
        if model['model_id'] in models or type(model['training_end']) is not int or not 1 <= model['training_end'] <= raw['training_end']:
            raise ValueError('consumer challenger comparison identity/training')
        models.add(model['model_id'])
        if type(model['predictions']) is not list or len(model['predictions']) != len(expected):
            raise ValueError('consumer challenger exact holdout prediction coverage')
        seen, bins = set(), {}
        for prediction in model['predictions']:
            if check(): raise InterruptedError('consumer challenger comparison cancelled')
            if type(prediction) is not dict or set(prediction) != {'account_id', 'horizon_days', 'predicted_default'}:
                raise ValueError('consumer challenger prediction fields')
            if type(prediction['account_id']) is not str or type(prediction['horizon_days']) is not int:
                raise ValueError('consumer challenger prediction identity type')
            key = (prediction['account_id'], prediction['horizon_days']); value = prediction['predicted_default']
            if key not in expected or key in seen or type(value) not in (int, float) or not math.isfinite(value) or not 0. <= value <= 1.:
                raise ValueError('consumer challenger prediction domain/coverage')
            seen.add(key); row = expected[key]
            group = tuple(row[k] for k in ('product', 'stratum', 'vintage', 'cohort_id', 'horizon_days'))
            bins.setdefault(group, []).append((row, value))
        metrics = []
        for group, rows in sorted(bins.items()):
            n = len(rows)
            predicted = math.fsum(p for _, p in rows)/n
            observed = math.fsum(r['evaluation_weight']*r['observed_default'] for r, _ in rows)/n
            brier = math.fsum(r['evaluation_weight']*(r['observed_default']-p)**2 for r, p in rows)/n
            if not math.isfinite(brier): raise ValueError('consumer challenger comparison score overflow')
            metrics.append(dict(zip(('product', 'stratum', 'vintage', 'cohort_id', 'horizon_days'), group),
                holdout_rows=n, predicted_default=predicted, ipcw_observed_default=observed,
                calibration_error=observed-predicted, brier_score=brier))
        reports.append(dict(model_id=model['model_id'], model_version=model['model_version'],
            training_source_id=model['training_source_id'], training_end=model['training_end'],
            holdout_metrics=metrics, predictions=model['predictions']))
    if check(): raise InterruptedError('consumer challenger comparison cancelled')
    return dict(challenger=base, comparisons=reports, adoption_eligible=False,
                scope='independent_analysis_only')
