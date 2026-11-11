"""Selected counted complete-origin source work, actual finite journals."""
from copy import deepcopy
import json,os
import subprocess
from pathlib import Path
import pytest
from portfolio_risk.core.commercial_candidate_native import run
from portfolio_risk.analytics.commercial_candidate_reference import verify
ROOT=Path(__file__).resolve().parents[3]
POLICY='counted_finite_transfer_contract_factor_work-1'
@pytest.fixture(autouse=True)
def artifacts(monkeypatch):
    selected=os.environ.get('FINITE_NATIVE_DEBUG_DIR')
    if selected:
        from portfolio_risk.core import product_strategy_native
        monkeypatch.setattr(product_strategy_native,'binary_path',lambda:Path(selected)/'portfolio-strategy.exe')
def owners(raw):
    for unit in raw['commercial']['units']:
        for p in unit['graph']['positions']:
            for h in p['original_pricing']+p.get('original_physical',[])+[h for hs in p['histories']for role in ('physical','pricing','pricing_down','pricing_up')for h in hs[role]]:
                yield h['source']
def request(selected=True):
    raw=json.loads((ROOT/'packages/portfolio-risk/tests/fixtures/commercial-secondary-prepaid-exercise-candidate-refusal.json').read_text())
    if selected:
        for owner in owners(raw):owner['source']['transfer']['source_work_policy']=POLICY
    return raw

def test_exact_original_omission_keeps_all_floor_bytes_and_real_refusal():
    with pytest.raises(ValueError,match='work=33936166,bytes=34146784'):
        run(request(False),timeout=60.)

def test_selected_real_joint_candidate_and_independent_complete_source_work():
    raw=request();actual,_=run(raw,timeout=60.)
    assert verify(raw,actual)['purchased_journal_replayed']
    a=actual['preparation'];assert a['admitted_work_upper']<=32_000_000 and a['retained_bytes_upper']<=128<<20

def test_selected_dual_holding_keeps_both_complete_original_graphs():
    from portfolio_risk.core.commercial_candidate_holding_native import run as holding
    from portfolio_risk.analytics.commercial_candidate_holding_reference import verify as verify_holding
    from test_commercial_secondary_acquisition import short_candidate
    original=short_candidate()
    for owner in owners(original):owner['source']['transfer']['source_work_policy']=POLICY
    raw=dict(schema='commercial-candidate-holding-journal-1',cut_ordinal=original['as_of_ordinal']+5,completed_candidate=original,revised_candidate=deepcopy(original))
    actual,_=holding(raw,timeout=120.)
    assert verify_holding(raw,actual)['verified']
    assert actual['admitted_work_upper']==25_398_000
    assert actual['retained_bytes_upper']==56_709_584
    assert actual['completed_candidate']['preparation']['native_preparation']['source']['native_graphs'][0]['original_oas']==actual['revised_candidate']['preparation']['native_preparation']['source']['native_graphs'][0]['original_oas']

def test_larger_joint_dual_holding_retains_both_actual_preparations_and_refuses():
    from portfolio_risk.core.commercial_candidate_holding_native import run as holding
    original=request();raw=dict(schema='commercial-candidate-holding-journal-1',cut_ordinal=original['as_of_ordinal']+1,completed_candidate=original,revised_candidate=deepcopy(original))
    with pytest.raises(ValueError,match='work=49586912,bytes=54466096'):holding(raw,timeout=60.)

def test_selected_complete_source_keeps_deadline_and_cancellation_before_publication():
    with pytest.raises((TimeoutError,ValueError)):run(request(),timeout=.001)
    with pytest.raises(InterruptedError):run(request(),cancelled=lambda:True,timeout=60.)

@pytest.mark.parametrize('target',['source','clock','principal','private_authority','policy'])
def test_selected_tier_preserves_original_source_clock_and_private_authority_guards(target):
    raw=request();owner=raw['commercial']['units'][0]['graph']['positions'][0]['histories'][0]['physical'][0]['source']
    if target=='source':owner['source']['transfer']['contract_origin']['contract']['revision']='changed'
    elif target=='clock':owner['source']['transfer']['economic_ordinal']+=1
    elif target=='principal':owner['source']['current']['opening_principal']=80.
    elif target=='private_authority':owner['counted_work']={'work_upper':0}
    else:owner['source']['transfer']['source_work_policy']={'work_upper':0}
    with pytest.raises(ValueError):run(raw,timeout=60.)

@pytest.mark.parametrize('target',['summary','work','query','origin'])
def test_independent_gate_rejects_selected_receipt_or_full_origin_tampering(target):
    raw=request();actual,_=run(raw,timeout=60.)
    source=next(s for s in actual['preparation']['native_preparation']['source']['native_graphs'][0]['native_sources']if s['role']=='physical')['projection']
    if target=='summary':source['current_source']['admission']['counted_work']['upper']['calendar']+=1
    elif target=='work':source['current_source']['admission']['work_upper']-=1
    elif target=='query':source['current_source']['admission']['query_bound']-=1
    else:source['secondary_acquisition']['full_contractual_source']['schedule']['events'][0]['components']['cash_interest']+=1.
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,actual)

@pytest.mark.parametrize('kind',['joint_candidate','dual_holding'])
def test_actual_js_selected_complete_journal_persisted_ordered_replay(tmp_path,monkeypatch,kind):
    raw=json.loads(subprocess.run(['node','-e','let s="";process.stdin.on("data",b=>s+=b);process.stdin.on("end",()=>process.stdout.write(JSON.stringify(JSON.parse(s))))'],input=json.dumps(request()),capture_output=True,text=True,check=True).stdout)
    replay=verify
    if kind=='dual_holding':
        from test_commercial_secondary_acquisition import short_candidate
        from portfolio_risk.core.commercial_candidate_holding_native import run as holding
        from portfolio_risk.analytics.commercial_candidate_holding_reference import verify as replay
        original=short_candidate()
        for owner in owners(original):owner['source']['transfer']['source_work_policy']=POLICY
        raw=dict(schema='commercial-candidate-holding-journal-1',cut_ordinal=original['as_of_ordinal']+5,completed_candidate=original,revised_candidate=deepcopy(original))
        actual,_=holding(raw,timeout=120.)
    else:
        actual,_=run(raw,timeout=60.)
        assert actual['preparation']['admitted_work_upper']==22_470_544
        assert actual['preparation']['retained_bytes_upper']==14_274_792
    assert replay(raw,actual)['verified']
    import polars as pl
    monkeypatch.syspath_prepend(str(ROOT/'apps/api'))
    from app.model_tables import format_sources,restore_sources
    formatted,layouts=format_sources(actual);paths={}
    def persist(value,path=''):
        if isinstance(value,pl.DataFrame):
            destination=tmp_path/(str(len(paths))+'.parquet');paths[path]=destination;value.write_parquet(destination)
            return pl.read_parquet(destination)
        if isinstance(value,dict):return {key:persist(value,path+'/'+key)for key,value in value.items()}
        return value
    reopened=persist(formatted)
    assert replay(raw,restore_sources(reopened,layouts))['persisted_partitions_replayed']
    journal_path='/tables/journal'if kind=='joint_candidate'else'/window_tables/journal'
    path=paths[journal_path];journal=pl.read_parquet(path)
    journal.with_row_index().with_columns(pl.when(pl.col('index')==0).then(pl.col('value')+1.).otherwise(pl.col('value')).alias('value')).drop('index').write_parquet(path)
    target=reopened
    for key in journal_path.strip('/').split('/')[:-1]:target=target[key]
    target['journal']=pl.read_parquet(path)
    with pytest.raises((ArithmeticError,ValueError)):replay(raw,restore_sources(reopened,layouts))
