"""Independent supplied Gaussian coupon/credit exercise evidence; test use only."""
from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass, asdict
from datetime import datetime, timezone
import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess
import time

from scipy.special import ndtr

from commercial_convergence_benchmark import (
    Cell, request as base_request, native_binary, source_identity, polynomial,
    FINANCIAL_ATOL, FINANCIAL_RTOL, POLICY_LOSS_LIMIT,
)
from portfolio_risk.core import model_native
from portfolio_risk.analytics.commercial_continuation_reference import verify as verify_holding
from portfolio_risk.analytics.commercial_stochastic_reference import verify, _tape
from portfolio_risk.analytics.credit_path_reference import _pair

ROOT = Path(__file__).resolve().parents[3]
FACE, RECOVERY = 100.0, 60.0
EXERCISE_DAYS, ACCRUAL_DAYS, MATURITY_DAYS = 15, 15, 35
LOADING, STRIKE = 20.0, 100.0
LAWS = {'credit_off': (0.0, 0.0), 'credit_enabled': (0.05, 0.01)}
DRIVERS = {'brownian': 0.0, 'ou': 0.7}
SEEDS = ((7, 11), (19, 23))
SAMPLE_COUNTS = (256, 512)
AMENDMENT = ('Before any 48-cell execution, signed hand controls showed zero interest recovery '
             'retains negative gross claims. The declared family therefore explicitly selects '
             'full signed accrued-interest recovery, including negative cash, with principal '
             'recovery60. Cell laws/rights/seeds/counts/strikes are unchanged; no failing '
             'executed family was replaced. Zero recovery is not a legal waiver or zero claim value.')


@dataclass(frozen=True)
class GaussianCreditCell:
    driver: str
    law: str
    right: str
    paths: int
    training_seed: int
    evaluation_seed: int

    @property
    def key(self):
        return '/'.join(map(str, asdict(self).values()))


def matrix():
    return tuple(GaussianCreditCell(driver, law, right, paths, train, evaluation)
                 for driver in DRIVERS for law in LAWS for right in ('none', 'call', 'put')
                 for paths in SAMPLE_COUNTS for train, evaluation in SEEDS)


def request(cell):
    if cell not in matrix():
        raise ValueError('undeclared Gaussian-credit cell')
    raw = base_request(Cell('analytic', cell.right, 'linear', 'brownian', paths=cell.paths))
    raw['identity']['revision'] = 'gaussian-credit/' + cell.key
    raw['drivers'][0]['mean_reversion'] = DRIVERS[cell.driver]
    raw['coupon_loadings'][0]['loading'] = LOADING
    for role, seed in (('training', cell.training_seed), ('evaluation', cell.evaluation_seed)):
        raw[role].update(paths=cell.paths, market_seed=[seed], credit_seed=seed)
    raw['source']['credit']['max_jumps_per_path'] = 1
    for interval, probability, days in zip(raw['source']['credit']['intervals'],
            LAWS[cell.law], (EXERCISE_DAYS, MATURITY_DAYS - EXERCISE_DAYS), strict=True):
        interval['hazards_per_year'] = [[0.0] * 5 for _ in range(3)]
        interval['hazards_per_year'][0][3] = -math.log1p(-probability) / (days / 365.0)
        interval['loss_given_default'] = [0.4] * 3
        interval['interest_recovery_fraction'] = [1.0] * 3
        interval['complete_recovery_tails'] = [[dict(lag_years=0.0, weight=1.0)] for _ in range(3)]
    return raw


def parameters(cell):
    before, after = LAWS[cell.law]
    e, tau, delta = EXERCISE_DAYS / 365.0, ACCRUAL_DAYS / 365.0, (MATURITY_DAYS - EXERCISE_DAYS) / 365.0
    a = DRIVERS[cell.driver]
    variance = 0.04 * (-math.expm1(-2.0 * a * e) / (2.0 * a) if a else e)
    hazard = -math.log1p(-after) / delta
    expected_accrual = -math.expm1(-hazard * tau) / hazard if hazard else tau
    return dict(before=before, after=after, survival=1.0 - before,
        pre_hazard=-math.log1p(-before) / e, post_hazard=hazard,
        sigma_x=math.sqrt(variance), mean=RECOVERY * after + FACE * (1.0 - after),
        loading=FACE * LOADING * expected_accrual, expected_accrual=expected_accrual)


def density(z):
    return 0.0 if not math.isfinite(z) else math.exp(-0.5 * z * z) / math.sqrt(2.0 * math.pi)


def stop_intervals(right, intercept, slope):
    if right == 'none':
        return []
    if slope == 0.0:
        stop = intercept > STRIKE if right == 'call' else intercept < STRIKE
        return [(-math.inf, math.inf)] if stop else []
    boundary = (STRIKE - intercept) / slope
    upper = (right == 'call') == (slope > 0.0)
    return [(boundary, math.inf)] if upper else [(-math.inf, boundary)]


def analytic(cell, learned_coefficients=None):
    """Exact Gaussian half-line moments; continuation is derived from the raw law."""
    p = parameters(cell)
    true_slope = p['loading'] * p['sigma_x']
    intercept, slope = (p['mean'], true_slope) if learned_coefficients is None else learned_coefficients
    live = p['mean']; stop_mass = 0.0
    intervals = stop_intervals(cell.right, intercept, slope)
    for left, right in intervals:
        mass = float(ndtr(right) - ndtr(left))
        first_moment = density(left) - density(right)
        live += (STRIKE - p['mean']) * mass - true_slope * first_moment
        stop_mass += mass
    return dict(value=RECOVERY * p['before'] + p['survival'] * live,
        option_free_value=RECOVERY * p['before'] + p['survival'] * p['mean'],
        conditional_mean=p['mean'], conditional_standard_deviation=true_slope,
        conditional_loading=p['loading'], optimal_boundary_x=(STRIKE-p['mean']) / p['loading'],
        live_stop_probability=stop_mass,
        stop_intervals=[dict(left=None if not math.isfinite(a) else a,
                             right=None if not math.isfinite(b) else b) for a, b in intervals])


def learned_coefficients(cell, regression):
    """Map every actual varying observed feature onto one independent normal."""
    if (regression['basis_degree'] != 1 or regression['observed_live_states'] != ['performing']
            or regression['rank'] != 2 or set(regression['active_features']) != {0, 5}
            or len(regression['means']) != 10 or len(regression['coefficients']) != 3):
        raise ArithmeticError('one genuinely varying reset state and its duplicate observed rate required')
    sigma = parameters(cell)['sigma_x']
    # X, P/F, legal/F, Watch, Delinquent, observed reset rate,
    # first opening entitlement/F and legal/F, second entitlement/F and legal/F.
    features = [[0.0, sigma], [1.0], [0.0], [0.0], [0.0],
                [0.0, LOADING * sigma], [1.0], [0.0], [1.0], [0.0]]
    for index, feature in enumerate(features):
        if index not in (0, 5) and (regression['means'][index] != feature[0]
                                   or regression['scales'][index] != 1.0):
            raise ArithmeticError('unobserved or piecewise varying contractual feature')
    fitted = polynomial(regression, features).coef
    if len(fitted) != 2 or not all(math.isfinite(float(v)) for v in fitted) or fitted[1] == 0.0:
        raise ArithmeticError('genuine finite varying learned continuation required')
    return [float(v) for v in fitted]


def default_time(cell, sample, path_key):
    p = parameters(cell)
    stream = dict(draw_stream=dict(kind='counter_splitmix64_v1', seed=sample['credit_seed']))
    budget = -math.log(_pair(stream, path_key, 0)[0])
    first = p['pre_hazard'] * EXERCISE_DAYS / 365.0
    if p['pre_hazard'] and budget <= first:
        return budget / p['pre_hazard']
    if p['post_hazard']:
        later = (budget - first) / p['post_hazard']
        if later <= (MATURITY_DAYS - EXERCISE_DAYS) / 365.0:
            return EXERCISE_DAYS / 365.0 + later
    return math.inf


def expected_path(cell, sample, index, x, coefficients):
    death = default_time(cell, sample, index + 1)
    e = EXERCISE_DAYS / 365.0
    if death <= e:
        return dict(value=RECOVERY, eligible=False, decision='continue',
                    death=death, signed_recovery_interest=0.0)
    prediction = None if coefficients is None else coefficients[0] + coefficients[1] * x / parameters(cell)['sigma_x']
    stop = cell.right != 'none' and (prediction > STRIKE if cell.right == 'call' else prediction < STRIKE)
    if stop:
        return dict(value=STRIKE, eligible=True, decision=cell.right, death=death,
                    signed_recovery_interest=0.0)
    if death <= MATURITY_DAYS / 365.0:
        interest = FACE * LOADING * x * min(death - e, ACCRUAL_DAYS / 365.0)
        return dict(value=RECOVERY + interest, eligible=True, decision='continue',
                    death=death, signed_recovery_interest=interest)
    return dict(value=FACE + FACE * LOADING * x * ACCRUAL_DAYS / 365.0,
                eligible=True, decision='continue', death=death, signed_recovery_interest=0.0)


def validate_domain(cell, raw):
    if raw != request(cell):
        raise ArithmeticError('declared Gaussian coupon, credit, clock or identity differs')
    source = raw['source']['source']['current']; origin = source['asof_ordinal']
    coupon = source['periods'][1]['coupon']; observation = coupon['observations'][0]
    clock = (origin + EXERCISE_DAYS - 719163) * 86400
    assert observation['fixing_ordinal'] == origin + EXERCISE_DAYS
    assert observation['selection_cutoff_unix_seconds'] == observation['source']['information_unix_seconds'] == clock
    assert coupon['reset_cutoff_unix_seconds'] == clock and coupon['calculation'] == 'term_simple'
    assert source['periods'][1]['start_ordinal'] == origin + EXERCISE_DAYS
    assert source['periods'][1]['end_ordinal'] == origin + EXERCISE_DAYS + ACCRUAL_DAYS
    assert source['periods'][1]['payment_ordinal'] == origin + MATURITY_DAYS
    assert raw['fixed_base_oas'] == raw['parallel_bump'] == 0.0
    assert raw['source']['source']['acquisition']['cost'] == FACE
    assert raw['source']['source']['acquisition']['original_annual_effective_rate'] == 0.0
    assert all(v == 1.0 for v in raw['discount_curve']['discount_factors'])
    assert not raw['hazard_loadings']
    assert raw['source']['source']['original']['measure'] == source['measure'] == 'pricing'
    assert raw['training']['randomness_domain'] != raw['evaluation']['randomness_domain']


def assess(cell, raw, actual):
    validate_domain(cell, raw)
    optimal = analytic(cell)
    if cell.right == 'none':
        if actual['regressions']:
            raise ArithmeticError('option-free fit differs')
        coefficients = None
    else:
        if len(actual['regressions']) != 1:
            raise ArithmeticError('one learned exercise date required')
        coefficients = learned_coefficients(cell, actual['regressions'][0])
    learned = analytic(cell, coefficients)
    levels = _tape(raw, raw['evaluation'], lambda: False)
    step = raw['time_ordinals'].index(raw['source']['source']['current']['asof_ordinal'] + EXERCISE_DAYS)
    states = []; values = []; origin = raw['source']['source']['current']['asof_ordinal']
    for index, row in enumerate(actual['evaluation_paths']):
        expected = expected_path(cell, raw['evaluation'], index, float(levels[index, step, 0]), coefficients)
        if abs(row['value'] - expected['value']) > FINANCIAL_ATOL + FINANCIAL_RTOL * abs(expected['value']):
            raise ArithmeticError('native path differs from signed-cash/default-time analytic support')
        ordinal = origin + EXERCISE_DAYS if expected['decision'] != 'continue' else None
        if (row['path_key'] != index + 1 or row['first_exercise_decision'] != expected['decision']
                or row['first_exercise_ordinal'] != ordinal or row['full_native_cash_replayed'] is not True):
            raise ArithmeticError('native eligibility, clock or learned stopping differs')
        states.append(dict(path_key=index+1, x=float(levels[index, step, 0]),
                           eligible=expected['eligible'], decision=expected['decision'],
                           default_year=None if not math.isfinite(expected['death']) else expected['death'],
                           signed_recovery_interest=expected['signed_recovery_interest']))
        values.append(expected['value'])
    if len(values) != cell.paths:
        raise ArithmeticError('incomplete evaluation path coverage')
    half = len(values) // 2; clusters = [(values[i] + values[i+half]) * 0.5 for i in range(half)]
    mean = statistics.fmean(values); se = statistics.stdev(clusters) / math.sqrt(half)
    for actual_value, expected_value in ((actual['evaluation_value'], mean), (actual['evaluation_standard_error'], se)):
        if abs(actual_value-expected_value) > FINANCIAL_ATOL + FINANCIAL_RTOL * abs(expected_value):
            raise ArithmeticError('native mean or half-sample cluster SE differs')
    if actual['evaluation_clusters'] != half:
        raise ArithmeticError('native cluster inventory differs')
    regret = (learned['value']-optimal['value']) * (1.0 if cell.right == 'call' else -1.0)
    sampling_error = mean - learned['value']; errors = []
    if not -FINANCIAL_ATOL <= regret <= POLICY_LOSS_LIMIT:
        errors.append('finite-family learned-policy regret exceeds unchanged two-bp-of-par gate')
    if abs(sampling_error) > 5.0 * se + FINANCIAL_ATOL:
        errors.append('evaluation outside existing five-SE diagnostic band')
    return dict(optimal=optimal, learned=learned, learned_coefficients_z=coefficients,
        policy_regret=regret, sampling_error=sampling_error, total_error=mean-optimal['value'],
        evaluation_value=mean, half_sample_cluster_standard_error=se, clusters=half,
        descriptive_normal95_covers_learned=abs(sampling_error) <= 1.96*se+FINANCIAL_ATOL,
        gate_errors=errors, states=states, raw_source_and_signed_cash_independently_replayed=True)


def benchmark_sources():
    sources = source_identity()
    sources['new_benchmark_files'] = {str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
        for p in (Path(__file__), Path(__file__).with_name('test_commercial_credit_gaussian_exercise.py'),
                  Path(__file__).with_name('test_commercial_regression_work.py'),
                  ROOT/'docs/models/commercial-credit-gaussian-exercise.md',
                  ROOT/'docs/models/commercial-checked-least-squares.md') if p.is_file()}
    return sources


def run_cell(cell):
    started = time.monotonic(); raw = request(cell)
    record = dict(cell=asdict(cell), key=cell.key, raw=raw, status='pending')
    try:
        actual, execution = model_native.run(raw['schema'], raw, timeout=120.0)
        record.update(actual=actual, execution=execution)
        verify(raw, actual)
        record['assessment'] = assess(cell, raw, actual)
        record['status'] = 'numerical_gate_failed' if record['assessment']['gate_errors'] else 'passed'
    except Exception as error:
        record.update(status='failed', error_type=type(error).__name__, error=str(error))
    record['elapsed_seconds'] = time.monotonic()-started
    return record


def run_matrix():
    sources = benchmark_sources(); identity = model_native.binary_identity()
    hands = run_hand_controls(); records = []
    for cell in matrix():
        row = run_cell(cell); records.append(row)
        print(json.dumps(dict(key=row['key'], status=row['status'], error=row.get('error'),
                              elapsed_seconds=row['elapsed_seconds'])), flush=True)
    if sources != benchmark_sources() or identity != model_native.binary_identity():
        raise RuntimeError('source/native identity changed; invalidate entire family')
    effects = paired_credit_effects(records)
    tail = 12.0; maximum_sigma = math.sqrt(0.04*EXERCISE_DAYS/365.0)
    cash_slope = FACE*LOADING*ACCRUAL_DAYS/365.0*maximum_sigma
    return dict(schema='commercial-credit-gaussian-exercise-evidence-1',
        recorded_at=datetime.now(timezone.utc).isoformat(),
        worktree_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        native_identity=identity, benchmark_sources=sources, pre_execution_policy_amendment=AMENDMENT,
        declared_cells=[asdict(c) for c in matrix()], signed_hand_controls=hands, records=records,
        passed=hands['passed'] and all(r['status']=='passed' for r in records)
            and any(r.get('native_learned_decision_changes',0) for r in effects),
        paired_credit_effects=effects,
        gaussian_domain=dict(checked_standard_deviation_range=tail,
            minimum_coupon_gross_factor_within_range=1.0-LOADING*ACCRUAL_DAYS/365.0*maximum_sigma*tail,
            outside_range_probability_bound=2.0*density(tail)/tail,
            affine_cash_tail_absolute_bound=100.0*2.0*float(ndtr(-tail))+cash_slope*2.0*density(tail),
            global_gaussian_native_domain_admission_claim=False),
        financial_atol=FINANCIAL_ATOL, financial_rtol=FINANCIAL_RTOL,
        policy_loss_limit=POLICY_LOSS_LIMIT, normal_diagnostics_are_coverage_guarantees=False,
        numerical_family_only=True, observed_validation=False, full_issue_complete=False)


def paired_credit_effects(records):
    effects=[]
    for row in records:
        cell=GaussianCreditCell(**row['cell'])
        if cell.law!='credit_enabled' or cell.right=='none':
            continue
        baseline=next(r for r in records if r['cell']==asdict(GaussianCreditCell(
            cell.driver,'credit_off',cell.right,cell.paths,cell.training_seed,cell.evaluation_seed)))
        effect=dict(credit_cell=cell.key,paired_cell=baseline['key'])
        if 'assessment' not in row or 'assessment' not in baseline:
            effect['comparison_unavailable_due_to_retained_failure']=True
        else:
            common=[(a,b) for a,b in zip(baseline['assessment']['states'],row['assessment']['states'],strict=True) if b['eligible']]
            if any(a['path_key']!=b['path_key'] or a['x']!=b['x'] for a,b in common):
                raise ArithmeticError('paired credit switch changed common market draws')
            effect.update(common_eligible_paths=len(common),
                native_learned_decision_changes=sum(a['decision']!=b['decision'] for a,b in common))
        effects.append(effect)
    return effects


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args=parser.parse_args()
    if args.output.exists():
        raise FileExistsError('never overwrite prior evidence')
    with native_binary(args.native):
        result=run_matrix()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x', encoding='utf-8') as file:
        json.dump(result,file,indent=2,sort_keys=True,allow_nan=False)
    print(json.dumps(dict(cells=len(result['records']), passed=result['passed'],
        receipt_sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))
    return 0 if result['passed'] else 1


def hand_request(rate, recovery_fraction, default_day):
    """Actual source owner with one exact supplied default draw, no fitted policy."""
    raw = base_request(Cell('analytic', 'none', 'linear', 'brownian'))['source']
    for role in ('original', 'current'):
        source = raw['source'][role]
        source['measure'] = 'physical'
        for period in source['periods']:
            if period['coupon']['kind'] == 'indexed':
                period['coupon']['observations'][0]['source']['measure'] = 'physical'
    raw['source']['current']['periods'][1]['coupon']['observations'][0]['rate'] = rate
    for interval in raw['credit']['intervals']:
        interval['hazards_per_year'] = [[0.0] * 5 for _ in range(3)]
        interval['loss_given_default'] = [0.4] * 3
        interval['interest_recovery_fraction'] = [recovery_fraction] * 3
    index = 0 if default_day < EXERCISE_DAYS else 1
    raw['credit']['intervals'][index]['hazards_per_year'][0][3] = 1.0
    elapsed = default_day if index == 0 else default_day - EXERCISE_DAYS
    raw['credit']['max_jumps_per_path'] = 1
    raw['credit']['draw_stream'] = dict(kind='supplied', stream_id='signed-hand-default',
        stream_revision='1', paths=[dict(path_key=17, pairs=[dict(
            waiting_uniform=math.exp(-elapsed / 365.0), destination_uniform=0.5)])])
    # This public lifecycle wrapper exposes the complete sealed owner and book;
    # the inner compiler proof itself is not a standalone lifecycle variant.
    return dict(schema='commercial-holding-continuation-1',
        cut_ordinal=raw['source']['current']['asof_ordinal'] + EXERCISE_DAYS,
        completed_source=raw, revised_source=deepcopy(raw))


def assess_hand(rate, recovery_fraction, default_day, raw, actual):
    verify_holding(raw, actual)
    owner = actual['revised']
    path = owner['carry']['credit']['result']['paths'][0]
    claim = path['default']
    if claim is None or abs(claim['year'] - default_day / 365.0) > 1e-12:
        raise ArithmeticError('supplied hand default time differs')
    earned = FACE * rate * max(0.0, min(default_day - EXERCISE_DAYS, ACCRUAL_DAYS)) / 365.0
    paid = sum(row['interest'] for row in claim['recoveries'])
    principal = sum(row['principal'] for row in claim['recoveries'])
    legal = owner['accounting']['closing'][0]['legal_interest']
    recognized = owner['accounting']['closing'][0]['recognized_interest']
    if any(abs(a - b) > 1e-9 for a, b in (
        (claim['accrued_interest'], earned), (paid, earned * recovery_fraction),
        (principal, RECOVERY), (legal, earned * (1.0 - recovery_fraction)),
        (recognized, earned * (1.0 - recovery_fraction)),
    )):
        raise ArithmeticError('signed default claim, actual cash or retained legal ownership differs')
    if owner['original_anchor_refitted'] or owner['original_cash_quote'] != FACE:
        raise ArithmeticError('hand source changed original acquisition anchor')
    return dict(default_year=claim['year'], earned_signed_coupon=earned,
        actual_principal_recovery=principal, actual_interest_recovery=paid,
        retained_legal_interest=legal, retained_recognized_interest=recognized,
        claims_fully_closed=owner['accounting']['claims_fully_closed'],
        complete_recovery_cash_tail=owner['accounting']['complete_native_recovery_tail_retained'],
        eventual_coupon_settlement_assumption=('full signed accrued amount paid once at default'
            if recovery_fraction==1.0 else 'no modeled recovery cash; legal/recognized claims retained without waiver or future settlement/value assumption'),
        retained_default_principal=owner['accounting']['closing'][0]['principal'],
        complete_native_and_independent_owner_replayed=True)


def run_hand_controls():
    records = []
    for rate in (0.1, 0.0, -0.1):
        for fraction in (0.0, 1.0):
            for day in (5, 20, 32):
                raw = hand_request(rate, fraction, day)
                record = dict(rate=rate, recovery_fraction=fraction, default_day=day,
                              raw=raw, status='pending')
                try:
                    actual, execution = model_native.run(raw['schema'], raw)
                    record.update(actual=actual, execution=execution)
                    record['assessment'] = assess_hand(rate, fraction, day, raw, actual)
                    record['status'] = 'passed'
                except Exception as error:
                    record.update(status='failed', error_type=type(error).__name__, error=str(error))
                records.append(record)
    return dict(schema='commercial-gaussian-credit-signed-hand-controls-1',
        records=records, passed=all(row['status'] == 'passed' for row in records))


if __name__ == '__main__':
    raise SystemExit(main())
