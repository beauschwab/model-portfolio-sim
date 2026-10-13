"""UNAPPLIED default-release gates. Run only after installing the registry proposal.

Every native operation executes in a fresh Python process outside reference context.
No skip/missing-native fallback; independent mathematics runs after native completion.
"""
from pathlib import Path
import json,subprocess,sys,os
import pytest

ROOT=Path(__file__).resolve().parents[3]
SCRIPT=r'''
from pathlib import Path
from copy import deepcopy
from unittest.mock import patch
from contextlib import ExitStack
import json,sys
from portfolio_risk.core import graph_native
from portfolio_risk.core.native import _DEFAULT_LIBRARY_PATH
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.core import pricing,curve,vol
from portfolio_risk.products import corp
from portfolio_risk.analytics import market_indexed_reference
assert RunConfig().compute_backend=='rust'
assert graph_native.library_path().resolve()==_DEFAULT_LIBRARY_PATH.resolve()
root=Path(sys.argv[2]);mode=sys.argv[1]
source=json.loads((root/'examples/models/market-indexed-adapter-1.json').read_bytes())['request']
raw=dict(schema='market-indexed-graph-1',product='indexed_loan' if mode=='loan' else 'vanilla_swap',source=source)
calls=[]
def forbidden(*args,**kwargs):raise AssertionError('Python financial callback during native graph execution')
with ExitStack() as stack:
    for module,name in [(pricing,'pv_from_A'),(pricing,'solve_oas_from_A'),(curve,'bootstrap_curve'),
                        (vol,'calibrate_abcd'),(corp,'corp_pv'),(market_indexed_reference,'reconstruct')]:
        stack.enter_context(patch.object(module,name,forbidden))
    graph=graph_native.NativeMarketIndexedGraph(raw)
    try:
        initial=graph.initial
        if mode in ['loan','swap','eviction']:
            current=graph.price(raw)
            assert initial['cases']==current['cases']
            assert all(c['computed']==0 for c in current['counts'].values())
        elif mode in ['anchor','zero','current','shape']:
            changed=deepcopy(raw)
            if mode=='anchor':changed['source']['bridge']['original']['locked_oas']+=1e-14
            elif mode=='zero':changed['source']['bridge']['original']['locked_oas']=-0.0
            elif mode=='current':changed['source']['bridge']['current_market']['curves'][0]['currency']='EUR'
            else:changed['source']['bridge']['current_market']['curves'][0]['right_tail']={'reject':None}
            try:graph.price(changed)
            except ValueError as error:
                assert ('BASE_ANCHOR_CHANGED' in str(error)) if mode in ['anchor','zero'] else bool(str(error))
            else:raise AssertionError('invalid warm request produced success')
            current=graph.price(raw);assert initial['cases']==current['cases']
        elif mode in ['path_guard','stamp_guard']:
            original_path=graph_native.library_path
            original_stamp=graph._library._portfolio_stamp
            if mode=='path_guard':stack.enter_context(patch.object(graph_native,'library_path',lambda:root/'unloaded-other.dll'))
            else:graph._library._portfolio_stamp=(-1,-1,-1)
            graph._request=forbidden
            try:graph.price(raw)
            except RuntimeError as error:assert 'MODEL_VERSION_CHANGED' in str(error)
            else:raise AssertionError('stale actor used captured library')
            graph._library._portfolio_stamp=original_stamp
            current=None
        else:raise AssertionError(mode)
    finally:graph.close()
    try:graph.price(raw)
    except RuntimeError as error:assert 'closed' in str(error)
    else:raise AssertionError('closed graph executed')
print(json.dumps(dict(raw=raw,initial=initial,current=current,default_backend='rust',callbacks_forbidden=True)))
'''

@pytest.mark.parametrize('mode',['loan','swap','anchor','zero','current','shape','path_guard','stamp_guard'])
def test_fresh_default_release_actor_ownership_and_identity(mode):
    env=dict(os.environ);env.pop('PORTFOLIO_RISK_RUST_LIB',None)
    process=subprocess.run([sys.executable,'-c',SCRIPT,mode,str(ROOT)],capture_output=True,text=True,encoding='utf-8',timeout=60,env=env)
    assert process.returncode==0,process.stderr
    value=json.loads(process.stdout)
    assert value['default_backend']=='rust' and value['callbacks_forbidden'] is True
    from portfolio_risk.analytics.market_indexed_graph_reference import verify
    verify(value['raw'],value['initial'],value['raw'])
    if value['current'] is not None:verify(value['raw'],value['current'],value['raw'])

LEGACY=r'''
from pathlib import Path
import json,sys
from portfolio_risk.core.quant_native import term_call
root=Path(sys.argv[1])
account=dict(market=dict(config=dict(paths=1,months=2,forwards=4,factors=1,dt=1/12,tenor=.25,shift=.03,curve_bump=.0001,vol_bump=.01),tenors=[1.,2.],swap_rates=[.02,.02],vol_quotes=[1.,.2,.2],seed=[1]),asof=738886,horizon=1,mbs=None,terms=[],deposits=None,mm=None,hedges=None,withdrawal_parameters=[],anchors={},deposit_initial_rate=None,forecast=None,draws=None,capture_anchor=False,capture_cashflows=False)
price=dict(base=account,current=account,spread_shift=0.,overrides={},include_analytics=False,include_key_rates=False,kpis=None,auxiliary=None)
handle=term_call('graph-1',dict(op='create',max_bytes=1000000,max_entries=100))['handle']
rows=[]
for request in [dict(op='info',handle=handle),dict(op='price',handle=handle,request=price),dict(op='compare',handle=handle,request=price,patches={},calibration_mode='hold'),dict(op='clear',handle=handle),dict(op='drop',handle=handle)]:rows.append(term_call('graph-1',request))
expected=json.loads((root/'packages/portfolio-risk-native/tests/fixtures/market_indexed_legacy_outputs.json').read_bytes())
assert json.dumps(rows,sort_keys=True,separators=(',',':')).encode()==json.dumps(expected,sort_keys=True,separators=(',',':')).encode()
print('legacy serialized identities passed')
'''

def test_default_release_preserves_captured_real_legacy_serialized_controls():
    env=dict(os.environ);env.pop('PORTFOLIO_RISK_RUST_LIB',None)
    process=subprocess.run([sys.executable,'-c',LEGACY,str(ROOT)],capture_output=True,text=True,encoding='utf-8',timeout=60,env=env)
    assert process.returncode==0,process.stderr
    assert 'legacy serialized identities passed' in process.stdout
