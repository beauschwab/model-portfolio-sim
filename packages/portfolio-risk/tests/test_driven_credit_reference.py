"""Independent frozen-driver reconstruction and native publication tamper gates."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import driven_credit_reference as reference
from portfolio_risk.analytics import model_ledger


def example():
    return json.loads((Path(__file__).resolve().parents[3]/'examples/models/driven-credit-ledger-1.json').read_text())


@pytest.fixture(scope='module')
def prepared(tmp_path_factory):
    raw=example()
    manifest_path=model_ledger.run_model_ledger(raw,tmp_path_factory.mktemp('driven-ledger'),partition_rows=7)
    manifest=json.loads(manifest_path.read_text())
    header=json.loads((manifest_path.parent/manifest['modeled_source']['path']).read_text())
    return raw,header['prepared_request'],header['source_receipt'],manifest


def test_frozen_raw_drivers_independently_recompute_hazards_and_source_contract(prepared):
    raw,request,receipt,manifest=prepared
    before=deepcopy((raw,request,receipt))
    flags=reference.verify_preparation(raw,request,receipt)
    assert flags['driven_credit_source_recomputed'] and flags['driver_hazards_recomputed']
    assert flags['research_only'] and not flags['adoption_output']
    assert not flags['empirical_validation'] and not flags['regulatory_compliance']
    assert manifest['validation']['driven_credit_source_recomputed']
    assert manifest['validation']['physical_paths_recomputed']
    assert (raw,request,receipt)==before
    for index,run in enumerate(request['runs']):
        assert run['batches'][0]['request']['path']['intervals'][0]['hazards_per_year'][0][3]==pytest.approx(.25 if index==0 else .5,abs=1e-12)


@pytest.mark.parametrize('failure',['hazards','date','lgd','crn','model','source','binding','run','unknown','units','empirical','work','raw_feature','raw_parameter'])
def test_preparation_corruption_rejects_independently(prepared,failure):
    raw,request,receipt,_=deepcopy(prepared)
    path=request['runs'][0]['batches'][0]['request']['path']
    projection=receipt['projections'][0]['projection']
    if failure=='hazards':path['intervals'][0]['hazards_per_year'][0][3]+=.01
    elif failure=='date':path['intervals'][0]['end_year']+=1./365.
    elif failure=='lgd':path['intervals'][0]['loss_given_default'][0]=.9
    elif failure=='crn':path['draw_stream']['paths'][0]['pairs'][0]['waiting_uniform']=.1
    elif failure=='model':projection['coefficient_identity_sha256']='0'*64
    elif failure=='source':projection['intervals'][0]['source']['revision']='changed'
    elif failure=='binding':request['runs'][0]['batches'][0]['request']['bindings'][0]['claim_id']='other'
    elif failure=='run':request['runs'][0]['severity']=2.
    elif failure=='unknown':request['shortcut']=True
    elif failure=='units':request['monetary_unit']='thousands'
    elif failure=='empirical':receipt['empirical_validation']=True
    elif failure=='work':receipt['projection_work_units']+=1
    elif failure=='raw_feature':raw['runs'][0]['batches'][0]['projection']['intervals'][0]['covariates'][0]=.5
    else:raw['runs'][0]['batches'][0]['projection']['model']['rows'][0]['edges'][0]['intercept']['value']+=.1
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,request,receipt)


@pytest.mark.parametrize('field',['path_key','seed','ordinal'])
def test_integer_identities_reject_float_coercion_even_when_numeric_value_matches(prepared,field):
    raw,request,receipt,_=deepcopy(prepared)
    if field=='path_key':request['runs'][0]['batches'][0]['request']['path']['paths'][0]['path_key']=17.
    elif field=='ordinal':receipt['raw_source']['runs'][0]['batches'][0]['projection']['origin_time']['ordinal']=float(raw['as_of_ordinal'])
    else:
        # Strict comparison applies even to seeds above the precise IEEE integer range.
        with pytest.raises(ArithmeticError):reference._equal({'seed':float(2**63+1)},{'seed':2**63+1})
        return
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,request,receipt)


def test_financial_tolerance_and_exact_large_keys_are_separate():
    reference._equal({'amount':100.+1e-10,'path_key':2**63+1},{'amount':100.,'path_key':2**63+1})
    with pytest.raises(ArithmeticError):reference._equal({'path_key':2**63},{'path_key':2**63+1})


def test_scaled_features_term_dummy_and_competing_cures_match_independent_hand_matrix():
    projection=example()['runs'][0]['batches'][0]['projection']
    model=projection['model'];model['term_levels']=['reference','recession']
    model['columns'][0].update(offset=1.,scale=2.,raw_lower=1.,raw_upper=5.)
    driver=projection['intervals'][0];driver.update(covariates=[5.],term_level='recession')
    def p(name,value):
        return dict(name=name,value=value,lower=value,upper=value,ridge_weight=0.,ridge_center=0.)
    import math
    model['rows'][0]=dict(kind='log_linear',edges=[
        dict(destination='watch',intercept=p('a',math.log(2.)),covariate_coefficients=[p('b',math.log(3.))],term_coefficients=[p('term',math.log(2.))]),
        dict(destination='default',intercept=p('default',math.log(4.)),covariate_coefficients=[p('zero',0.)],term_coefficients=[p('term',math.log(5.))])])
    model['rows'][1]['hazards_per_year']=[.5,0.,0.,.1,.2]
    out=reference._project(projection,True,lambda:False)
    expected=[[0.,36.,0.,20.,0.],[.5,0.,0.,.1,.2],[0.]*5]
    for actual,row in zip(out['intervals'][0]['hazards_per_year'],expected):
        assert actual==pytest.approx(row,rel=1e-12,abs=1e-9)


@pytest.mark.parametrize('failure',['future','term','domain','hazard'])
def test_independent_driver_domains_reject_without_native_validation(failure):
    projection=example()['runs'][0]['batches'][0]['projection'];driver=projection['intervals'][0]
    if failure=='future':driver['available_time']['ordinal']+=1
    elif failure=='term':driver['term_level']='unknown'
    elif failure=='domain':driver['covariates']=[2.]
    else:projection['model']['rows'][1]['hazards_per_year']=[101.,0.,0.,0.,0.]
    with pytest.raises(ArithmeticError):reference._project(projection,True,lambda:False)


def test_verifier_never_invokes_native_or_fit_or_execution_callbacks(prepared,monkeypatch):
    from portfolio_risk.analytics import model_contracts
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):raise AssertionError('native/financial execution in verifier')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(model_contracts,'fit_credit_covariates',forbidden)
    monkeypatch.setattr(model_contracts,'project_credit_covariates',forbidden)
    assert reference.verify_preparation(*prepared[:3])['driven_credit_source_recomputed']


def test_preparation_verification_cancellation(prepared):
    with pytest.raises(InterruptedError):reference.verify_preparation(*prepared[:3],check=lambda:True)


@pytest.mark.parametrize('field',['projected','receipt'])
def test_failed_preparation_verification_withholds_complete_publication(tmp_path,monkeypatch,field):
    original=reference.verify_preparation
    def changed(raw,request,receipt,check):
        if field=='projected':request['runs'][0]['batches'][0]['request']['path']['intervals'][0]['hazards_per_year'][0][3]=99.
        else:receipt['projections'][0]['projection']['intervals'][0]['hazards_per_year'][0][3]=99.
        return original(raw,request,receipt,check)
    monkeypatch.setattr(reference,'verify_preparation',changed)
    with pytest.raises(ArithmeticError):model_ledger.run_model_ledger(example(),tmp_path,partition_rows=7)
    assert not list(tmp_path.rglob('manifest.json'))
