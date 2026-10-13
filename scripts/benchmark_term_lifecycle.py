"""Fresh-process corporate/CD full spot-risk comparison, with output parity.

Synthetic inputs; first call includes disk-cached Numba load/JIT, excludes imports.
Each backend gets the same thread/path budget and cold, warm and contract-edit runs.
This is not the full mixed-book, solver and ledger acceptance benchmark.
"""
import argparse
import datetime as dt
import hashlib
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
    from portfolio_risk.core.native import library_path
    from portfolio_risk.core.runtime import RunConfig, run_context
    from portfolio_risk.products import corp, cds

    numba.set_num_threads(args.threads)
    asof=dt.date(2026,6,10)
    if args.product=='corporate':
        sample=demo.model_balance_sheet(scale=.001,asof=asof)['loans']
        book=pl.concat([sample]*((args.positions+len(sample)-1)//len(sample))).head(args.positions)
        fn=lambda book: corp.run_corp_risk(book,asof,*market,None,None,seed=27)
        notional='face'
    else:
        book=demo.demo_cd_book(args.positions,asof=asof)
        fn=lambda book: cds.run_cd_risk(book,asof,*market,seed=27)
        notional='balance'
    book=book.with_columns(pl.Series('id',[f'T{i:06d}' for i in range(args.positions)]),pl.lit(100.).alias('price'))
    market=demo.demo_market()
    edited=book.with_columns(pl.when(pl.col('id')=='T000000').then(pl.col(notional)*1.01)
                             .otherwise(pl.col(notional)).alias(notional))
    timings=[];financial={};baseline=memory()
    with run_context(RunConfig(args.paths,args.paths,3,compute_backend=args.child)) as ctx:
        for i in range(args.repeats+2):
            phase='cold' if i==0 else ('edited' if i==args.repeats+1 else f'warm_{i}')
            start=time.perf_counter()
            frame=fn(edited if phase=='edited' else book)
            elapsed=time.perf_counter()-start
            timings.append(dict(phase=phase,seconds=elapsed,python_cache_hits=ctx.hits,python_cache_misses=ctx.misses))
            if args.child=='rust':
                from portfolio_risk.core.quant_native import mortgage_cache_statistics
                timings[-1]['native_market_cache']=mortgage_cache_statistics()
            columns=['oas_bps','model_price','dv01']+[c for c in frame.columns if c.startswith(('krd01_','vega_'))]
            # Retain compact numeric outputs. Millions of Python list objects from
            # prior samples distort later timings through unrelated GC scans.
            financial[phase]=frame[columns].to_numpy()
    result=dict(backend=args.child,product=args.product,version=__version__,positions=args.positions,
                paths=args.paths,threads=args.threads,timings=timings,financial=financial,
                warm_median_seconds=statistics.median(t['seconds'] for t in timings if t['phase'].startswith('warm_')),
                baseline_memory=baseline,final_memory=memory())
    if args.child=='rust':
        result['native_library_sha256']=hashlib.sha256(library_path().read_bytes()).hexdigest()
    result['financial']={phase:values.tolist() for phase,values in financial.items()}
    Path(args.output).write_text(json.dumps(result,allow_nan=False),encoding='utf-8')


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--positions',type=int,default=256)
    p.add_argument('--paths',type=int,default=8)
    p.add_argument('--threads',type=int,default=4)
    p.add_argument('--repeats',type=int,default=3)
    p.add_argument('--product',choices=['corporate','cd'])
    p.add_argument('--child',choices=['python','rust'])
    p.add_argument('--output')
    args=p.parse_args()
    if min(args.positions,args.paths,args.threads,args.repeats)<1:
        p.error('positive dimensions required')
    if args.child:
        if not args.product or not args.output: p.error('child requires product/output')
        return child(args)
    results=[];errors={}
    import numpy as np
    with tempfile.TemporaryDirectory(prefix='term-lifecycle-bench-') as tmp:
        for product in ['corporate','cd']:
            pair=[]
            for backend in ['python','rust']:
                out=Path(tmp)/f'{product}-{backend}.json'
                command=[sys.executable,__file__,'--child',backend,'--product',product,'--output',str(out)]
                for name in ['positions','paths','threads','repeats']:
                    command+=['--'+name,str(getattr(args,name))]
                process=subprocess.run(command,capture_output=True,text=True,
                    env=os.environ|{'NUMBA_NUM_THREADS':str(args.threads)},
                    creationflags=subprocess.CREATE_NO_WINDOW if os.name=='nt' else 0)
                if process.returncode: raise RuntimeError(process.stdout+process.stderr)
                result=json.loads(out.read_text(encoding='utf-8'))
                pair.append(result.pop('financial'));results.append(result)
                print(product,backend,'completed',flush=True)
            for phase,values in pair[0].items():
                a,b=np.asarray(values),np.asarray(pair[1][phase])
                np.testing.assert_allclose(a,b,rtol=1e-7,atol=1e-5,err_msg=f'{product}/{phase}')
                errors[f'{product}/{phase}']=float(np.max(np.abs(a-b)))
    report=dict(scope='Full corporate and CD spot-risk only; excludes other drivers, solver and ledger',
        synthetic=True,platform=platform.platform(),python=platform.python_version(),results=results,
        tolerance=dict(rtol=1e-7,atol=1e-5),parity=True,max_absolute_errors=errors,
        script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        memory_note='OS process peak includes Python imports, Numba load/JIT, JSON/FFI transport and retained results; not standalone-Rust RSS. Warm runs use the same process and run context.')
    out=Path(args.output) if args.output else ROOT/'docs/reviews/2026-09-30-rust-term-lifecycle-benchmark.json'
    out.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
    print(out)


if __name__=='__main__':
    main()
