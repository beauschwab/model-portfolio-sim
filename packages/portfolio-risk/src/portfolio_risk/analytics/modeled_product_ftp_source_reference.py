"""Independent raw source/ordered GL and dated funding check; no native finance calls.

Existing explicitly independent installed source oracles are reused read-only.
Source arrays are checked before any topology receipt may be inspected. Funding
amounts below use freshly reconstructed source economics, not producer amounts.
"""
from copy import deepcopy
import json, math
import polars as pl
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as mortgage
from portfolio_risk.analytics import mortgage_mixed_reference as mortgage_stream
from portfolio_risk.analytics import product_event_reference as product
from portfolio_risk.analytics import product_rights_reference as card
from portfolio_risk.analytics import consumer_credit_reference as consumer
from portfolio_risk.analytics import card_credit_v2_reference as card_source
from portfolio_risk.analytics import _deposit_revision_calendar_reference as deposit
from portfolio_risk.analytics import _deposit_revision_source_reference as deposit_source
from portfolio_risk.analytics import mortgage_mixed_transport as mt
from portfolio_risk.analytics import deposit_revision_transport as dt
from portfolio_risk.analytics import deposit_continuation_reference as continuation
from portfolio_risk.analytics import _deposit_continuation_income_reference as continuation_types
from . import modeled_product_funding_reference as funding
from . import _modeled_product_deposit_portfolio_reference as portfolio_funding

SCHEMA='modeled-product-ftp-source-proof-1'
FLAGS=('independent_persisted_verification','ftp_bridge_adoption','capital_ftp_eligible','accounting_standard_approval','empirical_validation')
def probe(check):
    if check():raise InterruptedError('source proof cancelled')
def exact(a,b,label):mortgage.exact(a,b,label)
def near(a,b,label):mortgage.near(a,b,label)
def compare(a,b,label='computed'):
    if type(b) is dict:
        if type(a) is not dict or a.keys()!=b.keys():raise ArithmeticError(label+' fields')
        for k,v in b.items():compare(a[k],v,label+'/'+k)
    elif type(b) is list:
        if type(a) is not list or len(a)!=len(b):raise ArithmeticError(label+' rows')
        for i,(x,y) in enumerate(zip(a,b,strict=True)):compare(x,y,label+'/'+str(i))
    elif type(b) is float:near(a,b,label)
    else:exact(a,b,label)

class Frames:
    """Artifact in-memory facade only. Future public gates must reopen partitions."""
    def __init__(self,messages,schemas,json_fields):
        self.rows={};dictionary=[];self.completion=None
        for message in messages:
            if self.completion is not None:raise ArithmeticError('rows after completion')
            if 'complete' in message:
                exact(message,dict(complete=True,protocol=3),'actual completion');self.completion=message;continue
            if 'table' not in message:continue
            name=message['table']
            if name not in schemas:raise ArithmeticError('unknown actual source table')
            if name=='journal':
                dictionary+=message['dictionary'];cols=message['columns'];rows=[]
                for i in range(len(cols['value'])):
                    row={k:(dictionary[v[i]] if k in mortgage_stream.STR_COLUMNS else v[i]) for k,v in cols.items()}
                    value=row.pop('value');number=row.pop('transaction_number');row.update(transaction_id=row['scenario']+':'+str(number),debit=max(value,0.),credit=max(-value,0.));rows.append(row)
            else:rows=deepcopy(message['rows'])
            for row in rows:
                for key in json_fields.get(name,()):row[key+'_json']=json.dumps(row.pop(key),separators=(',',':'),allow_nan=False)
            self.rows.setdefault(name,[]).extend(rows)
        if self.completion is None:raise ArithmeticError('missing completion')
        self.schemas={name:{k:str(v) for k,v in schemas[name].items()} for name in self.rows}
        self.dtypes=schemas
    def frames(self,name):
        rows=self.rows.get(name,[])
        for at in range(0,len(rows),1024):yield pl.DataFrame(rows[at:at+1024],schema=self.dtypes[name])

def typed_deposit_spec(v,key=''):
    # Independent declared daily-spec monetary DTO fields. Original Value clones
    # stay exact at the outer authority boundary; neither native shape nor reported
    # hashes determine these types.
    floating={'cash','equity','cash_floor','cet1_floor','leverage_floor','balance','book_adjustment','opening_accrued','rate','annual_pd','allowance','commitment','monthly_runoff','floating_beta','migration_rate','draw_fraction','annual_fees','annual_costs','annual_dividends','tax_rate','risk_weight','liquidity_weight','asf_factor','rsf_factor','lgd','collateral_haircut','floating_spread','fixed_rate','spread'}
    if type(v)is dict:return {k:typed_deposit_spec(x,k) for k,x in v.items()}
    if type(v)is list:return [typed_deposit_spec(x,key) for x in v]
    if key in floating:
        if type(v)not in (int,float):raise ArithmeticError('declared monetary DTO')
        return float(v)
    return v

def checked_source(raw,actual,check):
    probe(check);kind=raw['input']['kind'];r=raw['input']['request']
    if kind=='mortgage':
        normalized,spec,events,bindings,admission=mortgage.admit(r,check)
        receipt=dict(schema='mortgage-mixed-preparation-1',raw_request=normalized,admission=actual['admission'],sources=actual['sources'],source_groups_sole_postings=True,memo_checkpoint_policy='native_complete_daily_boundary',**{k:False for k in mortgage_stream.PERMISSIONS})
        exact(actual['raw_request'],normalized,'exact normalized mortgage raw clone')
        control=mortgage_stream.verify_preparation(r,normalized['dated'],receipt,check)
        facade=Frames(actual['ledger_messages'],mt.SCHEMAS,mt.JSON_COLUMNS);facade.schemas=mt.SCHEMAS
        mortgage_stream.verify_stream(r,control,facade,check)
        sources={s['source_id']:mortgage.pending.reconstruct(s['request'],check) for s in r['sources']}
        return dict(sources=sources)
    if kind=='product':
        v=card if any(p['request']['kind']=='card_v2' for o in r['product_owners'] for run in o['runs'] for p in run['paths']) else product
        from .modeled_product_ftp_identity import mixed_clones
        mixed_clones(r,actual)
        v.verify(r,actual,check)
        # Source reconstruction remains raw-owned, even after complete GL checking.
        return {'sources':{(oi,ri,pi):(card_source.reconstruct(p['request']['request'],check) if p['request']['kind']=='card_v2' else consumer.reconstruct(p['request']['request'],check)) for oi,o in enumerate(r['product_owners']) for ri,run in enumerate(o['runs']) for pi,p in enumerate(run['paths'])}}
    if kind=='deposit_portfolio':return portfolio_funding.checked(r,actual,check,__import__(__name__,fromlist=["checked_source"]))
    if kind=='deposit_continuation':
        expected=continuation.reconstruct(r,check,include_tables=False)
        typed=deepcopy(r)
        for run,path in zip(typed['runs'],expected['source_receipt']['paths'],strict=True):
            run['source']=path['native']['raw_request'];run['severity']=1.
        exact(actual['native']['raw_request'],typed,'exact normalized continuation raw clone')
        messages=actual['ledger_messages'];header=messages[0]['workflow']
        exact(header['schema'],'model-ledger-preparation-1','workflow schema')
        exact(header['model_schema'],'deposit-performing-income-ledger-1','actual source kind')
        exact(header['source_receipt'],actual['native'],'retained source receipt')
        exact(header['prepared_request'],actual['prepared_request'],'retained prepared source')
        control=continuation.verify_preparation(typed,header['prepared_request'],header['source_receipt'],check)
        continuation.verify_stream(typed,control,messages[1:],check)
        return expected
    if kind=='deposit':
        typed_raw=deepcopy(r);typed_raw['source']=deposit_source.typed_source(r['source'])
        exact(actual['native']['raw_request'],typed_raw,'exact normalized revised deposit raw clone')
        messages=actual['ledger_messages'];header=messages[0]['workflow'];exact(header['prepared_request']['specification'],r['specification'],'exact original specification Value');typed=deepcopy(r);typed['source']=deposit_source.typed_source(r['source']);typed['specification']=typed_deposit_spec(r['specification']);prepared=deepcopy(header['prepared_request']);prepared['specification']=typed['specification'];workflow=dict(schema=r['schema'],raw_request=typed,prepared_request=prepared,source_receipt={k:v for k,v in header['source_receipt'].items() if k!='raw_request'})
        deposit.verify_stream(typed,workflow,messages[1:],check)
        return deposit.reconstruct(r,check)
    raise ArithmeticError('raw native source tag')

def empty(day,key):return dict(ordinal=day,source_checkpoint_id=key,channels=[dict(channel=k,amount=0.) for k in funding.LANES],origins=[])
def put(state,key,index,amount):
    state['channels'][index]['amount']+=amount
    state['origins'].append(dict(origin_id=key+':'+funding.LANES[index],source_event_id=state['source_checkpoint_id'],channel=funding.LANES[index],amount=float(amount)))
def right(s,key,amount,positive,negative):put(s,key,negative if amount<0. else positive,abs(amount))
def shared(s,core):
    for i,v in enumerate(core['live']):
        key='live:'+str(i);put(s,key,0,v['principal']);right(s,key,v['accrued_interest'],4,5);put(s,key,8,v['book_adjustment']);put(s,key,16,v['undrawn'])
    put(s,'allowance',17,core['allowance'])
    for c in core['claims']:
        key=c['claim_id'];put(s,key,3,c['outstanding_principal']);right(s,key,c['outstanding_accrued_interest'],6,7);put(s,key,10,c['outstanding_book_adjustment']);put(s,key,14,sum(x['amount'] for x in c['remaining_recoveries']))

def projected_path(source,checked,review,check):
    kind=source['input']['kind'];raw=source['input']['request'];oi,ri,pi=(review[k] for k in ('owner_index','run_index','path_index'));states=[];cash=[];earned=[]
    if kind=='mortgage':
        owner=raw['owners'][oi];run=owner['runs'][ri];path=run['paths'][pi];native=checked['sources'][path['source_id']]
        start=next(s for s in raw['sources'] if s['source_id']==path['source_id'])['request']['source']['contract']['as_of_ordinal'];end=native['daily'][-1]['ordinal']
        for b in native['daily']:
            d=b['ordinal'];s=empty(d,f"{path['source_id']}:daily:{d}")
            for i,c in enumerate(b['slots']):
                key=f'slot:{i}';put(s,key,0 if i<3 else 1 if i<6 else 2,c['principal']);right(s,key,c['recognized_interest'],4,5);put(s,key,8,c['basis']);put(s,key,12,b['borrower_principal'][i]);right(s,key+':borrower',b['borrower_contract_interest'][i],13,13)
            for c in b['claims']:
                key=c['claim_id'];put(s,key,3,c['book']['principal']);right(s,key,c['book']['recognized_interest'],6,7);put(s,key,10,c['book']['basis']);put(s,key,14,c['economic_principal']);right(s,key+':economic',c['economic_contract_interest'],15,15)
            states.append(s)
        for gi,g in enumerate(native['groups']):
            if g['kind']=='acquisition':continue
            for li,line in enumerate(g['lines']):
                event=f"group:{gi}:line:{li}:\"{g['source_id']}\"";a=line['account'];d=g['ordinal']
                if a=='cash':cash.append((d,event,line['amount']))
                elif a=='original_eir':earned.append((d,event,'original_eir',-line['amount']))
                elif a=='reviewed_income':
                    sourcekind=native['source']['actions'][line['source_action_index']]['kind'] if line['source_action_index'] is not None else g['kind'];earned.append((d,event,'mortgage:'+sourcekind,-line['amount']))
        anchor=owner['original_yield_anchor'];identity=owner['source_identity'];side='asset';currency=review['currency']
    elif kind=='product':
        owner=raw['product_owners'][oi];run=owner['runs'][ri];path=run['paths'][pi];tag=path['request']['kind'];native=checked['sources'][(oi,ri,pi)];contract=path['request']['request']['contract'];start=contract['as_of_ordinal'];end=start+contract['horizon_days']
        if tag=='installment':
            s=empty(start,'installment:acquisition');shared(s,native['acquisition_opening']);states.append(s)
            for b in native['daily']:
                if b['ordinal']==start:continue
                s=empty(b['ordinal'],f"installment:daily:{b['ordinal']}");shared(s,b['owned']);states.append(s)
            for ei,e in enumerate(native['intents']):
                for li,line in enumerate(e['postings']):
                    a=line['account'];event=f'installment:intent:{ei}:line:{li}';d=e['recognition_ordinal']
                    if a=='cash':cash.append((d,event,line['amount']))
                    elif a=='interest_income' and e['event']=='product_original_effective_interest':earned.append((d,event,'original_eir',-line['amount']))
                    elif a in ('interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'):earned.append((d,event,f"installment:{e['event']}:{a}",-line['amount']))
        elif tag=='card_v2':
            for b in native['daily']:
                snap=b['snapshot'];s=empty(b['ordinal'],f"card_v2:daily:{b['ordinal']}");shared(s,snap['core']);put(s,'live-Bi',9,snap['live_interest_basis'])
                for k,a in sorted(snap['claim_interest_basis'].items()):put(s,k,11,a)
                states.append(s)
            for ei,e in enumerate(native['events']):
                for li,line in enumerate(e['lines']):
                    a=line['account'];event=f'card_v2:event:{ei}:line:{li}';d=e['ordinal']
                    if a=='cash':cash.append((d,event,line['amount']))
                    elif a=='pnl:original_effective_interest':earned.append((d,event,'original_eir',-line['amount']))
                    elif a.startswith('pnl:') or a in ('interest_income','credit_loss_expense','basis_recognition_expense','recovery_income'):earned.append((d,event,'card_v2:'+a,-line['amount']))
        else:raise ArithmeticError('unsupported card1 ownership')
        anchor=owner['original_yield_anchor'];identity=owner['source_identity'];side='asset';currency=contract['identity']['currency']
    elif kind=='deposit_portfolio':
        projected=portfolio_funding.projected(raw,checked,review,check,__import__(__name__,fromlist=["projected_path"]))
        owner,run,path,start,end,identity,anchor,side,currency,states,cash,earned=(projected[k]for k in ('owner','run','path','start','end','identity','anchor','side','currency','states','cash','earned'))
    elif kind=='deposit_continuation':
        r=raw['runs'][ri]['source']['continuation'];receipt=checked['source_receipt']['paths'][ri];native=receipt['native']['current']
        owner=dict(unit_face=float(r['anchor']['original_principal']),allocated_face=float(r['anchor']['original_principal']))
        run=dict(scenario_id=raw['runs'][ri]['scenario_id'],ledger_run_id=raw['runs'][ri]['run_id']);path=dict(path_id='deposit',source_weight=1.)
        start=r['original']['as_of_ordinal'];end=start+r['original']['horizon_days'];identity=r['contract'];anchor=r['anchor']['original_yield_anchor'];side='liability';currency=r['original']['identity']['currency'];opening=native['income_request']
        s=empty(start,'deposit:original-opening');put(s,'principal',0,opening['opening_principal']);put(s,'basis',8,opening['opening_gross_carrying_value']-opening['opening_principal']);states.append(s)
        for i,row in enumerate(native['income']['rows']):
            probe(check);d=row['payment_ordinal'];s=empty(d,f'deposit:source-owned-income:{i}');put(s,'principal',0,row['closing_principal']);right(s,'coupon',row['closing_accrued_interest'],5,4);put(s,'basis',8,row['closing_book_adjustment'])
            for ii,lot in enumerate(native['source']['interest']):
                if lot['end_ordinal']>d:continue
                outstanding=lot['amount']
                for co in checked['source_receipt']['cash_origins']:
                    if co['run_id']!=raw['runs'][ri]['run_id']:continue
                    c=next(x for x in native['source']['cashflows']if x['event_id']==co['native_event_id'])
                    if c['ordinal']>d:continue
                    for allocation in co['allocations']:
                        if allocation['native_interest_index']==ii:outstanding-=allocation['amount']
                put(s,f'earned-origin:{ii}:"{lot["segment_id"]}"',13,outstanding)
            states.append(s);earned.append((d,f'deposit:source-owned-income:{i}','original_eir',-row['effective_interest']))
        for i,c in enumerate(native['source']['cashflows']):cash.append((c['ordinal'],f'deposit:cash:{i}:"{c["event_id"]}"',c['principal']+c['cash_interest']))
        earned.append((r['revision_ordinal'],'deposit:later-revision','deposit:revision',receipt['native']['basis_revision']))
    else:
        native=checked['source']['paths'][ri];owner=dict(unit_face=raw['source']['anchor']['original_principal'],allocated_face=raw['source']['anchor']['original_principal']);run=dict(scenario_id=native['scenario_id'],ledger_run_id=native['run_id']);path=dict(path_id='deposit',source_weight=1.);start=raw['source']['original']['as_of_ordinal'];end=start+raw['source']['original']['horizon_days'];identity=raw['source']['contract'];anchor=raw['source']['anchor']['original_yield_anchor'];side='liability';currency=raw['source']['original']['identity']['currency'];opening=native['income_request']
        s=empty(start,'deposit:revision');put(s,'principal',0,opening['opening_principal']);put(s,'basis',8,native['revised_carrying_value']-opening['opening_principal']);states.append(s)
        for i,row in enumerate(native['income']['rows']):
            d=row['payment_ordinal']
            if d==start:continue
            s=empty(d,f'deposit:income:{i}');put(s,'principal',0,row['closing_principal']);right(s,'coupon',row['closing_accrued_interest'],5,4);put(s,'basis',8,row['closing_book_adjustment'])
            for ii,lot in enumerate(native['source']['interest']):
                if lot['end_ordinal']>d:continue
                outstanding=lot['amount']
                for origin in checked['cash_origins']:
                    if origin['run_id']!=native['run_id']:continue
                    c=next(c for c in native['source']['cashflows'] if c['event_id']==origin['native_event_id'])
                    if c['ordinal']>d:continue
                    for allocation in origin['allocations']:
                        if allocation['native_interest_index']==ii:outstanding-=allocation['amount']
                put(s,f"earned-origin:{ii}:\"{lot['segment_id']}\"",13,outstanding)
            states.append(s);earned.append((d,f'deposit:income:{i}','original_eir',-row['effective_interest']))
        for i,c in enumerate(native['source']['cashflows']):cash.append((c['ordinal'],f"deposit:cash:{i}:\"{c['event_id']}\"",c['principal']+c['cash_interest']))
        earned.append((start,'deposit:day0-revision','deposit:revision',native['basis_revision']))
    exact(currency,review['currency'],'native currency');probe(check)
    balance=lambda s:sum(a['amount']*p['coefficient'] for a,p in zip(s['channels'],review['policy']['channels'],strict=True))
    changes=[dict(sequence=i+1,ordinal=b['ordinal'],source_event_id=b['source_checkpoint_id'],kind='reviewed_other_reallocation',amount=balance(b)-balance(a),cash_event_id=None,review=review['channel_change_review']) for i,(a,b) in enumerate(zip(states,states[1:]))]
    opening_income=[dict(channel=k,amount=0.) for k in funding.INCOME]
    events=[];used=set()
    for d,event,label,amount in earned:
        lane='original_eir' if label=='original_eir' else review['other_income'][label]
        if label!='original_eir':used.add(label)
        if d==start:next(x for x in opening_income if x['channel']==lane)['amount']+=amount
        else:events.append(dict(event_id=event,ordinal=d,channel=lane,bank_earnings=amount,review=review['income_review']))
    if set(review['other_income'])!=used:raise ArithmeticError('exact complete reviewed P&L classification')
    request=dict(schema='modeled-product-ftp-source-channels-2'if kind=='deposit_portfolio'else 'modeled-product-ftp-source-channels-1',measure='physical',source=identity,original_yield_anchor=anchor,path_id=path['path_id'],entity=review['entity'],currency_unit=currency,scenario_id=run['scenario_id'],side=side,start_ordinal=start,full_end_ordinal=end,source_weight=path['source_weight'],unit_face=owner['unit_face'],allocated_face=owner['allocated_face'],opening_funding_balance=balance(states[0]),policy=review['policy'],states=states,cash=[dict(event_id=e,ordinal=d,kind='source_atomic_cash',bank_cash=a,funding_change_sequence=None,exclusion_review=review['cash_exclusion_review']) for d,e,a in cash if d>start],changes=changes,earned=events,resets=review['resets'])
    return request,opening_income,sum(a for d,e,a in cash if d==start)

def preparation_bound(raw,check=lambda:False):
    total=0
    for review in raw['reviews']:
        probe(check);source=next(s for s in raw['sources'] if s['source_id']==review['source_id']);kind=source['input']['kind'];r=source['input']['request'];oi,ri,pi=(review[k] for k in ('owner_index','run_index','path_index'))
        if kind=='mortgage':
            path=r['owners'][oi]['runs'][ri]['paths'][pi];pending=next(s for s in r['sources'] if s['source_id']==path['source_id'])['request'];_,a=mortgage.pending.admit(pending,check);days=a['days'];origins=35+15*a['interval_count'];postings=4*a['max_groups']
        elif kind=='product':
            p=r['product_owners'][oi]['runs'][ri]['paths'][pi]['request'];q=p['request'];days=q['contract']['horizon_days']
            if p['kind']=='card_v2':a=card_source._admit(q,check);origins=19;postings=5*a['max_events']
            elif p['kind']=='installment':origins=17;postings=4*(2*days+6*(len(q['contract']['steps'])+len(q['chargeoffs'])+len(q['allowance_reviews']))+96)
            else:raise ArithmeticError('unsupported source channel ownership')
        elif kind=='deposit_portfolio':days=next(p for p in r['positions'][oi]['paths']if p['run_id']==r['runs'][ri]['run_id'])['source']['continuation']['original']['horizon_days'];origins=days+3;postings=6*days+8
        elif kind=='deposit_continuation':days=r['runs'][ri]['source']['continuation']['original']['horizon_days'];origins=days+3;postings=6*days+8
        else:days=r['source']['original']['horizon_days'];origins=days+3;postings=5*days
        if days>(365 if kind=='deposit_portfolio'else 64)or origins>(368 if kind=='deposit_portfolio'else 128):raise ArithmeticError('complete source/origin preparation bound')
        states=days+1;events=days+postings
        total+=states*18*16+events*states*20+states*origins*80+len(review['resets'])*16
    if total>(8000000 if any(s['input']['kind']=='deposit_portfolio'for s in raw['sources'])else 2000000):raise ArithmeticError('pre-generation preparation bound')
    return total

def binding(raw,review):
    kind=raw['input']['kind'];r=raw['input']['request'];oi,ri=review['owner_index'],review['run_index']
    if kind=='deposit_portfolio':return r['positions'][oi]['position_id'],r['positions'][oi]['account_id'],r['runs'][ri]['run_id'],r['specification']
    if kind=='deposit_continuation':return r['position_id'],r['account_id'],r['runs'][ri]['run_id'],r['specification']
    if kind=='deposit':return r['position_id'],r['account_id'],r['source']['runs'][ri]['run_id'],r['specification']
    owner=r['owners' if kind=='mortgage' else 'product_owners'][oi]
    return owner['position_id'],owner['account_id'],owner['runs'][ri]['ledger_run_id'],r['dated']['specification'] if kind=='mortgage' else r['combined_specification']

def raw_domain(raw):
    funding._keys(raw,'schema sources reviews')
    if raw['schema']!=SCHEMA or not 1<=len(raw['sources'])<=4 or not 1<=len(raw['reviews'])<=16:raise ArithmeticError('bounded source contract')
    portfolio_owners=set()
    for source in raw['sources']:
        if source['input']['kind']=='deposit_portfolio':
            r=source['input']['request']
            for owner in r['positions']:
                account=next(a for a in r['specification']['accounts']if a['id']==owner['account_id'])
                key=(account['entity'],account['currency'],owner['position_id'])
                if key in portfolio_owners:raise ArithmeticError('overlapping source2 economic owner across source jobs')
                portfolio_owners.add(key)
    ids=set()
    scopes={'mortgage':'exclusive_mortgage_owners_with_inherited_dated_sources_retained_not_funded-1','product':'exclusive_product_owners_with_inherited_dated_sources_retained_not_funded-1','deposit':'whole_isolated_deposit_owner-1','deposit_continuation':'whole_isolated_continuation_deposit_owner-1','deposit_portfolio':'whole_performing_deposit_portfolio-2'}
    for source in raw['sources']:
        funding._keys(source,'source_id scope scope_review input');funding._text(source['source_id']);funding._identity(source['scope_review']);funding._keys(source['input'],'kind request');kind=source['input']['kind']
        if kind not in scopes or source['scope']!=scopes[kind] or source['source_id'] in ids:raise ArithmeticError('explicit distinct source scope')
        ids.add(source['source_id'])
    for review in raw['reviews']:
        funding._keys(review,'source_id owner_index run_index path_index entity currency policy cash_assignment cash_exclusion_review channel_change_review other_income income_review resets')
        if review['source_id']not in ids or review['cash_assignment']!='actual_cash_retained_excluded_from_channel_rollforward-1':raise ArithmeticError('explicit reviewed source cash ownership')
        for k in ('owner_index','run_index','path_index'):funding._int(review[k],0,512)
        for k in ('entity','currency'):funding._text(review[k])
        for k in ('cash_exclusion_review','channel_change_review','income_review'):funding._identity(review[k])
        if type(review['other_income'])is not dict or any(type(k)is not str or v not in funding.INCOME[1:] for k,v in review['other_income'].items()):raise ArithmeticError('reviewed non-EIR income mapping')

def verify(raw,output,check=lambda:False):
    probe(check);raw_domain(raw);exact(output['raw_input'],raw,'exact original raw input');result=output['result'];exact(result['raw_request'],raw,'exact raw source/policy clone');exact(result['schema'],SCHEMA,'schema')
    for flag in FLAGS:exact(result[flag],False,flag)
    for flag in ('native_sources_rebuilt','source_journals_retained'):exact(result[flag],True,flag)
    exact([x['source_id'] for x in result['sources']],[x['source_id'] for x in raw['sources']],'complete source order')
    bound=preparation_bound(raw,check);exact(result['preparation_work_bound'],bound,'pre-generation preparation work');total_work=0
    checked={s['source_id']:checked_source(s,a['raw_native_result'],check) for s,a in zip(raw['sources'],result['sources'],strict=True)}
    expected_keys=[]
    for s in raw['sources']:
        r=s['input']['request'];kind=s['input']['kind']
        if kind=='deposit_portfolio':expected_keys.extend((s['source_id'],oi,ri,0)for oi,_ in enumerate(r['positions'])for ri,_ in enumerate(r['runs']))
        elif kind=='deposit_continuation':expected_keys.extend((s['source_id'],0,i,0) for i in range(len(r['runs'])))
        elif kind=='deposit':expected_keys.extend((s['source_id'],0,i,0) for i in range(len(r['source']['runs'])))
        else:expected_keys.extend((s['source_id'],oi,ri,pi) for oi,o in enumerate(r['owners' if kind=='mortgage' else 'product_owners']) for ri,run in enumerate(o['runs']) for pi,p in enumerate(run['paths']))
    keys=lambda xs:[tuple(x[k] for k in ('source_id','owner_index','run_index','path_index')) for x in xs]
    if set(keys(raw['reviews']))!=set(expected_keys) or len(keys(raw['reviews']))!=len(expected_keys):raise ArithmeticError('complete distinct unweighted coverage')
    exact(keys(result['paths']),expected_keys,'native source/path order')
    for path in result['paths']:
        probe(check);key=tuple(path[k] for k in ('source_id','owner_index','run_index','path_index'));review=next(v for v in raw['reviews'] if tuple(v[k] for k in ('source_id','owner_index','run_index','path_index'))==key);s=next(s for s in raw['sources'] if s['source_id']==path['source_id'])
        position,account,run,spec=binding(s,review);a=next(a for a in spec['accounts'] if a['id']==account)
        for k,v in (('position_id',position),('account_id',account),('ledger_run_id',run)):exact(path[k],v,'dated source binding/'+k)
        exact(review['entity'],a['entity'],'reviewed native entity');exact(review['currency'],a['currency'],'reviewed native currency')
        request,opening_income,opening_cash=projected_path(s,checked[s['source_id']],review,check);expected=funding.prepare(request,check,independent_source_residuals=True)
        total_work+=expected['admitted_work']
        compare(path['preparation'],expected,'dated funding');compare(path['opening_earnings'],opening_income,'all opening P&L lanes');near(path['opening_source_cash'],opening_cash,'opening actual source cash')
        for key in ('source_weight','unit_face','allocated_face','policy','resets','source','original_yield_anchor','entity','currency_unit','scenario_id','side','start_ordinal','full_end_ordinal'):exact(path['preparation']['raw_request'][key],funding._normalize(request)[key],'supplied typed identity/'+key)
    exact(result['preparation_work'],total_work,'actual preparation work');
    if total_work>bound:raise ArithmeticError('actual work exceeds raw bound')
    for source,receipt in zip(raw['sources'],result['sources'],strict=True):
        r=source['input']['request'];kind=source['input']['kind'];native=receipt['raw_native_result']
        if kind=='mortgage':
            *_,admission=mortgage.admit(r,check);exact(receipt['work_bound'],admission['work_units'],'independent raw mortgage admission');exact(receipt['capacity_bound'],admission['reserved_bytes'],'raw mortgage capacity')
        elif kind=='product':
            # Graph admission metadata is bounded and linked to the independently
            # verified native product receipt; it is not a financial coefficient.
            graph=native['preparation']['performing_preparation']['native_preparation']['preparation']['admitted_work']
            if type(graph)is not int or not 0<graph<=32000000:raise ArithmeticError('raw graph source budget')
            exact(receipt['work_bound'],8000000+graph,'charged full graph plus coordinator');exact(receipt['capacity_bound'],134217728,'coordinator source capacity')
        elif kind=='deposit_portfolio':
            a=checked[source['source_id']]['admission'];exact(receipt['work_bound'],a['work_units'],'complete portfolio native source reservation');exact(receipt['capacity_bound'],a['reserved_bytes'],'complete portfolio native capacity')
        elif kind=='deposit_continuation':
            a=checked[source['source_id']]['source_receipt']['admission'];exact(receipt['work_bound'],a['work_units'],'actual independently recomputed continuation admission');exact(receipt['capacity_bound'],a['reserved_bytes'],'actual continuation capacity')
        else:
            a=native['native']['admission'];n=len(r['source']['runs']);days=r['source']['original']['horizon_days'];w=a['native_source_work'];base=(n+1)*days*4096
            if type(w)is not int or w<=base or (w-base)%8:raise ArithmeticError('native source byte/work metadata')
            b=(w-base)//8
            if not 0<b<=1048576:raise ArithmeticError('native source byte metadata')
            exact(receipt['work_bound'],w+n*(days+1)*8208,'charged deposit whole replay');exact(receipt['capacity_bound'],(n+1)*(days+2)*32768+b*12+n*(days+1)*65536,'deposit replay capacity')
            if type(a['work_units'])is not int or not 0<a['work_units']<=receipt['work_bound']:raise ArithmeticError('deposit actual work within pre-generation upper bound')
    costs=sum(s['work_bound'] for s in result['sources']);conversion=portfolio_funding.conversion(raw)
    exact(result['source_work_bound'],costs+conversion,'charged conversion work');exact(result['joined_work_bound'],costs+conversion+bound,'joined source/preparation bound')
    if costs+conversion>48000000:raise ArithmeticError('source admission ceiling')
    for s in result['sources']:
        for k in ('work_bound','capacity_bound'):
            if type(s[k]) is not int or s[k]<=0:raise ArithmeticError('strict source admission metadata')
    byte_charge=result['normalized_raw_byte_charge']
    if type(byte_charge) is not int or not 0<byte_charge<=1048576:raise ArithmeticError('normalized byte metadata')
    capacity=sum(s['capacity_bound'] for s in result['sources'])+portfolio_funding.capacity(raw)+67108864+8*byte_charge
    exact(result['retained_capacity_bound'],capacity,'charged retained capacity')
    if capacity>268435456:raise ArithmeticError('retained capacity ceiling')
    probe(check)
    return dict(verified=True,native_raw_sources_independently_recomputed=True,ordered_source_journals_independently_recomputed=True,dated_funding_and_seven_income_lanes_recomputed=True,ftp_bridge_adoption=False,capital_ftp_eligible=False,empirical_validation=False)
