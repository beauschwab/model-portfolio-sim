"""Independent complete source2 histories -> funding; never a production backend."""
from copy import deepcopy
from . import deposit_portfolio_reference as portfolio
from . import _deposit_portfolio_principal_reference as primitive
from . import _modeled_product_dto_identity as declared

def checked(raw,actual,check,common):
    from .deposit_portfolio_transport import SCHEMAS,JSON_CELLS
    typed=declared.deposit_portfolio(raw)
    common.exact(actual['native']['raw_request'],typed,'exact nested portfolio typed raw clone')
    messages=actual['ledger_messages'];header=messages[0]['workflow']
    common.exact(header['schema'],'deposit-performing-portfolio-workflow-2','source2 workflow schema')
    common.exact(header['model_schema'],portfolio.SCHEMA,'source2 model schema')
    common.exact(header['source_receipt'],actual['native'],'source2 receipt clone')
    common.exact(header['prepared_request'],actual['prepared_request'],'source2 prepared clone')
    for owner in typed['positions']:
        position=next(i for i,p in enumerate(actual['prepared_request']['specification']['positions'])if p['id']==owner['position_id'])
        emitted=next(o for o in actual['native']['owners']if o['position_index']==position)
        for run in typed['runs']:
            raw_path=next(p['source']for p in owner['paths']if p['run_id']==run['run_id'])
            source=next(p['native']['native']for p in emitted['paths']if p['run_id']==run['run_id'])
            common.exact(source['raw_request'],raw_path,'exact original source2 typed source clone')
            original=raw_path['continuation'];anchor=original['anchor']
            for name in ('original','current'):
                request=source[name]['income_request']
                for field,want in (('contract',original['contract']),('original_yield_anchor',anchor['original_yield_anchor']),('original_annual_effective_rate',anchor['original_annual_effective_rate']),('opening_annual_effective_rate',anchor['original_annual_effective_rate'])):
                    common.exact(request[field],want,'immutable generated income anchor/'+field)
    control=portfolio.verify_preparation(typed,header['prepared_request'],header['source_receipt'],check)
    # Source2 JSON columns already have explicit names in the transport schema.
    import json
    converted=deepcopy(messages[1:])
    for frame in converted:
        for row in frame.get('rows',[]):
            for field in JSON_CELLS.get(frame.get('table'),()):row[field]=json.dumps(row[field],separators=(',',':'),allow_nan=False)
    facade=common.Frames(converted,SCHEMAS,{})
    facade.schemas={name:{k:str(v)for k,v in schema.items()}for name,schema in SCHEMAS.items()}
    portfolio.verify_stream(typed,control,facade,check)
    return {'sources':{(oi,ri):primitive.reconstruct(next(p for p in owner['paths']if p['run_id']==run['run_id'])['source'],check)for oi,owner in enumerate(raw['positions'])for ri,run in enumerate(raw['runs'])},'admission':portfolio.portfolio_admission(raw)}

def projected(raw,checked,review,check,common):
    oi,ri,pi=(review[k]for k in ('owner_index','run_index','path_index'))
    if pi!=0:raise ArithmeticError('complete supplied owner/run path_index0 required')
    owner=raw['positions'][oi];run=raw['runs'][ri]
    request=next(p for p in owner['paths']if p['run_id']==run['run_id'])['source']['continuation']
    native=checked['sources'][oi,ri];current=native['current'];opening=current['income_request']
    start=request['original']['as_of_ordinal'];end=start+request['original']['horizon_days'];key=f'deposit-portfolio:{oi}:{ri}'
    states=[];cash=[];earned=[]
    s=common.empty(start,key+':original-opening');common.put(s,'principal',0,opening['opening_principal']);common.put(s,'basis',8,opening['opening_gross_carrying_value']-opening['opening_principal']);states.append(s)
    allocations=portfolio.fifo(current['source'],check)
    for i,row in enumerate(current['income']['rows']):
        common.probe(check);d=row['payment_ordinal'];s=common.empty(d,f'{key}:source-owned-income:{i}');common.put(s,'principal',0,row['closing_principal']);common.right(s,'coupon',row['closing_accrued_interest'],5,4);common.put(s,'basis',8,row['closing_book_adjustment'])
        # Deliberately independent repeated scan, unlike producer's indexed cursors.
        for ii,lot in enumerate(current['source']['interest']):
            if lot['end_ordinal']>d:continue
            remaining=lot['amount']
            for cf,(_,alloc,_) in zip(current['source']['cashflows'],allocations,strict=True):
                if cf['ordinal']<=d:
                    for a in alloc:
                        if a['native_interest_index']==ii:remaining-=a['amount']
            common.put(s,f'earned-origin:{ii}:"{lot["segment_id"]}"',13,remaining)
        states.append(s);earned.append((d,f'{key}:source-owned-income:{i}','original_eir',-row['effective_interest']))
    for i,cf in enumerate(current['source']['cashflows']):cash.append((cf['ordinal'],f'{key}:cash:{i}:"{cf["event_id"]}"',cf['principal']+cf['cash_interest']))
    earned.append((request['revision_ordinal'],key+':later-revision','deposit:revision',native['basis_revision']))
    return dict(owner=dict(unit_face=float(request['anchor']['original_principal']),allocated_face=float(request['anchor']['original_principal'])),run=dict(scenario_id=run['scenario_id'],ledger_run_id=run['run_id']),path=dict(path_id=owner['position_id'],source_weight=1.),start=start,end=end,identity=request['contract'],anchor=request['anchor']['original_yield_anchor'],side='liability',currency=request['original']['identity']['currency'],states=states,cash=cash,earned=earned)

def states(raw):
    return sum((next(p for p in source['input']['request']['positions'][r['owner_index']]['paths']if p['run_id']==source['input']['request']['runs'][r['run_index']]['run_id'])['source']['continuation']['original']['horizon_days']+1)if source['input']['kind']=='deposit_portfolio'else 65 for r in raw['reviews']for source in raw['sources']if source['source_id']==r['source_id'])

def capacity(raw):
    kinds={s['source_id']:s['input']['kind']for s in raw['sources']}
    from ._deposit_continuation_income_reference import native_json_length
    return sum(16*1024*1024 if kinds[r['source_id']]=='deposit_portfolio'else 2*1024*1024 for r in raw['reviews'])+sum(native_json_length(declared.deposit_portfolio(s['input']['request']))*10 for s in raw['sources']if s['input']['kind']=='deposit_portfolio')


def conversion(raw):
    from ._deposit_continuation_income_reference import native_json_length
    total=states(raw)*18*256+sum(native_json_length(declared.deposit_portfolio(s['input']['request']))*16 for s in raw['sources']if s['input']['kind']=='deposit_portfolio')
    for review in raw['reviews']:
        source=next(s for s in raw['sources']if s['source_id']==review['source_id'])
        if source['input']['kind']=='deposit_portfolio':
            r=source['input']['request'];path=next(p for p in r['positions'][review['owner_index']]['paths']if p['run_id']==r['runs'][review['run_index']]['run_id'])
            days=path['source']['continuation']['original']['horizon_days'];n=days+1
            total+=n*(days+3)*96+n*len(raw['reviews'])*64
    return total
