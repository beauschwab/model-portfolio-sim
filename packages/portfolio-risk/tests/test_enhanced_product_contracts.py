"""Native transport ownership and hand calculations for additive product contracts."""
import json
from pathlib import Path
import pytest

from portfolio_risk.analytics import model_contracts as models

ROOT=Path(__file__).resolve().parents[3]


def example(name):
    return json.loads((ROOT/'examples/models'/name).read_text(encoding='utf-8'))


def test_consumer_installment_uses_actual_dates_original_payment_and_note_rate():
    raw=example('consumer-installment-1.json');result=models.project_consumer_installment(raw)
    assert result['identity']==raw['identity']
    assert result['closing_principal']==pytest.approx(0.,abs=1e-12)
    assert [e['principal'] for e in result['cashflows']]==pytest.approx([500.,500.])
    assert [e['interest'] for e in result['cashflows']]==pytest.approx([29.,15.5])
    assert sum(e['interest'] for e in result['recognition'])==pytest.approx(44.5)
    assert result['original_payment']==529.


def test_card_billed_and_unbilled_earned_rights_remain_separate():
    raw=example('consumer-card-1.json');result=models.simulate_consumer_card(raw)
    assert result['identity']==raw['identity']
    assert result['closing_principal']==1000.
    assert result['closing_interest']==pytest.approx(1.)
    assert result['closing_unbilled_interest']==pytest.approx(1.)
    assert sum(e['interest'] for e in result['recognition'])==pytest.approx(2.)
    assert result['cashflows']==[]


def test_dated_income_retains_original_yield_and_actual_cash():
    raw=example('dated-income-1.json');result=models.evaluate_dated_income(raw)
    row=result['rows'][0]
    assert row['effective_interest']==pytest.approx(10.,abs=2e-14)
    assert row['closing_gross_carrying_value']==0.
    assert row['closing_book_adjustment']==0.
    assert row['contractual_interest_earned']==10.


@pytest.mark.parametrize('name,adapter',[
    ('deposit-state-1.json',models.simulate_deposit_state),
    ('mortgage-state-1.json',models.project_mortgage_state),
    ('funding-contract-1.json',models.evaluate_funding_contract),
    ('model-market-1.json',models.analyze_model_market),
])
def test_product_state_examples_execute_native_and_reject_unversioned_options(name,adapter):
    raw=example(name)
    request=raw['request'] if 'request' in raw else raw
    assert isinstance(adapter(request),dict)
    with pytest.raises(ValueError):adapter(dict(request,python_backend=True))


@pytest.mark.parametrize('name,adapter',[
    ('term-indexed.json',models.generate_indexed_contract),
    ('term-exercise.json',models.value_conditional_exercise),
    ('term-cd-election.json',models.elect_cd),
])
def test_term_protocol_examples_reject_unknown_financial_options(name,adapter):
    raw=example(name)['request'];result=adapter(raw)
    assert isinstance(result,dict)
    with pytest.raises(ValueError):adapter(dict(raw,python_backend=True))


def test_missing_native_library_never_runs_a_python_model(monkeypatch):
    from portfolio_risk.core import quant_native
    monkeypatch.setattr(quant_native,'library_path',lambda:Path('absent-enhanced-native.dll'))
    quant_native._load.cache_clear()
    with pytest.raises(RuntimeError,match='not built'):
        models.simulate_consumer_card(example('consumer-card-1.json'))
