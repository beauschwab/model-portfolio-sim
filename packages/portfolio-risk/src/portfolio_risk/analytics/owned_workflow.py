"""Raw transport and immutable publication for the native end-to-end workflow.

Rust owns pricing, edits, units, HiGHS, accounting, mapping and daily simulation.
This adapter writes partitions and independently checks the persisted journal.
"""
import json
import math
import os
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import uuid

from ..core.runtime import run_context
from ..core.graph_native import graph_request
from ..core.lifecycle_native import unit_request
from ..strategy.unitlib import resolve_templates
from . import balance_stream as stream


def binary_path():
    configured=os.environ.get('PORTFOLIO_WORKFLOW_RUST_BIN')
    if configured: return Path(configured).resolve()
    from ..strategy.decision import _DECISION_DEFAULT
    return _DECISION_DEFAULT.parent/('portfolio-workflow.exe' if os.name=='nt' else 'portfolio-workflow')


def run_owned_workflow(*, books, asof, swap_rates, vol_pts, config, mbs_hists,
                       dep_hist, constraints, ledger, directory, markets=None,
                       seed=7, extras=None, steps=None, cache_bytes=128*1024*1024,
                       cache_entries=500_000, partition_rows=65536,
                       progress=None, cancelled=None, timeout=1800.):
    from numba import get_num_threads
    if config.compute_backend!='rust': raise ValueError('owned workflow requires Rust computation')
    if type(partition_rows) is not int or not 1<=partition_rows<=65536:
        raise ValueError('partition_rows must be in [1,65536]')
    if not math.isfinite(timeout) or not .001<=timeout<=3600:
        raise ValueError('timeout must be in [0.001,3600] seconds')
    if cancelled and cancelled(): raise InterruptedError('simulation cancelled')
    started=time.perf_counter()
    source=stream.source_identity()
    binary=binary_path()
    if not binary.is_file(): raise RuntimeError('build scripts/build_decision_native.py; no fallback was used')
    identity=stream._hash(binary)
    with run_context(config):
        graph=graph_request(books,valuation_books=books,asof=asof,swap_rates=swap_rates,
            vol_pts=vol_pts,config=config,seed=seed,mbs_hists=mbs_hists,dep_hist=dep_hist,
            balance_sheet_extras=extras)
        units=unit_request(swap_rates,vol_pts,mbs_hists,dep_hist,None,config.horizon,seed,asof,resolve_templates())
    request=dict(input=dict(graph=graph,units=units,threads=get_num_threads(),max_bytes=cache_bytes,max_entries=cache_entries,
        markets=[dict(name=name,swap_rates=sr.tolist(),vol_quotes=vp.ravel().tolist(),spread=spread)
                 for name,sr,vp,spread in (markets or [('base',swap_rates,vol_pts,0.)])]),
        constraints=constraints,steps=steps or [{}],ledger=ledger,timeout_ms=int(timeout*1000))
    del graph,units
    return _publish(request,ledger,directory,partition_rows,progress,cancelled,timeout,started,source,binary,identity)


def run_saved_workflow(bs,swap_rates,vol_pts,dep_hist,request,directory,*,seed=7,asof=None,progress=None,cancelled=None,timeout=1800.,materialize_specification=False):
    from numba import get_num_threads
    from ..core.lifecycle_native import accounting_request
    horizon=request['specification'].get('horizon_days',360)//30
    started=time.perf_counter();source=stream.source_identity();binary=binary_path();identity=stream._hash(binary)
    books=accounting_request(bs,swap_rates,vol_pts,dep_hist,horizon,seed,asof,None,None,False,None,False)
    allocation=request.get('allocation',[]);units=None
    if allocation:
        units=unit_request(swap_rates,vol_pts,bs['mbs_hists'],dep_hist,sorted({r['purchase_m'] for r in allocation}),
            horizon,seed,asof,resolve_templates(names=sorted({r['template'] for r in allocation})))
    ledger=dict(specification=request['specification'],position_mapping=request['position_mapping'],
        template_mapping=request.get('template_mapping',{}),amount_scale=request['amount_scale'],include_candidate=bool(allocation))
    raw=dict(mode='saved',input=dict(books=books,units=units,ledger=ledger,allocation=allocation,threads=get_num_threads(),materialize_specification=materialize_specification),timeout_ms=int(timeout*1000))
    del books,units
    return _publish(raw,ledger,directory,65536,progress,cancelled,timeout,started,source,binary,identity)


def _publish(request,ledger,directory,partition_rows,progress,cancelled,timeout,started,source,binary,identity):
    deadline = started + timeout
    def check_runtime():
        if cancelled and cancelled(): raise InterruptedError('simulation cancelled')
        if time.perf_counter() >= deadline: raise TimeoutError('workflow deadline exceeded')
        return False
    check_runtime()
    root=Path(directory).resolve();root.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.partial-',dir=root) as attempt:
        staging=Path(attempt);writer=stream.PartitionWriter(staging,partition_rows);workflow={}
        remaining=timeout-(time.perf_counter()-started)
        if remaining<=0: raise TimeoutError('workflow input preparation timed out')
        stream._rust(request,writer,progress,cancelled,remaining,executable=binary,workflow=workflow,consume_input=True)
        compute_end=time.perf_counter()
        if workflow.get('schema') not in ('owned-ledger-1','saved-ledger-1'): raise ValueError('invalid workflow protocol')
        # Identity metadata only; financial input validation and defaults are native.
        raw=ledger['specification']
        labels=dict(accounts=[SimpleNamespace(id=a['id']) for a in raw['accounts']],
                    scenarios=[SimpleNamespace(name=s['name']) for s in raw['scenarios']])
        validation=stream._finalize(labels,writer,check_runtime,native_reports=True)
        check_runtime()
        if stream._hash(binary)!=identity or stream.source_identity()!=source:
            raise RuntimeError('MODEL_VERSION_CHANGED: workflow binary or engine source changed during run')
        input_path=staging/'workflow-input.json'
        manifest=dict(version='balance-partitions-1',model_version=stream.MODEL_VERSION,backend='rust',
            execution=workflow['schema'],binary_sha256=identity,source_sha256=source,partition_rows=partition_rows,
            workflow=workflow,validation=validation,input=dict(path=input_path.name,sha256=stream._hash(input_path)),
            timings=dict(compute_and_partition_seconds=compute_end-started,replay_and_finalize_seconds=time.perf_counter()-compute_end),
            tables={name:dict(schema={k:str(v) for k,v in schema.items()},parts=writer.parts[name]) for name,schema in stream.SCHEMAS.items()})
        with (staging/'manifest.json').open('w',encoding='utf-8') as handle:
            json.dump(manifest,handle,allow_nan=False,indent=2);handle.flush();os.fsync(handle.fileno())
        check_runtime()
        final=root/('run-'+uuid.uuid4().hex);os.replace(staging,final)
    return final/'manifest.json'
