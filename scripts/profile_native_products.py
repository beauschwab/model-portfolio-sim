"""Attribute product graph time without changing the engine or native binaries.

Process-local wrappers measure Numba kernel calls and the Rust FFI separately
from its Python array adapter. Timings are inclusive; do not add nested totals.
"""
import argparse
from collections import defaultdict
from contextlib import redirect_stdout
from functools import wraps
import io
import json
from pathlib import Path
import time


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--backend',choices=['python','rust'],required=True)
    parser.add_argument('--positions',type=int,default=6000)
    args=parser.parse_args()
    import numba
    import polars as pl
    from portfolio_risk import demo
    from portfolio_risk.analytics.incremental import price_books,SUPPORTED_BOOKS
    from portfolio_risk.core.dependency import DependencyCache
    from portfolio_risk.core.runtime import RunConfig
    from portfolio_risk.core import kernels,lmm,quant_native
    from portfolio_risk.products import corp,cds,deposits
    numba.set_num_threads(4)
    bs=demo.model_balance_sheet(scale=.001);sr,vp=demo.demo_market()
    remaining=args.positions;original=sum(len(bs[k]) for k in SUPPORTED_BOOKS);books={}
    for i,key in enumerate(SUPPORTED_BOOKS):
        n=remaining if i==len(SUPPORTED_BOOKS)-1 else args.positions*len(bs[key])//original
        remaining-=n;frame=bs[key];idcol='cusip' if key=='mbs' else 'id'
        books[key]=pl.concat([frame]*((n+len(frame)-1)//len(frame))).head(n).with_columns(
            pl.Series(idcol,[f'{key}-{j}' for j in range(n)]))
    inputs=dict(books=books,asof=bs['asof'],swap_rates=sr,vol_pts=vp,mbs_hists=bs['mbs_hists'],
        dep_hist=demo.demo_deposit_history(),config=RunConfig(128,128,27,compute_backend=args.backend),
        include_analytics=True,seed=7)
    def run():return price_books(**inputs,cache=DependencyCache(max_bytes=512*1024*1024,max_entries=500_000))
    with redirect_stdout(io.StringIO()):run()
    timings=defaultdict(lambda:dict(calls=0,seconds=0.))
    def timed(key,fn):
        @wraps(fn)
        def invoke(*a,**kw):
            start=time.perf_counter()
            try:return fn(*a,**kw)
            finally:
                cell=timings[key];cell['calls']+=1;cell['seconds']+=time.perf_counter()-start
        return invoke
    if args.backend=='python':
        for fn in (lmm.lmm_simulate,kernels.engine,kernels.stress_engine,kernels.batched_pv_engine,
                   corp.corp_engine,cds.cd_engine,deposits.deposit_engine,deposits.deposit_stress_engine):
            cells=dict(zip(fn.__code__.co_freevars,fn.__closure__))
            cell=cells['reference'];cell.cell_contents=timed(cells['name'].cell_contents,cell.cell_contents)
    else:
        original_load=quant_native._load
        def load(path):
            lib,fn=original_load(path)
            def invoke(op,*a):return timed(f'ffi:{op}',fn)(op,*a)
            return lib,invoke
        quant_native._load=load
        original_call=quant_native.call
        def call(op,*a):return timed(f'adapter_inclusive:{op}',original_call)(op,*a)
        quant_native.call=call
    with redirect_stdout(io.StringIO()):
        start=time.perf_counter();result=run();elapsed=time.perf_counter()-start
    out=Path(__file__).resolve().parents[1]/f'docs/reviews/2026-09-30-profile-{args.backend}-{args.positions}.json'
    report=dict(backend=args.backend,positions=args.positions,paths=128,threads=4,seconds=elapsed,
        timings=dict(sorted(timings.items())),computed_nodes=sum(v['computed'] for v in result['graph'].values()))
    out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report,indent=2))


if __name__=='__main__':main()
