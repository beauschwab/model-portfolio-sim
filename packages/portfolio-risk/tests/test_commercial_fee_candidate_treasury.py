"""Real acquired fee source, ordered same-Sim journal and treasury controls."""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest

from portfolio_risk.core.commercial_fee_candidate_treasury_native import run
from portfolio_risk.core import product_strategy_native
from portfolio_risk.analytics.commercial_fee_candidate_treasury_reference import verify

ROOT=Path(__file__).resolve().parents[3]


@pytest.fixture(autouse=True)
def selected_binary(monkeypatch):
    if executable:=os.environ.get('PORTFOLIO_FEE_TREASURY_TEST_BINARY'):
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(executable))


def request():
    return json.loads((ROOT/'examples/models/commercial-candidate-fee-treasury-1.json').read_text())


@pytest.fixture
def result():
    raw=request();actual,_=run(raw)
    return raw,actual


def test_actual_acquired_fee_sources_replay_loan_fee_journal_capital_and_full_funding_tail(result):
    raw,actual=result;receipt=verify(raw,actual)
    assert receipt['verified'] and receipt['ordered_journal_replayed'] and receipt['persisted_partitions_replayed']
    assert receipt['closing_capital_verified'] and receipt['daily_ftp_verified']
    assert actual['source_candidate']['producer_completion']==dict(complete=True,protocol=7)
    assert actual['full_end_ordinal']==738931
    for path in actual['funding_paths']:
        assert len(path['request']['states'][0]['channels'])==24
        assert len(path['preparation']['days'])==45
        assert path['request']['states'][36]['channels'][0]['amount']==0.
        assert path['request']['states'][36]['channels'][22]['amount']==3.
    assert not receipt['mixed_ledger_adoption'] and not receipt['empirical_validation']


@pytest.mark.parametrize('target',['raw_policy','service','cost_release','gross','loan_eir','scale','cash','capital_group','treasury_profit','flag'])
def test_rehashed_fee_economics_or_authority_cannot_pass_raw_independent_replay(result,target):
    raw,actual=result
    if target=='raw_policy':actual['raw_request']['ftp']['reviews'][0]['policy']['channels'][22]['coefficient']=0
    elif target in ('service','cost_release'):
        channel='service_fee' if target=='service' else 'fee_cost_release'
        row=next(a for day in actual['funding_paths'][0]['preparation']['days'] for a in day['earnings_by_channel'] if a['channel']==channel and a['amount']!=0.)
        row['amount']+=1.
    elif target=='gross':actual['source_candidate']['tables']['commercial_fee_daily'][0]['native_fee_snapshot']['acquired_fee_asset']+=1.
    elif target=='loan_eir':actual['ftp']['prepared_specification']['positions'][0]['external_interest']+=1.
    elif target=='scale':actual['funding_paths'][0]['preparation']['scale']+=.1
    elif target=='cash':actual['funding_paths'][0]['opening_cash']+=1.
    elif target=='capital_group':actual['capital']['exposure_groups'][0]['policy_rwa_contribution']+=1.
    elif target=='treasury_profit':actual['ftp']['ftp_positions'][0]['business_profit']+=1.
    elif target=='flag':actual['regulatory_credit_rwa_approval']=True
    with pytest.raises((ArithmeticError,ValueError)):
        verify(raw,actual)


def test_zero_allocation_retains_unweighted_fee_source_and_tail():
    raw=request();raw['raw_candidate']['allocation'][0]['face']=0.
    actual,_=run(raw);verify(raw,actual)
    assert all(path['preparation']['scale']==0. for path in actual['funding_paths'])
    assert all(row['transaction_number'] is None for row in actual['source_candidate']['tables']['commercial_fee_lineage'])
    assert all(path['request']['cash'] for path in actual['funding_paths'])


def test_negative_fee_rights_keep_separate_gross_obligations_and_explicit_funding_policy():
    raw=request();position=raw['raw_candidate']['commercial']['units'][0]['graph']['positions'][0]
    def flip(value):
        if type(value)is dict:
            for key,item in value.items():
                if key in ('amount','entitlement','allocated_cost','original_signed_fee_quote'):value[key]=-item
                else:flip(item)
        elif type(value)is list:
            for item in value:flip(item)
    flip(position['fees']);position['fees']['total_purchase_cost']=position['original_signed_cash_quote']-5.
    for review in raw['ftp']['reviews']:
        next(c for c in review['policy']['channels'] if c['channel']=='acquired_fee_liability')['coefficient']=0
    actual,_=run(raw);verify(raw,actual)
    assert actual['funding_paths'][0]['request']['states'][0]['channels'][23]['amount']==5.
    assert actual['funding_paths'][0]['request']['states'][0]['channels'][22]['amount']==0.


def test_missing_or_incompatible_review_refuses_before_bridge_publication():
    for mutation in ('channel','fee_weight','calendar'):
        raw=request()
        if mutation=='channel':raw['ftp']['reviews'][0]['policy']['channels'].pop()
        elif mutation=='fee_weight':raw['capital']['policies'][0]['gl_risk_weights'].pop('commercial_acquired_fee_asset')
        else:raw['raw_candidate']['specification']['horizon_days']=65
        with pytest.raises((RuntimeError,ValueError)):
            run(raw)
