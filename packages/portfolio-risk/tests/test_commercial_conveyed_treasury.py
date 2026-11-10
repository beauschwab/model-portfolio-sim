"""Complete purchased fee source -> same journal -> reviewed capital and FTP."""
from copy import deepcopy
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.analytics.commercial_fee_candidate_treasury_reference import verify
from portfolio_risk.core.commercial_fee_candidate_treasury_native import run
from portfolio_risk.core import product_strategy_native

ROOT=Path(__file__).resolve().parents[3]

@pytest.fixture(autouse=True)
def selected_native(monkeypatch):
    if selected:=os.environ.get('PORTFOLIO_FEE_TREASURY_TEST_BINARY'):
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected))

def request():
    return json.loads((ROOT/'examples/models/commercial-conveyed-fee-treasury-1.json').read_text())

def negative(raw):
    position=raw['raw_candidate']['commercial']['units'][0]['graph']['positions'][0]
    def flip(v):
        if type(v)is dict:
            for k,x in v.items():
                if k in ('amount','entitlement','allocated_cost','original_signed_fee_quote'):v[k]=-x
                else:flip(x)
        elif type(v)is list:
            for x in v:flip(x)
    flip(position['fees'])
    position['fees']['total_purchase_cost']=position['original_signed_cash_quote']-5.
    # This Treasury adapter admits nonnegative asset funding. The reviewed
    # policy retains every gross liability but explicitly excludes its funding
    # credit; the standalone source4 proof supports signed research separately.
    for review in raw['ftp']['reviews']:
        for channel in review['policy']['channels']:
            if channel['channel'] in ('fee_payable','deferred_fee_income','acquired_fee_liability','acquired_fee_liability_valuation_adjustment'):
                channel['coefficient']=0

@pytest.mark.parametrize('sign',[1,-1])
@pytest.mark.parametrize('face',[0.,50.,100.])
def test_conveyed_gross_claim_contra_cash_and_buyer_earnings_use_one_source(sign,face):
    raw=request()
    if sign<0:negative(raw)
    raw['raw_candidate']['allocation'][0]['face']=face
    actual,_=run(raw);receipt=verify(raw,actual)
    assert receipt['verified'] and receipt['closing_capital_verified'] and receipt['daily_ftp_verified']
    assert actual['conveyed_fee_source_adoption']is True
    assert actual['capital']['conveyed_fee_asset_adjustment_grouped']is True
    for path in actual['funding_paths']:
        assert path['request']['schema']=='modeled-product-ftp-source-channels-4'
        channels={a['channel']:a['amount']for a in path['request']['states'][0]['channels']}
        assert len(channels)==26
        assert channels['fee_receivable'if sign>0 else 'fee_payable']==6.
        assert channels['acquired_fee_asset_valuation_adjustment'if sign>0 else 'acquired_fee_liability_valuation_adjustment']==1.
        assert path['opening_cash']==pytest.approx(-(99.+sign*5.),abs=1e-9)
        earnings=[a for d in path['preparation']['days']for a in d['earnings_by_channel']]
        assert sum(a['amount']for a in earnings if a['channel']=='fee_cost_release')==pytest.approx(sign*face/100.,abs=1e-9)
        assert not any(a['channel']=='service_fee'and a['amount']!=0. for a in earnings)
        assert path['preparation']['scale']==face/100.
    assert not actual['economic_loss_deducted_again']and not actual['regulatory_credit_rwa_approval']

@pytest.fixture(scope='module')
def completed():
    raw=request()
    with pytest.MonkeyPatch.context()as patch:
        if selected:=os.environ.get('PORTFOLIO_FEE_TREASURY_TEST_BINARY'):
            patch.setattr(product_strategy_native,'binary_path',lambda:Path(selected))
        result,_=run(raw)
    return raw,result

@pytest.mark.parametrize('target',['contra','gross','buyer_income','opening_cash','capital_group','flag','review'])
def test_conveyed_source_replay_rejects_rehashed_money_and_authority(completed,target):
    raw,original=completed;actual=deepcopy(original)
    if target in ('contra','gross'):
        name='acquired_fee_asset_valuation_adjustment'if target=='contra'else'fee_receivable'
        next(a for a in actual['funding_paths'][0]['request']['states'][0]['channels']if a['channel']==name)['amount']+=1.
    elif target=='buyer_income':
        next(a for d in actual['funding_paths'][0]['preparation']['days']for a in d['earnings_by_channel']if a['channel']=='fee_cost_release')['amount']+=1.
    elif target=='opening_cash':actual['funding_paths'][0]['opening_cash']-=1.
    elif target=='capital_group':actual['capital']['conveyed_fee_asset_adjustment_grouped']=False
    elif target=='flag':actual['conveyed_fee_source_adoption']=False
    else:actual['raw_request']['secondary_fee_review']['revision']='forged'
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,actual)

@pytest.mark.parametrize('target',['review','channel','weight','different_weight','unselected_source'])
def test_conveyed_adoption_requires_explicit_complete_review_before_generation(target):
    raw=request()
    if target=='review':raw.pop('secondary_fee_review')
    elif target=='channel':raw['ftp']['reviews'][0]['policy']['channels'].pop()
    elif target=='weight':raw['capital']['policies'][0]['gl_risk_weights'].pop('commercial_acquired_fee_asset_valuation_adjustment')
    elif target=='different_weight':raw['capital']['policies'][0]['gl_risk_weights']['commercial_fee_receivable']=.5
    else:
        old=json.loads((ROOT/'examples/models/commercial-candidate-fee-treasury-1.json').read_text())
        old['secondary_fee_review']=raw['secondary_fee_review'];raw=old
    with pytest.raises((ValueError,RuntimeError)):run(raw)

def test_negative_fee_tail_cannot_be_silently_clipped_into_asset_ftp():
    raw=request();negative(raw)
    for review in raw['ftp']['reviews']:
        for channel in review['policy']['channels']:
            if channel['channel']=='fee_payable':channel['coefficient']=-1
            elif channel['channel']=='acquired_fee_liability_valuation_adjustment':channel['coefficient']=1
    with pytest.raises(ValueError,match='negative funding balance'):run(raw)
