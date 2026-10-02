"""Native graph-to-solver-to-ledger parity and fail-closed artifact publication."""
from copy import deepcopy
from dataclasses import replace
import numpy as np
import polars as pl
from polars.testing import assert_frame_equal
import pytest

from portfolio_risk import demo
from portfolio_risk.core.runtime import RunConfig,run_context
from portfolio_risk.analytics.accounting import run_balance_sheet_nii
from portfolio_risk.analytics.balance_workflow import from_accounting,add_candidate
from portfolio_risk.analytics.balance_stress import run_balance_stress
from portfolio_risk.analytics.balance_stream import load_streamed_result,SCHEMAS
from portfolio_risk.analytics.owned_workflow import run_owned_workflow,binary_path
from portfolio_risk.strategy.decision import DecisionSession
from test_balance_stress import small
from test_balance_ledger import meta
from test_decision import LIMITS

pytestmark=pytest.mark.skipif(not binary_path().is_file(),reason='build native workflow')

@pytest.fixture(scope='module')
def inputs():
    import numba
    old=numba.get_num_threads();numba.set_num_threads(2)
    bs=demo.model_balance_sheet(scale=.001,basis='amortized_cost',include_markets_bs=True)
    for k in ('mbs','loans','debt','deposits','cds'):bs[k]=bs[k].head(2)
    sr,vp=demo.demo_market();dep=demo.demo_deposit_history();config=RunConfig(5,5,3, compute_backend='python')
    with run_context(config):
        accounting=run_balance_sheet_nii(bs,sr,vp,dep,horizon=3,capture_cashflows=True)
    openings=accounting['instrument_openings'].to_dicts();scale=1e-6
    specification=small();specification.update(horizon_days=90,positions=[])
    specification['accounts'][0].update(cash=100.,equity=100.+sum((r['balance']+r['book_adjustment'])*(1 if r['side']=='asset' else -1) for r in openings)*scale)
    mapping={f"{r['book']}:{r['id']}":meta('loan' if r['side']=='asset' else 'funding') for r in openings}
    from portfolio_risk.strategy.unitlib import TEMPLATES
    templates={name:meta('loan' if t.get('side',1)>0 else 'funding') for name,t in TEMPLATES.items()}
    args=dict(books={k:bs[k] for k in ('mbs','loans','debt','deposits','cds')},asof=bs['asof'],
        swap_rates=sr,vol_pts=vp,config=replace(config,compute_backend='rust'),mbs_hists=bs['mbs_hists'],
        dep_hist=dep,constraints=LIMITS,extras={'mm':bs['mm'],'equity':bs['equity']})
    ledger=dict(specification=specification,position_mapping=mapping,template_mapping=templates,
                amount_scale=scale,include_candidate=True)
    yield args,ledger,accounting
    numba.set_num_threads(old)


@pytest.mark.parametrize('include_candidate,edited',[(False,False),(True,False),(True,True)])
def test_full_native_workflow_matches_independent_daily_replay(tmp_path,inputs,include_candidate,edited):
    args,ledger,accounting=inputs;ledger=deepcopy(ledger);ledger['include_candidate']=include_candidate
    steps=[{}]
    if edited:
        bs=args['books'] | args['extras'] | {'mbs_hists':args['mbs_hists'],'asof':args['asof']}
        loan=bs['loans'].row(0,named=True); coupon=loan['coupon_or_spread']+.001
        steps.append({'edits':{f"loans:{loan['id']}":{'coupon_or_spread':coupon}}})
        with run_context(replace(args['config'],compute_backend='python')):
            anchor=run_balance_sheet_nii(bs,args['swap_rates'],args['vol_pts'],args['dep_hist'],horizon=3,seed=7,capture_anchor=True)['accounting_anchor']
            changed=bs | {'loans':bs['loans'].with_columns(pl.when(pl.col('id')==loan['id']).then(coupon).otherwise(pl.col('coupon_or_spread')).alias('coupon_or_spread'))}
            accounting=run_balance_sheet_nii(changed,args['swap_rates'],args['vol_pts'],args['dep_hist'],horizon=3,seed=7,accounting_anchor=anchor,capture_cashflows=True)
    expected_spec=from_accounting(ledger['specification'],accounting,ledger['position_mapping'],ledger['amount_scale'])
    s=DecisionSession(**(args | {'config':replace(args['config'],compute_backend='python')}))
    try:
        solved=s.update(version=0)
        if edited:solved=s.update(version=1,**steps[-1])
        if include_candidate:
            expected_spec=add_candidate(expected_spec,solved['allocation'],s.libraries[0],ledger['template_mapping'],ledger['amount_scale'])
        expected=run_balance_stress(expected_spec)
        actual=load_streamed_result(run_owned_workflow(**args,ledger=ledger,directory=tmp_path,partition_rows=97,steps=steps))
        np.testing.assert_allclose(actual['manifest']['workflow']['results'][-1]['worst_case_nii_$'],solved['worst_case_nii_$'],rtol=1e-7,atol=1e-5)
        for name in SCHEMAS:
            a,b=actual[name],expected[name]
            assert a.height==b.height,name
            assert_frame_equal(a.select(b.columns),b,check_dtypes=False,rel_tol=1e-7,abs_tol=1e-6)
        assert actual['manifest']['validation']['journal_replayed']
        assert actual['manifest']['workflow']['cashflow_rows']==len(expected_spec['cashflows'])
        assert not list(tmp_path.glob('.partial-*'))
    finally:s.close()


def test_native_workflow_has_no_python_financial_callbacks(tmp_path,inputs,monkeypatch):
    args,ledger,_=inputs
    saved=deepcopy(ledger)
    from portfolio_risk.analytics import accounting,balance_workflow,balance_stress,incremental
    from portfolio_risk.strategy import decision,unitlib
    def forbidden(*args,**kwargs):raise AssertionError('Python financial callback')
    for module,name in [(accounting,'run_balance_sheet_nii'),(balance_workflow,'from_accounting'),
        (balance_workflow,'add_candidate'),(balance_stress,'validate'),(balance_stress,'_simulate'),
        (incremental,'price_books'),(decision,'_apply_delta'),(unitlib,'build_unit_library')]:
        monkeypatch.setattr(module,name,forbidden)
    result=load_streamed_result(run_owned_workflow(**args,ledger=ledger,directory=tmp_path))
    assert result['manifest']['execution']=='owned-ledger-1'
    assert result['closing_statements'].height
    assert ledger==saved
    import json
    manifest=result['manifest']
    request=json.loads(next(tmp_path.glob('run-*/workflow-input.json')).read_text(encoding='utf-8'))
    assert request['input']['graph'] and request['ledger']==saved
    assert manifest['input']['sha256']


def test_saved_book_public_route_owns_orchestration(inputs,monkeypatch):
    from portfolio_risk.analytics import accounting,balance_workflow
    args,ledger,_=inputs
    bs=args['books'] | args['extras'] | {'mbs_hists':args['mbs_hists'],'asof':args['asof']}
    request={k:v for k,v in ledger.items() if k!='include_candidate'}
    request['allocation']=[dict(template='cml_fixed_5y',purchase_m=0,notional=1e6)]
    with run_context(replace(args['config'],compute_backend='python')):
        expected=balance_workflow.run_saved_book_stress(bs,args['swap_rates'],args['vol_pts'],args['dep_hist'],request,asof=args['asof'])
    def forbidden(*args,**kwargs):raise AssertionError('Python saved-book coordinator')
    for module,name in [(accounting,'run_balance_sheet_nii'),(balance_workflow,'from_accounting'),(balance_workflow,'add_candidate'),(balance_workflow,'check_mapping')]:
        monkeypatch.setattr(module,name,forbidden)
    with run_context(args['config']):
        actual=balance_workflow.run_saved_book_stress(bs,args['swap_rates'],args['vol_pts'],args['dep_hist'],request,asof=args['asof'])
    assert actual['execution']['orchestration']=='rust'
    assert actual['specification']['positions'] and actual['specification']['cashflows']
    for name in ('summary','path','closing_statements','consolidated','trial_balance','exposures'):
        a,b=actual[name],expected[name]
        keys=[k for k,dtype in b.schema.items() if dtype==pl.String or k=='day']
        assert_frame_equal(a.select(b.columns).sort(keys),b.sort(keys),check_dtypes=False,rel_tol=1e-7,abs_tol=1e-6)


@pytest.mark.parametrize('failure',['mapping','timeout','cancel','persist'])
def test_workflow_failures_publish_nothing(tmp_path,inputs,monkeypatch,failure):
    args,ledger,_=inputs;ledger=deepcopy(ledger);kwargs={}
    if failure=='mapping':ledger['position_mapping'].pop(next(iter(ledger['position_mapping'])))
    if failure=='timeout':kwargs['timeout']=.001
    if failure=='cancel':kwargs['cancelled']=lambda:True
    if failure=='persist':
        from portfolio_risk.analytics.balance_stream import PartitionWriter
        def fail(*args,**kwargs):raise OSError('injected persistence failure')
        monkeypatch.setattr(PartitionWriter,'flush',fail)
    with pytest.raises((ValueError,RuntimeError,TimeoutError,InterruptedError,OSError)):
        run_owned_workflow(**args,ledger=ledger,directory=tmp_path,**kwargs)
    assert not list(tmp_path.iterdir())


@pytest.mark.parametrize('stage', ['replay', 'manifest'])
def test_deadline_covers_finalization_and_atomic_publication(tmp_path, inputs, monkeypatch, stage):
    from portfolio_risk.analytics import owned_workflow as workflow
    args, ledger, _ = inputs
    clock = workflow.time.perf_counter
    elapsed = [0.]
    monkeypatch.setattr(workflow.time, 'perf_counter', lambda: clock() + elapsed[0])
    finalize = workflow.stream._finalize
    def delayed_finalize(*a, **kw):
        result = finalize(*a, **kw)
        if stage == 'replay': elapsed[0] = 120.
        else:
            fsync = workflow.os.fsync
            def delayed_sync(fd):
                fsync(fd)
                elapsed[0] = 120.
            monkeypatch.setattr(workflow.os, 'fsync', delayed_sync)
        return result
    monkeypatch.setattr(workflow.stream, '_finalize', delayed_finalize)
    with pytest.raises(TimeoutError, match='deadline'):
        run_owned_workflow(**args, ledger=deepcopy(ledger), directory=tmp_path, timeout=60.)
    assert not list(tmp_path.iterdir())
