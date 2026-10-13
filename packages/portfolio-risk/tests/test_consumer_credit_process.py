"""Fresh public lifecycle process, independently verified supplied credit sources.

The optional test-only DEBUG override must name the real portfolio-lifecycle CLI.
There is no skipped missing-artifact gate, fallback, or production backend choice.
"""
from copy import deepcopy
import json
import os
from pathlib import Path

import pytest

from portfolio_risk.analytics import consumer_credit_reference as oracle
from portfolio_risk.core import model_native
from test_consumer_credit_reference import raw_default, ident, ROOT

SCHEMA='consumer-credit-accounting-1'


@pytest.fixture(autouse=True)
def lifecycle_binary(monkeypatch):
    override=os.environ.get('CONSUMER_CREDIT_LIFECYCLE_TEST_BINARY')
    if override:
        path=Path(override).resolve()
        assert path.name in {'portfolio-lifecycle','portfolio-lifecycle.exe'},'only the public lifecycle CLI may produce these tests'
        monkeypatch.setattr(model_native,'binary_path',lambda:path)
    assert model_native.binary_path().is_file(),'build fresh native lifecycle artifact; missing artifacts are not skipped'


def accepted(case):
    raw=raw_default(auto=case=='auto',delay=0 if case=='same_day' else 10,chargeoff=case!='retained_claim')
    start=raw['contract']['as_of_ordinal']
    if case=='canonical':return json.loads((ROOT/'examples/models/consumer-credit-accounting-1.json').read_text())
    if case.startswith('payoff'):
        raw['contract']['steps'][0]['action']={'kind':'payoff'};raw['chargeoffs']=[];raw['opening_allowance']=0.
        raw['acquisition']['cost']={'payoff_par':100.,'payoff_premium':103.,'payoff_discount':90.,'payoff_negative_yield':90.}[case]
        raw['acquisition']['original_annual_effective_rate']=-.02 if case=='payoff_negative_yield' else .02
    elif case=='integer_money':
        raw['acquisition'].update(principal=100,cost=100);raw['contract'].update(original_principal=100,opening_principal=100)
        raw['opening_allowance']=40
    elif case=='zero_net':raw['contract']['product']['collection_cost']=60.
    elif case=='allowance_before':
        raw['opening_allowance']=0.
        raw['allowance_reviews']=[dict(review=ident('before'),recognition_ordinal=start+10,phase='before_credit',target_amount=40.)]
    elif case=='allowance_after':
        raw['allowance_reviews']=[dict(review=ident('after'),recognition_ordinal=start+20,phase='after_credit',target_amount=3.)]
    elif case=='expense_shortfall':
        raw['opening_allowance']=0.;raw['policy']['accounting']['allowance_shortfall']='expense_at_reviewed_chargeoff'
    elif case=='transferred_claim':
        raw['chargeoffs']=[];raw['acquisition'].update(cost=103.,original_annual_effective_rate=.02)
        raw['policy']['accounting'].update(accrued='transfer_until_reviewed_reversal',basis='transfer_until_reviewed_chargeoff')
    elif case=='partial_payment':
        raw['contract']['original_payment']=50.;raw['contract']['steps'][0]['ordinal']=start+20
        raw['contract']['product']['recovery_delay_days']=5;raw['chargeoffs']=[]
    return raw


@pytest.mark.parametrize('case',['canonical','auto','same_day','retained_claim','payoff_par','payoff_premium',
    'payoff_discount','payoff_negative_yield','zero_net','integer_money','allowance_before','allowance_after',
    'expense_shortfall','transferred_claim','partial_payment'])
def test_public_lifecycle_result_matches_complete_independent_source(case,monkeypatch):
    raw=accepted(case);original=deepcopy(raw)
    result,execution=model_native.run(SCHEMA,raw,timeout=30.)
    assert raw==original
    assert execution['backend']=='rust' and execution['schema']==SCHEMA
    assert execution['binary_sha256']==model_native.binary_identity()
    from portfolio_risk.core import quant_native
    def forbidden(*args,**kwargs):pytest.fail('independent publication oracle invoked native finance')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(model_native,'run',forbidden)
    receipt=oracle.verify(raw,result)
    assert receipt['verified'] and receipt['gross_recovery_cost_separated']
    assert not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']
    assert receipt==oracle.verify(json.loads(json.dumps(raw)),json.loads(json.dumps(result)))


@pytest.mark.parametrize('case',['missing_tail','surplus_gross','uncovered_cost','chargeoff_shortfall',
    'pricing','float_date','pcd','unknown_field'])
def test_public_lifecycle_rejects_unsupported_source_without_fallback(case):
    raw=raw_default();c=raw['contract']
    if case=='missing_tail':c['product']['recovery_delay_days']=21
    elif case=='surplus_gross':
        c['product']=dict(kind='auto',new_vehicle=False,vintage_ordinal=c['as_of_ordinal'],original_ltv=1.,channel='branch',
            collateral_value_as_of=101.,annual_depreciation=0.,sale_haircut=0.,repossession_cost=10.,recovery_delay_days=10)
    elif case=='uncovered_cost':c['product']['collection_cost']=61.
    elif case=='chargeoff_shortfall':raw['opening_allowance']=0.
    elif case=='pricing':c['identity']['measure']='pricing'
    elif case=='float_date':c['steps'][0]['ordinal']=float(c['steps'][0]['ordinal'])
    elif case=='pcd':raw['acquisition']['accounting_basis']='pcd'
    elif case=='unknown_field':raw['supplied_intents']=[]
    with pytest.raises(ArithmeticError):oracle.reconstruct(raw)
    with pytest.raises(ValueError,match='native model rejected input'):
        model_native.run(SCHEMA,raw,timeout=30.)


def test_public_transport_cancelled_request_emits_no_result():
    with pytest.raises(InterruptedError,match='cancelled'):
        model_native.run(SCHEMA,accepted('canonical'),cancelled=lambda:True,timeout=30.)
