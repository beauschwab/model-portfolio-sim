"""Unapplied operational successor: source + new-job publication fencing.

This contains no financial computation or finance runtime selection. Root will
wire server-owned repository snapshots around independently verified candidates.
"""
from dataclasses import dataclass
from copy import deepcopy
import time
import math
from dataclasses import field
import json

def exact(a,b):
    from portfolio_risk.analytics.mortgage_mixed_identity import exact_json_identity
    if not exact_json_identity(a,b):raise ValueError('exact capital publication identity changed')

@dataclass(frozen=True)
class Fence:
    expected:dict
    deadline:float
    _snapshot:bytes=field(repr=False)
    def check(self,read_current,clock=time.monotonic):
        exact(self.expected,json.loads(self._snapshot))
        if clock()>=self.deadline:raise TimeoutError('original capital-job deadline expired')
        current=read_current()
        if type(current) is not dict:raise InterruptedError('missing source/job fences')
        exact(current,self.expected)
        if current['source_status']!='done':raise InterruptedError('source publication is not complete')
        if current['job_status']!='running' or current['cancelled'] is not False or current['lease_owned'] is not True:
            raise InterruptedError('capital publication cancelled or worker lease lost')
        exact(current['producer_completion'],{'complete':True,'protocol':3})
        if clock()>=self.deadline:raise TimeoutError('capital-job deadline expired after fence reads')

def retain_fence(server_snapshot,deadline):
    """Server snapshot includes source/raw/partitions/receipt/current bundle hashes.

    Completed source deadline is merely retained provenance in snapshot; this
    deadline is the queued NEW capital job deadline, never recalculated at retry.
    """
    if type(deadline) not in (int,float) or not math.isfinite(deadline):
        raise ValueError('finite original monotonic capital-job deadline required')
    required={'source_job_id','source_status','source_result_sha256','source_input_sha256',
        'source_artifact_hashes','producer_completion','source_receipt_sha256',
        'current_native_bundle','job_id','job_status','attempt_token','cancelled','lease_owned'}
    if type(server_snapshot) is not dict or server_snapshot.keys()!=required:
        raise ValueError('complete server-owned capital/source publication snapshot required')
    encoded=json.dumps(server_snapshot,allow_nan=False,separators=(',',':')).encode()
    return Fence(deepcopy(server_snapshot),deadline,encoded)

def fenced_dump(fence,result,read_current,dump,commit,clock=time.monotonic):
    """Called only AFTER complete independent source and native capital replay.

    dump must call the passed check during its immutable object uploads. commit
    must check snapshot/status/token/lease/cancel again in its SQL transaction.
    No caller receipt flag grants financial permission through this helper.
    """
    def check():fence.check(read_current,clock)
    check()
    ref=dump(result,check)
    check()  # detects mutation/cancellation DURING dump before a done reference
    return commit(ref,fence.expected,check)
