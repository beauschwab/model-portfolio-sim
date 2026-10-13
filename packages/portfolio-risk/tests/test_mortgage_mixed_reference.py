"""Pure immutable-source/publication tests; actual native parity is separate."""
from copy import deepcopy
from pathlib import Path
import json
import pytest
from portfolio_risk.analytics import mortgage_mixed_reference as reference
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as financial
ROOT=Path(__file__).resolve().parents[3]

@pytest.fixture(scope='module')
def source():
    raw=json.loads((ROOT/'examples/models/mortgage-mixed-accounting-1.json').read_text())
    normalized,_,_,_,admission=financial.admit(raw)
    # Synthetic independent input exercises rejection/control mechanics only.
    # This is not a native-versus-reference financial parity assertion.
    sources=[dict(source_id=x['source_id'],native=financial.pending.reconstruct(x['request'])) for x in normalized['sources']]
    # Structural numerical-work metadata is required by the complete receipt
    # schema. Zero is an admitted synthetic diagnostic here, not measured native
    # work or native-versus-reference evidence; all money remains independently
    # reconstructed. Native process tests supply actual producer counters.
    for item in sources:
        for interval in item['native']['source']['intervals']:
            interval['operator']['work_multiplications'] = 0
    receipt=dict(schema='mortgage-mixed-preparation-1',raw_request=normalized,admission=admission,
        sources=sources,source_groups_sole_postings=True,memo_checkpoint_policy='native_complete_daily_boundary',
        **{key:False for key in reference.PERMISSIONS})
    return raw,receipt

@pytest.mark.parametrize('target',['raw_face','raw_sign_zero','original_anchor','source_terms','source_id','approval_alias'])
def test_raw_source_clones_and_permissions_are_exact(source,target):
    raw,original=source;receipt=deepcopy(original)
    if target=='raw_face':receipt['raw_request']['owners'][0]['allocated_face']+=1e-12
    elif target=='raw_sign_zero':receipt['raw_request']['owners'][0]['runs'][0]['severity']=-0.
    elif target=='original_anchor':receipt['sources'][0]['native']['source']['raw_request']['acquisition']['original_annual_effective_rate']+=1e-12
    elif target=='source_terms':receipt['sources'][0]['native']['source']['source']['native_projection']['original_principal']+=1e-12
    elif target=='source_id':receipt['sources'][0]['source_id']='unbound'
    else:receipt['accounting_standard_approval']=0
    with pytest.raises((ArithmeticError,ValueError)):reference.verify_preparation(raw,raw['dated'],receipt)

@pytest.mark.parametrize('where',['preparation','stream'])
def test_cancellation_cannot_return_verification_receipt(source,where):
    raw,receipt=source
    if where=='preparation':
        with pytest.raises(InterruptedError):reference.verify_preparation(raw,raw['dated'],receipt,lambda:True)
    else:
        control=reference.verify_preparation(raw,raw['dated'],receipt)
        with pytest.raises(InterruptedError):reference.verify_stream(raw,control,[],lambda:True)

@pytest.mark.parametrize('complete',[{'complete':1,'protocol':3},{'complete':True,'protocol':3.0}])
def test_missing_rows_and_completion_aliases_are_never_evidence(source,complete):
    raw,receipt=source;control=reference.verify_preparation(raw,raw['dated'],receipt)
    with pytest.raises(ArithmeticError):reference.verify_stream(raw,control,[complete])
