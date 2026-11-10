"""Independent checks against native core fixtures and deliberately corrupted evidence."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import consumer_macro_reference as reference

CASES=json.loads((Path(__file__).parent/'fixtures/consumer-macro-reference.json').read_bytes())

@pytest.mark.parametrize('case',CASES,ids=[c['name'] for c in CASES])
def test_native_macro_sources_and_hand_moments_verify(case):
    receipt=reference.verify(case['request'],case['result'])
    assert receipt['verified'] and not receipt['adoption_eligible']
    assert receipt['maximum_diagnostic_abs_error']==0.
    cov=case['result']['diagnostics']['training']['sample_annual_covariance']
    assert cov[0]==pytest.approx([4/3,8/3],rel=0,abs=1e-12)
    assert cov[1]==pytest.approx([8/3,16/3],rel=0,abs=1e-12)

@pytest.mark.parametrize('field',[
    'source_digest','training_digest','source','signed_zero','path_key','training_covariance',
    'holdout_mean','row_order','exclusion','adoption','pricing','empirical','identity_policy',
])
def test_coherent_metadata_cannot_certify_corrupted_source_or_diagnostics(field):
    case=deepcopy(CASES[0]);r=case['result']
    if field=='source_digest':r['source_sha256']='0'*64
    elif field=='training_digest':r['training_identity_sha256']='0'*64
    elif field=='source':r['raw_source']['drivers'][0]['innovation_scale']=2.
    elif field=='signed_zero':r['raw_source']['drivers'][1]['raw_domain'][0]=-0.0
    elif field=='path_key':r['raw_source']['scenarios'][0]['paths'][0]['path_key']=17.0
    elif field=='training_covariance':r['diagnostics']['training']['sample_annual_covariance'][0][0]*=2
    elif field=='holdout_mean':r['diagnostics']['holdout']['mean_annualized_standardized_innovation'][0]+=.1
    elif field=='row_order':r['diagnostics']['training']['row_ids'].reverse()
    elif field=='exclusion':r['diagnostics']['excluded_unavailable_at_split'].append('invented')
    elif field=='adoption':r['adoption_eligible']=True
    elif field=='pricing':r['pricing_measure_permission']=True
    elif field=='empirical':r['empirical_accuracy_validated']=True
    else:r['identity_policy']='caller-certified'
    with pytest.raises(ArithmeticError):reference.verify(case['request'],r)


def test_holdout_and_forecast_changes_do_not_fit_training_identity():
    base=deepcopy(CASES[0]['request'])
    _,first,identity=reference.expected(base)
    changed=deepcopy(base)
    changed['history']['observations'][-1]['end_values'][1]+=.5
    changed['scenarios'][-1]['paths'][0]['values'][0][1]+=1.
    _,second,other=reference.expected(changed)
    assert identity==other and first['training']==second['training']
    assert first['holdout']!=second['holdout']


@pytest.mark.parametrize('problem',['lag','future','pair','unknown','domain','scale','release','control_character','nonbaseline_float_path_key'])
def test_independent_raw_validation_rejects_unavailable_or_unsupported_inputs(problem):
    raw=deepcopy(CASES[0]['request'])
    if problem=='lag':del raw['drivers'][0]['availability_lag_days']
    elif problem=='future':raw['scenarios'][0]['issued_at']['ordinal']+=1
    elif problem=='pair':raw['scenarios'][1]['paths'][0]['short_rates_percent'][0]+=.1
    elif problem=='unknown':raw['drivers'][1]['metric']='unreviewed_proxy'
    elif problem=='domain':raw['drivers'][1]['raw_domain'][0]=-1.
    elif problem=='scale':raw['drivers'][1]['scaling_training_end']['ordinal']=raw['as_of']['ordinal']
    elif problem=='release':raw['history']['observations'][0]['published_at'][0]['ordinal']-=1
    elif problem=='control_character':raw['policy']['id']='not\u0085permitted'
    else:raw['scenarios'][1]['paths'][0]['path_key']=float(raw['scenarios'][1]['paths'][0]['path_key'])
    with pytest.raises(ArithmeticError):reference.expected(raw)


def test_verification_cancels_inside_source_walk_without_native_callbacks(monkeypatch):
    from portfolio_risk.core import model_native
    monkeypatch.setattr(model_native,'run',lambda *a,**k:pytest.fail('reference called native'))
    case=CASES[0]
    assert reference.verify(case['request'],case['result'])['verified']
    calls=[0]
    def cancel():
        calls[0]+=1
        return calls[0]>=50
    with pytest.raises(InterruptedError):reference.verify(case['request'],case['result'],cancel)
    assert calls[0]==50
