"""Full native state machine and immutable partition publication gates."""
from copy import deepcopy
import json
import time
from pathlib import Path

import polars as pl
from polars.testing import assert_frame_equal
import pytest

from portfolio_risk.analytics.balance_stress import run_balance_stress
from portfolio_risk.analytics.balance_stream import (run_streamed_balance_stress,load_streamed_result,
    binary_path,PartitionWriter,replay_partitions,SCHEMAS)
from test_ledger_native import fixture


@pytest.fixture(scope='module',autouse=True)
def native_available():
    if not binary_path().is_file():
        pytest.skip('optional native state engine is not built')


@pytest.mark.parametrize('case',['mixed','collateral','afs','trading','zero','forward','credit'])
@pytest.mark.parametrize('backend',['python','rust'])
def test_all_financial_outputs_and_streamed_replay(tmp_path,case,backend):
    spec=fixture(case);original=deepcopy(spec)
    expected=run_balance_stress(spec)
    path=run_streamed_balance_stress(spec,tmp_path,backend=backend,partition_rows=37)
    actual=load_streamed_result(path)
    assert spec==original
    for table in SCHEMAS:
        reference=expected[table]
        if reference.is_empty():
            assert actual[table].is_empty(),table
            continue
        assert_frame_equal(actual[table].select(reference.columns),reference,
                           check_dtypes=False,rel_tol=1e-12,abs_tol=1e-8)
    assert actual['manifest']['validation']['journal_replayed']
    assert actual['manifest']['validation']['journal_rows']==expected['journal'].height
    assert not list(tmp_path.glob('.partial-*'))
    assert all(p['rows']<=37 for t in actual['manifest']['tables'].values() for p in t['parts'])


def test_partition_write_failure_does_not_publish(tmp_path,monkeypatch):
    original=PartitionWriter.flush
    calls=[]
    def fail(self,table):
        calls.append(table)
        original(self,table)
        if len(calls)==2:
            raise OSError('injected disk failure')
    monkeypatch.setattr(PartitionWriter,'flush',fail)
    with pytest.raises(OSError,match='injected'):
        run_streamed_balance_stress(fixture('afs'),tmp_path,partition_rows=10)
    assert not list(tmp_path.iterdir())


def test_native_financial_reports_have_no_python_builder(tmp_path, monkeypatch):
    import portfolio_risk.analytics.balance_stream as stream
    def forbidden(*args, **kwargs):
        raise AssertionError('Python financial statement construction')
    monkeypatch.setattr(stream, 'statements', forbidden)
    result = load_streamed_result(run_streamed_balance_stress(fixture('mixed'), tmp_path))
    assert result['closing_statements'].height
    assert result['consolidated'].height
    assert result['attribution'].height


def test_missing_native_report_prevents_publication(tmp_path, monkeypatch):
    original = PartitionWriter.add
    def omit(self, table, rows):
        if table != 'closing_statements':
            original(self, table, rows)
    monkeypatch.setattr(PartitionWriter, 'add', omit)
    with pytest.raises(ArithmeticError, match='missing native closing'):
        run_streamed_balance_stress(fixture('mixed'), tmp_path)
    assert not list(tmp_path.iterdir())


def test_corrupt_partition_prevents_publication(tmp_path,monkeypatch):
    original=PartitionWriter.finish
    def corrupt(self):
        original(self)
        path=self.root/self.parts['journal'][0]['path']
        path.write_bytes(b'broken')
    monkeypatch.setattr(PartitionWriter,'finish',corrupt)
    with pytest.raises(ValueError,match='checksum'):
        run_streamed_balance_stress(fixture('afs'),tmp_path)
    assert not list(tmp_path.iterdir())


def test_cancelled_run_and_missing_backend_leave_no_manifest(tmp_path,monkeypatch):
    with pytest.raises(InterruptedError):
        run_streamed_balance_stress(fixture('mixed'),tmp_path,cancelled=lambda:True)
    monkeypatch.setenv('PORTFOLIO_BALANCE_RUST_BIN',str(tmp_path/'missing.exe'))
    with pytest.raises(RuntimeError,match='No fallback'):
        run_streamed_balance_stress(fixture('mixed'),tmp_path)
    assert not list(tmp_path.iterdir())


def test_saved_artifact_tamper_and_path_escape_rejected(tmp_path):
    path=run_streamed_balance_stress(fixture('afs'),tmp_path)
    manifest=json.loads(path.read_text())
    part=manifest['tables']['journal']['parts'][0]
    original=part['path']
    part['path']='../outside.parquet'
    path.write_text(json.dumps(manifest))
    with pytest.raises(ValueError,match='invalid or corrupt'):
        load_streamed_result(path)
    part['path']=original
    path.write_text(json.dumps(manifest))
    (path.parent/original).write_bytes(b'bad')
    with pytest.raises(ValueError,match='invalid or corrupt'):
        load_streamed_result(path)


def test_transaction_split_and_reordering_are_checked():
    rows=run_balance_stress(fixture('afs'))['journal']
    whole,n=replay_partitions([rows])
    split,m=replay_partitions([rows.slice(i,1) for i in range(rows.height)])
    assert whole==split and n==m
    # Reordering full transactions must fail even though each remains balanced.
    ids=rows['transaction_id'].unique(maintain_order=True)
    first=rows.filter(pl.col('transaction_id')==ids[0])
    with pytest.raises(ValueError,match='sequence'):
        replay_partitions([rows.filter(pl.col('transaction_id')!=ids[0]),first])


@pytest.mark.parametrize('backend',['python','rust'])
def test_mid_run_cancellation_discards_attempt(tmp_path,backend):
    flag=[False]
    def progress(_):
        flag[0]=True
    with pytest.raises(InterruptedError):
        run_streamed_balance_stress(fixture('mixed'),tmp_path,backend=backend,progress=progress,cancelled=lambda:flag[0])
    assert not list(tmp_path.iterdir())


def test_native_deadline_discards_attempt(tmp_path):
    with pytest.raises(TimeoutError):
        run_streamed_balance_stress(fixture('mixed'),tmp_path,timeout=1e-6)
    assert not list(tmp_path.iterdir())


def test_deadline_kills_child_that_emits_no_output(tmp_path,monkeypatch):
    import subprocess
    import sys
    from portfolio_risk.analytics import balance_stream
    original=subprocess.Popen
    launched=[]
    def sleeping(_command,**kwargs):
        process=original([sys.executable,'-c','import time; time.sleep(60)'],**kwargs)
        launched.append(process)
        return process
    monkeypatch.setattr(balance_stream.subprocess,'Popen',sleeping)
    with pytest.raises(TimeoutError):
        run_streamed_balance_stress(fixture('afs'),tmp_path,timeout=.1)
    assert launched[0].poll() is not None
    assert not list(tmp_path.iterdir())


@pytest.mark.parametrize('seed',range(4))
def test_randomized_mixed_state_outputs(tmp_path,seed):
    import numpy as np
    rng=np.random.default_rng(seed)
    spec=fixture('mixed')
    for p in spec['positions']:
        p['rate']=float(rng.uniform(-.02,.08))
    spec['policies'][0]['funding_tenor_days']=int(rng.integers(2,12))
    spec['positions'][0]['recovery_days']=int(rng.integers(1,8))
    for s in spec['scenarios']:
        s['start_day']=int(rng.integers(1,5))
    reference=run_balance_stress(spec)
    actual=load_streamed_result(run_streamed_balance_stress(spec,tmp_path))
    for table in SCHEMAS:
        if reference[table].height:
            assert_frame_equal(actual[table].select(reference[table].columns),reference[table],
                               check_dtypes=False,rel_tol=1e-12,abs_tol=1e-8)


def test_large_tier_does_not_change_default_admission(tmp_path):
    from portfolio_risk.analytics.balance_stress import validate
    spec=fixture('zero')
    spec['positions']=[dict(id=str(i),account='a',kind='loan',balance=1.) for i in range(2001)]
    spec['accounts'][0]['equity']=2001.
    with pytest.raises(ValueError,match='at most 2000'):
        validate(spec)
    path=run_streamed_balance_stress(spec,tmp_path,large_book=True)
    assert json.loads(path.read_text())['validation']['journal_replayed']


@pytest.mark.parametrize('backend',['python','rust'])
def test_stream_deadline_includes_persisted_replay(tmp_path,monkeypatch,backend):
    from portfolio_risk.analytics import balance_stream as stream
    clock=stream.time.perf_counter
    elapsed=[0.]
    monkeypatch.setattr(stream.time,'perf_counter',lambda:clock()+elapsed[0])
    finalize=stream._finalize
    def delayed(*args,**kwargs):
        output=finalize(*args,**kwargs)
        elapsed[0]=120.
        return output
    monkeypatch.setattr(stream,'_finalize',delayed)
    with pytest.raises(TimeoutError,match='deadline'):
        stream.run_streamed_balance_stress(fixture('zero'),tmp_path,backend=backend,timeout=60.)
    assert not list(tmp_path.iterdir())


def test_native_liquidity_caps_and_all_daily_limit_branches(tmp_path):
    spec=fixture('afs')
    spec['accounts'][0].update(equity=50.,lcr_floor=2.,nsfr_floor=1.,htm_asset_limit=.2)
    spec['positions'][0].update(classification='htm',hqla_level='level2a',hqla_weight=1.,lcr_inflow_weight=.9)
    spec['positions'][1]['lcr_outflow_weight']=1.
    reference=run_balance_stress(spec)
    actual=load_streamed_result(run_streamed_balance_stress(spec,tmp_path))
    assert {'lcr','nsfr','htm'}<=set(actual['breaches']['metric'])
    assert not actual['manifest']['validation']['dynamic_validated']
    for table in ('path','breaches','summary','closing_statements'):
        assert_frame_equal(actual[table].select(reference[table].columns),reference[table],
                           check_dtypes=False,rel_tol=1e-12,abs_tol=1e-8)


@pytest.mark.parametrize('classification',['afs','trading'])
def test_native_discount_basis_with_partial_sales_and_redemption(tmp_path,classification):
    spec=fixture(classification)
    spec['accounts'][0]['equity']=35.
    spec['positions'][0].update(book_adjustment=-10.,opening_market_price=.95)
    for flow in spec['cashflows']:
        flow['book_amortization']*=-1.
    reference=run_balance_stress(spec)
    actual=load_streamed_result(run_streamed_balance_stress(spec,tmp_path))
    for table in ('journal','trial_balance','path','closing_statements'):
        assert_frame_equal(actual[table].select(reference[table].columns),reference[table],
                           check_dtypes=False,rel_tol=1e-12,abs_tol=1e-8)


@pytest.mark.parametrize('seed', range(20))
def test_broader_daily_state_parity_matrix(tmp_path, seed):
    import numpy as np
    rng = np.random.default_rng(8000 + seed)
    spec = fixture('mixed')
    spec['reverse_severities'] = [0., .5, 2.] if seed % 4 == 0 else []
    for position in spec['positions']:
        position['rate'] = float(rng.uniform(-.01, .12))
        position['payment_interval_days'] = int(rng.choice([1, 7, 30]))
        if position['kind'] == 'loan':
            position['annual_pd'] = float(rng.uniform(0., .4))
            position['recovery_days'] = int(rng.integers(1, 31))
        if position['kind'] == 'deposit':
            position['monthly_runoff'] = float(rng.uniform(0., .15))
    for scenario in spec['scenarios']:
        scenario['rate_shift'] = float(rng.uniform(-.03, .08))
        scenario['start_day'] = int(rng.choice([1, 5, 30]))
    reference = run_balance_stress(spec)
    native = load_streamed_result(run_streamed_balance_stress(spec, tmp_path, partition_rows=101))
    for table in SCHEMAS:
        if reference[table].is_empty():
            assert native[table].is_empty()
        else:
            assert_frame_equal(native[table].select(reference[table].columns), reference[table],
                               check_dtypes=False, rel_tol=1e-12, abs_tol=1e-8)
