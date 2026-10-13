"""Pending official default-source/default-strategy acceptance, no overrides."""
from copy import deepcopy
from pathlib import Path
import hashlib,json,subprocess,sys
import pytest
from portfolio_risk.analytics import model_ledger
from portfolio_risk.analytics import mortgage_closing_capital_reference as reference
from portfolio_risk.core import mortgage_capital_native as runtime
ROOT=Path(__file__).resolve().parents[3]

def raw():return json.loads((ROOT/'examples/models/mortgage-closing-capital-1.json').read_text())

@pytest.fixture(scope='module')
def complete(tmp_path_factory):
    import polars as pl
    from portfolio_risk.analytics.mortgage_mixed_transport import SCHEMAS
    request=raw();source=request['raw_mixed']
    path=model_ledger.run_model_ledger(source,tmp_path_factory.mktemp('closing-capital-source'),partition_rows=7,timeout=60.)
    manifest=json.loads(path.read_text());header=json.loads((path.parent/manifest['modeled_source']['path']).read_text())
    class Frames:
        # The persisted verifier requires typed Polars schemas, not display names.
        schemas={name:dict(schema) for name,schema in SCHEMAS.items()}
        @property
        def completion(self):
            ref=manifest['producer_completion'];body=(path.parent/ref['path']).read_bytes()
            assert hashlib.sha256(body).hexdigest()==ref['sha256'];return json.loads(body)
        def frames(self,name):
            for part in manifest['tables'][name]['parts']:
                body=(path.parent/part['path']).read_bytes()
                assert hashlib.sha256(body).hexdigest()==part['sha256']
                yield pl.read_parquet(path.parent/part['path'])
    control=reference.verify_source(source,header,Frames())
    policies={k:request[k] for k in ('treasury','policies','amount_multiplier','exposure_policy')}|{'kind':'ledger'}
    prepared=reference.native_request(policies,source,request['closing_review'],control)
    result,execution=runtime.run(prepared,timeout=60.)
    return prepared,result,execution,control,manifest

def test_default_raw_rebuild_and_reopened_ordered_gl_authorize_only_closing(complete):
    request,result,execution,control,source=complete
    assert runtime.binary_path().parent.name=='release'
    receipt=reference.verify_capital(request,result,control)
    assert receipt['closing_capital_eligible'] is True
    for key in ('capital_ftp_eligible','interim_capital_eligible','accounting_standard_approval','empirical_validation','production_adoption','regulatory_credit_rwa_approval'):
        assert receipt[key] is False
    assert source['validation']['closing_capital_eligible'] is False
    assert execution['backend']=='rust'
    assert result['capital_bridge'][0]['assets']==pytest.approx(1050.2131168226465,abs=1e-9)
    assert any(row['balance']<0 for row in result['rwa_contributions'] if row['gl_account']=='book_adjustment')


def _browser_numbers(value):
    if type(value) is float and value.is_integer():
        return int(value)
    if type(value) is dict:
        return {key: _browser_numbers(item) for key, item in value.items()}
    if type(value) is list:
        return [_browser_numbers(item) for item in value]
    return value


def test_integral_reviewed_policy_fields_verify_all_native_capital_reports(complete):
    request, _, _, control, _ = complete
    changed = deepcopy(request)
    changed['policies'] = _browser_numbers(changed['policies'])
    changed['amount_multiplier'] = _browser_numbers(changed['amount_multiplier'])
    original = deepcopy(changed)
    result, _ = runtime.run(changed, timeout=60.)
    assert reference.verify_capital(changed, result, control)['verified'] is True
    reference.financial.exact(changed, original, 'unchanged original policy JSON')
    assert type(changed['policies'][0]['equity_exclusions']) is int
    assert type(result['capital_bridge'][0]['equity_exclusions']) is float
    assert all(type(row['risk_weight']) is float for row in result['exposure_groups'])
    bad = deepcopy(result)
    bad['exposure_groups'][0]['risk_weight'] += .01
    with pytest.raises(ArithmeticError):
        reference.verify_capital(changed, bad, control)


@pytest.mark.parametrize('target', ['include_aoci', 'weight_bool', 'capital_bool', 'requirement_bool', 'unknown'])
def test_private_financial_copy_rejects_boolean_money_and_unknown_policy(target):
    policy = _browser_numbers(raw()['policies'][0])
    if target == 'include_aoci':
        policy['include_aoci'] = 1
    elif target == 'weight_bool':
        policy['gl_risk_weights']['cash'] = False
    elif target == 'capital_bool':
        policy['capital']['eligible_tlac'] = True
    elif target == 'requirement_bool':
        next(iter(policy['capital']['requirements'].values()))['buffer'] = False
    else:
        policy['automatic_eligibility'] = True
    with pytest.raises((ArithmeticError, ValueError)):
        reference._financial_policy(policy)


def test_private_financial_copy_preserves_null_metadata_and_signed_zero():
    policy = raw()['policies'][0]
    policy['capital']['advanced_rwa'] = None
    policy['equity_exclusions'] = -0.
    original = deepcopy(policy)
    typed = reference._financial_policy(policy)
    assert typed['capital']['advanced_rwa'] is None
    assert typed['capital']['period'] == policy['capital']['period']
    assert typed['include_aoci'] is policy['include_aoci']
    assert typed['equity_exclusions'].hex() == (-0.).hex()
    reference.financial.exact(policy, original, 'unchanged original signed policy')

@pytest.mark.parametrize('target',['digest','closure_flag','basis','review_alias'])
def test_independent_capital_receipt_rejects_saved_tamper(complete,target):
    request,result,_,control,_=complete;changed=deepcopy(result)
    if target=='digest':changed['mortgage_closing_source']['raw_mixed_sha256']='a'*64
    elif target=='closure_flag':changed['mortgage_closing_source']['closing_capital_eligible']=1
    elif target=='basis':changed['rwa_contributions'][0]['balance']+=1.
    else:changed['mortgage_closing_source']['review']['revision']=1
    with pytest.raises((ArithmeticError,ValueError)):reference.verify_capital(request,changed,control)

@pytest.mark.parametrize('target',['source_extra','gross','maturity_policy','missing_scope'])
def test_invalid_raw_or_reviewed_scope_fails_native_before_verifier(target):
    request=raw()
    if target=='source_extra':request['raw_mixed']['sources'][0]['request']['source']['original_contract']['rate']['annual_rate']=0.
    elif target=='gross':request['raw_mixed']['dated']['dated']['sources'][0]['schedule']['events'][0]['components']['cash_interest']=10001.
    elif target=='missing_scope':request['policies'].pop()
    else:request['exposure_policy']['negative_net_carrying']='automatic_zero'
    with pytest.raises((ValueError,RuntimeError,ArithmeticError)):runtime.run(request,timeout=60.)

def test_fresh_default_strategy_process_forbids_unrelated_financial_callbacks():
    script='''
import json,sys
from pathlib import Path
from portfolio_risk.analytics import model_contracts,balance_stream,balance_stress
from portfolio_risk.core import model_native,quant_native,mortgage_capital_native as runtime
def forbidden(*a,**kw):raise AssertionError('financial callback outside raw native capital process')
model_native.run=forbidden;quant_native.term_call=forbidden
balance_stream._python=forbidden;balance_stress._simulate=forbidden
for name in ('account_mortgage_credit','account_card_credit','account_card_credit_v2'):setattr(model_contracts,name,forbidden)
assert runtime.binary_path().parent.name=='release'
result,execution=runtime.run(json.loads(Path(sys.argv[1]).read_text()),timeout=60.)
assert result['mortgage_closing_source']['native_raw_replayed'] is True
assert execution['backend']=='rust'
'''
    p=subprocess.run([sys.executable,'-c',script,str(ROOT/'examples/models/mortgage-closing-capital-1.json')],text=True,capture_output=True,timeout=90)
    assert p.returncode==0,p.stderr

def test_cancel_before_launch_has_no_fallback(monkeypatch):
    from portfolio_risk.core import product_strategy_native
    def forbidden(*a,**kw):raise AssertionError('cancelled request launched child')
    monkeypatch.setattr(product_strategy_native.subprocess,'Popen',forbidden)
    with pytest.raises(InterruptedError):runtime.run(raw(),cancelled=lambda:True)
