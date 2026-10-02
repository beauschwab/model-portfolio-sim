"""Full daily-state/Parquet/replay comparison with a process-tree memory ceiling.

uv run --project apps/api --with psutil python scripts/benchmark_streamed_balance.py
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import threading
import time

import polars as pl
import psutil
from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress,source_identity,binary_path
from profile_balance_ledger import fixture


def child(args):
    spec=fixture(args.positions,args.days)
    proc=psutil.Process()
    stop=threading.Event();peak=[0];native_peak=[0];limit=[False]
    def monitor():
        while not stop.wait(.01):
            try:
                kids=proc.children(recursive=True)
                rss=sum(p.memory_info().rss for p in kids if p.is_running())
                native_peak[0]=max(native_peak[0],rss)
                total=proc.memory_info().rss+rss;peak[0]=max(peak[0],total)
                if total>args.memory_mib*2**20:
                    limit[0]=True
                    for p in kids:
                        p.kill()
                    os._exit(42)  # harness owns temp directory and discards this attempt
            except psutil.NoSuchProcess:
                continue
    thread=threading.Thread(target=monitor,daemon=True);thread.start()
    start=time.perf_counter()
    try:
        path=run_streamed_balance_stress(spec,args.directory,backend=args.backend,large_book=args.positions>2000)
        elapsed=time.perf_counter()-start
    finally:
        stop.set();thread.join()
    manifest=json.loads(path.read_text())
    fingerprints={}
    for table,data in manifest['tables'].items():
        digest=hashlib.sha256()
        for part in data['parts']:
            frame=pl.read_parquet(path.parent/part['path'])
            # Rounded fingerprints are screening only; test suite compares raw
            # native/reference numbers at rtol 1e-12, atol 1e-8, with no changed gates.
            floats=[name for name,dtype in frame.schema.items() if dtype==pl.Float64]
            digest.update(frame.with_columns(pl.col(floats).round(8)).hash_rows(seed=173).to_numpy().tobytes())
        fingerprints[table]=digest.hexdigest()
    print(json.dumps(dict(backend=args.backend,positions=args.positions,days=args.days,
        scenarios=2,seconds=elapsed,peak_tree_rss_mib=peak[0]/2**20,native_peak_rss_mib=native_peak[0]/2**20,
        memory_budget_mib=args.memory_mib,**manifest['timings'],
        journal_rows=manifest['validation']['journal_rows'],gl_keys=manifest['validation']['gl_keys'],
        partitions=sum(len(t['parts']) for t in manifest['tables'].values()),
        parquet_mib=sum(p['bytes'] for t in manifest['tables'].values() for p in t['parts'])/2**20,
        fingerprints=fingerprints,source_sha256=manifest['source_sha256'],binary_sha256=manifest['binary_sha256'])))


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--child',action='store_true')
    parser.add_argument('--backend',choices=['python','rust'])
    parser.add_argument('--positions',type=int,default=2000)
    parser.add_argument('--days',type=int,default=30)
    parser.add_argument('--directory')
    parser.add_argument('--memory-mib',type=int,default=2048)
    parser.add_argument('--repeats',type=int,default=3)
    parser.add_argument('--output',default='docs/reviews/2026-09-29-streamed-native-benchmark.json')
    args=parser.parse_args()
    if args.child:
        child(args);return
    initial=source_identity();binary=hashlib.sha256(binary_path().read_bytes()).hexdigest()
    rows=[]
    matrix=[(2000,30),(2000,180),(10000,30),(60000,30)]
    for count,days in matrix:
        for repeat in range(args.repeats):
            for backend in (['python','rust'] if repeat%2==0 else ['rust','python']):
                with tempfile.TemporaryDirectory(prefix='streamed-ledger-bench-') as directory:
                    cmd=[sys.executable,__file__,'--child','--backend',backend,'--positions',str(count),
                         '--days',str(days),'--directory',directory,'--memory-mib',str(args.memory_mib)]
                    completed=subprocess.run(cmd,text=True,capture_output=True)
                    if completed.returncode:
                        raise RuntimeError(f'{backend} {count}/{days} failed ({completed.returncode}): {completed.stderr[-3000:]}')
                    row=json.loads(completed.stdout);row['repeat']=repeat;rows.append(row)
                    print(f'{count} positions/{days} days {backend}: {row["seconds"]:.2f}s, {row["peak_tree_rss_mib"]:.0f} MiB tree RSS',flush=True)
    assert source_identity()==initial and hashlib.sha256(binary_path().read_bytes()).hexdigest()==binary
    summary=[]
    for count,days in matrix:
        subset=[r for r in rows if (r['positions'],r['days'])==(count,days)]
        hashes=[r['fingerprints'] for r in subset]
        if not all(h==hashes[0] for h in hashes):
            raise AssertionError(f'rounded output fingerprint mismatch at {count}/{days}')
        for backend in ['python','rust']:
            group=[r for r in subset if r['backend']==backend]
            summary.append(dict(positions=count,days=days,backend=backend,
                **{k:statistics.median(r[k] for r in group) for k in ['seconds','peak_tree_rss_mib','compute_and_partition_seconds','replay_and_finalize_seconds','journal_rows','partitions','parquet_mib']},
                min_seconds=min(r['seconds'] for r in group),max_seconds=max(r['seconds'] for r in group)))
    report=dict(scope='synthetic daily loan state; baseline and +200bp; full partitioned journals, reports, all Parquet checksum/schema reads, independent ordered replay and local atomic manifest publication; includes native child startup; excludes harness Python interpreter startup, product Monte Carlo, API, S3 and distributed workloads',
        source_sha256=initial,binary_sha256=binary,source_unchanged=True,memory_budget_mib=args.memory_mib,
        parity='all table ordered-row fingerprints match after rounding float columns to 8 decimals; small mixed model tests compare unrounded values',
        summary=summary,samples=rows)
    Path(args.output).write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(summary,indent=2))


if __name__=='__main__':
    main()
