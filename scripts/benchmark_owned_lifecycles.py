"""Fresh-process accounting/KPI/unit-library/HiGHS comparison on identical books.

This excludes the incremental edit graph and daily ledger acceptance workload.
Reports actual full-book reruns, not cached response retrieval. Synthetic only.
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
    from benchmark_balance_sheet import fixture
    from benchmark_comparison import memory
    from portfolio_risk import demo,__version__
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.analytics.kpis import compute_kpis
    from portfolio_risk.strategy.unitlib import build_unit_library,evaluate_strategy
    from portfolio_risk.strategy.optimizer import optimize_balance_sheet
    from portfolio_risk.core.runtime import RunConfig,run_context
    from portfolio_risk.core.native import library_path
    numba.set_num_threads(args.threads)
    bs,metadata=fixture(args.positions);sr,vp=demo.demo_market();dh=demo.demo_deposit_history()
    bs['hedges']=demo.demo_hedge_book(asof=bs['asof'])
    edited=bs|{'loans':bs['loans'].with_columns(pl.when(pl.col('id')==bs['loans']['id'][0]).then(pl.col('face')*1.01).otherwise(pl.col('face')).alias('face'))}
    baseline=memory();timings=[];financial={}
    def collect(prefix,value):
        if isinstance(value,pl.DataFrame):
            for name,dtype in value.schema.items():
                if dtype.is_numeric():financial[prefix+'/'+name]=value[name].to_numpy()
        elif isinstance(value,dict):
            for name,v in value.items():collect(prefix+'/'+str(name),v)
        elif isinstance(value,np.ndarray):financial[prefix]=value
        elif isinstance(value,(list,tuple)):
            for i,v in enumerate(value):collect(prefix+'/'+str(i),v)
        elif isinstance(value,(int,float,np.generic)):financial[prefix]=np.asarray(value)
    config=RunConfig(args.paths,args.paths,args.horizon,compute_backend=args.child)
    with redirect_stdout(io.StringIO()),run_context(config):
        for i in range(args.repeats+2):
            phase='cold' if i==0 else 'edited' if i==args.repeats+1 else f'warm_{i}'
            book=edited if phase=='edited' else bs;stages={};start=time.perf_counter()
            nii=run_balance_sheet_nii(book,sr,vp,dh,horizon=args.horizon,seed=29,asof=book['asof'])
            stages['accounting']=time.perf_counter()-start;then=time.perf_counter()
            base=compute_kpis(book,sr,vp,dh,nii['monthly'],seed=29)
            stages['kpis']=time.perf_counter()-then;then=time.perf_counter()
            lib=build_unit_library(sr,vp,book['mbs_hists'],dh,horizon=args.horizon,seed=29,asof=book['asof'])
            stages['unit_library']=time.perf_counter()-then;then=time.perf_counter()
            solution=optimize_balance_sheet([(lib,base)],lcr_min=.01,nsfr_min=.01,cet1_min=.001,eve_limit=1.,max_total_assets=1e7,cash_budget=1e7,commercial=[])
            stages['solve']=time.perf_counter()-then;stages['total']=time.perf_counter()-start
            if not solution['validated']:raise AssertionError(solution)
            timings.append(dict(phase=phase,seconds=stages))
            collect(phase+'/accounting',nii);collect(phase+'/kpis',base)
            for key in ['nii','runoff','balance','cash_interest','dv01']:collect(phase+'/units/'+key,lib[key])
            collect(phase+'/objective',solution['worst_case_nii_$'])
        allocation=[dict(template='agency_mbs',purchase_m=0,notional=1e6),dict(template='cd_2y',purchase_m=0,notional=1e6)]
        samples=[]
        for _ in range(1000):
            before=time.perf_counter_ns();evaluate_strategy(lib,allocation,base);samples.append((time.perf_counter_ns()-before)/1e6)
    result=dict(backend=args.child,version=__version__,timings=timings,baseline_memory=baseline,final_memory=memory(),
                instrument_metadata=metadata,interactive_ms=dict(median=statistics.median(samples),p95=float(np.percentile(samples,95)),p99=float(np.percentile(samples,99))))
    if args.child=='rust':result['native_binary_sha256']=hashlib.sha256(library_path().read_bytes()).hexdigest()
    np.savez_compressed(str(args.output)+'.npz',**financial)
    Path(args.output).write_text(json.dumps(result,allow_nan=False),encoding='utf-8')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name,default in [('positions',375),('paths',8),('horizon',27),('threads',4),('repeats',2)]:p.add_argument('--'+name,type=int,default=default)
    p.add_argument('--output');p.add_argument('--child',choices=['python','rust']);a=p.parse_args()
    if a.child:return child(a)
    import numpy as np
    results=[];max_error=0.;checked=0
    with tempfile.TemporaryDirectory(prefix='owned-lifecycle-bench-') as tmp:
        paths=[]
        for backend in ['python','rust']:
            path=Path(tmp)/(backend+'.json');paths.append(path)
            cmd=[sys.executable,__file__,'--child',backend,'--output',str(path)]
            for key in ['positions','paths','horizon','threads','repeats']:cmd+=['--'+key,str(getattr(a,key))]
            done=subprocess.run(cmd,capture_output=True,text=True,env=os.environ|{'NUMBA_NUM_THREADS':str(a.threads)},creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
            if done.returncode:raise RuntimeError(done.stdout+done.stderr)
            results.append(json.loads(path.read_text()));print(backend,'finished',flush=True)
        with np.load(str(paths[0])+'.npz') as x,np.load(str(paths[1])+'.npz') as y:
            assert set(x.files)==set(y.files)
            for key in x.files:
                np.testing.assert_allclose(x[key],y[key],rtol=1e-7,atol=1e-5,err_msg=key)
                if x[key].size:max_error=max(max_error,float(np.nanmax(np.abs(x[key].astype(float)-y[key].astype(float)))))
                checked+=x[key].size
    report=dict(scope='Raw-book accounting, mixed-book parallel KPIs, raw unit library, HiGHS solve and interactive coefficient evaluation; excludes incremental graph and daily ledger',
                configuration=vars(a),results=results,parity=True,values_checked=checked,max_absolute_error=max_error,tolerance=dict(rtol=1e-7,atol=1e-5),
                platform=platform.platform(),python=platform.python_version(),script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                memory_note='Fresh process per backend. OS peak includes interpreter, imports, JIT/cache load, transport and retained compact numerical outputs. Identical thread/path budgets; no concurrent benchmark workloads.')
    out=Path(a.output) if a.output else ROOT/'docs/reviews/2026-09-30-owned-lifecycle-benchmark.json'
    out.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8');print(out)


if __name__=='__main__':main()
