"""Run the opt-in Rust compute-profile build against raw synthetic requests.
Build: cargo build --release --locked --features compute-profile --manifest-path packages/portfolio-risk-native/Cargo.toml --target-dir packages/portfolio-risk-native/target/profile --bin portfolio-lifecycle
"""
import argparse,datetime,json,os,subprocess,time,hashlib
from pathlib import Path

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--positions',type=int,default=60000)
    parser.add_argument('--paths',type=int,default=8)
    parser.add_argument('--ffi',action='store_true')
    parser.add_argument('--without-numba-pool',action='store_true')
    parser.add_argument('--output',default='docs/reviews/2026-10-01-native-stage-profile.json')
    args=parser.parse_args()
    import polars as pl
    from benchmark_balance_sheet import fixture
    from portfolio_risk import demo
    from portfolio_risk.core.runtime import RunConfig,run_context
    from portfolio_risk.core.lifecycle_native import accounting_request,balance_risk_request,kpi_request
    root=Path(__file__).resolve().parents[1]
    binary=root/'packages/portfolio-risk-native/target/profile/release'/('portfolio-lifecycle.exe' if os.name=='nt' else 'portfolio-lifecycle')
    bs,_=fixture(args.positions);sr,vp=demo.demo_market();dh=demo.demo_deposit_history()
    bs['hedges']=demo.demo_hedge_book(asof=bs['asof'])
    with run_context(RunConfig(args.paths,args.paths,27,compute_backend='rust')):
        requests=[('accounting-1',accounting_request(bs,sr,vp,dh,27,29,bs['asof'],None,None,False,None,False)),
            ('kpi-1',kpi_request('all',bs,bs['asof'],nii=pl.DataFrame({'nii':[0.]*27}),risk=balance_risk_request(bs,sr,vp,dh,29,25.,None)))]
    rows=[];previous={}
    if args.ffi:
        import ctypes
        import numba
        from portfolio_risk.core import quant_native
        dll=binary.parent/('portfolio_risk_native.dll' if os.name=='nt' else 'libportfolio_risk_native.so')
        os.environ['PORTFOLIO_RISK_RUST_LIB']=str(dll)
        if args.without_numba_pool:quant_native.get_num_threads=lambda:4
        else:numba.set_num_threads(4)
    def encode(v):
        if isinstance(v,datetime.date):return v.toordinal()
        raise TypeError(type(v).__name__)
    for schema,request in requests:
        if args.ffi:
            start=time.perf_counter();quant_native.term_call(schema,request);elapsed=time.perf_counter()-start
            lib,_=quant_native._load(str(dll));fn=lib.portfolio_compute_profile;fn.argtypes=[];fn.restype=ctypes.c_void_p
            ptr=fn()
            try:current=json.loads(ctypes.string_at(ptr))['inclusive_seconds_and_calls']
            finally:lib.portfolio_term_free(ptr)
            stats={k:{f:v[f]-previous.get(k,{}).get(f,0) for f in v} for k,v in current.items()}
            previous=current
            rows.append(dict(schema=schema,process_seconds=elapsed,inclusive_seconds_and_calls=stats))
        else:
            payload=json.dumps(dict(schema=schema,threads=4,request=request),default=encode,allow_nan=False,separators=(',',':'))
            start=time.perf_counter();done=subprocess.run([str(binary)],input=payload,capture_output=True,text=True,timeout=120)
            elapsed=time.perf_counter()-start
            assert done.returncode==0,(done.stdout,done.stderr)
            assert json.loads(done.stdout)['ok']
            rows.append(dict(schema=schema,process_seconds=elapsed,**json.loads(done.stderr)))
    report=dict(positions=args.positions,paths=args.paths,threads=4,ffi=args.ffi,without_numba_pool=args.without_numba_pool,mode=('instrumented persistent FFI process' if args.ffi else 'instrumented standalone cold process per request')+'; inclusive nested timings cannot be added; zero NII input only for KPI profiling',
        binary_sha256=hashlib.sha256((dll if args.ffi else binary).read_bytes()).hexdigest(),results=rows)
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report,indent=2))

if __name__=='__main__':main()
