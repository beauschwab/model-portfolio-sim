"""Actual dated principal20 before transfer; continuing coupon nominal100/live80."""
from copy import deepcopy
import json
import math
import os
import subprocess
from pathlib import Path

import pytest
from portfolio_risk.analytics.commercial_ledger_reference import verify_compiler
from portfolio_risk.analytics.commercial_source_reference import compare, project_owned
from portfolio_risk.core import model_native

ROOT=Path(__file__).resolve().parents[3]


def split_owner(raw):
    x=deepcopy(raw);source=x['source'];end=source['current']['tail_end_ordinal'];at=source['current']['asof_ordinal']
    future=dict(event_id='principal-final',payment_ordinal=end,amount=80.)
    source['transfer']['contract_origin']['principal_plan']=dict(kind='dated_supplied_period_opening_nominal-1',principal_events=[dict(event_id='principal-before-buyer',payment_ordinal=at-15,amount=20.),deepcopy(future)])
    for role in ('original','current'):
        source[role]['opening_principal']=80.;source[role]['principal_plan']=dict(kind='dated_supplied_period_opening_nominal-1',principal_events=[deepcopy(future)])
    source['transfer']['loan_clean_cost']=78.;source['acquisition']['cost']=79.
    y=math.expm1(math.log(83./79.)*365./(end-at));source['acquisition']['original_annual_effective_rate']=y
    x['accounting_rates'][0]['annual_effective_rates']=[y]*3
    return x


def owner():
    return split_owner(json.loads((ROOT/'examples/models/commercial-secondary-acquisition-1.json').read_text()))


@pytest.fixture(autouse=True)
def actual_debug(monkeypatch):
    directory=Path(os.environ.get('ENTITLEMENT_NATIVE_DEBUG_DIR',ROOT/'artifacts/nd/debug'))
    monkeypatch.setattr(model_native,'binary_path',lambda:ROOT/'packages/portfolio-risk-native/target/debug/portfolio-lifecycle.exe')
    from portfolio_risk.core import product_strategy_native
    monkeypatch.setattr(product_strategy_native,'binary_path',lambda:directory/'portfolio-strategy.exe')


def execute(raw):
    pair=dict(schema='commercial-holding-continuation-1',cut_ordinal=raw['source']['current']['asof_ordinal']+1,completed_source=raw,revised_source=deepcopy(raw))
    receipt,_=model_native.run(pair['schema'],pair,timeout=60.);actual=receipt['completed']
    verify_compiler(raw,actual)
    return actual


def test_real_owner_full_origin_and_independent_buyer_e100_p80():
    raw=owner();actual=execute(raw);transfer=actual['secondary_acquisition']
    assert transfer['principal']==80. and transfer['coupons'][0]['opening_entitlement_principal']==100.
    assert transfer['purchased_legal_interest']==pytest.approx(1.,abs=1e-12)
    events=transfer['full_contractual_source']['schedule']['events']
    assert [e['components']['principal'] for e in events]==[20.,0.,80.]
    assert [e['event_id'] for e in events]==['principal-before-buyer','coupon','principal-final']
    assert actual['accounting']['groups'][0]['next']['gross_carry']==pytest.approx(79.,abs=1e-12)
    expected,_=project_owned(raw['source'],'current',policy=raw['factor_policy'])
    assert sum(e['components']['cash_interest'] for e in expected['schedule']['events'])==pytest.approx(3.,abs=1e-12)


@pytest.mark.parametrize('target',['nominal','past_cash','buyer_cash','purchased_lot'])
def test_independent_replay_rejects_rehashed_origin_and_buyer_tamper(target):
    raw=owner();actual=execute(raw)
    if target=='nominal':actual['current_source']['source']['audit'][0]['opening_principal']=80.
    elif target=='past_cash':actual['secondary_acquisition']['full_contractual_source']['schedule']['events'][0]['components']['principal']=19.
    elif target=='buyer_cash':actual['current_source']['source']['schedule']['events'][-1]['components']['principal']=100.
    else:actual['secondary_acquisition']['coupons'][0]['opening_entitlement_principal']=80.
    with pytest.raises((ArithmeticError,ValueError)):verify_compiler(raw,actual)


def test_real_default_recovers_coupon_nominal_separately_from_live_principal():
    raw=owner();raw['credit']['intervals'][0]['hazards_per_year'][0][3]=100.;actual=execute(raw)
    assert actual['carry']['credit']['result']['paths'][0]['default']['principal']==80.
    paid=actual['carry']['native_coupon_recoveries']
    assert sum(p['interest']for p in paid if p['earning_origin_index']==3)==pytest.approx(.5,abs=1e-12)


def candidate(with_fees=False):
    raw=json.loads((ROOT/'examples/models/commercial-secondary-candidate-journal-1.json').read_text());unit=raw['commercial']['units'][0];position=unit['graph']['positions'][0]
    if not with_fees:position.pop('fees',None)
    else:position['fees']['total_purchase_cost']=84.
    for history in position['original_pricing']+[h for group in position['histories']for role in ('physical','pricing','pricing_down','pricing_up')for h in group[role]]:
        history['source']=split_owner(history['source'])
    position['original_signed_cash_quote']=79.;position['oas_target']['base_oas']=math.log(83./79.)*365./60.;unit['unit_face']=80.
    return raw


def test_real_graph_units_candidate_and_persisted_independent_replay(tmp_path,monkeypatch):
    from portfolio_risk.core.commercial_candidate_native import run
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    raw=json.loads(subprocess.run(['node','-e','const fs=require("fs");process.stdout.write(JSON.stringify(JSON.parse(fs.readFileSync(0,"utf8"))))'],input=json.dumps(candidate()),capture_output=True,text=True,check=True).stdout)
    assert type(raw['commercial']['units'][0]['graph']['positions'][0]['original_pricing'][0]['source']['source']['original']['principal_plan']['principal_events'][0]['amount'])is int
    actual,_=run(raw,timeout=60.);assert verify(raw,actual)['purchased_journal_replayed']
    # Persist the real journal/report schemas and reopen all partitions. Only
    # the storage frame provider is replaced; raw financial oracles stay intact.
    import polars as pl
    from portfolio_risk.analytics import commercial_candidate_reference as reference
    receipt=tmp_path/'raw-native-result.json';receipt.write_text(json.dumps(actual,allow_nan=False),encoding='utf-8')
    reopened=json.loads(receipt.read_text(encoding='utf-8'))
    frames=reference.Frames(actual['tables'],actual['producer_completion'],lambda:False)
    for name,frame in frames.tables.items():frame.write_parquet(tmp_path/(name+'.parquet'))
    class ReopenedFrames:
        completion=deepcopy(actual['producer_completion'])
        def frames(self,name):return iter([pl.read_parquet(tmp_path/(name+'.parquet'))])
    monkeypatch.setattr(reference,'Frames',lambda *_:ReopenedFrames())
    assert verify(raw,reopened)['persisted_partitions_replayed']
    path=tmp_path/'journal.parquet';journal=pl.read_parquet(path)
    changed=journal.with_row_index().with_columns(pl.when(pl.col('index')==0).then(pl.col('debit')+1.).otherwise(pl.col('debit')).alias('debit')).drop('index')
    changed.write_parquet(path)
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,reopened)
    source=actual['preparation']['native_preparation']['source'];assert raw['commercial']['units'][0]['unit_face']==80.
    assert source['units'][0]['quote_price_per_face_unit']==pytest.approx(79./80.,abs=1e-12)


def test_negative_coupon_actual_owner_preserves_acquired_payable():
    raw=owner();source=raw['source']
    for role in ('original','current'):source[role]['periods'][0]['coupon']['annual_rate']=-.12
    source['transfer']['contract_origin']['periods'][0]['coupon']['annual_rate']=-.12
    source['transfer']['loan_clean_cost']=80.;source['transfer']['coupon_allocations'][0]['purchased_accrued_cost']=-1.
    y=math.expm1(math.log(77./79.)*365./60.);source['acquisition']['original_annual_effective_rate']=y;raw['accounting_rates'][0]['annual_effective_rates']=[y]*3
    actual=execute(raw);opening=actual['accounting']['groups'][0]['next']
    assert opening['principal']==80. and opening['legal_interest']==pytest.approx(-1.,abs=1e-12)


def test_acquired_units_cannot_substitute_original_face_for_live_principal():
    raw=candidate()['commercial'];raw['units'][0]['unit_face']=100.
    with pytest.raises(ValueError,match='unit face'):
        model_native.run(raw['schema'],raw,timeout=60.)


def test_nonaccrual_keeps_legal_nominal_one_hundred_and_bought_recognition_separate():
    raw=owner();raw['accounting']['recognition'][0]['by_origin']=['legal_only_income_stopped']*3;raw['accounting_rates'][0]['annual_effective_rates']=[0.]*3
    actual=execute(raw)
    group=next(g for g in actual['accounting']['groups'] if g['phase']=='live_sojourn')
    assert group['next']['principal']==80.
    assert group['next']['legal_interest']>1. and group['next']['recognized_interest']==pytest.approx(1.,abs=1e-12)
    earned=[c for c in actual['carry']['earned_origin_contributions']if c['origin_index']==0 and c['delta_factor']!=0.]
    assert earned and all(c['actual_principal']==100. for c in earned)


def test_complete_fee_and_loan_candidate_keeps_distinct_acquired_nominals():
    from portfolio_risk.core.commercial_candidate_native import run
    from portfolio_risk.analytics.commercial_candidate_reference import verify
    raw=candidate(with_fees=True);actual,_=run(raw,timeout=60.);assert verify(raw,actual)['purchased_journal_replayed']
    source=actual['preparation']['native_preparation']['source'];assert source['units'][0]['quote_price_per_face_unit']==pytest.approx(84./80.,abs=1e-12)
    physical=next(s for s in source['native_graphs'][0]['native_sources']if s['role']=='physical')
    transfer=physical['projection']['secondary_acquisition'];assert transfer['principal']==80. and transfer['coupons'][0]['opening_entitlement_principal']==100.
    fee=next(g['next']for g in physical['fee_projection']['bound_path']['source']['groups']if g['event']['action']=='acquire')
    assert fee['fee_receivable']==6. and fee['acquired_fee_asset_valuation_adjustment']==-1.
