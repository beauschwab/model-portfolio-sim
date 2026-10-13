"""Transport actual-purchase native product coefficients and owned allocation."""
from copy import deepcopy
import threading
import numpy as np


def library_from_result(request,result,scenario_id=None):
    """Reshape an existing native receipt; perform no financial calculation."""
    if result.get('schema')!='product-units-1' or result.get('raw_source')!=request:
        raise ValueError('native product result and exact raw source required')
    from .unitlib import METRICS
    if result['metrics']!=['nii','balance','dv01','hqla_l2a','outflow30','asf','rsf','rwa','assets','liabilities']:
        raise ValueError('native product metric order differs')
    scenario_id=scenario_id or result['base_scenario_id']
    index=result['scenario_ids'].index(scenario_id)
    values=np.asarray(result['values'],dtype=np.float64).reshape(result['shape'])[index]
    dates=result['reporting_ordinals'];horizon=len(dates)-1
    if values.shape!=(len(result['units']),len(METRICS),horizon):
        raise ValueError('native product coefficient shape differs')
    units=[dict(template=u['template_id'],h=dates.index(u['purchase_ordinal']),
        side=u['side'],unit_id=u['unit_id'],accounting_category=u['accounting_category'],
        capital_policy_revision=u['capital_policy_revision']) for u in result['units']]
    templates={}
    for raw,u in zip(request['units'],units,strict=True):
        templates[u['template']]=dict(raw['weights'],kind=raw['purchase_graph']['kind'],side=u['side'])
    funding=result['funding']
    if funding['scenario_ids']!=result['scenario_ids']:
        raise ValueError('native product funding scenario identity differs')
    demands=np.asarray(funding['demand'],dtype=np.float64).reshape(funding['shape'])[index]
    return dict(preparation_schema='product-units-1',units=units,templates=templates,
        horizon=horizon,grid_m=sorted({u['h'] for u in units}),
        nii=values[:,0,:].copy(),balance=values[:,1,:].copy(),dv01=values[:,2,0].copy(),
        vectors={(u['template'],u['h']):values[i].copy() for i,u in enumerate(units)},
        reporting_ordinals=dates,scenario_id=scenario_id,
        funding_ordinals=funding['ordinals'],funding_policy=funding['policy'],
        funding_vectors={(u['template'],u['h']):demands[i].copy() for i,u in enumerate(units)},
        funding_sources=deepcopy({k:v for k,v in funding.items() if k!='demand'}),
        model_identity=deepcopy(result['identity']),unit_sources=deepcopy(result['units']),
        interpolation_diagnostics=dict(method='exact supplied purchase and age grid; no interpolation'),
        source_request=deepcopy(request),model_scope=result['scope'])


def build_library(request,scenario_id=None):
    from ..analytics.model_contracts import prepare_product_units
    return library_from_result(request,prepare_product_units(request),scenario_id)


def optimize(request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None,face_limits=()):
    from ..core.decision_native import NativeDecision
    return NativeDecision().call(op='optimize_product',schema='strategy-product-1',product=request,
        bases=bases,constraints=constraints,capital_dependency=capital_dependency,
        max_purchase_cost_assets=max_purchase_cost_assets,face_limits=list(face_limits))


class ProductSession:
    """Native owner-thread preparation; updates stage before explicit publication."""
    def __init__(self,request,bases,constraints,*,capital_dependency=None,max_purchase_cost_assets=None,face_limits=()):
        from ..core.decision_native import NativeDecision
        self.native=NativeDecision();self.lock=threading.RLock()
        result=self.native.call(op='create_product',schema='strategy-product-1',product=request,
            bases=bases,constraints=constraints,capital_dependency=capital_dependency,
            max_purchase_cost_assets=max_purchase_cost_assets,face_limits=list(face_limits))
        self.handle,self.version=result['handle'],result['version'];self.snapshot=result['snapshot']

    def _call(self,op,**fields):
        if self.handle is None:raise RuntimeError('product session is closed')
        return self.native.call(op=op,handle=self.handle,version=self.version,**fields)

    def stage(self,**fields):
        with self.lock:return self._call('update_product',**fields)

    def publish(self,token):
        with self.lock:
            result=self._call('publish',token=token);self.version=result['version'];return result

    def abort(self,token):
        with self.lock:return self._call('abort',token=token)

    def evaluate(self,allocation):
        with self.lock:return self._call('eval',allocation=allocation)

    def solve(self):
        with self.lock:return self._call('solve')

    def status(self):
        with self.lock:return self._call('status')

    def close(self):
        with self.lock:
            if self.handle is not None:
                self.native.call(op='close',handle=self.handle);self.handle=None

    def __enter__(self):return self

    def __exit__(self,*args):self.close()
