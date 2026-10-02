"""Controlled MBS loop ablations; compilation excluded, same arrays and threads.

Creates Numba variants in this process only. No production code is changed.
"""
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import statistics
import sys
import time
from types import FunctionType
from unittest.mock import patch

import numba
import numpy as np
import polars as pl
from portfolio_risk import demo
from portfolio_risk.core import kernels,scenarios,quant_native

numba.set_num_threads(4)
with redirect_stdout(io.StringIO()):
    bs=demo.model_balance_sheet(scale=.001);sr,vp=demo.demo_market();cc,ps=bs['mbs_hists']
    port=pl.concat([bs['mbs']]*16)
    models,b,abcd,sec,*_=scenarios.setup(port,sr,vp,cc,ps)
    paths=scenarios.build_paths(sr,vp,abcd,b,models,scenarios.CRN(128,7))
captured=[]
with patch('portfolio_risk.core.scenarios.engine',side_effect=lambda *a:captured.append(a)):
    scenarios.run_engine(paths,sec)
args=captured[0]
double=tuple(a.astype(np.float64) if isinstance(a,np.ndarray) and a.dtype.kind=='f' else a for a in args)
reference=kernels.engine.__wrapped__
native=lambda *a:quant_native.call(4,a,[(len(sec[0]),360),(len(sec[0]),1),(len(sec[0]),1),
    (len(sec[0]),128,1),(len(sec[0]),128,1),(len(sec[0]),360),(len(sec[0]),360)])
strict=numba.njit(parallel=True,fastmath=False,boundscheck=False)(reference.py_func)
checked=numba.njit(parallel=True,fastmath=False,boundscheck=True)(reference.py_func)
strict_globals=dict(reference.py_func.__globals__)
for key in ('_fsig','_sig_exact','_lut','_spline_eval'):
    strict_globals[key]=numba.njit(inline='always',fastmath=False)(strict_globals[key].py_func)
strict_function=FunctionType(reference.py_func.__code__,strict_globals,'strict_helper_engine',
                             reference.py_func.__defaults__,reference.py_func.__closure__)
strict_helpers=numba.njit(parallel=True,fastmath=False,boundscheck=False)(strict_function)
variants=[('numba_fast_original_dtype',reference,args),('numba_fast_f64',reference,double),
          ('numba_strict_f64',strict,double),('numba_strict_checked_f64',checked,double),
          ('numba_strict_including_helpers_f64',strict_helpers,double),
          ('rust_f64_abi',native,args)]
for path in (p for p in sys.argv[1:] if not p.startswith('--')):
    def candidate(*a, library=path):
        from unittest.mock import patch
        with patch.object(quant_native,'library_path',return_value=Path(library)):
            return native(*a)
    variants.append((f'rust_candidate:{Path(path).parents[2].name}',candidate,args))
if '--native-only' in sys.argv:
    variants=[v for v in variants if v[0].startswith('rust')]
expected=reference(*args)
results=[]
for name,fn,a in variants:
    actual=fn(*a)  # Warm/compile separately.
    for x,y in zip(expected,actual):np.testing.assert_allclose(x,y,rtol=1e-10,atol=1e-10)
    samples=[]
    for _ in range(5):
        start=time.perf_counter();fn(*a);samples.append(time.perf_counter()-start)
    result=dict(name=name,seconds=samples,median=statistics.median(samples),parity=True)
    results.append(result);print(json.dumps(result),flush=True)
root=Path(__file__).resolve().parents[1]
filename='2026-09-30-mortgage-native-ablation.json' if '--native-only' in sys.argv else '2026-09-30-mortgage-ablation.json'
(root/'docs/reviews'/filename).write_text(json.dumps(dict(
    instruments=len(port),paths=128,months=360,threads=4,results=results,
    caveat='Strict variants change outer-loop flags; shared inlined helpers retain their declared fastmath.'),indent=2)+'\n',encoding='utf-8')
