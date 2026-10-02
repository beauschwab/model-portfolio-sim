"""Synthetic tape -> cohorts -> individual native pricing -> Parquet audit.

This measures mortgage/NMD numerical approximation, not calibrated consumer
credit models, solver/ledger acceptance, HTTP or live object/database services.
"""
import argparse
import io
import json
from pathlib import Path
import tempfile
import threading
import time


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--loans',type=int,default=60000)
    p.add_argument('--paths',type=int,default=8)
    p.add_argument('--months',type=int,default=3)
    p.add_argument('--threads',type=int,default=4)
    p.add_argument('--refine-age',action='store_true',help='Separate the four ages in this synthetic fixture; not production bucket calibration')
    p.add_argument('--output',required=True)
    args=p.parse_args()
    import numba
    import polars as pl
    import psutil
    from portfolio_risk import demo,__version__
    from portfolio_risk.analytics.cohorts import build,read_tape,example,presets
    from portfolio_risk.analytics.cohort_validation import audit
    from portfolio_risk.core.runtime import RunConfig
    numba.set_num_threads(args.threads)
    seed=read_tape(example().encode(),'csv')['frame'].filter(pl.col('product').is_in(['mortgage','deposit'])).to_dicts()
    records=[seed[i%len(seed)] | {'loan_id':f'loan-{i:08d}','audit_group':str((i//8)%120)} for i in range(args.loans)]
    source=pl.DataFrame(records).write_csv().encode();del records
    config=presets()
    for product in ('mortgage','deposit'):
        config['rules'][product]['dimensions'].append({'field':'audit_group'})
        if args.refine_age:
            for dimension in config['rules'][product]['dimensions']:
                if dimension['field']=='age_months': dimension['edges']=[25.5,26.5,27.5]
    proc=psutil.Process();peak=[proc.memory_info().rss];stop=threading.Event()
    def monitor():
        while not stop.wait(.02):peak[0]=max(peak[0],proc.memory_info().rss)
    thread=threading.Thread(target=monitor,daemon=True);thread.start()
    started=time.perf_counter();parts={};rows={};bytes_written=0
    try:
        tape=read_tape(source,'csv');cohorts=build(tape,config);build_seconds=time.perf_counter()-started
        bs=demo.model_balance_sheet(scale=.001);sr,vp=demo.demo_market()
        with tempfile.TemporaryDirectory(prefix='cohort-audit-benchmark-') as directory:
            def emit(name,frame):
                nonlocal bytes_written
                for part in frame.iter_slices(65536):
                    path=Path(directory)/f'{name}-{parts.get(name,0)}.parquet'
                    part.write_parquet(path,compression='zstd')
                    assert pl.read_parquet(path).height==len(part)
                    parts[name]=parts.get(name,0)+1;rows[name]=rows.get(name,0)+len(part)
                    bytes_written+=path.stat().st_size
            summary=audit(cohorts,products=['mortgage','deposit'],asof=bs['asof'],
                swap_rates=sr,vol_pts=vp,config=RunConfig(args.paths,args.paths,args.months,compute_backend='rust'),
                mbs_hists=bs['mbs_hists'],dep_hist=demo.demo_deposit_history(),emit=emit,
                progress=lambda checks,failed:print(f'{checks} checks, {failed} outside tolerance',flush=True))
        result=dict(version=__version__,configuration=vars(args),seconds=time.perf_counter()-started,
            build_seconds=build_seconds,peak_rss_mib=peak[0]/2**20,parquet_mib=bytes_written/2**20,
            input_bytes=len(source),cohorts=cohorts['summary'],audit=summary,rows=rows,partitions=parts,
            exclusions=['dedicated consumer credit models','solver/daily ledger','HTTP','live PostgreSQL/S3','empirical model validation'])
        Path(args.output).write_text(json.dumps(result,indent=2,allow_nan=False)+'\n',encoding='utf-8')
        print(json.dumps(result,indent=2),flush=True)
    finally:stop.set();thread.join()


if __name__=='__main__':main()
