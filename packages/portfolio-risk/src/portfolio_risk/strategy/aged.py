"""Transport native aged-contract preparation into existing coefficient evaluation."""
from copy import deepcopy
import numpy as np
from ..core.quant_native import term_call


def build_library(request,scenario_id=None):
    """Rust projects/replays/reprices; this function only reshapes its typed output."""
    result=term_call('aged-units-1',request)
    return library_from_result(request,result,scenario_id)


def library_from_result(request,result,scenario_id=None):
    """Reshape already prepared native output without invoking a financial model."""
    if result.get('schema')!='aged-units-1':raise ValueError('native aged result required')
    scenario_id=scenario_id or result['base_scenario_id']
    index=result['scenario_ids'].index(scenario_id)
    values=np.asarray(result['values'],dtype=np.float64).reshape(result['shape'])[index]
    units=[{k:u[k] for k in ('template','h','side')} for u in result['units']]
    templates={t['template_id']:dict(t['weights'],kind=t['term']['product'],side=t['side']) for t in request['templates']}
    funding=result.get('funding')
    if not funding or funding['scenario_ids']!=result['scenario_ids']:
        raise ValueError('REBUILD_REQUIRED: native full-tail funding coefficients required')
    demands=np.asarray(funding['demand'],dtype=np.float64).reshape(funding['shape'])[index]
    return dict(preparation_schema='aged-units-1',units=units,templates=templates,
        horizon=result['horizon'],grid_m=sorted({u['h'] for u in units}),
        nii=values[:,0,:].copy(),balance=values[:,1,:].copy(),dv01=values[:,2,0].copy(),
        vectors={(u['template'],u['h']):values[i].copy() for i,u in enumerate(units)},
        reporting_ordinals=result['reporting_ordinals'],scenario_id=scenario_id,
        funding_ordinals=funding['ordinals'],funding_policy=funding['policy'],
        funding_vectors={(u['template'],u['h']):demands[i].copy() for i,u in enumerate(units)},
        funding_sources=deepcopy({k:v for k,v in funding.items() if k!='demand'}),
        model_identity=deepcopy(result['identity']),unit_sources=deepcopy(result['units']),
        interpolation_diagnostics=deepcopy(result['interpolation_diagnostics']),
        source_request=deepcopy(request),model_scope=result['scope'])


def optimize(request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None):
    """Transport raw dated products; Rust prepares coefficients, solves and replays."""
    from ..core.decision_native import NativeDecision
    return NativeDecision().call(op='optimize_aged',schema='strategy-aged-1',aged=request,
        bases=bases,constraints=constraints,capital_dependency=capital_dependency,max_purchase_cost_assets=max_purchase_cost_assets)


class AgedSession:
    """Transport an owned native transactional dated preparation and solver.

    ``stage`` returns a candidate. The caller independently verifies it and then
    chooses ``publish`` or ``abort``; no staged economics replace the live state.
    """
    def __init__(self,request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None):
        from ..core.decision_native import NativeDecision
        import threading
        self.native=NativeDecision()
        self.lock=threading.RLock()
        result=self.native.call(op='create_aged',schema='strategy-aged-1',aged=request,
            bases=bases,constraints=constraints,capital_dependency=capital_dependency,
            max_purchase_cost_assets=max_purchase_cost_assets)
        self.handle,self.version=result['handle'],result['version']
        self.snapshot=result['snapshot']

    def _call(self,op,**fields):
        if self.handle is None:raise RuntimeError('aged session is closed')
        return self.native.call(op=op,handle=self.handle,version=self.version,**fields)

    def stage(self,**fields):
        with self.lock:return self._call('update_aged',**fields)

    def publish(self,token):
        with self.lock:
            result=self._call('publish',token=token)
            self.version=result['version']
            return result

    def abort(self,token):
        with self.lock:return self._call('abort',token=token)

    def evaluate(self):
        with self.lock:return self._call('eval')

    def status(self):
        with self.lock:return self._call('status')

    def close(self):
        with self.lock:
            if self.handle is not None:
                self.native.call(op='close',handle=self.handle)
                self.handle=None

    def __enter__(self):return self

    def __exit__(self,*args):self.close()
