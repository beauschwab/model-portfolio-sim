"""Independent immutable Physical source catalogs and observed draw state.

No native producer rates, cash, policy coefficients or monetary checkpoints enter
these constructors. Explicit path keys and original anchors are never remapped.
"""
from copy import deepcopy
from .commercial_dto import typed

SCHEMA='commercial-generated-physical-path-inputs-1'

def contracts(raw):
    yield None,raw['source']['current']
    for draw in raw.get('performing_draw_tranches',[]):yield draw['review']['id'],draw['source']['current']

def loading_contract(raw,loading):
    target=loading.get('draw_tranche_id')
    try:return next(contract for tranche,contract in contracts(raw)if tranche==target)
    except StopIteration as exc:raise ArithmeticError('generated draw loading source absent')from exc

def feature_bound(r):
    return 2*len(r['drivers'])+4+sum((2 if tranche is not None else 0)+sum(len(p['coupon'].get('observations',[]))+2 for p in contract['periods']) for tranche,contract in contracts(r['source']))

def append_observed(raw,point,date,opening,features):
    for tranche,contract in contracts(raw):
        acquired=next((d for d in point.get('draw_tranches',[])if d['tranche_id']==tranche),None)
        known=tranche is None or acquired is not None
        if tranche is not None:features.extend([float(known),acquired['nominal_draw']/opening if acquired else 0.])
        for period in contract['periods']:
            features.extend(o['rate'] if known and o['fixing_ordinal']<=date['ordinal'] and o['selection_cutoff_unix_seconds']<=(date['ordinal']-719163)*86400 else 0. for o in period['coupon'].get('observations',[]))
        rights={p['coupon_id']:p for p in point.get('canonical_legal_coupons',[])}
        for period in contract['periods']:
            observed=period['start_ordinal']<=date['ordinal'] or period['payment_ordinal']<date['ordinal'] or period['payment_ordinal']==date['ordinal'] and date['timing']=='after_scheduled_cash'
            coupon=rights.get(period['period_id']) if known and observed else None
            features.extend([coupon['opening_entitlement_principal']/opening,coupon['legal_remaining']/opening]if coupon else[0.,0.])

def _id(value):
    if type(value)is not dict or set(value)!={'id','revision'} or any(type(v)is not str or not v or len(v.encode())>256 or any(ord(c)<32 or 127<=ord(c)<=159 for c in v)for v in value.values()):raise ArithmeticError('reviewed catalog identity required')

def _contract_shape(contract,origin):
    selected={k:deepcopy(contract[k])for k in ('contract','position_id','currency','monetary_unit','asof_ordinal','opening_principal','principal_plan','periods')}
    for period in selected['periods']:
        for o in period['coupon'].get('observations',[]):
            if o['fixing_ordinal']>origin and o['source']['kind']=='conditional_projection':
                o['rate']=0.;o['source']=dict(kind='conditional_projection',model=dict(id='native-contractual-shape',revision='1'),path=dict(id='native-contractual-shape',revision='1'),information_unix_seconds=o['selection_cutoff_unix_seconds'],measure='physical')
    return typed(selected)

def same_draw_catalog(a,b):
    left=a.get('performing_draw_tranches',[]);right=b.get('performing_draw_tranches',[])
    if len(left)!=len(right):raise ArithmeticError('canonical Q/P draw catalog length differs')
    origin=a['source']['current']['asof_ordinal']
    for left,right in zip(left,right,strict=True):
        if any(typed(left[k])!=typed(right[k])for k in ('review','draw_ordinal','eligible_origins')) or typed(left['source']['acquisition'])!=typed(right['source']['acquisition']) or _contract_shape(left['source']['current'],origin)!=_contract_shape(right['source']['current'],origin):raise ArithmeticError('canonical Q/P draw source/feature/acquisition inventory differs')

def source(law,inputs,index,original=False):
    raw=deepcopy(law['source']);raw['credit']['paths'][0].update(path_key=index+1,path_id=f'generated-{index}')
    domain=law['evaluation']['randomness_domain'];raw['credit']['draw_stream']=dict(kind='counter_splitmix64_v1',stream_id=domain['id'],stream_revision=domain['revision'],seed=law['evaluation']['credit_seed'])
    if inputs is not None:
        path=inputs['paths'][index]
        if path['path_key']!=index+1:raise ArithmeticError('Physical exact path order differs')
        raw['performing_draw_tranches']=deepcopy(inputs['source_recipes'][path['source_recipe_index']]['performing_draw_tranches'])
        if path.get('reviewed_credit_actions')is None:raw.pop('reviewed_credit_actions',None)
        else:raw['reviewed_credit_actions']=deepcopy(path['reviewed_credit_actions'])
    if original:
        raw['source']['current']=deepcopy(raw['source']['original'])
        for draw in raw.get('performing_draw_tranches',[]):draw['source']['current']=deepcopy(draw['source']['original'])
    return typed(raw)

def _role(law,inputs,check):
    if set(inputs)!={'source_recipes','paths'} or len(inputs['paths'])!=law['evaluation']['paths']or not 1<=len(inputs['source_recipes'])<=512:raise ArithmeticError('complete bounded Physical input coverage required')
    ids=set();used=set()
    for recipe in inputs['source_recipes']:
        if set(recipe)!={'identity','performing_draw_tranches'}:raise ArithmeticError('Physical source recipe fields differ')
        _id(recipe['identity']);key=(recipe['identity']['id'],recipe['identity']['revision'])
        if key in ids:raise ArithmeticError('duplicate Physical recipe identity')
        ids.add(key)
    ids=set()
    for index,path in enumerate(inputs['paths']):
        check();_id(path['review']);key=(path['review']['id'],path['review']['revision'])
        if not {'path_key','review','source_recipe_index'}<=set(path) or set(path)-{'path_key','review','source_recipe_index','reviewed_credit_actions'} or type(path['path_key'])is not int or path['path_key']!=index+1 or key in ids or type(path['source_recipe_index'])is not int or not 0<=path['source_recipe_index']<len(inputs['source_recipes']):raise ArithmeticError('exact unique Physical path/source coverage required')
        ids.add(key);used.add(path['source_recipe_index']);raw=source(law,inputs,index);same_draw_catalog(law['source'],raw)
        actions=raw.get('reviewed_credit_actions')
        if actions is not None:
            for kind in ('allowance_reviews','chargeoffs','legal_waivers'):
                if any(type(row['path_key'])is not int or row['path_key']!=index+1 for row in actions.get(kind,[])):raise ArithmeticError('reviewed action exact generated path differs')
    if used!=set(range(len(inputs['source_recipes']))):raise ArithmeticError('unused Physical source recipe')

def _anchors(raw):
    return typed([dict(review=d['review'],draw_ordinal=d['draw_ordinal'],eligible_origins=d['eligible_origins'],original=d['source']['original'],acquisition=d['source']['acquisition'])for d in raw.get('performing_draw_tranches',[])])

def validate(g,check=lambda:False):
    for law in [g['original_physical'],*g['current_physical'],*g['current_pricing']]:
        q=g['pricing_training']
        if law['regression_policy']!=q['regression_policy'] or law['policy']!=q['policy'] or law['time_ordinals']!=q['time_ordinals'] or law['coupon_loadings']!=q['coupon_loadings'] or [(d['driver_id'],d['observation_policy'])for d in law['drivers']]!=[(d['driver_id'],d['observation_policy'])for d in q['drivers']]:raise ArithmeticError('canonical Q/P policy basis and observable inventory differ')
        if law['source'].get('reviewed_credit_actions')is not None:raise ArithmeticError('reviewed generated actions need explicit path inputs')
        same_draw_catalog(g['pricing_training']['source'],law['source'])
    for law in g['current_pricing']:same_draw_catalog(g['pricing_training']['source'],law['source'])
    original=deepcopy(g['original_physical']['source'])
    for draw in original.get('performing_draw_tranches',[]):draw['source']['current']=deepcopy(draw['source']['original'])
    same_draw_catalog(g['original_physical']['source'],original)
    inputs=g.get('physical_path_inputs')
    if inputs is None:return
    if set(inputs)!={'schema','identity','original_physical','current_physical'}or inputs['schema']!=SCHEMA or len(inputs['current_physical'])!=len(g['current_physical']):raise ArithmeticError('versioned complete Physical path schema required')
    _id(inputs['identity']);_role(g['original_physical'],inputs['original_physical'],check);seen=set()
    for scene,law in zip(inputs['current_physical'],g['current_physical'],strict=True):
        if set(scene)!={'scenario_id','inputs'}or scene['scenario_id']!=law['source']['source']['current']['scenario_id']or scene['scenario_id']in seen:raise ArithmeticError('Physical path scenario order differs')
        seen.add(scene['scenario_id']);_role(law,scene['inputs'],check)
        for index in range(law['evaluation']['paths']):
            if _anchors(source(g['original_physical'],inputs['original_physical'],index,True))!=_anchors(source(law,scene['inputs'],index)):raise ArithmeticError('current exact path original tranche anchors changed')

def current(g,law):
    inputs=g.get('physical_path_inputs')
    if inputs is None:return None
    try:return next(p['inputs']for p in inputs['current_physical']if p['scenario_id']==law['source']['source']['current']['scenario_id'])
    except StopIteration as exc:raise ArithmeticError('Physical source role absent')from exc
