"""Independent signed-default cash, Gaussian policy integrals and actual native replay."""
from copy import deepcopy
import json
import math
import os
from pathlib import Path

import pytest
from scipy.integrate import quad

from commercial_credit_gaussian_exercise_benchmark import (
    GaussianCreditCell, FACE, RECOVERY, LOADING, EXERCISE_DAYS, ACCRUAL_DAYS,
    MATURITY_DAYS, analytic, assess, assess_hand, benchmark_sources, density,
    learned_coefficients, matrix, native_binary, paired_credit_effects,
    parameters, request, run_matrix, validate_domain,
)
from portfolio_risk.core import model_native


@pytest.fixture(scope='module')
def receipt(tmp_path_factory):
    selected=os.environ.get('PORTFOLIO_CONVERGENCE_NATIVE')
    path=Path(os.environ['PORTFOLIO_GAUSSIAN_CREDIT_RECEIPT']) if os.environ.get(
        'PORTFOLIO_GAUSSIAN_CREDIT_RECEIPT') else tmp_path_factory.mktemp('gaussian-credit')/'receipt.json'
    def load():
        if path.exists():
            result=json.loads(path.read_text())
            assert result['benchmark_sources']==benchmark_sources()
            assert result['native_identity']==model_native.binary_identity()
            return result
        result=run_matrix()
        with path.open('x')as file:json.dump(result,file,indent=2,sort_keys=True,allow_nan=False)
        return result
    with native_binary(selected or model_native.binary_path()):return load()


def test_complete_family_predeclared_before_execution():
    cells=matrix()
    assert len(cells)==len({c.key for c in cells})==48
    assert {c.paths for c in cells}=={256,512}
    assert {(c.training_seed,c.evaluation_seed)for c in cells}=={(7,11),(19,23)}
    assert {c.driver for c in cells}=={'brownian','ou'}
    for cell in cells:
        raw=request(cell);validate_domain(cell,raw)
        if cell.right!='none':assert raw['policy']['exercise_dates'][0]['timing']=='after_scheduled_cash'
        assert all(i['interest_recovery_fraction']==[1.0]*3 for i in raw['source']['credit']['intervals'])


def test_conditional_continuation_coefficient_includes_all_signed_default_accrual():
    for cell in matrix():
        p=parameters(cell);hazard=p['post_hazard'];tau=ACCRUAL_DAYS/365.0
        delta=(MATURITY_DAYS-EXERCISE_DAYS)/365.0
        expected=math.exp(-hazard*delta)*tau+quad(lambda t:hazard*math.exp(-hazard*t)*min(t,tau),
            0.,delta,points=[tau],epsabs=1e-13)[0]
        assert p['expected_accrual']==pytest.approx(expected,abs=1e-14)
        assert p['loading']==pytest.approx(FACE*LOADING*expected,abs=1e-12)
        for x in (-.1,0.,.1):
            actual=RECOVERY*(1.-math.exp(-hazard*delta))+math.exp(-hazard*delta)*(FACE+FACE*LOADING*x*tau)
            actual+=quad(lambda t:hazard*math.exp(-hazard*t)*FACE*LOADING*x*min(t,tau),
                0.,delta,points=[tau],epsabs=1e-13)[0]
            assert actual==pytest.approx(p['mean']+p['loading']*x,abs=1e-12)


def test_optimal_and_actual_learned_policy_gaussian_integrals_are_distinct():
    for cell in matrix():
        if cell.right=='none':continue
        p=parameters(cell);sigma=p['loading']*p['sigma_x'];optimal=analytic(cell)
        boundary=(100.-p['mean'])/sigma;choose=min if cell.right=='call'else max
        integral=quad(lambda z:choose(100.,p['mean']+sigma*z)*density(z),-12.,12.,
            points=[boundary],epsabs=1e-10)[0]
        assert optimal['value']==pytest.approx(RECOVERY*p['before']+p['survival']*integral,abs=1e-10)
        stop=analytic(cell,[101. if cell.right=='call'else 99.,0.])
        assert stop['value']==pytest.approx(RECOVERY*p['before']+p['survival']*100.)
        continuation=analytic(cell,[99. if cell.right=='call'else 101.,0.])
        assert continuation['value']==optimal['option_free_value']
        assert abs(continuation['value']-optimal['value'])>.1


def test_every_varying_feature_maps_independently_to_the_observed_reset():
    cell=matrix()[0];p=parameters(cell)
    fit=dict(basis_degree=1,observed_live_states=['performing'],rank=2,active_features=[0,5],
        means=[0.,1.,0.,0.,0.,0.,1.,0.,1.,0.],scales=[1.]*10,
        coefficients=[p['mean'],p['loading'],0.])
    assert learned_coefficients(cell,fit)==pytest.approx([p['mean'],p['loading']*p['sigma_x']])
    fit['active_features']=[0,6]
    with pytest.raises(ArithmeticError,match='genuinely varying'):learned_coefficients(cell,fit)


@pytest.mark.parametrize('cell',matrix(),ids=lambda c:c.key)
def test_native_declared_gaussian_credit_cell(cell,receipt):
    row=next(r for r in receipt['records']if r['key']==cell.key)
    assert row['status']=='passed',row.get('error',row.get('assessment'))
    assert row['assessment']==assess(cell,row['raw'],row['actual'])
    assert not row['assessment']['gate_errors']
    if cell.right!='none':
        decisions={s['decision']for s in row['assessment']['states']if s['eligible']}
        assert decisions=={'continue',cell.right}


def test_signed_hand_owners_preserve_fraction_zero_boundary_and_pay_fraction_one_once(receipt):
    hand=receipt['signed_hand_controls'];assert hand['passed']and len(hand['records'])==18
    for row in hand['records']:
        assert row['status']=='passed',row.get('error')
        result=assess_hand(row['rate'],row['recovery_fraction'],row['default_day'],row['raw'],row['actual'])
        assert result==row['assessment']
        assert not result['claims_fully_closed']  # Uncharged-off default principal remains40.
        if row['rate']<0 and row['default_day']>EXERCISE_DAYS:
            if row['recovery_fraction']==0.:
                assert result['retained_legal_interest']<0 and result['retained_recognized_interest']<0
                assert result['actual_interest_recovery']==0.
            else:assert result['actual_interest_recovery']<0 and result['retained_legal_interest']==0.


def test_credit_changes_actual_learned_native_stopping_with_paired_market_draws(receipt):
    effects=paired_credit_effects(receipt['records'])
    assert effects==receipt['paired_credit_effects']and len(effects)==16
    assert not any(r.get('comparison_unavailable_due_to_retained_failure')for r in effects), 'retained failure prevents a paired decision comparison'
    assert sum(r['native_learned_decision_changes']for r in effects)>0
    assert all(r['common_eligible_paths']>0 for r in effects)
    signed=[s['signed_recovery_interest']for r in receipt['records']if 'assessment'in r
        for s in r['assessment']['states']]
    assert any(v<0 for v in signed)and any(v>0 for v in signed)


@pytest.mark.parametrize('mutation',['cash','mean','se','cluster_count','decision','feature','fraction','clock'])
def test_sampling_oracle_rejects_changed_financial_state_or_policy(mutation,receipt):
    row=next(r for r in receipt['records']if r['cell']['law']=='credit_enabled'and r['cell']['right']=='call')
    cell=GaussianCreditCell(**row['cell']);raw=deepcopy(row['raw']);actual=deepcopy(row['actual'])
    if mutation=='cash':actual['evaluation_paths'][0]['value']+=.001
    elif mutation=='mean':actual['evaluation_value']+=.001
    elif mutation=='se':actual['evaluation_standard_error']+=.001
    elif mutation=='cluster_count':actual['evaluation_clusters']+=1
    elif mutation=='decision':actual['evaluation_paths'][0]['first_exercise_decision']='put'
    elif mutation=='feature':actual['regressions'][0]['active_features']=[0,6]
    elif mutation=='fraction':raw['source']['credit']['intervals'][1]['interest_recovery_fraction']=[0.]*3
    else:raw['source']['source']['current']['periods'][1]['coupon']['observations'][0]['source']['information_unix_seconds']+=86400
    with pytest.raises(ArithmeticError):assess(cell,raw,actual)


def test_complete_matrix_retains_limits_and_does_not_claim_wider_acceptance(receipt):
    assert len(receipt['records'])==len(receipt['declared_cells'])==48
    assert receipt['passed']and receipt['numerical_family_only']
    assert not receipt['full_issue_complete']and not receipt['observed_validation']
    assert not receipt['normal_diagnostics_are_coverage_guarantees']
    domain=receipt['gaussian_domain']
    assert domain['minimum_coupon_gross_factor_within_range']>0.
    assert domain['affine_cash_tail_absolute_bound']<1e-25
    assert not domain['global_gaussian_native_domain_admission_claim']
