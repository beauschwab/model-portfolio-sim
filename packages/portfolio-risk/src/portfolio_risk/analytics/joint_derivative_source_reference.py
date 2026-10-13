"""Independent complete saved/accounting source reconstruction for a joint GL."""
from collections import defaultdict
from copy import deepcopy
from .saved_derivative_mapping_reference import exact,identity,require,verify_with_check
from .saved_derivative_financial_reference import checked_derive
from .saved_derivative_mixed_reference import opening_lines,posting,book_key,LIABILITIES
from .saved_derivative_identity import probe
from .saved_derivative_dto_reference import typed,saved_dto

def reconstruct(owners,native_sources,spec,asof,check=lambda:False):
    require(type(owners)is list and 1<=len(owners)<=64,'bounded derivative owners')
    accounts={a['id']:a for a in spec['accounts']};scenarios=['baseline']+[s['name']for s in spec['scenarios']]
    flat=[(o,r)for o in owners for r in o['runs']]
    require(len(flat)==len(native_sources),'complete derivative native source cardinality')
    books=set();netting=set();result=[];by_scene={s:[]for s in scenarios}
    for owner in owners:
        require(set(owner)=={'account_id','runs'} and owner['account_id']in accounts,'exact derivative account owner')
        require(type(owner['runs'])is list and 0<len(owner['runs'])<=64,'bounded derivative run coverage')
        first=owner['runs'][0]['saved'];key=book_key(first['book']);require(key not in books,'duplicate saved book');books.add(key)
        require(len(owner['runs'])==len(scenarios)and {r['ledger_scenario']for r in owner['runs']}==set(scenarios),'exact full derivative baseline/scenario coverage')
        mapping=first['physical']['derivative']['operation']['request']['mapping']
        for legal in {(owner['account_id'],m['counterparty'],m['netting_set_id'])for m in mapping if m['enforceable_netting']}:
            require(legal not in netting,'split legal netting bucket');netting.add(legal)
        for run in owner['runs']:
            require(set(run)=={'ledger_scenario','saved'},'unknown derivative run fields')
            exact(typed(run['saved']['book']),typed(first['book']))
            exact(typed(run['saved']['physical']['opening']),typed(first['physical']['opening']))
    for (owner,run),native in zip(flat,native_sources):
        probe(check);saved=run['saved'];scenario=run['ledger_scenario'];account=owner['account_id']
        verify_with_check(saved,{'original_request':saved,'result':native},check)
        expected=checked_derive(saved_dto(saved)['physical'],check)
        # Same raw economics, independently recomputed; immutable headers are exact.
        for field in ('run','measure','basis','entity','currency','monetary_unit','policy','opening'):
            exact(native['source'][field],expected[field])
        for field in ('policy','conditional_model','crn','legal_mapping'):
            exact(native['source']['native'][field],expected['native'][field])
        require(len(native['source']['native']['paths'])==len(expected['native']['paths']),'physical source path cardinality')
        for got,want in zip(native['source']['native']['paths'],expected['native']['paths']):
            for field in ('path_id','probability','source','closing_legal_review','mpor_policy','replacement_value_policy'):exact(got[field],want[field])
        a=accounts[account]
        require(a['entity']==expected['entity']and a['currency']==expected['currency']and asof==expected['opening']['ordinal'],'source date/entity/currency mismatch')
        exact(float(a['cash']),float(expected['opening']['cash']))
        lines=opening_lines(saved,expected);opening=[];stocks=[]
        for gl,instrument,value in lines:
            stock=gl not in {'cash','opening_equity'}and not gl.startswith('memo:')
            opening.append(dict(scenario=scenario,account=account,book_key=book_key(saved['book']),ordinal=asof,gl_account=gl,instrument_id=instrument,debit_amount=float(value),posted_in_shared_opening=stock,custody_memo=gl.startswith('memo:')))
            if stock:stocks.append((gl,instrument,float(value)))
        groups=[]
        for index,intent in enumerate(expected['intents']):
            rows=[];mapped=[]
            for posting_index,p in enumerate(intent['postings']):
                gl,instrument,value=posting(book_key(saved['book']),p['account'],p['debit_amount'],p['position_id'],p['bucket'])
                mapped.append((gl,instrument,float(value)))
                rows.append(dict(scenario=scenario,account=account,book_key=book_key(saved['book']),source_event_id=intent['event_id'],source_intent_index=index,source_posting_index=posting_index,ordinal=intent['ordinal'],day=intent['ordinal']-asof,gl_account=gl,instrument_id=instrument,debit_amount=float(value)))
            require(asof<=intent['ordinal']<=asof+spec['horizon_days'],'complete derivative action tail')
            groups.append(dict(ordinal=intent['ordinal'],memo=intent['ledger']=='custody_memo',lines=mapped,lineage=rows))
        context=dict(account=account,scenario=scenario,opening=lines,opening_lineage=opening,stocks=stocks,groups=groups,source=expected)
        by_scene[scenario].append(context);result.append(context)
    return by_scene,result

def gross_state(book):
    assets=liabilities=0.
    for (gl,_),value in book.items():
        if gl in {'cash','opening_equity'}or gl.startswith(('pnl:','memo:')):continue
        if gl in LIABILITIES:liabilities-=value
        else:assets+=value
    return assets,liabilities
