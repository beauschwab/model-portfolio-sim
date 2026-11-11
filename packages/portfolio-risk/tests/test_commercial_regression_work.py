"""Independent exact integer checks for the new numerical reserve."""
from copy import deepcopy
from math import comb
from pathlib import Path
import json
import pytest
from portfolio_risk.analytics.commercial_regression_work_reference import (
    numerical_reserve, shape_bound, verify_numeric_reserve, active_feature_upper,scan_work_upper,
    constant_fee_observations, full_feature_upper,
)
from commercial_credit_gaussian_exercise_benchmark import matrix, request


@pytest.mark.parametrize("m,k", [(8,1),(16,3),(256,3),(256,128),(512,11),(1024,512)])
def test_reserve_is_exact_checked_structural_census(m,k):
    fast=32*m*k**2+8*k**3+65*(2*k**2+8*k)*(m+k)+16384*k+8*m*k
    repair=32*k*(k-1)*(2*m+k+32)+16*m*k+12*k**2+16*m+128*k
    checks=3*m*k**2+2*k**3+12*m*k+8*m+16*k**2+64*k
    assert shape_bound(m,k)==(fast+repair+2*checks,8*(5*m*k+4*k**2+4*m+32*k)+1024)


@pytest.mark.parametrize("m,k", [(0,0),(1,1),(8,5),(1025,1),(8,2**100),(True,1)])
def test_invalid_shape_is_rejected(m,k):
    with pytest.raises(ArithmeticError):shape_bound(m,k)


def test_no_fit_has_no_numerical_admission_delta():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[]
    assert numerical_reserve(raw)==(0,0)


@pytest.mark.parametrize('hazard',[0.,.01])
def test_physical_policy_application_keeps_scan_workspace_but_executes_no_fit(hazard):
    raw=request(matrix()[0])
    raw['policy']['exercise_dates']=[{}]
    raw['source']['credit']['intervals'][0]['hazards_per_year'][0][3]=hazard
    complete=numerical_reserve(raw)
    applied=numerical_reserve(raw,fits_continuation=False)
    assert applied==(8*scan_work_upper(raw),complete[1])
    assert complete[0]>applied[0]
    raw['policy']['exercise_dates']=[]
    assert numerical_reserve(raw,fits_continuation=False)==(0,0)


def test_fixed_coupons_only_reduce_new_structural_fit_ticket_from_raw_law():
    fixture=json.loads((Path(__file__).resolve().parents[3]/'examples/models/commercial-generated-candidate-journal-1.json').read_text())
    raw=fixture['commercial']['units'][0]['graph']['positions'][0]['generated_policy']['pricing_training']
    assert any(c>0 for c in raw['annual_covariance'])
    assert active_feature_upper(raw)==len(raw['drivers'])
    assert numerical_reserve(raw)[0]<shape_bound(raw['training']['paths'],10)[0]
    raw['source']['reviewed_credit_actions']={}
    assert active_feature_upper(raw)>len(raw['drivers'])


def test_every_training_fit_is_reserved_and_workspace_is_once():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[{},{}]
    one=shape_bound(raw["training"]["paths"],3)
    assert numerical_reserve(raw,columns=3)==(2*one[0]+8*scan_work_upper(raw),one[1]+128)


def test_missing_work_or_workspace_is_independently_rejected():
    raw=request(matrix()[0]);raw["policy"]["exercise_dates"]=[{}]
    work,workspace=numerical_reserve(raw)
    actual={"admitted_work_upper":work,"retained_bytes_upper":workspace}
    assert verify_numeric_reserve(raw,actual)==(work,workspace)
    for key in actual:
        bad=deepcopy(actual);bad[key]-=1
        with pytest.raises(ArithmeticError):verify_numeric_reserve(raw,bad)


def test_raw_law_constants_are_proved_without_sample_or_feature_pruning():
    from portfolio_risk.analytics.commercial_generated_physical_reference import feature_bound
    raw=request(matrix()[0])
    assert active_feature_upper(raw)==5
    zero=deepcopy(raw);zero['annual_covariance']=[0.]*len(zero['annual_covariance'])
    assert active_feature_upper(zero)==0
    credit=deepcopy(raw);credit['source']['credit']['intervals'][0]['hazards_per_year'][1][3]=.01
    assert active_feature_upper(credit)==feature_bound(credit)
    payment=deepcopy(raw);payment['source']['source']['current']['principal_plan']={'kind':'retained_payment','original_payment':10.,'terminal':{'kind':'residual_payoff'}}
    assert active_feature_upper(payment)==feature_bound(payment)
    anchored=deepcopy(raw);anchored['hazard_loadings']=[dict(interval_index=0,origin_index=0,destination_index=3,betas=[0.])]
    assert active_feature_upper(anchored)==6
    drawn=deepcopy(raw);drawn['source']['performing_draw_tranches']=[dict(review=raw['identity'],draw_ordinal=1,eligible_origins=[True,False,False],source=raw['source']['source'])]
    assert active_feature_upper(drawn)==feature_bound(drawn)


@pytest.mark.parametrize('driver,active,columns',[('brownian',5,56),('correlated_ou',6,84)])
def test_predeclared_cubic_cases_have_a_finite_raw_law_numerical_bound(driver,active,columns):
    from commercial_convergence_benchmark import Cell,request as general_request
    raw=general_request(Cell('learned','call','cubic',driver,paths=256))
    assert active_feature_upper(raw)==active
    assert numerical_reserve(raw)==(2*shape_bound(256,columns)[0]+8*scan_work_upper(raw),shape_bound(256,columns)[1]+128)
    assert 1_400_901_632+numerical_reserve(raw)[0]<4_000_000_000


def fee_law():
    graph=json.loads((Path(__file__).resolve().parents[3]/'examples/models/commercial-generated-fee-graph-1.json').read_text())
    return graph['positions'][0]['generated_policy']['pricing_training']


def test_independent_fee_proof_counts_every_feature_and_recomputes_each_raw_role():
    raw=fee_law()
    assert any(c>0 for c in raw['annual_covariance'])
    assert constant_fee_observations(raw)
    assert full_feature_upper(raw)>=22
    assert active_feature_upper(raw)<full_feature_upper(raw)
    for hazard in [float.fromhex('0x0.0000000000001p-1022'),.01,-.01,float('nan')]:
        changed=deepcopy(raw)
        changed['source']['credit']['intervals'][-1]['hazards_per_year'][2][3]=hazard
        assert not constant_fee_observations(changed)
        assert active_feature_upper(changed)==full_feature_upper(changed)
        assert constant_fee_observations(raw)
    zero=deepcopy(raw);zero['annual_covariance']=[0.]*len(zero['annual_covariance'])
    assert active_feature_upper(zero)==0
    del zero['nonintegral_fees']
    assert not constant_fee_observations(zero)


@pytest.mark.parametrize('mutation',['payment','dated','review','draw','fee_transfer','loan_transfer'])
def test_unsupported_fee_constancy_headers_retain_the_full_numerical_bound(mutation):
    raw=fee_law();source=raw['source']
    if mutation=='payment':
        source['source']['current']['principal_plan']={'kind':'retained_payment','original_payment':10.,'terminal':{'kind':'residual_payoff'}}
    elif mutation=='dated':
        source['source']['current']['principal_plan']={'kind':'dated_supplied_period_opening_nominal-1','principal_events':[]}
    elif mutation=='review':
        source['reviewed_credit_actions']={}
    elif mutation=='draw':
        source['performing_draw_tranches']=[dict(review=raw['identity'],draw_ordinal=1,eligible_origins=[True,False,False],source=source['source'])]
    elif mutation=='fee_transfer':raw['nonintegral_fees']['policy']['transfer']={}
    else:source['source']['transfer']={}
    # These are header-predicate controls, not accepted native financial requests.
    assert not constant_fee_observations(raw)
    assert active_feature_upper(raw)==full_feature_upper(raw)


def draw_law(floating=False):
    from test_commercial_generated_physical import graph_request
    return graph_request(floating)['positions'][0]['generated_policy']['pricing_training']


@pytest.mark.parametrize('floating,active,scan',[(False,1,64),(True,5,65)])
def test_supplied_draw_bound_counts_every_contract_and_pays_header_scan(floating,active,scan):
    raw=draw_law(floating)
    assert active_feature_upper(raw)==active
    assert scan_work_upper(raw)==scan
    assert full_feature_upper(raw)==12+int(floating)
    degree=2
    columns=min(comb(active+degree,degree),raw['training']['paths']//2)
    expected=shape_bound(raw['training']['paths'],columns)
    assert numerical_reserve(raw)==(len(raw['policy']['exercise_dates'])*expected[0]+8*scan,expected[1]+128)
    assert numerical_reserve(raw,fits_continuation=False)==(8*scan,expected[1]+128)


@pytest.mark.parametrize('loading',[0.,-0.,float.fromhex('0x0.0000000000001p-1022'),1.,-1.,float('nan')])
def test_draw_loading_constancy_uses_literal_raw_scalars(loading):
    raw=draw_law(True);raw['coupon_loadings'][0]['loading']=loading
    assert active_feature_upper(raw)==(1 if loading==0. else 5)


@pytest.mark.parametrize('mutation',[
    'original_date','current_date','draw_before_source','draw_transfer','rate_review',
    'original_principal','current_principal','root_review','root_transfer','positive_hazard'])
def test_unsupported_draw_headers_keep_full_bound_without_sample_inference(mutation):
    raw=draw_law();source=raw['source'];draw=source['performing_draw_tranches'][0]
    if mutation=='original_date':draw['source']['original']['asof_ordinal']-=1
    elif mutation=='current_date':draw['source']['current']['asof_ordinal']-=1
    elif mutation=='draw_before_source':draw['draw_ordinal']=source['source']['current']['asof_ordinal']
    elif mutation=='draw_transfer':draw['source']['transfer']={}
    elif mutation=='rate_review':draw['source']['current_accounting_rate_reviews']=[{}]
    elif mutation in ('original_principal','current_principal'):
        role=mutation.split('_')[0]
        draw['source'][role]['principal_plan']={'kind':'dated_supplied_period_opening_nominal-1','principal_events':[]}
    elif mutation=='root_review':source['reviewed_credit_actions']={}
    elif mutation=='root_transfer':source['source']['transfer']={}
    else:source['credit']['intervals'][0]['hazards_per_year'][0][3]=float.fromhex('0x0.0000000000001p-1022')
    assert active_feature_upper(raw)==full_feature_upper(raw)
    if mutation!='positive_hazard':
        raw['annual_covariance']=[0.]*len(raw['annual_covariance'])
        assert active_feature_upper(raw)==full_feature_upper(raw)


def test_draw_ccf_anchor_and_zero_covariance_have_separate_raw_guards():
    raw=draw_law()
    raw['source']['credit']['intervals'][0]['credit_conversion_factor']=[1.]*3
    assert active_feature_upper(raw)==1
    raw['hazard_loadings']=[dict(interval_index=0,origin_index=0,destination_index=3,betas=[0.])]
    assert active_feature_upper(raw)==2
    raw['annual_covariance']=[0.]*len(raw['annual_covariance'])
    assert active_feature_upper(raw)==0
