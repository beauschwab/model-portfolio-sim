"""Unapplied default release gates; no artifact imports or binary override.

These are authored acceptance requirements, not executed acceptance evidence.
The positive public price_books case currently exposes the documented 360/128
grid admission incompatibility and must pass before public adoption.
"""
from pathlib import Path
import json
import os
import subprocess
import sys
import pytest

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = r'''
from pathlib import Path
from copy import deepcopy
from contextlib import ExitStack
from unittest.mock import patch
import datetime, json, math, sys
import numpy as np
import polars as pl
from portfolio_risk.core import graph_native,pricing,curve,vol
from portfolio_risk.core.native import _DEFAULT_LIBRARY_PATH
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.core.quant_native import term_call
from portfolio_risk.core.dependency import DependencyCache
from portfolio_risk.products import corp
from portfolio_risk.analytics import incremental,market_indexed_reference
root=Path(sys.argv[2]);mode=sys.argv[1]
assert RunConfig().compute_backend=='rust'
assert graph_native.library_path().resolve()==_DEFAULT_LIBRARY_PATH.resolve()
raw=json.loads((root/'examples/models/market-indexed-books-1.json').read_bytes())
def forbidden(*a,**k):raise AssertionError('Python financial callback during native book graph')
def serialize(result):
    return result|dict(positions={k:v.to_dicts() if hasattr(v,'to_dicts') else v for k,v in result['positions'].items()})
def public_args():
    rows=[]
    for id,row in zip(raw['legacy']['base']['terms'][0]['ids'],raw['legacy']['base']['terms'][0]['deck']['contracts']):
        rows.append(dict(id=id,face=row['notional'],maturity=datetime.date.fromordinal(row['maturity']),
                         coupon_or_spread=row['coupon'],freq_months=row['freq_months'],daycount=row['daycount'],
                         is_float=row['is_float'],price=row['price']))
    from portfolio_risk.core import config as cfg
    return dict(books={'loans':pl.DataFrame(rows)},asof=datetime.date.fromordinal(raw['identity']['as_of_ordinal']),
                swap_rates=np.full(len(cfg.SWAP_TENORS),.02),vol_pts=np.array([[.25,.25,.2]]),
                config=RunConfig(1,1,1),seed=1,include_analytics=False,include_key_rates=False)
if mode=='signed_zero':
    for binding in raw['bindings']:
        bridge=binding['graph']['source']['bridge'];bridge['original']['locked_oas']=0.0
        collateral,index=bridge['original']['market']['curves']
        dfs=collateral['discount_factors'];ids=index['discount_factors'];margin=bridge['contract']['margin']
        quote=100*((ids[0]/ids[1]-1+margin*.5)*dfs[1]+(1+ids[1]/ids[2]-1+margin*.5)*dfs[2])
        for field in ['original_row','current_row']:binding[field]['price']=quote
        for which in ['base','current']:
            j=raw['legacy'][which]['terms'][0]['ids'].index(binding['position_id'])
            raw['legacy'][which]['terms'][0]['deck']['contracts'][j]['price']=quote
current=deepcopy(raw);original=deepcopy(raw);initial=None;out=None
with ExitStack() as stack:
    for module,name in [(pricing,'pv_from_A'),(pricing,'solve_oas_from_A'),(curve,'bootstrap_curve'),
                        (vol,'calibrate_abcd'),(corp,'corp_pv'),(market_indexed_reference,'reconstruct')]:
        stack.enter_context(patch.object(module,name,forbidden))
    for name in ['Evaluation','fingerprint','paths','solve_oas_from_A','bootstrap_curve','calibrate_abcd','factor_loadings','build_paths','build_rate_paths','run_engine']:
        if hasattr(incremental,name):stack.enter_context(patch.object(incremental,name,forbidden))
    if mode in ['public','custom_backend','none']:
        args=public_args();cache=DependencyCache(max_bytes=8388608,max_entries=1024)
        if mode=='custom_backend':
            try:incremental.price_books(**args,cache=cache,backend=object(),market_indexed_bindings={k:raw[k] for k in ['schema','identity','bindings']})
            except ValueError as e:assert 'Rust without a custom' in str(e)
            else:raise AssertionError('custom discount accepted')
        elif mode=='none':
            left=serialize(incremental.price_books(**args,cache=cache))
            right=serialize(incremental.price_books(**args,cache=DependencyCache(max_bytes=8388608,max_entries=1024),market_indexed_bindings=None))
            assert json.dumps(left,sort_keys=True)==json.dumps(right,sort_keys=True)
            assert left['positions']['loans'] and all(math.isfinite(x['market_value']) for x in left['positions']['loans'])
        else:
            # Use the real native transport preparation, not monkeypatched market grids.
            from portfolio_risk.core.runtime import run_context
            with run_context(args['config']):
                legacy=graph_native.graph_request(args['books'],valuation_books=args['books'],asof=args['asof'],swap_rates=args['swap_rates'],vol_pts=args['vol_pts'],config=args['config'],seed=args['seed'],mbs_hists=None,dep_hist=None,include_analytics=False,include_key_rates=False)
            for binding in current['bindings']:
                i=legacy['base']['terms'][0]['ids'].index(binding['position_id'])
                binding['original_row']=deepcopy(legacy['base']['terms'][0]['deck']['contracts'][i])
                binding['current_row']=deepcopy(binding['original_row'])
            bundle={k:current[k] for k in ['schema','identity','bindings']}
            out=serialize(incremental.price_books(**args,cache=cache,market_indexed_bindings=bundle))
            original=deepcopy(current);original['legacy']=legacy;current=deepcopy(original)
            assert cache._native.__class__.__name__=='NativeMarketIndexedBooks'
        if cache._native is not None:cache._native.close()
    else:
        budget=0 if mode=='eviction' else 8388608
        if mode=='cold_quote':
            bad=deepcopy(raw)
            for row in ['original_row','current_row']:bad['bindings'][0][row]['price']=100.
            for which in ['base','current']:bad['legacy'][which]['terms'][0]['deck']['contracts'][0]['price']=100.
            try:graph_native.NativeMarketIndexedBooks(bad,max_bytes=budget,max_entries=1024)
            except ValueError as e:assert 'quote' in str(e)
            else:raise AssertionError('conflicting original quote fitted')
            mode='price'
        graph=graph_native.NativeMarketIndexedBooks(raw,max_bytes=budget,max_entries=0 if mode=='eviction' else 1024)
        try:
            initial=deepcopy(graph.initial)
            if mode=='watchdog':
                print('READY',flush=True)
                sys.stdin.readline()
                raise AssertionError('watchdog failed to terminate isolated actor')
            elif mode in ['price','eviction','index','collateral','revision','quote']:
                if mode in ['index','collateral']:current['bindings'][0]['graph']['source']['bridge']['current_market']['curves'][1 if mode=='index' else 0]['discount_factors'][1]*=.999
                elif mode=='revision':current['bindings'][0]['graph']['source']['bridge']['current_market']['identity']['source_revision']='reviewed-successor'
                elif mode=='quote':current['bindings'][0]['current_row']['price']+=1.;current['legacy']['current']['terms'][0]['deck']['contracts'][0]['price']+=1.
                out=graph.price(current)
                if mode=='price':assert all(c['computed']==0 for s in out['market_indexed']['sources'] for c in s['counts'].values())
                if mode in ['collateral','quote']:assert out['market_indexed']['sources'][0]['counts']['indexed-cash']['computed']==0
            elif mode=='lifecycle':
                assert graph.call('info')['owner']=='market-indexed-books-1'
                graph.call('clear');assert graph.call('info')['cache']['entries']==0
                out=graph.price(current)
                legacy=graph_native.NativeGraph(0,0)
                try:
                    for target,op,request in [(graph,'price',raw['legacy']),(legacy,'price_market_indexed_books',raw)]:
                        try:target.call(op,request=request)
                        except ValueError as e:assert 'OWNER_MISMATCH' in str(e)
                        else:raise AssertionError('wrong owner priced')
                finally:legacy.close()
            elif mode in ['path_guard','stamp_guard']:
                oldstamp=graph._library._portfolio_stamp
                if mode=='path_guard':stack.enter_context(patch.object(graph_native,'library_path',lambda:root/'unloaded.dll'))
                else:graph._library._portfolio_stamp=(-1,-1,-1)
                graph._request=forbidden
                try:graph.price(raw)
                except RuntimeError as e:assert 'MODEL_VERSION_CHANGED' in str(e)
                else:raise AssertionError('actor artifact changed')
                graph._library._portfolio_stamp=oldstamp
            else:
                b=current['bindings'][0]
                if mode=='original':b['graph']['source']['bridge']['original']['locked_oas']+=1e-14
                elif mode=='signed_zero':b['graph']['source']['bridge']['original']['locked_oas']=-0.0
                elif mode=='shape':b['graph']['source']['bridge']['current_market']['curves'][0]['right_tail']={'flat_forward':None}
                elif mode=='currency':b['graph']['source']['bridge']['current_market']['curves'][0]['currency']='EUR'
                elif mode=='quote_conflict':
                    b['original_row']['price']=100.;b['current_row']['price']=100.
                    for which in ['base','current']:current['legacy'][which]['terms'][0]['deck']['contracts'][0]['price']=100.
                elif mode=='warm_price':b['current_row']['price']=-1.;current['legacy']['current']['terms'][0]['deck']['contracts'][0]['price']=-1.
                elif mode=='face':b['current_row']['notional']=1.
                elif mode=='cutoff':b['graph']['source']['bridge']['current_market']['information_unix_seconds']+=86400
                elif mode=='cap':b['graph']['source']['bridge']['current_market']['curves'][1]['discount_factors'][1]=.001
                elif mode=='extras':current['balance_sheet_extras']={'mm':[{}]}
                else:raise AssertionError(mode)
                try:graph.price(current)
                except ValueError:pass
                else:raise AssertionError('invalid current/original source admitted')
                out=graph.price(raw);current=deepcopy(raw)
        finally:graph.close()
        try:graph.price(raw)
        except RuntimeError as e:assert 'closed' in str(e)
        else:raise AssertionError('dropped owner priced')
print(json.dumps(dict(original=original,current=current,initial=initial,out=out,mode=mode,callbacks_forbidden=True),default=lambda d:d.toordinal()))
'''

MODES = ['price','eviction','index','collateral','revision','quote','lifecycle','path_guard','stamp_guard',
         'original','signed_zero','shape','currency','quote_conflict','cold_quote','warm_price','face','cutoff','cap','extras','none','custom_backend','public']

def normalized(request):
    from copy import deepcopy
    from portfolio_risk.analytics import market_indexed_reference as ref
    r=deepcopy(request)
    def row(c):
        defaults=dict(is_float=False,cap=10.,floor=-10.,call_threshold=.005,amort_type='bullet',sink_schedule=None,call_schedule=None,put_schedule=None,channel='',penalty_months=0.,ew_mult=1.)
        out=defaults|c
        for key in ['coupon','notional','price','cap','floor','call_threshold','penalty_months','ew_mult']:out[key]=float(out[key])
        return out
    for a in [r['legacy']['base'],r['legacy']['current']]:
        for book in a['terms']:
            book['deck']['calendar'].setdefault('extra_holidays',[])
            book['deck']['contracts']=[row(c) for c in book['deck']['contracts']]
    for b in r['bindings']:
        b['original_row']=row(b['original_row']);b['current_row']=row(b['current_row'])
        b['graph']['source']=ref.admit(b['graph']['source'])[0]
    return r

@pytest.mark.parametrize('mode', MODES)
def test_default_release_book_owner_and_real_transport(mode):
    env=dict(os.environ)
    env.pop('PORTFOLIO_RISK_RUST_LIB',None)
    process=subprocess.run([sys.executable,'-c',SCRIPT,mode,str(ROOT)],capture_output=True,text=True,encoding='utf-8',timeout=90,env=env)
    assert process.returncode==0,process.stderr
    value=json.loads(process.stdout)
    assert value['callbacks_forbidden'] is True
    from portfolio_risk.analytics.market_indexed_graph_reference import verify
    from portfolio_risk.analytics import market_indexed_reference as ref
    for key,raw in [('initial',value['original']),('out',value['current'])]:
        out=value[key]
        if out is None:continue
        meta=out['market_indexed'];cache=out['cache']
        assert meta['capital_ftp_eligible'] is meta['production_adoption'] is False
        base=normalized(value['original']);current=normalized(raw)
        ref._source_equal(meta['original_request'],base,lambda:False,'complete_original_book_parent')
        ref._source_equal(meta['current_request'],current,lambda:False,'complete_current_book_parent')
        size=max(ref._size(base),ref._size(current))
        cells=max(a['market']['config']['paths']*a['market']['config']['months']*a['market']['config']['forwards'] for a in [current['legacy']['base'],current['legacy']['current']])
        rows=max(sum(len(b['ids']) for b in a['terms']) for a in [current['legacy']['base'],current['legacy']['current']])
        legacy_work=cells*max(rows,1)*128
        expected_bytes=32*size+cache['max_bytes']+4*legacy_work+262144
        expected_work=legacy_work+64*size
        for b,old,source in zip(raw['bindings'],value['original']['bindings'],meta['sources']):
            verify(b['graph'],source,old['graph'],0,0,shared_cache={'schema':'market-indexed-books-1','max_bytes':cache['max_bytes'],'max_entries':cache['max_entries']})
            expected_bytes+=source['admitted_joined_bytes'];expected_work+=source['admitted_work']
        ref._source_equal(meta['admitted_bytes'],expected_bytes,lambda:False,'joined_bytes')
        ref._source_equal(meta['admitted_work'],expected_work,lambda:False,'joined_work')
        assert expected_bytes<=128*1024*1024 and expected_work<=60000000
        assert cache['bytes']<=cache['max_bytes'] and cache['entries']<=cache['max_entries']
        for b in raw['bindings']:
            row=next(x for x in out['positions'][b['book']] if x['id']==b['position_id'])
            assert row['base_oas_bp']==pytest.approx(b['graph']['source']['bridge']['original']['locked_oas']*1e4,rel=1e-12,abs=1e-9)

def test_default_actor_process_watchdog_emits_no_completed_result():
    """Process termination only; this does not establish cooperative native cancel."""
    from concurrent.futures import ThreadPoolExecutor
    env=dict(os.environ);env.pop('PORTFOLIO_RISK_RUST_LIB',None)
    process=subprocess.Popen([sys.executable,'-c',SCRIPT,'watchdog',str(ROOT)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding='utf-8',env=env)
    try:
        with ThreadPoolExecutor(max_workers=1) as pool:
            future=pool.submit(process.stdout.readline)
            try:assert future.result(timeout=90).strip()=='READY'
            except BaseException:
                process.kill();process.wait(timeout=10);raise
        process.kill()
        out,err=process.communicate(timeout=10)
        assert process.returncode!=0
        assert not out.strip(),(out,err)
    finally:
        if process.poll() is None:process.kill();process.wait(timeout=10)
