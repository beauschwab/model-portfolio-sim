"""Synthetic native analysis convergence; Python formulas are independent oracles.

Each native request has at most 128 paths. Batched path prefixes and block-summed
finest-grid normals preserve CRN. This does not fit data or adopt product graphs.
"""
import copy
import json
import math
from pathlib import Path

import numpy as np
import pytest

from portfolio_risk.analytics.model_contracts import analyze_model_market
from portfolio_risk.core.runtime import RunConfig, run_context


EXAMPLE = Path(__file__).resolve().parents[3] / 'examples/models/model-market-1.json'


def request():
    raw = json.loads(EXAMPLE.read_text())
    raw['identity'].update(input_revision='convergence-synthetic-v1',
                           assumption_id='controlled-analysis-convergence',
                           rationale='Synthetic mechanics, not empirical calibration')
    raw.update(curve_queries=[], option_queries=[], joint=None)
    return raw


def native(raw):
    with run_context(RunConfig(compute_backend='rust')):
        return analyze_model_market(raw)


def cdf(x):
    return .5 * math.erfc(-x / math.sqrt(2.))


def normal_call(forward, strike, std):
    if std == 0.:
        return max(forward - strike, 0.)
    d = (forward - strike) / std
    return (forward - strike) * cdf(d) + std * math.exp(-d*d/2.) / math.sqrt(2.*math.pi)


def black_call(forward, strike, vol, expiry, shift):
    f, k = forward + shift, strike + shift
    s = vol * math.sqrt(expiry)
    d = math.log(f/k)/s + s/2.
    return f*cdf(d) - k*cdf(d-s)


def test_negative_rate_multicurve_refinement_has_quadratic_logdf_error(record_property):
    # Smooth supplied truth logD(t)=.01t-.003t², index logD differs by -.002t.
    # Piecewise logDF interpolation converges to that supplied function; it is
    # not evidence that this synthetic curve matches any market observation.
    errors = []
    for intervals in (1, 2, 4, 8):
        raw = request()
        nodes = np.linspace(0., 2., intervals+1)
        for i, curve in enumerate(raw['curves']):
            curve.update(times_years=nodes.tolist(), right_tail='reject',
                         discount_factors=np.exp(.01*nodes-.003*nodes**2-.002*i*nodes).tolist())
        raw['curve_queries'] = [dict(collateral_curve_id='collateral', index_curve_id='index',
            start_year=0., end_year=float(t), accrual_fraction=float(t))
            for t in (.125, .375, .625, .875, 1.125, 1.375, 1.625, 1.875)]
        rows = native(raw)['curve_results']
        error = max(abs(row['collateral_discount_factor']-math.exp(.01*q['end_year']-.003*q['end_year']**2))
                    for q, row in zip(raw['curve_queries'], rows))
        errors.append(error)
        for q, row in zip(raw['curve_queries'], rows):
            t = q['end_year']
            df = row['collateral_discount_factor']
            # Both grids are identical; the linear log basis is reproduced exactly.
            expected_basis = (math.exp(.002*t)-1.)/(df*t)
            assert row['index_basis'] == pytest.approx(expected_basis, abs=1e-14)
        assert rows[0]['collateral_implied_forward'] < 0.
    assert all(right < left for left, right in zip(errors, errors[1:]))
    record_property('synthetic_logdf_max_errors', json.dumps(errors))
    print('synthetic logDF refinement errors:', errors)


@pytest.mark.parametrize('convention', ['normal', 'shifted_lognormal'])
def test_option_heavy_smile_nodes_match_independent_negative_rate_prices(convention):
    raw = request()
    expiries, tenors, strikes = [.25, 1., 2.], [1., 5.], [-.015, -.005, .005, .02]
    shift = .04
    vols = [(.008 if convention == 'normal' else .3) * (1.+.15*t+.1*j+.08*k)
            for t in expiries for j in range(2) for k in range(4)]
    raw['smile'].update(convention=dict(kind=convention, **({'shift': shift} if convention != 'normal' else {})),
        expiries_years=expiries, tenors_years=tenors, strikes=strikes, volatilities=vols)
    raw['option_queries'] = [dict(expiry_years=t, tenor_years=tenor, strike=k,
        forward=-.005, payoff_unit=.97, side=side)
        for t in expiries for tenor in tenors for k in strikes for side in ('call', 'put')]
    rows = native(raw)['option_results']
    for n, (q, row) in enumerate(zip(raw['option_queries'], rows)):
        vol = vols[n//2]
        call = (normal_call(q['forward'], q['strike'], vol*math.sqrt(q['expiry_years']))
                if convention == 'normal' else black_call(q['forward'], q['strike'], vol, q['expiry_years'], shift))
        expected = .97*(call if q['side'] == 'call' else call-q['forward']+q['strike'])
        assert row['value'] == pytest.approx(expected, rel=1e-11, abs=1e-13)
        assert row['volatility'] == vol and not row['interpolated']


def controlled_joint(fine, steps, factors, batch_start):
    raw = request()
    joint = json.loads(EXAMPLE.read_text())['joint']
    std = np.array([.01, .02, .1])
    correlation = np.array([[1., .4, -.2], [.4, 1., .1], [-.2, .1, 1.]])
    covariance = correlation * std[:, None] * std[None, :]
    # The native contract requires exact symmetry, including the last binary bit.
    covariance = np.triu(covariance) + np.triu(covariance, 1).T
    block = 12//steps
    z = fine.reshape(len(fine), steps, block, 3).sum(axis=2)/math.sqrt(block)
    joint.update(source_revision='synthetic-positive-definite-covariance-v1',
        shared_draw_id=f'controlled-seed20261002-fine12-batch{batch_start}-v1',
        annual_covariance=covariance.ravel().tolist(), durations_years=[1./steps]*steps,
        paths=len(fine), retained_factors=factors,
        draws=dict(kind='supplied_standard_normals', values=z.ravel().tolist()))
    raw['joint'] = joint
    return raw, covariance


def test_paired_timestep_factor_path_grids_separate_sampling_from_truncation(record_property):
    fine = np.random.default_rng(20261002).standard_normal((512, 12, 3))
    loading = np.array([1., .35, -.04])  # Explicit linear Gaussian basket, not HPI-level equity.
    strikes = (-.015, 0., .015)
    results = []
    timestep_error = 0.
    for factors in (1, 2, 3):
        terminal_by_step = []
        retained = None
        covariance = None
        for steps in (1, 4, 12):
            parts = []
            for start in range(0, 512, 128):
                raw, covariance = controlled_joint(fine[start:start+128], steps, factors, start)
                out = native(raw)['joint']
                assert out['shared_draw_id'] == raw['joint']['shared_draw_id']
                retained = np.asarray(out['diagnostics']['retained_annual_covariance']).reshape(3, 3)
                parts.append(np.asarray(out['terminal_changes']).reshape(128, 3))
            terminal_by_step.append(np.concatenate(parts))
        for terminal in terminal_by_step[1:]:
            np.testing.assert_allclose(terminal, terminal_by_step[0], rtol=1e-12, atol=1e-14)
            timestep_error = max(timestep_error, float(np.max(np.abs(terminal-terminal_by_step[0]))))
        # Independently construct the declared correlation eigenfactor model.
        std = np.sqrt(np.diag(covariance))
        corr = covariance/std[:, None]/std[None, :]
        eigenvalues, eigenvectors = np.linalg.eigh(corr)
        selected = eigenvectors[:, -factors:]
        expected_cov = std[:, None] * (selected @ np.diag(eigenvalues[-factors:]) @ selected.T) * std[None, :]
        np.testing.assert_allclose(retained, expected_cov, rtol=1e-12, atol=1e-15)
        if factors == 3:
            np.testing.assert_allclose(retained, covariance, rtol=1e-12, atol=1e-15)
        sample_cov = np.cov(terminal_by_step[0], rowvar=False)
        covariance_se = np.sqrt((np.diag(expected_cov)[:, None]*np.diag(expected_cov)[None, :]
                                + expected_cov**2)/511.)
        assert np.all(np.abs(sample_cov-expected_cov) <= 6.*covariance_se + 1e-15)
        terminal = terminal_by_step[0] @ loading
        for paths in (128, 256, 512):
            for strike in strikes:
                payoff = np.maximum(terminal[:paths]-strike, 0.)
                estimate = float(payoff.mean())
                se = float(payoff.std(ddof=1)/math.sqrt(paths))
                exact = normal_call(0., strike, math.sqrt(float(loading @ expected_cov @ loading)))
                full = normal_call(0., strike, math.sqrt(float(loading @ covariance @ loading)))
                # Six standard errors is a sampling diagnostic, not a relaxed price tolerance.
                assert abs(estimate-exact) <= 6.*se + 1e-13
                results.append(dict(factors=factors, paths=paths, strike=strike,
                    estimate=estimate, standard_error=se, retained_model_exact=exact,
                    full_model_exact=full, factor_bias=exact-full))
    record_property('synthetic_joint_option_study', json.dumps(results))
    record_property('synthetic_max_paired_timestep_error', timestep_error)
    print('synthetic maximum paired timestep error:', timestep_error)
    print('synthetic ATM joint option study:', json.dumps([r for r in results if r['strike'] == 0.]))


def test_native_seed_exact_replay_and_curve_smile_covariance_tail_guards():
    raw = request()
    raw['joint'] = json.loads(EXAMPLE.read_text())['joint']
    raw['joint'].update(paths=64, durations_years=[.25]*4)
    first, second = native(raw)['joint'], native(copy.deepcopy(raw))['joint']
    assert first['increments'] == second['increments']
    assert first['terminal_changes'] == second['terminal_changes']
    zero = copy.deepcopy(raw)
    zero['joint']['annual_covariance'] = [0.]*9
    assert native(zero)['joint']['increments'] == [0.]*(64*4*3)
    bad = copy.deepcopy(raw)
    bad['joint']['annual_covariance'][1] = .01
    bad['joint']['annual_covariance'][3] = .01
    with pytest.raises(ValueError, match='covariance|correlation'):
        native(bad)
    bad = request()
    bad['curves'][0]['right_tail'] = 'reject'
    bad['curve_queries'] = [dict(collateral_curve_id='collateral', index_curve_id='index',
                                start_year=0., end_year=3., accrual_fraction=3.)]
    with pytest.raises(ValueError, match='tail|beyond'):
        native(bad)
    bad = request()
    bad['option_queries'] = [dict(expiry_years=1., tenor_years=1., strike=.02,
                                 forward=-.005, payoff_unit=1., side='call')]
    with pytest.raises(ValueError, match='tail|outside'):
        native(bad)
    bad['smile']['convention'] = dict(kind='shifted_lognormal', shift=0.)
    with pytest.raises(ValueError, match='shifted strike'):
        native(bad)
