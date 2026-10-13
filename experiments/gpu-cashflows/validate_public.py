"""Public numeric output gates: CUDA kernels in reference orchestration vs Rust.

This process-local patch is only a validation harness, not a new public backend.
No timing claim is made for this hybrid reference orchestration.
"""
from contextlib import redirect_stdout
import inspect
import io
import json
from unittest.mock import patch

import numba
import numpy as np
import polars as pl

from benchmark import Gpu, ROOT, compare, sha, shapes_for
from portfolio_risk import demo
from portfolio_risk.analytics.accounting import run_balance_sheet_nii
from portfolio_risk.analytics.risk import run_risk
from portfolio_risk.core import scenarios, kernels, quant_native
from portfolio_risk.core.runtime import RunConfig, run_context
from portfolio_risk.products import deposits


def numeric_columns(a,b):
    assert a.columns==b.columns and a.shape==b.shape
    results=[]
    for name,dtype in a.schema.items():
        if dtype.is_numeric():
            results.append(dict(column=name,**compare([a[name].to_numpy()],[b[name].to_numpy()],
                                                       rtol=1e-7,atol=1e-5)))
        else:
            assert a[name].to_list()==b[name].to_list()
    return results


def main():
    numba.set_num_threads(4)
    gpu=Gpu()
    book=demo.model_balance_sheet(scale=.001)
    sr,vp=demo.demo_market(); cc,ps=book['mbs_hists']
    mortgage=book['mbs'].head(16); deposit=book['deposits'].head(16)
    dep_hist=demo.demo_deposit_history()
    bs={'mbs':mortgage,'deposits':deposit,'mbs_hists':(cc,ps),'asof':book['asof']}
    def run():
        return {'mortgage_risk':run_risk(mortgage,sr,vp,cc,ps,seed=7),
                'deposit_risk':deposits.run_deposit_risk(deposit,sr,vp,dep_hist,seed=7),
                'nii':run_balance_sheet_nii(bs,sr,vp,dep_hist,horizon=27,seed=7)}
    with redirect_stdout(io.StringIO()):
        with run_context(RunConfig(128,128,27,compute_backend='rust')):
            expected=run()
    calls={4:0,6:0}
    def adapter(op,reference):
        signature=inspect.signature(reference)
        def invoke(*args,**kwargs):
            bound=signature.bind(*args,**kwargs); bound.apply_defaults()
            a=list(bound.arguments.values())
            outputs=list(gpu.call(op,a,shapes_for(op,a))[0]); calls[op]+=1
            indices=[3,4] if op==4 else [4]
            for i in indices: outputs[i]=outputs[i].astype(np.float32)
            return tuple(outputs)
        return invoke
    batch_signature=inspect.signature(kernels.batched_pv_engine.__wrapped__)
    def batch_adapter(*args,**kwargs):
        bound=batch_signature.bind(*args,**kwargs); bound.apply_defaults()
        a=list(bound.arguments.values())
        pv=[]
        for scenario in range(int(a[5])):
            selected=np.flatnonzero(a[4]==scenario)
            assert 0<len(selected)<=2048
            standard=[v[selected] for v in a[:4]]+a[6:24]+[np.array([0]),False,a[25]]
            output=gpu.call(4,standard,shapes_for(4,standard))[0]; calls[4]+=1
            n,t=output[0].shape
            offsets=np.arange(n+1)*t
            times=((np.arange(t)+1)/12.+a[24][:,None]).reshape(-1)
            # The public batched caller divides the result by its path count.
            pv.append(quant_native.csr_price(offsets,times,output[0].ravel(),a[23],len(selected))*len(selected))
        return np.stack(pv)
    with redirect_stdout(io.StringIO()):
        with patch.object(scenarios,'engine',adapter(4,scenarios.engine.__wrapped__)), \
             patch.object(deposits,'deposit_engine',adapter(6,deposits.deposit_engine.__wrapped__)), \
             patch.object(kernels,'batched_pv_engine',batch_adapter), \
             run_context(RunConfig(128,128,27,compute_backend='python')):
            actual=run()
    checks={key:numeric_columns(expected[key],actual[key]) for key in ('mortgage_risk','deposit_risk')}
    for key in ('monthly','summary','book_yields'):
        checks['nii:'+key]=numeric_columns(expected['nii'][key],actual['nii'][key])
    assert calls[4]>=40 and calls[6]>=40
    report=dict(passed=True,positions_per_product=16,paths=128,checks=checks,
                cuda_calls=calls,gpu_binary_sha256=sha(gpu.path),
                scope='Public spot risk columns including OAS bps, price, KRD and vega, plus full monthly NII tables. Rust public routes compared with Python reference orchestration patched to CUDA prepared cashflow kernels. Validation only; not API or native workflow GPU integration.')
    out=ROOT/'docs/reviews/2026-10-01-gpu-cashflows-public-validation.json'
    out.write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
    print(json.dumps(dict(passed=True,numeric_columns=sum(len(v) for v in checks.values()),
                         cuda_calls=calls,report=str(out))))


if __name__=='__main__':
    main()
