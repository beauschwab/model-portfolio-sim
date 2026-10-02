from copy import deepcopy

import numba
import polars as pl
from polars.testing import assert_frame_equal
import pytest

from portfolio_risk.analytics.cohorts import build, read_tape, example, presets
from portfolio_risk.analytics.cohort_validation import audit, RISK, FLOWS
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.demo import demo_market, demo_deposit_history, model_balance_sheet


@pytest.fixture(scope='module')
def inputs():
    old = numba.get_num_threads(); numba.set_num_threads(2)
    bs = model_balance_sheet(scale=.001)
    sr, vp = demo_market()
    yield dict(asof=bs['asof'], swap_rates=sr, vol_pts=vp, config=RunConfig(8,8,3,compute_backend='rust'),
        mbs_hists=bs['mbs_hists'], dep_hist=demo_deposit_history())
    numba.set_num_threads(old)


def test_homogeneous_cohorts_match_every_loan_and_batch_boundaries(inputs):
    tape = read_tape(example().encode(), 'csv')
    frame = tape['frame'].filter(pl.col('product').is_in(['mortgage','deposit']))
    rows = []
    for product in ('mortgage','deposit'):
        first = frame.filter(pl.col('product')==product).row(0,named=True)
        rows.extend(first | {'loan_id':f'{product}-{i}'} for i in range(4))
    tape['frame'] = pl.DataFrame(rows)
    saved = build(tape,presets())
    tolerance = {p:{m:{'absolute':1e-6,'relative':1e-7} for m in RISK+FLOWS} for p in ('mortgage','deposit')}
    results = []
    for size in (1,256):
        tables = {}
        summary = audit(saved, products=['mortgage','deposit'], **inputs, batch_size=size,
            tolerances=tolerance, emit=lambda name,f:tables.setdefault(name,[]).append(f))
        assert summary['passed'] and summary['failed_cohorts']==0
        assert summary['checks'] == 2*(3*3+3*4)
        errors = pl.concat(tables['errors']).sort(['cohort_id','shock_bp','month','metric'])
        loan_risk = pl.concat(tables['loan_risk'])
        assert loan_risk['loan_id'].n_unique()==8
        for group in loan_risk.partition_by('loan_id'):
            assert group['base_oas_bp'].max() == group['base_oas_bp'].min()
        results.append(errors)
    assert_frame_equal(*results, rel_tol=1e-7, abs_tol=1e-6)


def test_heterogeneous_cohorts_report_errors_and_split_suggestions(inputs):
    tape=read_tape(example().encode(),'csv')
    saved=build(tape,presets());tables={}
    summary=audit(saved,products=['mortgage'],**inputs,shocks=[0.],
        tolerances={'mortgage':{m:{'absolute':0.,'relative':0.} for m in RISK+FLOWS}},
        emit=lambda name,f:tables.setdefault(name,[]).append(f))
    assert not summary['passed'] and summary['failed_cohorts']==1
    assert tables['suggestions'][0]['field'][0] in {'fico','age_months','remaining_term_months','oltv'}
    assert 'loan_cashflows' in tables


@pytest.mark.parametrize('options,match', [({'shocks':[100.]},'including zero'),
    ({'shocks':[0.,float('nan')]},'finite'), ({'batch_size':257},'batch_size'),
    ({'tolerances':{'mortgage':{'duration':{}}}},'unknown audit metric'),
    ({'tolerances':{'mortgage':{'nii':{'relative':-1.}}}},'nonnegative')])
def test_audit_rejects_invalid_requests(inputs,options,match):
    saved=build(read_tape(example().encode(),'csv'),presets())
    with pytest.raises(ValueError,match=match): audit(saved,products=['mortgage'],**inputs,**options)


def test_cancelled_audit_emits_nothing(inputs):
    saved=build(read_tape(example().encode(),'csv'),presets());parts=[]
    with pytest.raises(InterruptedError):
        audit(saved,products=['mortgage'],**inputs,cancelled=lambda:True,emit=lambda *a:parts.append(a))
    assert parts==[]
