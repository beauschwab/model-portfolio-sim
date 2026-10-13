"""Independent raw-source reference for an explicit named-index pricing graph.

Test/reference only; never an application finance backend or native callback.
"""
from copy import deepcopy
import json, math
from . import market_indexed_reference as ref

def canonical(v):
    if isinstance(v,dict):
        out={k:canonical(x) for k,x in v.items()}
        if 'scenario_id' in out:out['scenario_id']='paired-pricing-index-slice-1'
        for k in ['path','rate_path']:
            if k in out and isinstance(out[k],dict) and set(out[k])=={'id','revision'}:out[k]['id']='paired-pricing-index-slice-1'
        if 'scenarios' in out:out['scenarios']=['paired-pricing-index-slice-1']
        return out
    if isinstance(v,list):return [canonical(x) for x in v]
    return v
def verify(raw,out,original,cache=8*1024*1024,entries=1024,check=lambda:False,*,shared_cache=None):
    ref._check(check)
    for request in [raw,original]:
        if type(request) is not dict or set(request)!={'schema','product','source'} or request['schema']!='market-indexed-graph-1' or request['product'] not in ['indexed_loan','vanilla_swap']:
            raise ValueError('graph raw version/product/fields')
    expected=ref.reconstruct(raw['source'],check);norm=deepcopy(raw);norm['source']=expected['cache_parent']
    base=deepcopy(original);base['source']=ref.admit(base['source'],check)[0]
    ref._source_equal(out['current_request'],norm,check,'current_request')
    ref._source_equal(out['original_anchor'],base,check,'original_anchor')
    ref._equal(out['schema'],'market-indexed-graph-1',check)
    assert len(out['cases'])==8
    selected=[];cash_keys=set()
    for actual,want in zip(out['cases'],expected['cases']):
        ref._check(check)
        ref._equal(actual['role'],want['scenario_id'],check)
        for field in ['indexed_request','indexed_result','swap_request','swap_result']:
            ref._equal(actual[field],canonical(want[field]),check,field)
        ref._source_equal(actual['source'],want['rate_source'],check,'source')
        ref._source_equal(actual['original_oas'],want['original_locked_oas'],check,'oas')
        ref._equal(actual['spread_adjusted_swap_pv'],want['spread_adjusted_swap_pv'],check)
        curve={p['ordinal']:p['discount_factor'] for p in want['swap_request']['discount_curve']['points']}
        # Independent full contractual loan cash, including exactly one terminal principal.
        loan=math.fsum((e['components']['principal']+e['components']['cash_interest']+e['components']['fees'])*curve[e['payment_ordinal']]*math.exp(-want['original_locked_oas']*e['day_offset']/365.) for e in want['indexed_result']['schedule']['events'])
        ref._equal(actual['indexed_pv'],loan,check)
        val=loan if raw['product']=='indexed_loan' else want['spread_adjusted_swap_pv'];selected.append(val)
        ref._equal(actual['selected_pv'],val,check)
        cash_keys.add(json.dumps(canonical(want['indexed_request']),sort_keys=True,separators=(',',':')))
    for i,role in enumerate(['collateral','index','parallel']):
        ref._equal(out['risk'][i],dict(curve_role=role,selected_rate_gradient_per_bp=(selected[3+2*i]-selected[2+2*i])/(2*raw['source']['bridge']['risk_bump'])*0.0001),check)
    for field,want in [('pricing_only',True)]+[(k,False) for k in ['stochastic_crn','physical_accounting','production_adoption','saved_library_adoption','capital_ftp_eligible']]:
        ref._source_equal(out[field],want,check,field)
    ref._source_equal(out['pairing_policy'],'supplied_deterministic_paired_curve_bumps-1',check,'pairing')
    _,a,b=ref.admit(raw['source'],check);size=max(ref._size(norm),ref._size(base))
    ref._source_equal(out['admitted_joined_bytes'],4*a['retained_bytes']+3*b['retained_bytes']+16*size+cache+262144,check,'bytes')
    ref._source_equal(out['admitted_work'],a['numerical_work']+b['admitted_work']+64*size,check,'work')
    assert out['admitted_joined_bytes']<=128*1024*1024 and out['admitted_work']<=60000000
    if shared_cache is None:
        ref._source_equal(out['cache']['max_bytes'],cache,check,'cache_bytes')
        ref._source_equal(out['cache']['max_entries'],entries,check,'cache_entries')
    else:
        if type(shared_cache) is not dict or set(shared_cache)!={'schema','max_bytes','max_entries'} or shared_cache['schema']!='market-indexed-books-1' or cache!=0 or entries!=0:
            raise ValueError('explicit books shared cache context')
        for field,maximum in [('max_bytes',16*1024*1024),('max_entries',4096)]:
            if type(shared_cache[field]) is not int or not 0<=shared_cache[field]<=maximum:
                raise ValueError('shared cache bounded integer configuration')
            ref._source_equal(out['cache'][field],shared_cache[field],check,field)
        if not 0<=out['cache']['bytes']<=shared_cache['max_bytes'] or not 0<=out['cache']['entries']<=shared_cache['max_entries']:
            raise ValueError('shared cache retained admission')
    ref._check(check)
    return len(cash_keys)
