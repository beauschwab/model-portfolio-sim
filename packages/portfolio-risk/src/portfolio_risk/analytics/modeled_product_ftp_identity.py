"""Exact declared raw DTO clones; no finance or native execution callbacks."""
from .modeled_json_identity import exact_json_identity

def exact(a,b,label):
    if not exact_json_identity(a,b):raise ValueError(label)

from ._modeled_product_dto_identity import mixed,performing,units,owners,primitive

def typed_mixed_clone(value,key=''):
    if value.get('schema')=='product-mixed-product-credit-1':return mixed(value)
    if value.get('schema')=='product-mixed-dated-1':return performing(value)
    if value.get('schema')=='product-units-1':return units(value)
    raise ValueError('explicit mixed DTO schema required')

def mixed_clones(raw,native):
    exact(native['preparation']['raw_request'],typed_mixed_clone(raw),'Card2 retained typed raw clone differs')
    exact(native['preparation']['product_request']['owners'],owners(raw['product_owners']),'compiled raw product-owner clone differs')
    p=native['preparation']['performing_preparation']
    exact(p['raw_request'],typed_mixed_clone(raw['performing']),'performing retained raw clone differs')
    exact(p['native_preparation']['preparation']['raw_source'],typed_mixed_clone(raw['performing']['candidate']['product']),'product-unit retained raw source differs')
    for source in native['product_sources']:
        oi,ri,pi=(source[k]for k in ('owner_index','run_index','path_index'))
        if any(type(i)is not int or i<0 for i in (oi,ri,pi)):raise ValueError('Card2 source integer mapping domain')
        original=raw['product_owners'][oi]['runs'][ri]['paths'][pi]['request']['request']
        kind=raw['product_owners'][oi]['runs'][ri]['paths'][pi]['request']['kind']
        exact(source['native']['kind'],kind,'product source native tag differs')
        normalized=primitive(original,kind)
        exact(source['native']['result']['raw_request'],normalized,'product retained primitive raw clone differs')
        result=source['native']['result']
        if kind=='card_v2':
            anchor=normalized['review']
            requests=[s['request']for s in result['income_steps']]
            identities=dict(contract=anchor['economic_source'],original_yield_anchor=anchor['original_yield_anchor'],policy=anchor['identity'])
        else:
            anchor=normalized['acquisition'];requests=[result['income_prefix_request']]
            identity=normalized['contract']['identity']
            identities=dict(contract=dict(id=identity['original_loan_id'],revision=identity['source_revision']),original_yield_anchor=anchor['original_yield_anchor'],policy=anchor['income_review'])
        identities.update(original_annual_effective_rate=anchor['original_annual_effective_rate'],opening_annual_effective_rate=anchor['original_annual_effective_rate'])
        for request in requests:
            for k,v in identities.items():exact(request[k],v,'original income request immutable '+k)

