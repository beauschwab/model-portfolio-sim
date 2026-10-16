"""Predeclared credit/one-exercise family; independent analytic test oracle.

This module is a test harness, never a production pricing implementation. Zero
coupons/discounting and deterministic hazards leave one constant continuation
state among surviving paths. Defaulted paths have no exercisable live right.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import statistics
import subprocess
import sys
import time

import scipy
from scipy.stats import binomtest

from commercial_convergence_benchmark import (
    Cell, FINANCIAL_ATOL, FINANCIAL_RTOL, POLICY_LOSS_LIMIT,
    native_binary, request as base_request, source_identity,
)
from portfolio_risk.analytics.commercial_stochastic_reference import verify
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]
FACE, LGD = 100.0, 0.4
EXERCISE_DAYS, MATURITY_DAYS = 15, 35
FAMILY_ALPHA = 0.01
LAWS = {'credit_off': (0.0, 0.0), 'moderate': (0.10, 0.25),
        'front_loaded': (0.35, 0.50)}
SEEDS = ((7, 11), (19, 23))
SAMPLE_COUNTS = (128, 256)
STRIKE_GAP = 8.0


@dataclass(frozen=True)
class CreditExerciseCell:
    law: str
    right: str
    strike_side: str
    paths: int
    training_seed: int
    evaluation_seed: int

    @property
    def key(self):
        return '/'.join(map(str, asdict(self).values()))


def matrix():
    """Full60-cell declared design, including option-free paired CRN controls."""
    return tuple(CreditExerciseCell(law, right, side, paths, train, evaluation)
                 for law in LAWS
                 for right, side in (('none', 'none'), ('call', 'below'),
                                     ('call', 'above'), ('put', 'below'), ('put', 'above'))
                 for paths in SAMPLE_COUNTS for train, evaluation in SEEDS)


def parameters(cell):
    before, after = LAWS[cell.law]
    recovery = FACE * (1.0 - LGD)
    continuation = recovery * after + FACE * (1.0 - after)
    strike = None if cell.right == 'none' else continuation + (
        STRIKE_GAP if cell.strike_side == 'above' else -STRIKE_GAP)
    return before, after, recovery, continuation, strike


def choose(right, strike, continuation):
    return ((right == 'call' and strike < continuation)
            or (right == 'put' and strike > continuation))


def analytic(cell, *, learned_stop=None):
    """Integrate the raw hazard law; no simulated cash or regression targets."""
    before, after, recovery, continuation, strike = parameters(cell)
    optimal_stop = choose(cell.right, strike, continuation)
    selected_stop = optimal_stop if learned_stop is None else learned_stop
    live_value = strike if selected_stop else continuation
    return dict(option_free_value=recovery * before + (1.0-before) * continuation,
                value=recovery * before + (1.0-before) * live_value,
                conditional_continuation=continuation, strike=strike,
                optimal_stop=optimal_stop, selected_stop=selected_stop,
                survival_to_exercise=1.0-before,
                default_probability=before if selected_stop else before+(1.0-before)*after,
                low_cash=recovery, high_cash=strike if selected_stop else FACE)


def request(cell):
    if cell not in matrix():
        raise ValueError('undeclared credit/exercise cell')
    raw = base_request(Cell('analytic', 'none', 'linear', 'brownian', paths=cell.paths))
    raw['identity']['revision'] = 'credit-exercise/' + cell.key
    for name, seed in (('training', cell.training_seed), ('evaluation', cell.evaluation_seed)):
        raw[name].update(paths=cell.paths, credit_seed=seed, market_seed=[seed])
    raw['annual_covariance'] = [0.0]
    raw['coupon_loadings'], raw['hazard_loadings'] = [], []
    for role in ('original', 'current'):
        for period in raw['source']['source'][role]['periods']:
            period['coupon'] = dict(kind='fixed', annual_rate=0.0)
    before, after, _, _, strike = parameters(cell)
    credit = raw['source']['credit']
    credit['max_jumps_per_path'] = 1
    for interval, probability, days in zip(credit['intervals'], (before, after),
                                         (EXERCISE_DAYS, MATURITY_DAYS-EXERCISE_DAYS), strict=True):
        interval['hazards_per_year'] = [[0.0]*5 for _ in range(3)]
        interval['hazards_per_year'][0][3] = -math.log1p(-probability)/(days/365.0)
        interval['loss_given_default'] = [LGD]*3
        interval['complete_recovery_tails'] = [[dict(lag_years=0.0, weight=1.0)] for _ in range(3)]
    if cell.right != 'none':
        origin = raw['source']['source']['current']['asof_ordinal']
        raw['policy']['exercise_dates'] = [dict(ordinal=origin+EXERCISE_DAYS,
            timing='after_scheduled_cash', call_clean_price_factor=strike/FACE if cell.right=='call' else None,
            put_clean_price_factor=strike/FACE if cell.right=='put' else None, call_fee=0.0, put_fee=0.0)]
    return raw


def validate_domain(cell, raw):
    """Check every assumption needed for the two-state analytic integration."""
    if raw != request(cell):
        raise ArithmeticError('credit/exercise raw declared cell identity differs')
    source = raw['source']['source']['current']; origin = source['asof_ordinal']
    assert source['opening_principal'] == FACE
    assert source['tail_end_ordinal'] == origin+MATURITY_DAYS
    assert [p['payment_ordinal']-origin for p in source['periods']] == [EXERCISE_DAYS, MATURITY_DAYS]
    assert source['principal_plan'] == dict(kind='supplied', principal_payments=[0.0, FACE])
    assert all(p['coupon']==dict(kind='fixed', annual_rate=0.0) and p['fee']==0.0 for p in source['periods'])
    assert raw['fixed_base_oas'] == raw['parallel_bump'] == 0.0
    assert all(df==1.0 for df in raw['discount_curve']['discount_factors'])
    assert not raw['coupon_loadings'] and not raw['hazard_loadings']
    assert raw['annual_covariance'] == [0.0]
    assert [p['initial_live_state'] for p in raw['source']['credit']['paths']] == ['performing']
    assert not raw['source'].get('performing_draw_tranches') and not raw['source']['source'].get('transfer')
    for interval, end in zip(raw['source']['credit']['intervals'], (EXERCISE_DAYS, MATURITY_DAYS), strict=True):
        assert interval['end_year'] == end/365.0
        assert interval['loss_given_default'] == [LGD]*3
        assert interval['complete_recovery_tails'] == [[dict(lag_years=0.0,weight=1.0)]]*3


def assess(cell, raw, actual):
    validate_domain(cell, raw)
    optimal = analytic(cell)
    regressions = actual['regressions']
    if cell.right == 'none':
        if regressions:
            raise ArithmeticError('option-free cell unexpectedly fits a policy')
        prediction, learned_stop = None, False
    else:
        if len(regressions)!=1:
            raise ArithmeticError('one observed exercise regression required')
        fit = regressions[0]
        if (fit['active_features'] or len(fit['coefficients'])!=1 or fit['rank']!=1
                or fit['observed_live_states']!=['performing'] or fit['observations']<2):
            raise ArithmeticError('constant surviving-state analytic policy domain differs')
        prediction = fit['coefficients'][0]
        if not math.isfinite(prediction):
            raise ArithmeticError('finite learned continuation required')
        learned_stop = choose(cell.right, optimal['strike'], prediction)
    learned = analytic(cell, learned_stop=learned_stop)
    values=[]; defaults=0; exercised=0
    origin=raw['source']['source']['current']['asof_ordinal']
    for i, row in enumerate(actual['evaluation_paths']):
        if row['path_key']!=i+1 or row['full_native_cash_replayed'] is not True:
            raise ArithmeticError('complete ordered native path/cash evidence required')
        value=row['value']; is_default=math.isclose(value,learned['low_cash'],rel_tol=0.0,abs_tol=FINANCIAL_ATOL)
        if not is_default and not math.isclose(value,learned['high_cash'],rel_tol=0.0,abs_tol=FINANCIAL_ATOL):
            raise ArithmeticError('cash outside independent learned-policy support')
        stopped=learned_stop and not is_default
        if row['first_exercise_decision']!=(cell.right if stopped else 'continue') or row['first_exercise_ordinal']!=(origin+EXERCISE_DAYS if stopped else None):
            raise ArithmeticError('defaulted/ineligible right or actual learned stopping differs')
        values.append(value); defaults+=is_default; exercised+=stopped
    if len(values)!=cell.paths:
        raise ArithmeticError('complete declared evaluation sample required')
    mean=statistics.fmean(values); half=len(values)//2
    clusters=[(values[i]+values[half+i])/2.0 for i in range(half)]
    se=statistics.stdev(clusters)/math.sqrt(half)
    if not math.isclose(actual['evaluation_value'],mean,rel_tol=0.0,abs_tol=1e-12):
        raise ArithmeticError('native sample mean differs')
    if not math.isclose(actual['evaluation_standard_error'],se,rel_tol=0.0,abs_tol=1e-12) or actual['evaluation_clusters']!=half:
        raise ArithmeticError('native half-sample cluster standard error differs')
    alpha=FAMILY_ALPHA/len(matrix())
    ci=binomtest(defaults,cell.paths).proportion_ci(confidence_level=1.0-alpha,method='exact')
    low,high=learned['low_cash'],learned['high_cash']
    interval=[high-(high-low)*ci.high,high-(high-low)*ci.low]
    loss=0.0 if cell.right=='none' else (learned['value']-optimal['value'])*(1.0 if cell.right=='call' else -1.0)
    gates=[]
    if loss < -FINANCIAL_ATOL or loss>POLICY_LOSS_LIMIT:
        gates.append('predeclared two-bp-of-par policy-loss limit')
    if not ci.low<=learned['default_probability']<=ci.high:
        gates.append('predeclared ideal-IID exact binomial interval missed learned policy law')
    if cell.law=='credit_off' and not math.isclose(mean,optimal['value'],rel_tol=FINANCIAL_RTOL,abs_tol=FINANCIAL_ATOL):
        gates.append('credit-off deterministic parity')
    return dict(optimal=optimal,learned=learned,learned_continuation=prediction,
                learned_stop=learned_stop,policy_loss=loss,sample_value=mean,
                sampling_error=mean-learned['value'],total_error=mean-optimal['value'],
                defaults=defaults,exercised=exercised,clusters=half,cluster_standard_error=se,
                default_probability_interval=[ci.low,ci.high],learned_value_interval=interval,
                exact_interval_contains_learned=ci.low<=learned['default_probability']<=ci.high,
                descriptive_normal95_contains_learned=abs(mean-learned['value'])<=1.959963984540054*se,
                zero_event_sample=defaults==0,per_cell_alpha=alpha,gate_errors=gates)


def run_cell(cell):
    raw=request(cell); started=time.monotonic()
    record=dict(cell=asdict(cell),key=cell.key,raw=raw,status='pending',request_sha256=hashlib.sha256(
        json.dumps(raw,sort_keys=True,separators=(',',':'),allow_nan=False).encode()).hexdigest())
    try:
        actual,execution=model_native.run(raw['schema'],raw,timeout=120.0)
        record.update(actual=actual,execution=execution)
        verify(raw,actual)
        record['independent_raw_source_replayed']=True
        record['assessment']=assess(cell,raw,actual)
        record['status']='numerical_gate_failed' if record['assessment']['gate_errors'] else 'passed'
    except Exception as error:
        record.update(status='failed',error_type=type(error).__name__,error=str(error))
    record['elapsed_seconds']=time.monotonic()-started
    return record


def benchmark_sources():
    sources=source_identity()
    sources['new_benchmark_files']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest()
        for p in (Path(__file__),Path(__file__).with_name('test_commercial_credit_exercise_convergence.py'),
                  ROOT/'docs/models/commercial-credit-exercise-convergence.md') if p.is_file()}
    return sources


def run_matrix():
    sources=benchmark_sources(); identity=model_native.binary_identity()
    records=[]
    for cell in matrix():
        record=run_cell(cell);records.append(record)
        print(json.dumps(dict(key=record['key'],status=record['status'],
                              elapsed_seconds=record['elapsed_seconds'],error=record.get('error'))),flush=True)
    if sources!=benchmark_sources() or identity!=model_native.binary_identity():
        raise RuntimeError('benchmark source/native identity changed; invalidate entire matrix')
    return dict(schema='commercial-credit-exercise-convergence-evidence-1',
                recorded_at=datetime.now(timezone.utc).isoformat(),
                benchmark_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
                native_identity=identity,benchmark_sources=sources,python_version=sys.version,scipy_version=scipy.__version__,
                declared_cells=[asdict(c) for c in matrix()],records=records,
                passed=all(r['status']=='passed' for r in records),family_alpha=FAMILY_ALPHA,
                financial_atol=FINANCIAL_ATOL,financial_rtol=FINANCIAL_RTOL,policy_loss_limit=POLICY_LOSS_LIMIT,
                numerical_family_only=True,observed_validation=False,full_issue_complete=False)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--native',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists():raise FileExistsError('retain immutable prior matrix receipt')
    with native_binary(args.native):result=run_matrix()
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with args.output.open('x',encoding='utf-8')as file:json.dump(result,file,indent=2,sort_keys=True,allow_nan=False)
    print(json.dumps(dict(cells=len(result['records']),passed=result['passed'],receipt_sha256=hashlib.sha256(args.output.read_bytes()).hexdigest())))
    if not result['passed']:raise SystemExit(1)


if __name__=='__main__':main()
