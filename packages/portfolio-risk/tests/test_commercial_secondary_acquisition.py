"""Actual secondary raw graph -> buyer journal, independently replayed."""
from copy import deepcopy
import json
import os
from pathlib import Path
import subprocess

import pytest
from portfolio_risk.analytics.commercial_candidate_reference import verify
from portfolio_risk.core.commercial_candidate_native import run

ROOT=Path(__file__).resolve().parents[3]

@pytest.fixture(autouse=True)
def selected_debug_artifact(monkeypatch):
    selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')

def request():
    return json.loads((ROOT/'examples/models/commercial-secondary-candidate-journal-1.json').read_text())

def browser_request():
    # Exercise the actual JS scalar-category changes, including integral money.
    js='const fs=require("fs");process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(process.argv[1],"utf8"))))'
    return json.loads(subprocess.run(['node','-e',js,str(ROOT/'examples/models/commercial-secondary-candidate-journal-1.json')],check=True,capture_output=True,text=True).stdout)

@pytest.fixture(scope='module')
def completed():
    raw=browser_request()
    with pytest.MonkeyPatch.context()as patch:
        selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
        if selected:
            from portfolio_risk.core import product_strategy_native
            patch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')
        result,_=run(raw,timeout=60.)
    return raw,result

def test_browser_secondary_buyer_cash_stock_income_and_full_raw_replay(completed):
    raw,result=completed
    assert verify(raw,result)['purchased_journal_replayed']
    assert result['preparation']['raw_request']==raw
    graph=result['preparation']['native_preparation']['source']['native_graphs'][0]
    physical=next(s for s in graph['native_sources']if s['role']=='physical')
    inventory=physical['projection']['secondary_acquisition']
    assert inventory['principal']==100. and inventory['purchased_legal_interest']==pytest.approx(1.,abs=1e-12)
    assert inventory['buyer_cost']==99. and inventory['buyer_book_adjustment']==pytest.approx(-2.,abs=1e-12)
    assert inventory['seller_income_inherited']is False and inventory['accounting_approval']is False
    stock=next(g['next']for g in physical['fee_projection']['bound_path']['source']['groups']if g['event']['action']=='acquire')
    assert stock['fee_receivable']==6. and stock['acquired_fee_asset_valuation_adjustment']==-1.
    assert stock.get('acquired_fee_liability',0.)==0.
    opening=next(r for r in result['tables']['path']if r['scenario']=='baseline'and r['day']==0)
    assert opening['assets']==1000. and opening['liabilities']==0. and opening['equity']==1000.
    assert all(r['cash']==pytest.approx(1005.,abs=1e-9)for r in result['tables']['closing_statements'])
    assert not any(r['gl_account']=='pnl:commercial_nonintegral_fee_income'for r in result['tables']['journal'])

@pytest.mark.parametrize('target',['origin','inventory','cost','gross_claim','contra','cash','purchased_lot'])
def test_rehashed_secondary_monetary_and_lineage_tamper_rejects(completed,target):
    raw,original=completed;result=deepcopy(original)
    graph=result['preparation']['native_preparation']['source']['native_graphs'][0]
    source=next(s for s in graph['native_sources']if s['role']=='physical')
    inventory=source['projection']['secondary_acquisition']
    stock=next(g['next']for g in source['fee_projection']['bound_path']['source']['groups']if g['event']['action']=='acquire')
    if target=='origin':inventory['contract_origin_ordinal']+=1
    elif target=='inventory':inventory['purchased_legal_interest']+=1.
    elif target=='cost':inventory['buyer_cost']+=1.
    elif target=='gross_claim':stock['fee_receivable']+=1.
    elif target=='contra':stock['acquired_fee_asset_valuation_adjustment']-=1.
    elif target=='cash':next(r for r in result['tables']['journal']if r['gl_account']=='cash'and r['value']<0.)['value']-=1.
    else:next(l for r in result['tables']['commercial_daily']for l in r['unweighted_book']['lots']if l['earning_origin_index']==3)['recognized_remaining']+=1.
    with pytest.raises((ArithmeticError,ValueError,KeyError)):verify(raw,result)

@pytest.mark.parametrize('target',['partial','separate_accrued','split_clock','existing_default','extra_checkpoint','unreviewed_regime'])
def test_secondary_unsupported_owner_selection_refuses_before_journal(target):
    raw=request();owner=raw['commercial']['units'][0]['graph']['positions'][0]['histories'][0]['physical'][0]['source']
    transfer=owner['source']['transfer']
    if target=='partial':transfer['conveyance']='partial'
    elif target=='separate_accrued':transfer['regime']['income_policy']='exclude_purchased_accrued_from_eir'
    elif target=='split_clock':transfer['settlement_ordinal']+=1
    elif target=='existing_default':owner['credit']['paths'][0]['initial_live_state']='watch'
    elif target=='extra_checkpoint':transfer['seller_carry']=99.
    else:transfer['regime']['framework']='ifrs9_approved'
    with pytest.raises(ValueError):run(raw,timeout=60.)


def short_candidate():
    raw=request();at=raw['as_of_ordinal'];graph=raw['commercial']['units'][0]['graph'];position=graph['positions'][0]
    import math
    y=math.expm1(math.log(103./99.)*365./10.)
    for h in position['original_pricing']+[h for hs in position['histories']for role in ('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        owner=h['source'];source=owner['source'];transfer=source['transfer'];origin=transfer['contract_origin']
        origin['asof_ordinal']=at-5;origin['observed_publication_cutoff_unix_seconds']=(at-5-719163)*86400
        for contract in (origin,source['original'],source['current']):
            contract['reporting_end_ordinal']=at+5;contract['tail_end_ordinal']=at+10
            contract['periods'][0].update(start_ordinal=at-5,end_ordinal=at+10,payment_ordinal=at+10,coupon=dict(kind='fixed',annual_rate=.72))
        source['acquisition']['original_annual_effective_rate']=y;owner['accounting_rates'][0]['annual_effective_rates']=[y]*3
        owner['credit']['intervals'][0]['end_year']=10/365.
    position['oas_target']['base_oas']=math.log(103./99.)*365./10.
    fees=position['fees'];fees['discount_spread']=math.log(6./5.)*365./20.
    for h in fees['original_pricing']+[h for hs in fees['histories']for role in ('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
        policy=h['policy'];policy['complete_end_ordinal']=at+35
        origin=policy['transfer']['origin_service'];origin.update(asof_ordinal=at-5,reporting_end_ordinal=at+5,complete_end_ordinal=at+35)
        origin['events'][0]['ordinal']=at-3;origin['events'][1]['ordinal']=at+20
        for e in policy['events']:e['ordinal']=at+20
        for e in h.get('acquisition_releases',[]):e['ordinal']=at+20
    raw['reporting_end_ordinal']=at+5;raw['specification']['horizon_days']=30
    graph['reporting_ordinals']=[at,at+5];raw['commercial']['reporting_ordinals']=[at,at+5]
    return raw


def test_secondary_dual_candidate_holding_refuses_unchanged_combined_budget():
    from portfolio_risk.core.commercial_candidate_holding_native import run as continue_holding
    completed=short_candidate();raw=dict(schema='commercial-candidate-holding-journal-1',cut_ordinal=completed['as_of_ordinal']+5,completed_candidate=completed,revised_candidate=deepcopy(completed))
    # Both complete original buyers must coexist. A smaller contractual source
    # does not waive the combined financial work proof or increase32M.
    with pytest.raises(ValueError,match='holding joined32M/128MiB'):
        continue_holding(raw,timeout=60.)


def test_secondary_post_purchase_default_journal_and_acquired_claims_independently_replay():
    raw=request();position=raw['commercial']['units'][0]['graph']['positions'][0]
    for histories in position['histories']:
        for h in histories['physical']:h['source']['credit']['intervals'][0]['hazards_per_year'][0][3]=100.
    result,_=run(raw,timeout=60.);assert verify(raw,result)['purchased_journal_replayed']
    acquired=[lot for row in result['tables']['commercial_closing']for lot in row['unweighted_book']['lots']if lot['earning_origin_index']==3]
    assert acquired and all(lot['default_claim']for lot in acquired)


def test_secondary_loan_full_vs_later_cut_uses_complete_buyer_origin_and_exact_purchased_lot():
    from portfolio_risk.core.model_native import run as source_run
    from portfolio_risk.analytics.commercial_continuation_reference import verify as verify_continuation
    source=json.loads((ROOT/'examples/models/commercial-secondary-acquisition-1.json').read_text())
    raw=dict(schema='commercial-holding-continuation-1',cut_ordinal=source['source']['current']['asof_ordinal']+15,completed_source=source,revised_source=deepcopy(source))
    selected=os.environ.get('SECONDARY_NATIVE_DEBUG_DIR')
    with pytest.MonkeyPatch.context()as patch:
        if selected:
            from portfolio_risk.core import model_native
            patch.setattr(model_native,'binary_path',lambda:Path(selected)/'portfolio-lifecycle.exe')
        result,_=source_run(raw['schema'],raw,timeout=60.)
    assert verify_continuation(raw,result)['secondary_acquisition_adoption']
    assert result['second_acquisition_posted']is False
    opening=result['openings'][0];assert any(l['earning_origin_index']==3 for l in opening['book']['lots'])
    assert opening['book']['gross_carry']==pytest.approx(99.*(103./99.)**.25,abs=1e-9)
    assert opening['book']['legal_interest']==pytest.approx(1.5,abs=1e-9)
    assert opening['subsequent_cash']==pytest.approx(103.,abs=1e-9)
    altered=deepcopy(result);altered['openings'][0]['book']['lots'][0]['recognized_remaining']+=1.
    with pytest.raises((ArithmeticError,ValueError)):verify_continuation(raw,altered)
