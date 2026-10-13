"""Independent dated-source/GL gates; production execution is always Rust."""
from copy import deepcopy
import datetime as dt

import polars as pl
import pytest

from portfolio_risk.analytics import balance_stress
from portfolio_risk.analytics.dated_ledger import (
    DATED_SCHEMAS, load_dated_ledger, run_dated_ledger, verify_dated_sources,
)
from portfolio_risk.analytics.balance_stream import PartitionWriter, _hash
from portfolio_risk.analytics.ledger_native import validate_spec


def request(*,pay=3,end=3,interest=3.,principal=100.,payment=False):
    asof=dt.date(2024,2,28).toordinal();sign=-1. if payment else 1.
    selector={'kind':'all'}
    event=dict(position_id='p',event_id='coupon',scenario=selector,payment_ordinal=asof+pay,
        day_offset=pay,accrual_period=dict(start_ordinal=asof,end_ordinal=asof+end),currency='USD',
        components=dict(principal=sign*principal,cash_interest=sign*interest,accrued_interest=sign*interest,book_amortization=0.,fees=0.),
        measure='contractual',provenance=dict(kind='contractual',source_id='original-loan',source_revision='terms-1'),adjustment=dict(kind='none'))
    return dict(schema='dated-ledger-1',monetary_unit='currency_units',
        recognition_policy=dict(policy_id='coupon',policy_revision='r1',recognition_model='supplied_calendar_segments-1',fees='immediate_earnings'),
        specification=dict(version='balance-stress-2',horizon_days=30,
            accounts=[dict(id='a',entity='bank',currency='USD',cash=100. if payment else 0.,equity=0. if payment else 100.,cet1_floor=0.,leverage_floor=0.)],
            positions=[dict(id='p',account='a',kind='deposit' if payment else 'loan',balance=100.,rate=.5,maturity_day=1)],
            scenarios=[dict(name='shock',rate_shift=.02,pd_multiplier=10.,deposit_flight=.5)]),
        dated=dict(schema='dated-store-1',as_of_ordinal=asof,reporting_horizon_days=30,
            runs=[dict(run_id='base',scenario_id='baseline',severity=1.,input_revision='base1'),dict(run_id='stress',scenario_id='shock',severity=1.,input_revision='stress1')],
            owners=[dict(position_id='p',account_id='a',currency='USD',direction='payment' if payment else 'receipt',ownership='contractual_dated_native_state',source_id='source',source_revision='r1')],
            sources=[dict(source_id='source',source_revision='r1',coverage=[dict(position_id='p',run=selector,status=dict(kind='complete_event_set'))],
                schedule=dict(schema='dated-cashflow-1',as_of_ordinal=asof,horizon_days=max(pay,end,30),scenarios=['base','stress'],events=[event]))],
            accrual_plans=[dict(position_id='p',event_id='coupon',run=selector,segments=[dict(start_ordinal=asof,end_ordinal=asof+end,accrued_interest=sign*interest,book_amortization=0.)])]))


def balance(result,gl):
    return result['trial_balance'].filter((pl.col('scenario')=='baseline')&(pl.col('gl_account')==gl))['balance'].sum()


@pytest.mark.parametrize('payment,pay,end,principal,wants',[
    (False,3,3,100.,{'cash':103.,'asset_principal':0.,'pnl:dated_recognition':-3.}),
    (False,0,60,100.,{'cash':103.,'asset_principal':0.,'dated_deferred_income':-1.5,'pnl:dated_recognition':-1.5}),
    (False,60,30,100.,{'cash':0.,'asset_principal':100.,'dated_interest_receivable':3.}),
    (True,0,60,0.,{'cash':97.,'funding_principal':-100.,'dated_prepaid_expense':1.5,'pnl:dated_recognition':1.5}),
])
def test_native_daily_source_and_persisted_journal_reconcile(tmp_path,monkeypatch,payment,pay,end,principal,wants):
    def forbidden(*args,**kwargs):raise AssertionError('Python financial callback')
    monkeypatch.setattr(balance_stress,'validate',forbidden);monkeypatch.setattr(balance_stress,'_simulate',forbidden)
    raw=request(pay=pay,end=end,principal=principal,payment=payment);original=deepcopy(raw)
    path=run_dated_ledger(raw,tmp_path,partition_rows=3);result=load_dated_ledger(path)
    assert raw==original
    for gl,value in wants.items():assert balance(result,gl)==pytest.approx(value,abs=1e-10)
    assert result['manifest']['backend']=='rust'
    assert result['manifest']['validation']['journal_replayed']
    assert result['manifest']['validation']['dated_source_reconciled']
    assert not result['manifest']['validation']['producer_empirically_validated']
    assert result['event_lineage'].filter(pl.col('phase')=='settlement').height==(2 if pay<=30 else 0)
    assert result['dated_rights'].height==2


def _writer_from_result(result,tmp_path):
    writer=PartitionWriter(tmp_path,schemas=DATED_SCHEMAS)
    for table in DATED_SCHEMAS:writer.add(table,result[table])
    writer.finish();return writer


@pytest.mark.parametrize('tamper',['amount','source','missing','duplicate','balanced_journal','right','statement'])
def test_independent_source_gate_rejects_balanced_or_identity_tampering(tmp_path,tamper):
    raw=request();result=load_dated_ledger(run_dated_ledger(raw,tmp_path/'initial'))
    if tamper in {'amount','source','missing','duplicate'}:
        rows=result['event_lineage'].to_dicts()
        if tamper=='amount':rows[0]['accrued_interest']+=1.
        elif tamper=='source':rows[0]['source_revision']='other'
        elif tamper=='missing':rows.pop(0)
        else:rows.insert(0,dict(rows[0]))
        result['event_lineage']=pl.DataFrame(rows,schema=DATED_SCHEMAS['event_lineage'])
    elif tamper=='balanced_journal':
        rows=result['journal'].to_dicts();tx=next(r['transaction_id'] for r in rows if r['event']=='dated_settlement')
        for row in rows:
            if row['transaction_id']==tx:
                if row['gl_account']=='cash':row['debit']+=1.
                elif row['gl_account']=='asset_principal':row['credit']+=1.
        result['journal']=pl.DataFrame(rows,schema=DATED_SCHEMAS['journal'])
    elif tamper=='right':result['dated_rights']=result['dated_rights'].with_columns((pl.col('earned_interest')+1.).alias('earned_interest'))
    else:result['closing_statements']=result['closing_statements'].with_columns((pl.col('assets')+1.).alias('assets'))
    folder=tmp_path/'tampered';folder.mkdir();writer=_writer_from_result(result,folder)
    with pytest.raises(ArithmeticError):verify_dated_sources(raw,validate_spec(raw),writer)


def test_cancelled_and_invalid_requests_publish_no_manifest(tmp_path):
    raw=request()
    with pytest.raises(InterruptedError):run_dated_ledger(raw,tmp_path,cancelled=lambda:True)
    bad=deepcopy(raw);bad['dated']['sources'][0]['schedule']['events'][0]['measure']='pricing'
    with pytest.raises(ValueError):run_dated_ledger(bad,tmp_path)
    assert not list(tmp_path.rglob('manifest.json'))


def test_persisted_replay_and_final_publication_cancellation_are_fenced(tmp_path,monkeypatch):
    from portfolio_risk.analytics import dated_ledger
    original=dated_ledger.verify_dated_sources;flag=False
    def cancel_after_verification(*args,**kwargs):
        nonlocal flag
        result=original(*args,**kwargs);flag=True;return result
    monkeypatch.setattr(dated_ledger,'verify_dated_sources',cancel_after_verification)
    with pytest.raises(InterruptedError):run_dated_ledger(request(),tmp_path,cancelled=lambda:flag)
    assert not list(tmp_path.rglob('manifest.json'))
    assert not list(tmp_path.glob('.partial-*'))


def test_immutable_raw_request_and_partition_corruption_rejected(tmp_path):
    path=run_dated_ledger(request(),tmp_path);result=load_dated_ledger(path)
    assert result['manifest']['input']['sha256']==_hash(path.parent/'dated-input.json')
    (path.parent/'dated-input.json').write_text('{}',encoding='utf-8')
    with pytest.raises(ValueError):load_dated_ledger(path)


def test_named_physical_schedules_own_scenario_amounts_without_legacy_overlay(tmp_path):
    raw=request();dated=raw['dated'];dated['owners'][0]['ownership']='fully_scenario_generated'
    first=dated['sources'][0]['schedule']['events'][0];first['scenario']={'kind':'named','scenario_id':'base'}
    first['measure']='physical';first['provenance']={'kind':'modeled','model_id':'explicit-product','model_version':'v1','input_revision':'base1'}
    second=deepcopy(first);second['scenario']={'kind':'named','scenario_id':'stress'}
    second['provenance']['input_revision']='stress1';second['components']['cash_interest']=6.;second['components']['accrued_interest']=6.
    dated['sources'][0]['schedule']['events'].append(second)
    plan=dated['accrual_plans'][0];plan['run']={'kind':'named','scenario_id':'base'}
    second_plan=deepcopy(plan);second_plan['run']={'kind':'named','scenario_id':'stress'};second_plan['segments'][0]['accrued_interest']=6.
    dated['accrual_plans'].append(second_plan)
    result=load_dated_ledger(run_dated_ledger(raw,tmp_path))
    assert balance(result,'cash')==103.
    assert result['trial_balance'].filter((pl.col('scenario')=='shock')&(pl.col('gl_account')=='cash'))['balance'].sum()==106.
    assert result['manifest']['validation']['dated_source_reconciled']


def test_negative_coupon_and_explicit_fee_and_basis_have_separate_postings(tmp_path):
    raw=request(interest=-3.);raw['specification']['positions'][0]['book_adjustment']=1.
    raw['specification']['accounts'][0]['equity']=101.
    event=raw['dated']['sources'][0]['schedule']['events'][0];event['components']['book_amortization']=-1.;event['components']['fees']=2.
    raw['dated']['accrual_plans'][0]['segments'][0]['book_amortization']=-1.
    result=load_dated_ledger(run_dated_ledger(raw,tmp_path))
    assert balance(result,'cash')==99.
    assert balance(result,'book_adjustment')==pytest.approx(0.,abs=1e-10)
    assert balance(result,'pnl:dated_recognition')==pytest.approx(4.)
    assert balance(result,'pnl:dated_settlement')==pytest.approx(-2.)


def test_day_zero_fee_tax_and_activity_are_closed_on_the_settlement_day(tmp_path):
    raw=request(pay=0,interest=0.)
    raw['specification']['accounts'][0]['tax_rate']=.5
    raw['dated']['sources'][0]['schedule']['events'][0]['components']['fees']=2.
    result=load_dated_ledger(run_dated_ledger(raw,tmp_path,partition_rows=3))
    assert balance(result,'cash')==101.
    assert balance(result,'pnl:dated_settlement')==-2.
    day_zero=result['ledger'].filter((pl.col('scenario')=='baseline')&(pl.col('day')==0))
    assert day_zero.height==2
    assert set(day_zero['event'])=={'dated_settlement','tax'}
    assert result['ledger'].filter((pl.col('scenario')=='baseline')&(pl.col('day')==1)).is_empty()
    assert result['manifest']['validation']['dated_source_reconciled']


def test_opening_accrual_anchor_is_verified_and_immutable(tmp_path):
    raw=request(pay=10,end=5,interest=10.)
    asof=raw['dated']['as_of_ordinal'];event=raw['dated']['sources'][0]['schedule']['events'][0]
    event['accrual_period']['start_ordinal']=asof-5;raw['dated']['accrual_plans'][0]['segments'][0]['start_ordinal']=asof-5
    raw['specification']['positions'][0]['opening_accrued']=5.
    raw['specification']['accounts'][0]['equity']=105.
    result=load_dated_ledger(run_dated_ledger(raw,tmp_path/'good'))
    assert balance(result,'cash')==110.
    assert balance(result,'pnl:dated_recognition')==pytest.approx(-5.)
    bad=deepcopy(raw);bad['specification']['positions'][0]['opening_accrued']=4.;bad['specification']['accounts'][0]['equity']=104.
    with pytest.raises(ValueError,match='opening dated'):run_dated_ledger(bad,tmp_path/'bad')


def test_fresh_production_defaults_native_with_python_financial_callbacks_forbidden(tmp_path):
    import json
    import subprocess
    import sys
    script='''
import json, sys
import numpy as np
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.analytics import balance_stress, balance_stream
from portfolio_risk.analytics.dated_ledger import run_dated_ledger, load_dated_ledger
assert RunConfig().compute_backend == 'rust'
def forbidden(*args, **kwargs): raise AssertionError('Python financial execution')
balance_stress.validate = forbidden
balance_stress._simulate = forbidden
balance_stream._python = forbidden
np.random.default_rng = forbidden
raw = json.load(sys.stdin)
path = run_dated_ledger(raw, sys.argv[1], partition_rows=5)
result = load_dated_ledger(path)
assert result['manifest']['backend'] == 'rust'
assert result['manifest']['validation']['dated_source_reconciled']
'''
    result=subprocess.run([sys.executable,'-c',script,str(tmp_path)],input=json.dumps(request()),
        capture_output=True,text=True,timeout=60)
    assert result.returncode==0,result.stdout+result.stderr
