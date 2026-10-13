"""Complete native graph/edit/unit/HiGHS/accounting/daily-ledger scale acceptance.

Includes immutable Parquet output and independent persisted journal replay.
This is a synthetic workflow benchmark, not a Python speedup or model-validation claim.
"""
import argparse
import json
import os
from pathlib import Path
import platform
import tempfile
import threading
import time


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--positions',type=int,default=375)
    p.add_argument('--paths',type=int,default=8)
    p.add_argument('--horizon',type=int,default=27)
    p.add_argument('--threads',type=int,default=4)
    p.add_argument('--memory-mib',type=int,default=4096)
    p.add_argument('--output',required=True)
    args=p.parse_args()
    import numba
    import polars as pl
    import psutil
    from benchmark_balance_sheet import fixture
    from portfolio_risk import demo,__version__
    from portfolio_risk.analytics.owned_workflow import run_owned_workflow
    from portfolio_risk.analytics.incremental import SUPPORTED_BOOKS
    from portfolio_risk.core.runtime import RunConfig
    from portfolio_risk.strategy.unitlib import TEMPLATES
    numba.set_num_threads(args.threads)
    bs,metadata=fixture(args.positions);sr,vp=demo.demo_market();history=demo.demo_deposit_history()
    scale=1e-6;mapping={};opening=0.
    for book in (*SUPPORTED_BOOKS,'mm'):
        frame=bs[book];id_col='cusip' if book=='mbs' else 'id'
        balance='current_face' if book=='mbs' else 'face' if book in ('loans','debt') else 'balance'
        for row in frame.iter_rows(named=True):
            asset=book in ('mbs','loans') or book=='mm' and row['side']=='asset'
            opening+=row[balance]*(1 if asset else -1)*scale
            mapping[f'{book}:{row[id_col]}']=dict(account='bank',kind='loan' if asset else 'funding',
                classification='ac',risk_weight=1. if asset else 0.,asf_weight=0. if asset else 1.,
                rsf_weight=1. if asset else 0.,lcr_outflow_weight=0.,hqla_weight=0.)
    template_mapping={name:dict(account='bank',kind='loan' if t.get('side',1)>0 else 'funding',classification='ac',
        risk_weight=t.get('rwa',0.),asf_weight=t.get('asf',0.),rsf_weight=t.get('rsf',0.),
        lcr_outflow_weight=t.get('outflow30',0.),hqla_weight=t.get('hqla_l2a',0.)) for name,t in TEMPLATES.items()}
    ledger=dict(specification=dict(version='balance-stress-2',horizon_days=args.horizon*30,
        accounts=[dict(id='bank',entity='synthetic-bank',currency='USD',cash=1000.,equity=1000.+opening)],
        scenarios=[dict(name='funding_stress',rate_shift=.01,spread_shift=.005,deposit_flight=.05)],positions=[]),
        position_mapping=mapping,template_mapping=template_mapping,amount_scale=scale,include_candidate=True)
    key='loans:'+bs['loans']['id'][0]
    steps=[{}, {}, {'edits':{key:{'coupon_or_spread':float(bs['loans']['coupon_or_spread'][0])+.001}}}]
    proc=psutil.Process();stop=threading.Event();peak=[0];child_peak=[0];exceeded=[]
    def monitor():
        while not stop.wait(.02):
            try:
                children=proc.children(recursive=True)
                child=sum(c.memory_info().rss for c in children if c.is_running())
                total=proc.memory_info().rss+child;peak[0]=max(peak[0],total);child_peak[0]=max(child_peak[0],child)
                if total>args.memory_mib*2**20:
                    exceeded.append(total)
                    for c in children:c.kill()
                    return
            except psutil.NoSuchProcess:continue
    thread=threading.Thread(target=monitor,daemon=True);thread.start();started=time.perf_counter()
    counts={}
    def progress(table):
        counts[table]=counts.get(table,0)+1
    try:
        with tempfile.TemporaryDirectory(prefix='native-workflow-bench-') as directory:
            path=run_owned_workflow(books={k:bs[k] for k in SUPPORTED_BOOKS},asof=bs['asof'],swap_rates=sr,vol_pts=vp,
                config=RunConfig(args.paths,args.paths,args.horizon,compute_backend='rust'),mbs_hists=bs['mbs_hists'],
                dep_hist=history,constraints=dict(lcr_min=.01,nsfr_min=.01,cet1_min=.001,eve_limit=1.,max_total_assets=1e7,cash_budget=1e7,commercial=[]),
                ledger=ledger,directory=directory,extras={'mm':bs['mm'],'equity':bs['equity']},seed=29,steps=steps,progress=progress)
            manifest=json.loads(path.read_text(encoding='utf-8'))
            if exceeded:raise AssertionError('process-tree memory ceiling exceeded')
            summary=pl.concat([pl.read_parquet(path.parent/part['path']) for part in manifest['tables']['summary']['parts']])
            report=dict(version=__version__,platform=platform.platform(),configuration=vars(args),seconds=time.perf_counter()-started,
                peak_tree_rss_mib=peak[0]/2**20,native_peak_rss_mib=child_peak[0]/2**20,**manifest['timings'],
                workflow=manifest['workflow'],validation=manifest['validation'],summary=summary.to_dicts(),
                rows={name:sum(p['rows'] for p in value['parts']) for name,value in manifest['tables'].items()},
                parquet_mib=sum(p['bytes'] for t in manifest['tables'].values() for p in t['parts'])/2**20,
                binary_sha256=manifest['binary_sha256'],source_sha256=manifest['source_sha256'],instruments=metadata,
                daily_mapping_scope='Synthetic amortized-cost loan/funding mapping; no active credit-default, collateral, netting or management-action fixture in this scale run',
                exclusions=['full key-rate sweep','saved hedge-to-netting mapping','HTTP','live database or object storage','production model validation'])
            Path(args.output).write_text(json.dumps(report,indent=2,allow_nan=False)+'\n',encoding='utf-8')
            print(json.dumps({k:report[k] for k in ['seconds','peak_tree_rss_mib','native_peak_rss_mib','rows','validation']},indent=2),flush=True)
    finally:stop.set();thread.join()


if __name__=='__main__':main()
