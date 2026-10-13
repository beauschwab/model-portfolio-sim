"""Separate raw input, JSON and native FFI costs for the full CD risk driver.

Run after other compute jobs finish. Instrumentation is local to this process;
no engine source, binary, numerical tolerance, or allocator is changed.
"""
import argparse
import datetime as dt
import functools
import hashlib
import json
from pathlib import Path
import statistics
import time


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=60000)
    parser.add_argument('--repeats',type=int,default=5)
    parser.add_argument('--threads',type=int,default=4)
    parser.add_argument('--output',required=True)
    args=parser.parse_args()
    import numba
    import polars as pl
    from portfolio_risk import demo
    from portfolio_risk.core import quant_native
    from portfolio_risk.core.native import library_path
    from portfolio_risk.core.runtime import RunConfig,run_context
    from portfolio_risk.products import cds
    numba.set_num_threads(args.threads)
    asof=dt.date(2026,6,10)
    book=demo.demo_cd_book(args.positions,asof=asof).with_columns(pl.lit(100.).alias('price'))
    market=demo.demo_market();records=[];current={}
    def timed(name,fn):
        @functools.wraps(fn)
        def call(*a,**kw):
            start=time.perf_counter()
            try:return fn(*a,**kw)
            finally:current[name]=current.get(name,0.)+time.perf_counter()-start
        return call
    with run_context(RunConfig(8,8,3,compute_backend='rust')):
        cds.run_cd_risk(book,asof,*market,seed=27) # warm native functions and ABI signatures
        library=quant_native._load(str(library_path()))[0]
        original_deck=quant_native._term_deck_request
        original_json=json.dumps
        original_invoke=library.portfolio_term_risk_into
        quant_native._term_deck_request=timed('raw_contract_transport',original_deck)
        json.dumps=timed('json_encoding',original_json)
        library.portfolio_term_risk_into=timed('native_ffi_including_decode',original_invoke)
        try:
            for _ in range(args.repeats):
                current={};start=time.perf_counter()
                frame=cds.run_cd_risk(book,asof,*market,seed=27)
                current['total']=time.perf_counter()-start
                current['other_transport']=current['total']-sum(v for k,v in current.items() if k!='total')
                records.append(current)
                assert frame.height==args.positions
        finally:
            quant_native._term_deck_request=original_deck
            json.dumps=original_json
            library.portfolio_term_risk_into=original_invoke
    report=dict(scope='instrumented warm full CD spot risk, 8 paths; native FFI includes parsing and output copy',threads=args.threads,
        positions=args.positions,records=records,
        medians={k:statistics.median(r[k] for r in records) for k in records[0]},
        binary_sha256=hashlib.sha256(library_path().read_bytes()).hexdigest())
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report['medians'],indent=2))


if __name__=='__main__':main()
