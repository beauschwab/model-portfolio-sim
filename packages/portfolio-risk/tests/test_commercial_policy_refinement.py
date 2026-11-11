"""Controlled path/basis refinement against normal integrals, not parity alone.

These synthetic controls separate evaluation sampling uncertainty from the loss
of a learned issuer policy. They do not assert monotone Monte Carlo errors or
turn a standard error into a looser financial cash-replay tolerance.
"""
from copy import deepcopy
import json
import math
from pathlib import Path

import numpy as np
import pytest
from scipy.integrate import quad
from scipy.optimize import brentq
from scipy.special import ndtr

from portfolio_risk.analytics.commercial_stochastic_reference import evaluate, verify
from portfolio_risk.core import model_native

ROOT = Path(__file__).resolve().parents[3]


def request(degree, paths, *, two_dates=True, put=False):
    raw = json.loads((ROOT/'examples/models/commercial-generated-exercise-1.json').read_text())
    raw['regression_policy'] = f'native_observable_{degree}_svd_train_evaluate-1'
    raw['identity']['revision'] = f'{degree}-{paths}'
    for name in ('training', 'evaluation'):
        raw[name]['paths'] = paths
    if put:
        raw['policy']['exercise_dates'][0].update(
            call_clean_price_factor=None, put_clean_price_factor=1.)
    if two_dates:
        origin = raw['time_ordinals'][0]
        raw['time_ordinals'].insert(1, origin+5)
        early = deepcopy(raw['policy']['exercise_dates'][0])
        early.update(ordinal=origin+5, call_clean_price_factor=.999)
        raw['policy']['exercise_dates'].insert(0, early)
    return raw


def density(z):
    return math.exp(-.5*z*z)/math.sqrt(2.*math.pi)


def two_date_values(regression):
    # X follows the supplied sigma=.2 Brownian law. At day15 a par call caps
    # principal plus the next coupon at100. At day5 the issuer can pay99.9.
    # No regression, native path or estimated monetary checkpoint supplies this
    # conditional normal expectation or optimal exercise boundary.
    coupon = 100.*15./365.
    early_sigma = .2*math.sqrt(5./365.)
    increment_sigma = .2*math.sqrt(10./365.)
    strike = 99.9

    def conditional(x):
        ratio = x/increment_sigma
        return 100.+coupon*(x*ndtr(-ratio)-increment_sigma*density(ratio))

    boundary = brentq(lambda z: conditional(early_sigma*z)-strike, -12., 12.)
    optimal = (quad(lambda z: conditional(early_sigma*z)*density(z), -12., boundary,
                    epsabs=1e-11, epsrel=1e-12)[0]
               + strike*(ndtr(12.)-ndtr(boundary)))
    assert regression['active_features'] == [0]
    coefficients = list(regression['coefficients'])
    coefficients[0] -= strike
    # Independently integrate the policy actually selected by TRAINING. Split
    # at all real polynomial roots, including nonmonotone fitted boundaries.
    roots = np.polynomial.polynomial.polyroots(coefficients)
    mean, scale = regression['means'][0], regression['scales'][0]
    split = [-12., 12.]
    split.extend((float(root.real)*scale+mean)/early_sigma for root in roots
                 if abs(root.imag)<1e-10
                 and -12.<(float(root.real)*scale+mean)/early_sigma<12.)
    split.sort()
    learned = 0.
    for left, right in zip(split, split[1:]):
        z = (left+right)*.5
        normalized = (early_sigma*z-mean)/scale
        stop = np.polynomial.polynomial.polyval(normalized, coefficients)>0.
        learned += (strike*(ndtr(right)-ndtr(left)) if stop else
            quad(lambda t: conditional(early_sigma*t)*density(t), left, right,
                 epsabs=1e-11, epsrel=1e-12)[0])
    return float(optimal), float(learned)


@pytest.mark.parametrize('degree,paths', [
    (degree, paths) for degree in ('linear','quadratic','cubic') for paths in (128,256)
] + [('linear',512),('quadratic',512)])
def test_two_date_native_path_basis_refinement_separates_policy_loss_from_sampling(degree, paths):
    raw = request(degree, paths)
    result, receipt = model_native.run(raw['schema'], raw, timeout=120.)
    verify(raw, result)
    assert receipt['backend'] == 'rust'
    optimal, learned = two_date_values(result['regressions'][0])
    # A feasible issuer policy cannot beat the exact minimum in expectation.
    # This control sets a separate 2bp-of-par policy approximation gate.
    assert -1e-10 <= learned-optimal <= .02
    assert abs(result['evaluation_value']-learned) <= 5.*result['evaluation_standard_error']+1e-9
    assert result['evaluation_clusters'] == paths//2
    assert not result['convergence_established'] and not result['empirical_validation']
    for regression in result['regressions']:
        assert regression.get('basis_degree', 2) == {'linear':1,'quadratic':2,'cubic':3}[degree]
        assert regression['observations'] >= 2*len(regression['coefficients'])
    if degree == 'quadratic':
        assert all('basis_degree' not in row for row in result['regressions'])


@pytest.mark.parametrize('degree', ['linear', 'cubic'])
def test_selected_basis_keeps_evaluation_out_of_training_and_metadata(degree):
    raw = request(degree, 128)
    before = evaluate(raw)
    raw['evaluation']['market_seed'] = [23]
    raw['evaluation']['credit_seed'] = 23
    after = evaluate(raw)
    assert before['regressions'] == after['regressions']
    assert before['training_value'] == after['training_value']
    assert before['evaluation_paths'] != after['evaluation_paths']


@pytest.mark.parametrize('degree', ['linear', 'quadratic', 'cubic'])
def test_one_date_call_put_normal_integral_and_option_free_bounds(degree):
    call_raw = request(degree, 128, two_dates=False)
    call = evaluate(call_raw)
    put = evaluate(request(degree, 128, two_dates=False, put=True))
    option = 100.*15./365.*.2*math.sqrt(15./365.)/math.sqrt(2.*math.pi)
    for result, exact in ((call,100.-option),(put,100.+option)):
        assert abs(result['evaluation_value']-exact) <= 5.*result['evaluation_standard_error']+1e-9
    assert call['evaluation_value'] <= 100. <= put['evaluation_value']
    # Shared antithetic paths retain the option-free par expectation exactly.
    assert call['evaluation_value']+put['evaluation_value'] == pytest.approx(200., abs=1e-10)


def test_unknown_or_automatic_evaluation_selected_basis_refuses():
    raw = request('automatic_best_holdout', 128)
    with pytest.raises(ArithmeticError, match='explicit generated exercise'):
        evaluate(raw)


def test_cubic_full_sample_work_refuses_instead_of_increasing_generated_budget():
    raw = request('cubic', 512)
    with pytest.raises(ValueError, match='aggregate work/byte admission exceeded'):
        model_native.run(raw['schema'], raw, timeout=120.)
