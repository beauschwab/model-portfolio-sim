"""Transport verified actual-date native commercial coefficients and sessions."""
from copy import deepcopy
import threading
import numpy as np

def library_from_result(request,result,scenario_id=None):
    from ..analytics.commercial_dto import graph_identity as _raw
    from .unitlib import METRICS
    if result.get('schema')!='commercial-units-1':raise ValueError('verified native commercial result required')
    # Identity typing admits only the native DTO's explicit optional defaults;
    # it performs no source, credit, yield, pricing or coefficient calculation.
    _raw(request,result['raw_source'])
    if result['metrics']!=['nii','balance','dv01','hqla_l2a','outflow30','asf','rsf','rwa','assets','liabilities']:raise ValueError('commercial native metric order differs')
    scenario_id=scenario_id or request['base_scenario_id'];index=result['scenario_ids'].index(scenario_id)
    values=np.asarray(result['values'],dtype=np.float64).reshape(result['shape'])[index]
    dates=request['reporting_ordinals'];horizon=len(dates)-1
    if values.shape!=(len(result['units']),len(METRICS),horizon):raise ValueError('native commercial coefficient shape differs')
    units=[dict(template=u['template_id'],h=dates.index(u['purchase_ordinal']),side=u['side'],unit_id=u['unit_id'],accounting_category=u['accounting_category'],capital_policy_revision=u['capital_policy_revision']) for u in result['units']]
    keys=[(u['template'],u['h']) for u in units]
    if len(set(keys))!=len(keys):raise ValueError('commercial template/purchase identities must be unique for library evaluation')
    templates={u['template']:dict(raw['weights'],kind='commercial',side=u['side']) for raw,u in zip(request['units'],units,strict=True)}
    funding=result['funding']
    if funding['scenario_ids']!=result['scenario_ids']:raise ValueError('commercial funding scenarios differ')
    demands=np.asarray(funding['demand'],dtype=np.float64).reshape(funding['shape'])[index]
    return dict(preparation_schema='commercial-units-1',units=units,templates=templates,horizon=horizon,grid_m=sorted({u['h'] for u in units}),nii=values[:,0,:].copy(),balance=values[:,1,:].copy(),dv01=values[:,2,0].copy(),vectors={key:values[i].copy() for i,key in enumerate(keys)},reporting_ordinals=dates,scenario_id=scenario_id,funding_ordinals=funding['ordinals'],funding_policy=funding['policy'],funding_vectors={key:demands[i].copy() for i,key in enumerate(keys)},funding_sources=deepcopy({k:v for k,v in funding.items() if k!='demand'}),model_identity=deepcopy(request['identity']),unit_sources=deepcopy(result['units']),interpolation_diagnostics=dict(method='exact supplied commercial purchase/reporting grid; no interpolation'),source_request=deepcopy(request),model_scope=result['scope'])

def build_library(request,scenario_id=None):
    from ..analytics.model_contracts import prepare_commercial_units
    return library_from_result(request,prepare_commercial_units(request),scenario_id)

def optimize(request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None,face_limits=()):
    from ..core.decision_native import NativeDecision
    return NativeDecision().call(op='optimize_commercial',schema='strategy-commercial-1',commercial=request,bases=bases,constraints=constraints,capital_dependency=capital_dependency,max_purchase_cost_assets=max_purchase_cost_assets,face_limits=list(face_limits))

class CommercialSession:
    def __init__(self,request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None,face_limits=()):
        from ..core.decision_native import NativeDecision
        self.native=NativeDecision();self.lock=threading.RLock()
        result=self.native.call(op='create_commercial',schema='strategy-commercial-1',commercial=request,bases=bases,constraints=constraints,capital_dependency=capital_dependency,max_purchase_cost_assets=max_purchase_cost_assets,face_limits=list(face_limits))
        self.handle,self.version=result['handle'],result['version'];self.snapshot=result['snapshot']
    def _call(self,op,**fields):
        if self.handle is None:raise RuntimeError('commercial session is closed')
        return self.native.call(op=op,handle=self.handle,version=self.version,**fields)
    def stage_constraints(self,**fields):
        with self.lock:return self._call('update_constraints',**fields)
    def publish(self,token):
        with self.lock:
            result=self._call('publish',token=token);self.version=result['version'];return result
    def abort(self,token):
        with self.lock:return self._call('abort',token=token)
    def evaluate(self,allocation):
        with self.lock:return self._call('eval',allocation=allocation)
    def solve(self):
        with self.lock:return self._call('optimize')
    def status(self):
        with self.lock:return self._call('status')
    def close(self):
        with self.lock:
            if self.handle is not None:self.native.call(op='close',handle=self.handle);self.handle=None
    def __enter__(self):return self
    def __exit__(self,*args):self.close()
