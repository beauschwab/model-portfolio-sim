"""Real native boundaries reject discarded financial instruction fields."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.core import model_native, product_strategy_native
from portfolio_risk.analytics import ledger_native

ROOT=Path(__file__).resolve().parents[3]


def example(schema):
    return json.loads((ROOT/'examples/models'/f'{schema}.json').read_text())


@pytest.fixture(autouse=True)
def no_python_financial_callbacks(monkeypatch):
    from portfolio_risk.core import quant_native, decision_native
    def forbidden(*args,**kwargs):pytest.fail('Python or in-process financial callback')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)


@pytest.mark.parametrize('kind',['all_assets','all_liabilities'])
@pytest.mark.parametrize('field',['unit_id','template_id','amount'])
def test_actual_product_strategy_rejects_narrowing_fields_on_whole_side(kind,field):
    raw=example('strategy-product-1')
    raw['face_limits'][0]['selector']={'kind':kind,field:0. if field=='amount' else 'loan-unit-1'}
    before=deepcopy(raw)
    with pytest.raises(ValueError,match='unknown field'):
        product_strategy_native.run(raw,threads=1,timeout=30.)
    assert raw==before


@pytest.mark.parametrize('schema',['card-credit-accounting-1','card-credit-accounting-2'])
@pytest.mark.parametrize('field',['ordinal','amount'])
def test_actual_card_source_default_cannot_ignore_settlement_fields(schema,field):
    if schema.endswith('-2'):
        from test_card_credit_v2_reference import raw as case, ACCEPTED
        raw=next(case(i) for i in ACCEPTED if case(i)['termination']['kind']=='source_default')
    else:raw=example(schema)
    raw['termination'][field]=0.
    before=deepcopy(raw)
    with pytest.raises(ValueError,match='unknown field'):
        model_native.run(schema,raw,timeout=30.)
    assert raw==before


@pytest.mark.parametrize('field',['ordinal','amount'])
def test_actual_csa_retention_cannot_ignore_settlement_fields(field):
    from test_derivative_accounting import mpor_request,csa
    raw=mpor_request()
    instruction=csa(raw)['paths'][0]['default']['mpor']['settlement_instructions'][0]
    instruction['disposition'][field]=0.
    with pytest.raises(ValueError,match='unknown field'):
        model_native.run('derivative-accounting-1',raw,timeout=30.)


@pytest.mark.parametrize('field',['principal_fraction','amount'])
def test_actual_credit_validation_cannot_ignore_maturity_payment_fields(field):
    raw=example('credit-ledger-1')
    raw['runs'][0]['positions'][0]['boundaries'][0]['action'][field]=[0.,0.,0.] if field=='principal_fraction' else 0.
    before=deepcopy(raw)
    with pytest.raises(ValueError,match='unknown field'):
        ledger_native.validate_spec(raw)
    assert raw==before
