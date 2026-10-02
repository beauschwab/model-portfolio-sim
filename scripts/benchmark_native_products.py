"""Fresh-process built-in product/strategy comparison; synthetic data only.

uv run --project apps/api python scripts/benchmark_native_products.py --positions 375 --paths 128
Reports cold and uncached warm computation separately from dependency-cache hits.
The daily state-machine scale benchmark remains a separate workload.
"""
import argparse
from contextlib import redirect_stdout
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time

ROOT=Path(__file__).resolve().parents[1]


def child(args):
    import numba
    import numpy as np
    import polars as pl
    from benchmark_comparison import memory
    from portfolio_risk import __version__, demo
    from portfolio_risk.analytics.incremental import price_books,SUPPORTED_BOOKS
    from portfolio_risk.analytics.kpis import compute_kpis
    from portfolio_risk.core.dependency import DependencyCache
    from portfolio_risk.core.runtime import RunConfig,run_context
    from portfolio_risk.core.native import library_path
    from portfolio_risk.strategy.unitlib import build_unit_library
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    numba.set_num_threads(min(args.threads,numba.config.NUMBA_NUM_THREADS))
    bs=demo.model_balance_sheet(scale=.001);sr,vp=demo.demo_market();dh=demo.demo_deposit_history()
    books={};remaining=args.positions
    original_count=sum(len(bs[k]) for k in SUPPORTED_BOOKS)
    for i,key in enumerate(SUPPORTED_BOOKS):
        n=remaining if i==len(SUPPORTED_BOOKS)-1 else args.positions*len(bs[key])//original_count
        remaining-=n;frame=bs[key];idcol='cusip' if key=='mbs' else 'id'
        books[key]=pl.concat([frame]*((n+len(frame)-1)//len(frame))).head(n).with_columns(
            pl.Series(idcol,[f'{key}-{j}' for j in range(n)]))
    bs.update(books)
    # Preserve the auxiliary balance sheet when replicating the five product books.
    multiple=args.positions/original_count
    bs['equity']*=multiple
    if bs.get('mm') is not None:
        bs['mm']=bs['mm'].with_columns((pl.col('balance')*multiple).alias('balance'))
    config=RunConfig(args.paths,args.paths,args.horizon,compute_backend=args.child)
    inputs=dict(books=books,asof=bs['asof'],swap_rates=sr,vol_pts=vp,mbs_hists=bs['mbs_hists'],
        dep_hist=dh,config=config,include_analytics=True,seed=7)
    elapsed=[]
    with redirect_stdout(io.StringIO()):
        for _ in range(args.repeats+1):
            cache=DependencyCache(max_bytes=512*1024*1024,max_entries=500_000)
            start=time.perf_counter();out=price_books(**inputs,cache=cache);elapsed.append(time.perf_counter()-start)
        start=time.perf_counter();warm=price_books(**inputs,cache=cache);cached=time.perf_counter()-start
        with run_context(config):
            start=time.perf_counter()
            lib=build_unit_library(sr,vp,bs['mbs_hists'],dh,grid_m=[0,2],horizon=args.horizon)
            base=compute_kpis(bs,sr,vp,dh)
            build=time.perf_counter()-start
            start=time.perf_counter()
            solved=optimize_balance_sheet([(lib,base)],lcr_min=.01,nsfr_min=.01,cet1_min=.001,
                eve_limit=1.,max_total_assets=1e7,cash_budget=1e7,commercial=[])
            solve=time.perf_counter()-start
    assert solved['validated'],solved
    financial={key:frame.drop('id').to_numpy().tolist() for key,frame in out['positions'].items()}
    financial['nii']=out['nii']['monthly']['nii'].to_list()
    financial['objective']=[solved['worst_case_nii_$']]
    result=dict(backend=args.child,version=__version__,positions=args.positions,paths=args.paths,horizon=args.horizon,
        threads=numba.get_num_threads(),cold_seconds=elapsed[0],uncached_warm_seconds=elapsed[1:],
        uncached_warm_median=statistics.median(elapsed[1:]),cached_seconds=cached,
        cached_computed_nodes=sum(v['computed'] for v in warm['graph'].values()),
        library_and_base_seconds=build,solver_seconds=solve,financial=financial,**memory())
    if args.child=='rust':result['product_binary_sha256']=hashlib.sha256(library_path().read_bytes()).hexdigest()
    Path(args.output).write_text(json.dumps(result,allow_nan=False),encoding='utf-8')


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--positions',type=int,default=375)
    parser.add_argument('--paths',type=int,default=128);parser.add_argument('--threads',type=int,default=4)
    parser.add_argument('--horizon',type=int,default=27);parser.add_argument('--repeats',type=int,default=3)
    parser.add_argument('--child',choices=['python','rust']);parser.add_argument('--output')
    args=parser.parse_args()
    if args.child:return child(args)
    results=[]
    with tempfile.TemporaryDirectory(prefix='native-product-bench-') as tmp:
        for backend in ('python','rust'):
            out=Path(tmp)/f'{backend}.json'
            command=[sys.executable,__file__,'--child',backend,'--output',str(out)]
            for field in ('positions','paths','threads','horizon','repeats'):command += ['--'+field,str(getattr(args,field))]
            completed=subprocess.run(command,capture_output=True,text=True,env=os.environ|{'NUMBA_NUM_THREADS':str(args.threads)},
                creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
            if completed.returncode:
                raise RuntimeError(completed.stdout + completed.stderr)
            results.append(json.loads(out.read_text()));print(backend,'complete',flush=True)
    import numpy as np
    a,b=[r.pop('financial') for r in results];errors={}
    for key in a:
        x,y=np.asarray(a[key]),np.asarray(b[key]);np.testing.assert_allclose(x,y,rtol=1e-7,atol=1e-4)
        errors[key]=float(np.max(np.abs(x-y))) if x.size else 0.
    report=dict(platform=platform.platform(),python=platform.python_version(),synthetic=True,
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),results=results,
        max_absolute_errors=errors,parity=True,tolerance=dict(rtol=1e-7,atol=1e-4))
    out=Path(args.output) if args.output else ROOT/f'docs/reviews/2026-09-30-native-products-{args.positions}.json'
    out.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(out)


if __name__=='__main__':main()
