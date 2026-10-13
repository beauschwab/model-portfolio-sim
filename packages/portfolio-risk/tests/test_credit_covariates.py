"""Public native hazard fit/projection against independent exact-spell arithmetic."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_contracts as models
ROOT=Path(__file__).resolve().parents[3]
def example():return json.loads((ROOT/'examples/models/credit-covariate-1.json').read_text())
def test_covariate_fit_hand_rate_ratio_and_holdout_identity():
    raw=example();result=models.fit_credit_covariates(raw);edge=result['model']['rows'][0]['edges'][0]
    assert math.exp(edge['intercept']['value'])==pytest.approx(.25,abs=1e-9)
    assert math.exp(edge['covariate_coefficients'][0]['value'])==pytest.approx(2.,abs=1e-9)
    assert not result['model']['adoption_eligible']
    changed=deepcopy(raw);holdout=deepcopy(raw['segments'][0])
    holdout.update(segment_id='held-out',account_id='held-out',source=raw['holdout_source'],start=raw['train_end'],end=raw['holdout_end'])
    changed['segments'].append(holdout)
    assert models.fit_credit_covariates(changed)['model']==result['model']
def test_projection_preserves_physical_measure_and_rejects_synthetic_adoption():
    raw=example();model=models.fit_credit_covariates(raw)['model'];origin=raw['holdout_end'];end=dict(origin,ordinal=origin['ordinal']+1)
    identity=dict(id='explicit-projection-review',revision='1')
    request=dict(schema='credit-covariate-1',measure='physical',model=model,model_revision=identity,review=identity,
        use_policy='analytical_only',position_id='loan',scenario_id='base',origin_time=origin,end_time=end,timeout_ms=60000,
        intervals=[dict(interval_id='day',start=origin,end=end,available_time=origin,kind='supplied_projected',source=identity,covariates=[1.],term_level='reference')])
    result=models.project_credit_covariates(request)
    assert result['intervals'][0]['hazards_per_year'][0][3]==pytest.approx(.5,abs=1e-9)
    assert result['measure']=='physical' and not result['adoption_output']
    assert result['coefficient_identity_sha256']==model['coefficient_identity_sha256']
    with pytest.raises(ValueError):models.project_credit_covariates(dict(request,use_policy='adoption'))
    with pytest.raises(ValueError):models.project_credit_covariates(dict(request,measure='pricing'))
    revised=deepcopy(request);revised['model']['rows'][0]['edges'][0]['intercept']['value']+=.01
    with pytest.raises(ValueError):models.project_credit_covariates(revised)
