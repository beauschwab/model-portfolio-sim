from pathlib import Path
import json,time,statistics,sys,hashlib
import numpy as np
import numba
from portfolio_risk.core.quant_native import call
from portfolio_risk.core.native import library_path
from portfolio_risk.analytics.accounting import book_yield
numba.set_num_threads(4)
n=10000;t=360;h=27
rates=np.linspace(.015,.08,n);cf=np.repeat((rates/12)[:,None],t,axis=1);cf[:,-1]+=1
prices=np.linspace(.85,1.15,n)
expected=book_yield(cf,prices)
values=[]
for _ in range(5):
 start=time.perf_counter();out=call(14,[cf,prices,h,60],[(n,h),(n,h),(n,)]);values.append(time.perf_counter()-start)
np.testing.assert_allclose(out[2],expected,rtol=1e-10,atol=1e-12)
result=dict(seconds=values,warm_median=statistics.median(values[1:]),max_yield_error=float(np.max(abs(out[2]-expected))),n=n,months=t,threads=4,binary_sha256=hashlib.sha256(library_path().read_bytes()).hexdigest())
Path(sys.argv[1]).write_text(json.dumps(result,indent=2));print(result)
