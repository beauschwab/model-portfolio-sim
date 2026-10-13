"""Original EIR source arithmetic is independent of pricing and journal replay."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest
from portfolio_risk.analytics import model_ledger as engine, deposit_income_reference as reference

ROOT=Path(__file__).resolve().parents[3]
def example():return json.loads((ROOT/'examples/models/deposit-graph-income-ledger-1.json').read_text())

@pytest.mark.parametrize('case',['seasoned','negative_yield','quote','basis','terminal','blank_source'])
def test_independent_original_anchor_domain_rejects_before_source_projection(case):
    raw=example();anchor=raw['income_anchors'][0]
    if case=='seasoned':anchor['purchase_ordinal']-=1
    elif case=='negative_yield':anchor['original_annual_effective_rate']=-.01
    elif case=='quote':anchor['purchase_price_per100']=100.
    elif case=='basis':anchor['basis_revision_policy']='silently_catchup'
    elif case=='terminal':anchor['terminal_basis_policy']='cash_plug'
    else:anchor['original_yield_anchor']['id']=' '
    receipt=dict(raw_request=deepcopy(raw),schema='deposit-graph-income-ledger-source-1',
        normalization_policy='native_original_eir_deposit_opening_anchors-1',capture=dict(schema=reference.CAPTURE))
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,{},receipt)

def test_original_income_source_cancellation_is_cooperative():
    with pytest.raises(InterruptedError):reference.verify_preparation({}, {}, {}, lambda:True)

@pytest.fixture(scope='module')
def saved(tmp_path_factory):
    raw=example();path=engine.run_model_ledger(raw,tmp_path_factory.mktemp('original-deposit'),partition_rows=7)
    manifest=json.loads(path.read_text());source=json.loads((path.parent/manifest['modeled_source']['path']).read_text())
    return raw,source,manifest,path

def test_native_source_original_yield_and_publication_have_independent_gates(saved):
    raw,source,manifest,_=saved;result=reference.verify_preparation(raw,source['prepared_request'],source['source_receipt'])
    assert result['original_deposit_income_recomputed'] and result['original_yield_anchors_retained']
    assert result['physical_deposit_source_recomputed'] and manifest['validation']['journal_replayed']
    assert manifest['validation']['dated_source_reconciled'] and not result['pricing_graph_independently_verified']
    rows=source['source_receipt']['income_sources'][0]['result']['rows'];c=1031./1.1**(31./365.)
    assert rows[0]['effective_interest']==pytest.approx(c*(1.1**(1./365.)-1.),abs=1e-9)
    assert sum(r['effective_interest'] for r in rows)==pytest.approx(1031.-c,abs=1e-9)
    assert rows[-1]['closing_gross_carrying_value']==pytest.approx(0.,abs=1e-9)

def test_day30_ledger_is_eir_expense_and_unpaid_coupon_not_cash(saved):
    import polars as pl
    raw,_,manifest,path=saved
    frames=[pl.read_parquet(path.parent/p['path']) for p in manifest['tables']['trial_balance']['parts']]
    rows=pl.concat(frames).filter(pl.col('scenario')=='baseline').to_dicts()
    gl={r['gl_account']:r['balance'] for r in rows};c=raw['income_anchors'][0]['original_gross_carrying_value'];expense=c*(1.1**(30./365.)-1.)
    assert gl['cash']==pytest.approx(c,abs=1e-9);assert gl['funding_principal']==pytest.approx(-1000.)
    assert gl['dated_interest_payable']==pytest.approx(-30.);assert gl['pnl:dated_recognition']==pytest.approx(expense,abs=1e-9)
    assert gl['book_adjustment']==pytest.approx(-(c+expense-1030.),abs=1e-9)

@pytest.mark.parametrize('case',['yield','quote','earned','eir','origin','extra','book','terminal','run'])
def test_persisted_income_source_tamper_is_not_hidden_by_cash_slice_verification(saved,case):
    raw,source,_,_=saved;source=deepcopy(source);receipt=source['source_receipt']
    if case=='yield':receipt['income_sources'][0]['anchor']['original_annual_effective_rate']+=.01
    elif case=='quote':receipt['raw_request']['income_anchors'][0]['purchase_quote']['revision']='forged'
    elif case=='earned':receipt['income_sources'][0]['request']['events'][0]['contractual_interest_earned']+=1.
    elif case=='eir':receipt['income_sources'][0]['result']['rows'][0]['effective_interest']+=1.
    elif case=='origin':next(p for p in receipt['capture']['accrual_plans'] if p['event_id'].startswith('deposit-eir:'))['segments'][0]['origin_id']='forged'
    elif case=='extra':receipt['capture']['sources'][0]['schedule']['events'].append(deepcopy(receipt['capture']['sources'][0]['schedule']['events'][-1]))
    elif case=='book':source['prepared_request']['specification']['positions'][0]['book_adjustment']+=1.
    elif case=='terminal':receipt['income_sources'][0]['result']['remaining_gross_carrying_value']=1.
    else:receipt['income_sources'][1]['run_id']='forged'
    with pytest.raises(ArithmeticError):reference.verify_preparation(raw,source['prepared_request'],receipt)

def test_changed_expected_cash_rejects_entire_publication(tmp_path):
    raw=example();raw['graph']['positions'][0]['histories'][1]['physical'][0]['request']['segments'][0]['repricing']['target']['intercept']=.2
    with pytest.raises(RuntimeError):engine.run_model_ledger(raw,tmp_path)
    assert not list(tmp_path.glob('run-*'))

def test_full_report_retains_original_funding_and_closes_without_cash_plug(tmp_path):
    import polars as pl
    raw=example();raw['graph']['reporting_ordinals'].append(raw['graph']['as_of_ordinal']+31);raw['specification']['horizon_days']=31
    path=engine.run_model_ledger(raw,tmp_path,partition_rows=7);manifest=json.loads(path.read_text())
    rows=pl.concat([pl.read_parquet(path.parent/p['path']) for p in manifest['tables']['trial_balance']['parts']]).filter(pl.col('scenario')=='baseline').to_dicts()
    gl={r['gl_account']:r['balance'] for r in rows};c=raw['income_anchors'][0]['original_gross_carrying_value']
    for name in ['funding_principal','dated_interest_payable','book_adjustment']:assert gl.get(name,0.)==pytest.approx(0.,abs=1e-9)
    assert gl['cash']==pytest.approx(c-1031.,abs=1e-9);assert gl['pnl:dated_recognition']==pytest.approx(1031.-c,abs=1e-9)

def test_exact_zero_income_origins_survive_independent_source_and_journal_gate(tmp_path):
    raw=example();anchor=raw['income_anchors'][0]
    anchor.update(original_annual_effective_rate=0.,original_gross_carrying_value=1000.,purchase_price_per100=100.)
    raw['specification']['accounts'][0]['cash']=1000.;raw['specification']['positions'][0]['book_adjustment']=0.
    for history in raw['graph']['positions'][0]['histories']:
        for role in ['physical','pricing','pricing_down','pricing_up']:
            segment=history[role][0]['request']['segments'][0]
            segment['opening_paid_rate']=0.;segment['repricing']['target']['intercept']=0.
    path=engine.run_model_ledger(raw,tmp_path,partition_rows=7);manifest=json.loads(path.read_text())
    source=json.loads((path.parent/manifest['modeled_source']['path']).read_text())
    assert manifest['validation']['original_deposit_income_recomputed'] and manifest['validation']['journal_replayed']
    plans=source['source_receipt']['capture']['accrual_plans']
    assert len(plans)==31*3 and all(p['segments'][0]['book_amortization']==0. for p in plans)
