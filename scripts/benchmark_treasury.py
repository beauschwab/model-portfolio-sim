"""Bounded synthetic FTP reporting benchmark, not product repricing or ledger simulation."""
import argparse
import json
from pathlib import Path
import threading
import time
import psutil
from portfolio_risk.analytics.treasury import example,evaluate

p=argparse.ArgumentParser()
p.add_argument('--positions',type=int,default=60000)
p.add_argument('--output',type=Path,required=True)
args=p.parse_args()
spec=example();templates=spec['positions']
spec['positions']=[templates[i%2]|dict(id=f'position-{i}',loan_id=f'loan-{i}' if i%2==0 else None)
                   for i in range(args.positions)]
process=psutil.Process();peak=[process.memory_info().rss];stop=threading.Event()
def sample():
    while not stop.wait(.01):peak[0]=max(peak[0],process.memory_info().rss)
thread=threading.Thread(target=sample,daemon=True);thread.start()
try:
    started=time.perf_counter();out=evaluate(spec);elapsed=time.perf_counter()-started
finally:stop.set();thread.join()
receipt=dict(positions=args.positions,capital_snapshots=len(spec['capital']),elapsed_s=elapsed,
    peak_process_rss_mib=peak[0]/1024**2,backend=out['backend'],input_sha256=out['input_sha256'],
    ftp_rows=out['ftp_positions'].height,capital_ratios=out['capital_metrics'].height,
    max_elimination_error=out['ftp_reconciliation']['elimination_error'].abs().max(),
    scope='Synthetic native FTP and supplied capital report including JSON transport and Polars formatting; no pricing, solver, ledger, HTTP, or persistence')
args.output.write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt,indent=2))
