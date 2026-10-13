"""Raw histories -> independent native-owned sources, FIFO origins and one GL.

No producer cash/transition/income is authority. This independent publication gate is not a
financial execution backend or empirical fit.
"""
from copy import deepcopy
from collections import defaultdict
from dataclasses import fields
import hashlib,json,math,struct
from . import _deposit_source_principal_reference as source
from . import _deposit_continuation_income_reference as c
from ._deposit_continuation_calendar import _normalize,_backbook,_rights,_cumulative,Account,Position,Scenario
from ._deposit_continuation_replay import _replay
SCHEMA='deposit-performing-income-ledger-1'

def _spec(raw):
    spec=_normalize(raw)
    # DTO-declared financial f64 inputs may be integer JSON. Retained raw is
    # untouched; the independently validated numeric DTO has explicit f64 fields.
    for group,cls in [('accounts',Account),('positions',Position),('scenarios',Scenario)]:
        for row in spec[group]:
            for field in fields(cls):
                if field.type is float:row[field.name]=float(row[field.name])
    return spec

def _fifo(physical,check):
    remaining=[x['amount'] for x in physical['interest']];out=[]
    for cash in physical['cashflows']:
        c.tick(check);need=-cash['cash_interest'];segments=[];allocations=[]
        full=cash['kind'] in ('interest_payment','contractual_maturity')
        for i,origin in enumerate(physical['interest']):
            if need<=0. and not full:break
            if remaining[i]<=0.:continue
            if origin['end_ordinal']>cash['ordinal']:
                c.near(need,0.);break
            amount=remaining[i] if full else min(need,remaining[i])
            if amount>0.:
                if segments and segments[-1]['end_ordinal']<origin['start_ordinal']:
                    segments.append(dict(start_ordinal=segments[-1]['end_ordinal'],end_ordinal=origin['start_ordinal'],accrued_interest=0.,book_amortization=0.))
                segments.append(dict(start_ordinal=origin['start_ordinal'],end_ordinal=origin['end_ordinal'],accrued_interest=-amount,book_amortization=0.))
                allocations.append(dict(native_interest_index=i,amount=amount))
            remaining[i]-=amount;need-=amount
        c.near(need,0.);out.append((segments,allocations,full))
    c.near(sum(abs(x) for x in remaining),0.);return out

def reconstruct(raw,check=lambda:False,*,include_tables=True):
    c.tick(check);c.keys(raw,'schema position_id account_id runs recognition_policy interest_rights_policy specification')
    c.exact(raw['schema'],SCHEMA);c.exact(raw['interest_rights_policy'],'native_earned_fifo_cash_origin-1')
    if not 2<=len(raw['runs'])<=8:raise ArithmeticError('bounded explicit runs')
    spec=_spec(raw['specification']);p=spec['positions'][0];account=spec['accounts'][0]
    if len(spec['positions'])!=1 or len(spec['accounts'])!=1:raise ArithmeticError('one account/liability')
    required={'baseline'}|{s['name'] for s in spec['scenarios']}
    if {r['scenario_id'] for r in raw['runs']}!=required or len({r['run_id'] for r in raw['runs']})!=len(raw['runs']):raise ArithmeticError('run coverage')
    paths=[];source_work=0;reserved=0;original=None
    for run in raw['runs']:
        c.keys(run,'run_id scenario_id severity source');c.exact(float(run['severity']),1.)
        if type(run['severity']) not in (int,float):raise ArithmeticError('severity numeric')
        native=source.reconstruct(run['source'],check)
        identity={k:native['raw_request']['continuation'][k] for k in ('original','anchor','contract')}
        if original is not None:c.exact(identity,original)
        original=identity;source_work+=native['admission']['work_units'];reserved+=native['admission']['reserved_bytes']
        paths.append(dict(run_id=run['run_id'],scenario_id=run['scenario_id'],severity=1.,native=native))
    original_raw=raw['runs'][0]['source']['continuation'];start=original_raw['original']['as_of_ordinal'];d=original_raw['original']['horizon_days'];anchor=original_raw['anchor']
    if spec['horizon_days']!=d:raise ArithmeticError('complete tail')
    c.exact(p['balance'],float(anchor['original_principal']));c.exact(p['book_adjustment'],float(anchor['original_gross_carrying_value'])-float(anchor['original_principal']))
    if p['id']!=raw['position_id'] or p['account']!=raw['account_id'] or p['kind']!='deposit' or p['classification']!='ac':raise ArithmeticError('original actual ownership')
    for k in 'opening_accrued start_day rate annual_pd allowance commitment monthly_runoff floating_beta migration_rate draw_fraction encumbered_fraction'.split():
        if p[k]!=0.:raise ArithmeticError('financial overlay '+k)
    for k in 'annual_fees annual_costs annual_dividends tax_rate'.split():
        if account[k]!=0.:raise ArithmeticError('financial account overlay '+k)
    for x in [p['balance'],p['book_adjustment'],p['balance']+p['book_adjustment'],account['cash'],account['equity']]:c.money(x)
    # Typed scalar source normalizations are separate from retained specification Value.
    typed=deepcopy(raw)
    for run,path in zip(typed['runs'],paths):run['source']=path['native']['raw_request'];run['severity']=1.
    rb=c.native_json_length(typed);run_days=len(paths)*(d+1)
    ledger_work=run_days*(8192+16);derivation_work=len(paths)*d*(3*d+2)*64
    reserved+=rb*12+run_days*65536;work=source_work+ledger_work+derivation_work+rb*8
    if rb>1048576 or reserved>33554432 or work>8000000:raise ArithmeticError('joined pre-generation admission')
    admission=dict(work_units=work,reserved_bytes=reserved,native_source_work=source_work,ledger_work=ledger_work,derivation_work=derivation_work)
    events=[];plans=[];runs=[];coverage=[];origins=[]
    for pi,path in enumerate(paths):
        current=path['native']['current'];physical=current['source'];selector=dict(kind='named',scenario_id=path['run_id'])
        revision=physical['identity']['source_revision'];runs.append(dict(run_id=path['run_id'],scenario_id=path['scenario_id'],severity=1.,input_revision=revision))
        coverage.append(dict(position_id=p['id'],run=deepcopy(selector),status=dict(kind='complete_event_set')))
        provenance=dict(kind='modeled',model_id='deposit_source_principal_continuation',model_version='deposit-source-income-1',input_revision=revision)
        def add(eventid,day,components,segments):
            period=dict(start_ordinal=segments[0]['start_ordinal'],end_ordinal=segments[-1]['end_ordinal']) if segments else None
            events.append(dict(position_id=p['id'],event_id=eventid,scenario=deepcopy(selector),payment_ordinal=day,day_offset=day-start,accrual_period=period,currency=account['currency'],components=components,measure='physical',provenance=deepcopy(provenance),adjustment=dict(kind='none')))
            if segments:plans.append(dict(position_id=p['id'],event_id=eventid,run=deepcopy(selector),segments=segments))
        for ci,(segments,allocations,full) in enumerate(_fifo(physical,check)):
            cash=physical['cashflows'][ci];eventid=f'cash:{pi}:{ci}'
            add(eventid,cash['ordinal'],dict(principal=cash['principal'],cash_interest=cash['cash_interest'],accrued_interest=sum((s['accrued_interest'] for s in segments),0.),book_amortization=0.,fees=0.),segments)
            origins.append(dict(run_id=path['run_id'],event_id=eventid,native_event_id=cash['event_id'],native_kind=cash['kind'],full_native_accrued_settlement=full,allocations=allocations))
        for i,row in enumerate(current['income']['rows']):
            day=row['payment_ordinal'];book=-row['basis_amortization'];segment=dict(start_ordinal=day-1,end_ordinal=day,accrued_interest=0.,book_amortization=book)
            add(f'book:{pi}:{i}',day,dict(principal=0.,cash_interest=0.,accrued_interest=0.,book_amortization=book,fees=0.),[segment])
    events.sort(key=lambda e:e['payment_ordinal'])
    identity=original_raw['contract'];owned=dict(position_id=p['id'],account_id=account['id'],currency=account['currency'],direction='payment',ownership='fully_scenario_generated',source_id=identity['id'],source_revision=identity['revision'])
    schedule=dict(schema='dated-cashflow-1',as_of_ordinal=start,horizon_days=d,scenarios=[p['run_id'] for p in paths],events=events)
    dated=dict(schema='dated-store-1',as_of_ordinal=start,reporting_horizon_days=d,runs=runs,owners=[owned],sources=[dict(source_id=identity['id'],source_revision=identity['revision'],schedule=schedule,coverage=coverage)],accrual_plans=plans)
    prepared=dict(schema='dated-ledger-1',monetary_unit='currency_units',recognition_policy=deepcopy(raw['recognition_policy']),specification=deepcopy(raw['specification']),dated=dated)
    _,_,admitted,_=_backbook(prepared,spec,check)
    for e in admitted:e['owner']=owned
    inputs=(dict(backbook=prepared),spec,admitted,[],start+d,{},{},{p['run_id']:p for p in paths})
    tables={}
    if include_tables:
        tables=_replay(*inputs[:7],check,inputs[7])
        tables.pop('reported_daily_balance',None);tables.pop('reporting_rights',None);tables.pop('acquisition_lineage',None)
        tables['path']=tables.pop('daily_balance')
        tables={k:v for k,v in tables.items() if v}
    return dict(prepared_request=prepared,source_receipt=dict(raw_request=deepcopy(raw),paths=paths,admission=admission,cash_origins=origins,
        phase_policy='opening_recognition_actual_cash_source_atom_reclass_then_revision-1',accounting_standard_approval=False,installed_adoption=False,saved_book_adoption=False,capital_ftp_eligible=False),tables=tables,replay_inputs=inputs)

def verify_preparation(raw,prepared,receipt,check=lambda:False):
    expected=reconstruct(raw,check,include_tables=False);c.exact(receipt['raw_request'],raw)
    c.equal(prepared,expected['prepared_request'],'prepared');c.equal(receipt,expected['source_receipt'],'receipt')
    c.exact(prepared['specification'],raw['specification']);c.exact(receipt['admission'],expected['source_receipt']['admission'])
    for key in ('runs','owners'):c.exact(prepared['dated'][key],expected['prepared_request']['dated'][key])
    c.exact(prepared['recognition_policy'],raw['recognition_policy'])
    for actual,want in zip(receipt['paths'],expected['source_receipt']['paths'],strict=True):
        for key in ('run_id','scenario_id','severity'):c.exact(actual[key],want[key])
        c.exact(actual['native']['raw_request'],want['native']['raw_request'])
        c.exact(actual['native']['admission'],want['native']['admission'])
        for branch in ('original','current'):
            c.exact(actual['native'][branch]['atoms'],want['native'][branch]['atoms'])
            c.exact(actual['native'][branch]['transitions'],want['native'][branch]['transitions'])
            for key in ('contract','policy','original_yield_anchor','original_annual_effective_rate','opening_annual_effective_rate','currency','monetary_unit','measure'):
                c.exact(actual['native'][branch]['income_request'][key],want['native'][branch]['income_request'][key])
            for got,row in zip(actual['native'][branch]['income']['rows'],want['native'][branch]['income']['rows'],strict=True):
                c.exact(got['annual_effective_rate'],row['annual_effective_rate']);c.exact(got['yield_review'],row['yield_review'])
    for k in ('accounting_standard_approval','installed_adoption','saved_book_adoption','capital_ftp_eligible'):c.exact(receipt[k],False)
    return CheckedControl(deepcopy(raw),expected,check)

class CheckedControl:
    __slots__=('raw','expected','seal')
    def __init__(self,raw,expected,check):c.tick(check);self.raw=raw;self.expected=expected;self.seal=_digest((raw,expected))
def _digest(value):return hashlib.sha256(json.dumps(value,separators=(',',':'),sort_keys=True,allow_nan=False).encode()).hexdigest()

def decode(messages):
    dictionary=[];tables=defaultdict(list);completion=None
    for message in messages:
        if 'complete' in message:
            if completion is not None:raise ArithmeticError('duplicate completion')
            c.exact(message,dict(complete=True,protocol=3));completion=message;continue
        if completion is not None:raise ArithmeticError('post-completion data')
        table=message['table']
        if 'columns' in message:
            if table!='journal':raise ArithmeticError('unknown column frame')
            dictionary+=message['dictionary'];columns=message['columns'];n=len(columns['value'])
            if any(len(v)!=n for v in columns.values()):raise ArithmeticError('unequal columns')
            for i in range(n):tables[table].append({k:(dictionary[v[i]] if k in ('scenario','account','event','gl_account','instrument_id') else v[i]) for k,v in columns.items()})
        else:tables[table]+=message['rows']
    c.exact(completion,dict(complete=True,protocol=3));return dict(tables)

def verify_stream(raw,control,frames,check=lambda:False):
    if type(control) is not CheckedControl or control.seal!=_digest((control.raw,control.expected)):raise ArithmeticError('private control mutation')
    c.exact(raw,control.raw);c.tick(check)
    inputs=control.expected['replay_inputs'];counts=defaultdict(int)
    if hasattr(frames,'frames'):
        c.exact(frames.completion,dict(complete=True,protocol=3))
        def persisted_rows(table):
            if table not in frames.schemas:raise ArithmeticError('missing persisted table')
            for frame in frames.frames(table):
                c.tick(check)
                c.exact({k:str(v) for k,v in frame.schema.items()},frames.schemas[table])
                for actual in frame.iter_rows(named=True):
                    for k in ('source_atom','transition','review','gross_rights','source_income','original_yield_anchor'):
                        if k in actual and type(actual[k]) is str:actual[k]=json.loads(actual[k])
                    if table=='journal' and 'value' not in actual:
                        actual['value']=actual.pop('debit')-actual.pop('credit');tx=actual.pop('transaction_id');actual['transaction_number']=int(tx.rsplit(':',1)[-1])
                    yield actual
        cursors={table:iter(persisted_rows(table)) for table in frames.schemas}
    else:
        actual=decode(frames);cursors={table:iter(rows) for table,rows in actual.items()}
    def emit(table,want):
        c.tick(check)
        if table not in cursors:raise ArithmeticError('missing table '+table)
        actual=next(cursors[table],None)
        if actual is None:raise ArithmeticError('missing row '+table)
        c.equal(actual,want,'stream/'+table+'/'+str(counts[table]));counts[table]+=1
        if table=='deposit_principal_lineage' or (table=='journal' and want['event']=='source_principal_basis_reclassification'):c.exact(actual,want)
    remaining=_replay(*inputs[:7],check,inputs[7],emit)
    for table,rows in remaining.items():
        if table in ('reported_daily_balance','reporting_rights','acquisition_lineage','daily_balance'):continue
        for row in rows:emit(table,row)
    if not (set(cursors)-set(counts)).issubset({'actions','reverse_grid','breaches','funding_claims','accounting_events'}):raise ArithmeticError('unexpected table inventory')
    for table,cursor in cursors.items():
        if next(cursor,None) is not None:raise ArithmeticError('extra row '+table)
    c.tick(check)
    return dict(physical_deposit_source_principal_recomputed=True,original_yield_continuation_recomputed=True,
        gross_origin_rights_reconciled=True,ordered_same_sim_journal_replayed=True,
        accounting_standard_approval=False,installed_adoption=False,saved_book_adoption=False,capital_ftp_eligible=False)
