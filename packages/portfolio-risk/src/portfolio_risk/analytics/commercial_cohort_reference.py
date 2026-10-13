"""Independent commercial representative/member audit from complete raw sources."""
import math
from .commercial_graph_reference import _typed,verify_graph
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

def typed(value,key=''):
    value=_typed(value,key)
    if isinstance(value,dict):return {k:typed(v,k) for k,v in value.items()}
    if isinstance(value,list):return [typed(v,key) for v in value]
    if key in {'amount_multiplier','absolute_amount','relative'}:
        if type(value)not in (float,int) or not math.isfinite(value):raise ArithmeticError('finite cohort monetary field required')
        return float(value)
    return _typed(value,key)

def amounts(receipt,multiplier):
    out={}
    for m in receipt['metrics']:
        for name in ('pv','dv01'):out[(m['scenario_id'],f'date/{m["ordinal"]}/{name}')]=m[name]
    def add(scene,key,value):out[(scene,key)]=out.get((scene,key),0.)+value
    for s in receipt['sources']:
        if s['role']!='physical':continue
        scene=s['scenario_id'];w=s['weight']
        for name in ('principal','interest','fees'):
            add(scene,'source_full_cash/'+name,w*math.fsum(c[name] for c in s['cash']))
        add(scene,'source_full_gross_interest',w*math.fsum(c['contractual_earned'] for c in s['reference']['earned_origin_contributions']))
        add(scene,'source_full_effective_interest',-w*math.fsum(p['posting']['amount'] for g in s['groups'] for p in g['postings'] if p['posting']['account']=='interest_income'))
        for metric,field in [('principal','principal'),('principal_loss','economic_principal_loss')]:
            add(scene,'source_full_default/'+metric,w*math.fsum(p['default'][field] for p in s['reference']['credit']['result']['paths'] if p['default']))
    return {key:value*multiplier for key,value in out.items()}

def verify(raw,result,check=lambda:False):
    check()
    if raw.get('schema')!='graph-cohort-audit-1' or result.get('schema')!=raw['schema'] or not exact_json_identity(typed(raw),result['raw_source']):raise ArithmeticError('commercial cohort raw source identity differs')
    if any(row['graph']['kind']!='commercial' for c in raw['cohorts'] for row in [c['representative'],*c['members']]):raise ArithmeticError('independent commercial cohort publication requires exclusively commercial rows')
    rows=[];errors=[]
    for c in raw['cohorts']:
        value_maps=[];tolerances={v['metric']:v for v in c['tolerances']}
        for ri,row in enumerate([c['representative'],*c['members']]):
            check();actual=result['rows'][len(rows)];g=row['graph']['request'];receipt=verify_graph(g,actual['native_result'],check);values=amounts(receipt,row['amount_multiplier']);value_maps.append(values)
            expected=dict(cohort_id=c['cohort_id'],row_id=row['row_id'],role='representative' if ri==0 else 'member',product='commercial',currency=g['currency'],amount_multiplier=float(row['amount_multiplier']),amounts=[dict(scenario_id=scene,metric=metric,amount=value) for (scene,metric),value in sorted(values.items())])
            compare(expected,{k:actual[k] for k in expected},check=check);rows.append(expected)
        representative=value_maps[0]
        if any(set(v)!=set(representative) for v in value_maps[1:]):raise ArithmeticError('commercial cohort metric coverage differs')
        if set(tolerances)!={metric for _,metric in representative}:raise ArithmeticError('commercial cohort exact tolerance coverage required')
        for (scene,metric),value in sorted(representative.items()):
            members=math.fsum(v[(scene,metric)] for v in value_maps[1:]);signed=value-members;absolute=abs(signed);t=tolerances[metric];allowed=t['absolute_amount']+t['relative']*abs(members)
            errors.append(dict(cohort_id=c['cohort_id'],product='commercial',currency=c['representative']['graph']['request']['currency'],scenario_id=scene,metric=metric,representative=value,summed_members=members,signed_error=signed,absolute_error=absolute,relative_error=None if members==0. else absolute/abs(members),absolute_tolerance=float(t['absolute_amount']),relative_tolerance=float(t['relative']),allowed_error=allowed,passes=absolute<=allowed))
    if len(rows)!=len(result['rows']):raise ArithmeticError('commercial cohort row coverage differs')
    compare(errors,result['errors'],check=check)
    if result.get('all_metrics_within_tolerance') is not all(e['passes'] for e in errors):raise ArithmeticError('commercial cohort tolerance receipt differs')
    for name,flag in [('diagnostic_only',True),('empirical_validation',False),('cohort_publication_allowed',False)]:
        if result.get(name)is not flag:raise ArithmeticError('commercial cohort adoption eligibility differs')
    return dict(verified=True,commercial_raw_sources_recomputed=True,representative_member_amounts_recomputed=True,full_effective_income_and_cash_recomputed=True,empirical_validation=False,diagnostic_only=True)
