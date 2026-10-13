"""Authored default-release requirements; not executed artifact evidence.

No artifact imports, alternate binaries or reference backend during native work.
"""
import json,os,subprocess,sys
from pathlib import Path
import pytest
ROOT=Path(__file__).resolve().parents[3]
SCRIPT=r'''
import json,time,sys
from copy import deepcopy
from pathlib import Path
from contextlib import ExitStack
from unittest.mock import patch
from portfolio_risk.core import graph_native,pricing,curve,vol
from portfolio_risk.core.native import _DEFAULT_LIBRARY_PATH
from portfolio_risk.core.quant_native import term_call,_load
from portfolio_risk.core.runtime import RunConfig
from portfolio_risk.analytics import market_indexed_reference
root=Path(sys.argv[1]);mode=sys.argv[2]
assert RunConfig().compute_backend=="rust"
assert graph_native.library_path().resolve()==_DEFAULT_LIBRARY_PATH.resolve()
lib=_load(str(graph_native.library_path()))[0];assert lib.portfolio_graph_runtime_version()==1
raw=json.loads((root/"examples/models/market-indexed-books-1.json").read_bytes())
def rt(**kw):return dict(schema="graph-call-control-1",timeout_ms=10000,**kw)
def forbidden(*a,**k):raise AssertionError("Python financial callback during controlled native graph")
with ExitStack() as stack:
    for module,name in [(pricing,"pv_from_A"),(pricing,"solve_oas_from_A"),(curve,"bootstrap_curve"),(vol,"calibrate_abcd"),(market_indexed_reference,"reconstruct")]:stack.enter_context(patch.object(module,name,forbidden))
    if mode in ["precancel","callback_error","callback_type","posthandoff"]:
        calls=[0]
        def lease():
            calls[0]+=1
            if mode=="callback_error":raise RuntimeError("lease exception")
            if mode=="callback_type":return 1
            return mode=="precancel" or calls[0]>=2
        try:graph_native.NativeMarketIndexedBooks(raw,8388608,1024,runtime=rt(cancelled=lease))
        except (InterruptedError,ValueError,RuntimeError):pass
        else:raise AssertionError("cancelled creation published actor")
        owner=graph_native.NativeMarketIndexedBooks(raw,8388608,1024,runtime=rt());owner.close()
        output=None
    elif mode=="wrong_owner":
        owner=graph_native.NativeGraph(8388608,1024)
        try:
            try:owner.call("info",runtime=rt())
            except ValueError as e:assert "OWNER_MISMATCH" in str(e)
            else:raise AssertionError("legacy owner admitted")
        finally:owner.close()
        output=None
    elif mode=="missing_capability":
        class Missing:
            def __getattr__(self,key):
                if key=="portfolio_graph_runtime_version":raise AttributeError(key)
                return getattr(lib,key)
        try:term_call("graph-1",dict(op="create_market_indexed_books",max_bytes=8388608,max_entries=1024,request=raw),_library=Missing(),runtime=rt())
        except RuntimeError as e:assert "NATIVE_GRAPH_CONTROL_REQUIRED" in str(e)
        else:raise AssertionError("uncontrolled fallback")
        output=None
    elif mode=="public_forward":
        import datetime,numpy as np,polars as pl
        from portfolio_risk.core import config as cfg
        from portfolio_risk.core.dependency import DependencyCache
        from portfolio_risk.analytics import incremental
        rows=[]
        for id,row in zip(raw["legacy"]["base"]["terms"][0]["ids"],raw["legacy"]["base"]["terms"][0]["deck"]["contracts"]):
            rows.append(dict(id=id,face=row["notional"],maturity=datetime.date.fromordinal(row["maturity"]),coupon_or_spread=row["coupon"],freq_months=row["freq_months"],daycount=row["daycount"],is_float=row["is_float"],price=row["price"]))
        cache=DependencyCache(max_bytes=8388608,max_entries=1024)
        try:
            output=incremental.price_books(books={"loans":pl.DataFrame(rows)},asof=datetime.date.fromordinal(raw["identity"]["as_of_ordinal"]),swap_rates=np.full(len(cfg.SWAP_TENORS),.02),vol_pts=np.array([[.25,.25,.2]]),config=RunConfig(1,1,1),seed=1,include_analytics=False,include_key_rates=False,cache=cache,market_indexed_bindings={k:raw[k] for k in ["schema","identity","bindings"]},graph_runtime=rt())
            output=output|dict(positions={k:v.to_dicts() if hasattr(v,"to_dicts") else v for k,v in output["positions"].items()})
        finally:
            if cache._native is not None:cache._native.close()
    else:
        owner=graph_native.NativeMarketIndexedBooks(raw,8388608,1024,runtime=rt())
        try:
            if mode=="environment_drift":
                import os
                key="PORTFOLIO_RISK_RUST_LIB";old=os.environ.get(key);os.environ[key]=str(root/"unbuilt-foreign-product.dll")
                try:
                    try:owner.price(raw,runtime=rt())
                    except RuntimeError as e:assert "MODEL_VERSION_CHANGED" in str(e)
                    else:raise AssertionError("changed environment priced")
                    owner.close()  # same captured artifact, no foreign DLL load
                finally:
                    if old is None:os.environ.pop(key,None)
                    else:os.environ[key]=old
                output=None
            elif mode=="fresh_warm":
                time.sleep(.02);output=owner.price(raw,runtime=rt())
            elif mode=="clear":owner.call("clear",runtime=rt());output=owner.price(raw,runtime=rt())
            elif mode in ["current_validation","original_parent"]:
                changed=deepcopy(raw)
                if mode=="current_validation":changed["bindings"][0]["current_row"]["price"]=0
                else:changed["bindings"][0]["graph"]["source"]["bridge"]["original"]["locked_oas"]+=1e-12
                try:owner.price(changed,runtime=rt())
                except ValueError:pass
                else:raise AssertionError("invalid source hit admitted")
                output=None
            else:output=owner.price(raw,runtime=rt())
        finally:owner.close()
print(json.dumps(dict(raw=raw,output=output,callbacks_forbidden=True)))
'''
@pytest.mark.parametrize("mode",["environment_drift","public_forward","create","fresh_warm","clear","current_validation","original_parent","wrong_owner","missing_capability","precancel","callback_error","callback_type","posthandoff"])
def test_default_release_controlled_graph(mode):
    env=dict(os.environ);env.pop("PORTFOLIO_RISK_RUST_LIB",None)
    process=subprocess.run([sys.executable,"-c",SCRIPT,str(ROOT),mode],env=env,text=True,encoding="utf-8",capture_output=True,timeout=90)
    assert process.returncode==0,process.stderr
    actual=json.loads(process.stdout);assert actual["callbacks_forbidden"]
    out=actual["output"]
    if out is not None:
        from portfolio_risk.analytics.market_indexed_graph_reference import verify
        raw=actual["raw"]
        for binding,source in zip(raw["bindings"],out["market_indexed"]["sources"]):
            verify(binding["graph"],source,binding["graph"],0,0,shared_cache={"schema":"market-indexed-books-1","max_bytes":8388608,"max_entries":1024})
        assert out["market_indexed"]["production_adoption"] is False
