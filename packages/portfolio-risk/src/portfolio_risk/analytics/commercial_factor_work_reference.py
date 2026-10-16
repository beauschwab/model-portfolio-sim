"""Independent algorithmic source-work census; no monetary source generation.

Counts byte scans, period/observation/segment loops, worst-case ordering/identity
comparisons and each earned/canonical/pending/terminal query family separately.
The selector never accepts numeric allowances.
"""
from copy import deepcopy
import json
POLICY='counted_native_contract_factor_work-1'

def structural(value):
    if type(value)in(int,float):return 32
    if isinstance(value,list):return 2+len(value)+sum(map(structural,value))
    if isinstance(value,dict):return 2+2*len(value)+sum(len(json.dumps(k,ensure_ascii=False).encode())+structural(v)for k,v in value.items())
    return len(json.dumps(value,ensure_ascii=False,separators=(',',':')).encode())

def _metadata(raw):
    value=deepcopy(raw)
    def identity(v):
        for k in ('id','revision'):
            if k in v:v[k]='\\'*128
    identity(value['path'])
    for p in value['periods']:
        for o in p['coupon'].get('observations',[]):
            for name in ('model','path'):
                if name in o['source']:identity(o['source'][name])
    return structural(value)

def _scan(start,end,basis):
    span=end-start
    if not 0<=span<=36600:raise ArithmeticError('counted calendar source domain exceeded')
    return (span+364)//365+2 if basis=='act_act_isda' else 1

def summary(raw,queries,observable=False,terminal=False):
    if type(queries)is not int or not 0<=queries<=4096:raise ArithmeticError('bounded native query count required')
    p=len(raw['periods']);s=o=binding=years=query_years=0
    if not 1<=p<=4096:raise ArithmeticError('bounded source period inventory required')
    for period in raw['periods']:
        basis=period['day_count'];years+=_scan(period['start_ordinal'],period['end_ordinal'],basis)
        observations=period['coupon'].get('observations')
        if observations is None:
            s+=1;query_years+=_scan(period['start_ordinal'],period['end_ordinal'],basis)+1
        else:
            o+=len(observations);s+=len(observations)
            if observable:binding+=len(observations)**2
            shift=period['coupon']['observation_shift']
            for obs in observations:
                interest=_scan(obs['interest_start_ordinal'],obs['interest_end_ordinal'],basis)
                observation=_scan(obs['observation_start_ordinal'],obs['observation_end_ordinal'],basis)
                query_years+=(2*interest+2*observation+2 if observable else interest+observation+2)if shift else interest+1
                years+=interest+observation
    if not 1<=s<=4096:raise ArithmeticError('bounded source segment inventory required')
    released=terminal and queries>0
    uses=dict(earned=queries,canonical=queries*(3 if observable else 1)+(s+1 if released else 0),pending=2*queries+68+66*s if observable and queries else 0,terminal=int(released))
    metadata=_metadata(raw)
    upper=dict(input_bytes=metadata,output_bytes=8*metadata+8192*(p+s+1),periods=p,observations=o,segments=s,
        calendar=years+((0 if observable else 2*uses['earned'])+uses['canonical'])*query_years,ordering=p*p,binding=2*p*p+2*p*s+binding,validation=4*p*p+32,
        earned_calls=uses['earned'],earned_steps=uses['earned']*(3*s+p),canonical_calls=uses['canonical'],canonical_steps=uses['canonical']*(p*s+s+p),
        pending_calls=uses['pending'],pending_steps=uses['pending']*s,terminal_calls=uses['terminal'],terminal_steps=uses['terminal']*(s*s+p*s))
    return dict(policy=POLICY,use_bound=uses,upper=upper)

def work(upper):
    return 16*upper['input_bytes']+upper['output_bytes']+256*upper['calendar']+128*sum(upper[k]for k in ('periods','observations','segments','ordering','binding','validation','earned_steps','canonical_steps','pending_steps','terminal_steps'))

def validate_generated(g):
    selected=g.get('physical_path_inputs',{}).get('source_work_policy')
    if selected is None:return
    if selected!=POLICY:raise ArithmeticError('unknown counted source-work policy')
    for law in [g['pricing_training'],g['original_physical'],*g['current_pricing'],*g['current_physical']]:
        raw=law['source'];observable=raw['factor_policy']['overnight']=='observable_index_prefix_simple_margin-1'
        for source in [raw['source'],*[d['source']for d in raw.get('performing_draw_tranches',[])]]:
            for role in ('original','current'):summary(source[role],0,observable,bool(raw['factor_policy'].get('terminal_obligations')))


def cash_event_bound(raw):
    contracts=[raw['source']['current'],*[d['source']['current']for d in raw.get('performing_draw_tranches',[])]]
    periods=sum(len(c['periods'])for c in contracts)
    segments=sum(len(p['coupon'].get('observations',[None]))for c in contracts for p in c['periods'])
    tails=max((len(t)for i in raw['credit']['intervals']for t in i['complete_recovery_tails']),default=0)
    return periods+len(raw.get('performing_draw_tranches',[]))+tails+4+(segments*(tails+1)if raw['factor_policy'].get('terminal_obligations')else 0)
