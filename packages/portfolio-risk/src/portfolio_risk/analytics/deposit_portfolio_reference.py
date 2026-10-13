"""Independent rawâ†’native-economic controls and one chronological pooled GL.
Research-only oracle; no native financial callbacks or producer monetary authority.
"""
from copy import deepcopy
from dataclasses import fields
from fractions import Fraction
import json,math,struct
from . import _deposit_portfolio_income_reference as c
from . import _deposit_portfolio_principal_reference as source
from ._deposit_portfolio_calendar import _normalize,_backbook,Account,Position,Scenario
from ._deposit_portfolio_replay import _replay

SCHEMA='deposit-performing-portfolio-ledger-2'

# Windows64 typed ABI reservation contract, independently pinned by metadata-only sizes mode.
SIZES=dict(allocation=16,atom=152,cash=96,group=112,income_event=192,income_row=224,interest=40,line=32,origin=40,point=128,settlement=80,transition=48)
def source_plan(raw):
    r=c.typed(raw);x=r['continuation'];c.admit(x);h=x['original']['horizon_days'];rb=c.native_json_length(r);cash=3*h+2;atoms=h+2;alloc=h+cash;groups=3*h+3;s=SIZES
    static=(h+1)*s['point']+h*s['interest']+cash*s['cash']+atoms*(s['atom']+s['transition'])+h*(s['income_event']+s['income_row']+s['origin'])+cash*s['settlement']+alloc*s['allocation']
    ids=sum(len(x['contract'][k].encode())+len(x['policy']['review'][k].encode()) for k in ('id','revision'))
    retained=65536+rb*12+static*4+groups*2*(s['group']+3*s['line'])+h*(4096+ids*24)
    output=65536+rb*4+h*(12000+ids*24);work=output+rb*8+h*(2048+(len(x['original']['segments'][0]['segment_id'].encode())+48)*48)
    plan=dict(horizon_days=h,point_slots=2*(h+1),interest_slots=2*h,cash_slots=2*cash,atom_slots=2*atoms,income_slots=2*h,allocation_slots=3*alloc,group_slots=groups,raw_bytes=rb,retained_reserved=retained,output_reserved=output,reserved_bytes=retained+output,work_units=work)
    if rb>1048576 or work>8000000 or retained+output>33554432:raise ArithmeticError('source joined pre-generation admission')
    return plan

def portfolio_admission(raw):
    if type(raw['runs']) is not list or not 2<=len(raw['runs'])<=8:raise ArithmeticError('explicit complete run domain')
    if type(raw['positions']) is not list or not 1<=len(raw['positions'])<=256:raise ArithmeticError('bounded exclusive owner domain')
    for run in raw['runs']:
        c.keys(run,'run_id scenario_id severity')
        if type(run['severity']) not in (int,float) or not math.isfinite(run['severity']) or run['severity']!=1.:raise ArithmeticError('explicit ordinary severity domain')
    typed=deepcopy(raw)
    for p in typed['positions']:
        for path in p['paths']:path['source']=c.typed(path['source'])
    for run in typed['runs']:run['severity']=float(run['severity'])
    rb=c.native_json_length(typed);plans=[source_plan(p['source']) for o in raw['positions'] for p in o['paths']]
    sw=sum(p['work_units'] for p in plans);sr=sum(p['retained_reserved'] for p in plans);so=sum(p['output_reserved'] for p in plans);lr=sum(p['horizon_days']*8192 for p in plans)
    d=raw['specification']['horizon_days'];rd=len(raw['runs'])*(d+1);od=rd*len(raw['positions']);lw=od*2048+rd*len(raw['specification']['accounts'])*256;stream=od*12288+(len(raw['specification']['positions'])+len(raw['specification']['accounts']))*16384
    work=sw+so+lw+rb*8;reserve=sr+so+lr+stream+rb*12
    if work>8000000 or reserve>33554432 or rb>1048576:raise ArithmeticError('joined portfolio pre-generation admission')
    return dict(work_units=work,reserved_bytes=reserve,source_work=sw,source_retained=sr,source_output=so,ledger_work=lw,ledger_retained=lr,stream_reserved=stream,raw_bytes=rb)

def gross(values):
    total=Fraction()
    for value in values:
        if type(value) not in (int,float) or not math.isfinite(value):raise ArithmeticError('finite gross domain')
        total+=abs(Fraction.from_float(float(value)))
    if total>10000:raise ArithmeticError('exact aggregate monetary domain')

def fifo(physical,check):
    # Deliberately historical O(I*C) reference; independent from native counted FIFO.
    remaining=[x['amount'] for x in physical['interest']];result=[]
    for cash in physical['cashflows']:
        c.tick(check);need=-cash['cash_interest'];segments=[];alloc=[];full=cash['kind'] in ('interest_payment','contractual_maturity')
        for i,origin in enumerate(physical['interest']):
            if need<=0. and not full:break
            if remaining[i]<=0.:continue
            if origin['end_ordinal']>cash['ordinal']:c.near(need,0.);break
            amount=remaining[i] if full else min(need,remaining[i])
            if amount>0.:
                if segments and segments[-1]['end_ordinal']<origin['start_ordinal']:segments.append(dict(start_ordinal=segments[-1]['end_ordinal'],end_ordinal=origin['start_ordinal'],accrued_interest=0.,book_amortization=0.))
                segments.append(dict(start_ordinal=origin['start_ordinal'],end_ordinal=origin['end_ordinal'],accrued_interest=-amount,book_amortization=0.))
                alloc.append(dict(native_interest_index=i,amount=amount))
            remaining[i]-=amount;need-=amount
        c.near(need,0.);result.append((segments,alloc,full))
    c.near(sum(abs(x) for x in remaining),0.);return result

def decode(data):
    frames=[json.loads(v) for v in data.splitlines() if v];header=frames.pop(0);completion=frames.pop();c.exact(completion,dict(complete=True,protocol=3));tables={};dictionary=[]
    for f in frames:
        if 'columns' in f:
            c.exact(f['table'],'journal');dictionary+=f['dictionary'];columns=f['columns'];n=len(columns['value'])
            if any(len(v)!=n for v in columns.values()):raise ArithmeticError('column cardinality')
            tables.setdefault('journal',[]).extend({k:(dictionary[v[i]] if k in ('scenario','account','event','gl_account','instrument_id') else v[i]) for k,v in columns.items()} for i in range(n))
        elif 'table' in f:tables.setdefault(f['table'],[]).extend(f['rows'])
    return header,tables

def verify(raw,data,check=lambda:False,*,_control=False):
    c.tick(check);c.keys(raw,'schema runs positions recognition_policy interest_rights_policy specification');c.exact(raw['schema'],SCHEMA)
    c.exact(raw['interest_rights_policy'],'native_earned_counted_fifo_cash_origin-2')
    h,tables=decode(data);c.keys(h,'schema model_schema prepared_request source_receipt');c.exact(h['schema'],'deposit-performing-portfolio-workflow-2');c.exact(h['model_schema'],SCHEMA)
    receipt=h['source_receipt'];c.keys(receipt,'raw_request owners cash_origins admission usage accounting_standard_approval empirical_validation installed_adoption saved_book_adoption capital_ftp_eligible');c.exact(receipt['raw_request'],raw);c.exact(receipt['admission'],portfolio_admission(raw))
    if len(data)>receipt['admission']['source_output']:raise ArithmeticError('complete serialization work allowance')
    for k in ('accounting_standard_approval','empirical_validation','installed_adoption','saved_book_adoption','capital_ftp_eligible'):c.exact(receipt[k],False)
    spec=_normalize(raw['specification'])
    for group,cls in [('accounts',Account),('positions',Position),('scenarios',Scenario)]:
        for row in spec[group]:
            for f in fields(cls):
                if f.type is float:row[f.name]=float(row[f.name])
    gross([v for p in spec['positions'] for v in (p['balance'],p['book_adjustment'],p['opening_accrued'])]+[v for a in spec['accounts'] for v in (a['cash'],a['equity'])])
    scenes={'baseline'}|{x['name'] for x in spec['scenarios']};c.exact({x['scenario_id'] for x in raw['runs']},scenes)
    if len(raw['runs'])!=len(scenes):raise ArithmeticError('duplicate/missing run')
    raw_owners={p['position_id']:p for p in raw['positions']}
    if len(raw_owners)!=len(raw['positions']) or set(raw_owners)!={p['id'] for p in spec['positions']}:raise ArithmeticError('exclusive owners')
    start=None;days=spec['horizon_days'];plans=[];schedules=[];owners=[];origins=[];income_paths={}
    for oi,pos in enumerate(spec['positions']):
        request=raw_owners[pos['id']];account=next(a for a in spec['accounts'] if a['id']==pos['account']);paths={p['run_id']:p for p in request['paths']}
        c.exact(set(paths),{r['run_id'] for r in raw['runs']})
        if len(paths)!=len(request['paths']):raise ArithmeticError('duplicate paths')
        emitted=receipt['owners'][oi];c.keys(emitted,'position_index account_index paths');c.exact(emitted['position_index'],oi);c.exact(emitted['account_index'],next(i for i,a in enumerate(spec['accounts']) if a['id']==pos['account']));c.exact(len(emitted['paths']),len(raw['runs']));events=[];coverage=[]
        for pi,run in enumerate(raw['runs']):
            c.tick(check);path_receipt=emitted['paths'][pi];c.keys(path_receipt,'run_id scenario_id severity native atom_events');c.exact(path_receipt['run_id'],run['run_id']);c.exact(path_receipt['scenario_id'],run['scenario_id']);c.exact(path_receipt['severity'],float(run['severity']));src=paths[run['run_id']]['source'];wrapper=path_receipt['native'];c.keys(wrapper,'schema native typed_plan retained_used output_bytes charged_work_used installed_adoption empirical_validation capital_ftp_eligible');plan=source_plan(src);c.exact(wrapper['typed_plan'],plan)
            c.exact(wrapper['schema'],'deposit-source-income-2')
            for flag in ('installed_adoption','empirical_validation','capital_ftp_eligible'):c.exact(wrapper[flag],False)
            for metric,cap in [('retained_used','retained_reserved'),('output_bytes','output_reserved'),('charged_work_used','work_units')]:
                if type(wrapper[metric]) is not int or not 0<=wrapper[metric]<=plan[cap]:raise ArithmeticError('observed component reservation')
            actual_size=c.native_json_length(wrapper['native']);c.exact(wrapper['output_bytes'],actual_size)
            c.exact(wrapper['charged_work_used'],actual_size+plan['raw_bytes']*8+plan['horizon_days']*(2048+(len(src['continuation']['original']['segments'][0]['segment_id'].encode())+48)*48))
            native=source.reconstruct(src,check);actual=deepcopy(emitted['paths'][pi]['native']['native']);actual.pop('admission');expected=deepcopy(native);expected.pop('admission');c.equal(actual,expected)
            # These raw source-owned atoms and roundoff partition are independently exact.
            for k in ('original','current'):
                c.exact(actual[k]['atoms'],expected[k]['atoms']);c.exact(actual[k]['transitions'],expected[k]['transitions'])
            x=src['continuation'];start=x['original']['as_of_ordinal'];c.exact(x['original']['horizon_days'],days);c.exact(x['current']['identity']['run_id'],run['run_id'])
            c.exact(pos['balance'],float(x['anchor']['original_principal']));c.exact(pos['book_adjustment'],float(x['anchor']['original_gross_carrying_value'])-float(x['anchor']['original_principal']))
            physical=native['current']['source'];selector=dict(kind='named',scenario_id=run['run_id']);coverage.append(dict(position_id=pos['id'],run=selector,status=dict(kind='complete_event_set')))
            prov=dict(kind='modeled',model_id='deposit_source_counted_portfolio',model_version='deposit-source-income-2',input_revision=physical['identity']['source_revision'])
            def add(eid,date,components,segments):
                period=dict(start_ordinal=segments[0]['start_ordinal'],end_ordinal=segments[-1]['end_ordinal']) if segments else None
                events.append(dict(position_id=pos['id'],event_id=eid,scenario=deepcopy(selector),payment_ordinal=date,day_offset=date-start,accrual_period=period,currency=account['currency'],components=components,measure='physical',provenance=deepcopy(prov),adjustment=dict(kind='none')))
                if segments:plans.append(dict(position_id=pos['id'],event_id=eid,run=deepcopy(selector),segments=segments))
            for ci,(segments,alloc,full) in enumerate(fifo(physical,check)):
                cash=physical['cashflows'][ci];eid=f'cash:{oi}:{pi}:{ci}'
                add(eid,cash['ordinal'],dict(principal=cash['principal'],cash_interest=cash['cash_interest'],accrued_interest=sum((s['accrued_interest'] for s in segments),0.),book_amortization=0.,fees=0.),segments)
                origins.append(dict(position_id=pos['id'],run_id=run['run_id'],event_id=eid,native_event_id=cash['event_id'],native_kind=cash['kind'],full_native_accrued_settlement=full,allocations=alloc))
            for i,row in enumerate(native['current']['income']['rows']):
                day=row['payment_ordinal'];b=-row['basis_amortization'];seg=dict(start_ordinal=day-1,end_ordinal=day,accrued_interest=0.,book_amortization=b)
                add(f'book:{oi}:{pi}:{i}',day,dict(principal=0.,cash_interest=0.,accrued_interest=0.,book_amortization=b,fees=0.),[seg])
            income_paths[(pos['id'],run['run_id'])]=dict(native=native)
        events.sort(key=lambda e:e['payment_ordinal']);identity=request['paths'][0]['source']['continuation']['contract'];sid='owned:'+pos['id']
        owners.append(dict(position_id=pos['id'],account_id=pos['account'],currency=account['currency'],direction='payment',ownership='fully_scenario_generated',source_id=sid,source_revision=identity['revision']))
        schedules.append(dict(source_id=sid,source_revision=identity['revision'],schedule=dict(schema='dated-cashflow-1',as_of_ordinal=start,horizon_days=days,scenarios=[r['run_id'] for r in raw['runs']],events=events),coverage=coverage))
    prepared=dict(schema='dated-ledger-1',monetary_unit='currency_units',recognition_policy=deepcopy(raw['recognition_policy']),specification=deepcopy(raw['specification']),dated=dict(schema='dated-store-1',as_of_ordinal=start,reporting_horizon_days=days,runs=[dict(r,severity=float(r['severity']),input_revision='explicit-owner-source-revisions') for r in raw['runs']],owners=owners,sources=schedules,accrual_plans=plans))
    c.equal(h['prepared_request'],prepared);c.equal(receipt['cash_origins'],origins)
    ordered_events=[e for s in schedules for e in s['schedule']['events']];ordered_events.sort(key=lambda e:e['payment_ordinal'])
    event_index={e['event_id']:i for i,e in enumerate(ordered_events)}
    cash_index={(o['position_id'],o['run_id'],o['native_event_id']):event_index[o['event_id']] for o in origins}
    for pos,emitted in zip(spec['positions'],receipt['owners']):
        for path in emitted['paths']:
            native=income_paths[(pos['id'],path['run_id'])]['native']
            c.exact(path['atom_events'],[[cash_index[(pos['id'],path['run_id'],eid)] for eid in a['cash_event_ids']] for a in native['current']['atoms']])
    usage=receipt['usage'];ad=receipt['admission'];wrappers=[p['native'] for o in receipt['owners'] for p in o['paths']]
    for k,field,cap in [('source_retained_used','retained_used','source_retained'),('source_output_bytes','output_bytes','source_output'),('source_charged_work','charged_work_used','source_work')]:
        c.exact(usage[k],sum(w[field] for w in wrappers))
        if usage[k]>ad[cap]:raise ArithmeticError('joined observed reservation')
    if type(usage['dated_store_capacity']) is not int or not 0<usage['dated_store_capacity']<=ad['ledger_retained']:raise ArithmeticError('store observed reservation')
    _,_,admitted,_=_backbook(prepared,spec,check)
    by_owner={o['position_id']:o for o in owners}
    for e in admitted:e['owner']=by_owner[e['event']['position_id']]
    inputs=(dict(backbook=prepared),spec,admitted,[],start+days,{},{},income_paths)
    if _control:return inputs
    expected=_replay(*inputs[:7],check,inputs[7]);expected['path']=expected.pop('daily_balance');expected={k:v for k,v in expected.items() if v}
    c.equal(tables,expected)
    return dict(raw_sources_recomputed=True,original_eir_recomputed=True,full_unweighted_origins_verified=True,ordered_single_gl_replayed=True,rows=sum(map(len,tables.values())),capital_ftp_eligible=False,installed_adoption=False)

# Contained publication seam: source/header verified before opaque control creation;
# producer partitions are consumed incrementally. Control rows are independently
# constructed and bounded separately, never caller-produced financial authority.
import hashlib
class CheckedControl:
    __slots__=('_raw','_tables','_pin')
    def __init__(self,*args):raise TypeError('private verification control')
def _identity(value,memo=None):
    if memo is None:memo={}
    if type(value) in (dict,list,tuple):
        if id(value) in memo:return ['ref',memo[id(value)]]
        index=len(memo);memo[id(value)]=index
    if type(value) is dict:
        ordered=sorted(value,key=lambda k:json.dumps(_identity(k),separators=(',',':')))
        return ['dict',index,[[_identity(k,memo),_identity(value[k],memo)] for k in ordered]]
    if type(value) in (tuple,list):return [type(value).__name__,index,[_identity(v,memo) for v in value]]
    if type(value) is float:
        if not math.isfinite(value):raise ArithmeticError('finite private control')
        return ['float',struct.pack('>d',value).hex()]
    if type(value) not in (str,int,bool,type(None)):raise ArithmeticError('private control type')
    return [type(value).__name__,value]
def _seal(raw,tables):return hashlib.sha256(json.dumps(_identity([raw,tables]),separators=(',',':'),allow_nan=False).encode()).hexdigest()
def verify_preparation(raw,prepared,receipt,check=lambda:False):
    header=dict(schema='deposit-performing-portfolio-workflow-2',model_schema=SCHEMA,prepared_request=prepared,source_receipt=receipt)
    data=json.dumps(header,allow_nan=False).encode()+b'\n'+b'{"complete":true,"protocol":3}\n'
    expected=verify(raw,data,check,_control=True)
    retained=len(json.dumps(_identity(expected),separators=(',',':'),allow_nan=False).encode())
    if retained>33554432:raise ArithmeticError('independent control retention32MiB')
    control=object.__new__(CheckedControl);control._raw=deepcopy(raw);control._tables=expected;control._pin=_seal(control._raw,expected);return control

def verify_stream(raw,control,facade,check=lambda:False):
    from collections import defaultdict
    from .deposit_portfolio_transport import SCHEMAS,JSON_CELLS
    if type(control) is not CheckedControl:raise ArithmeticError('private checked source required')
    c.tick(check);c.exact(raw,control._raw)
    if control._pin!=_seal(control._raw,control._tables):raise ArithmeticError('private checked control mutated')
    c.exact(facade.completion,dict(complete=True,protocol=3))
    c.exact(facade.schemas,{name:{k:str(v) for k,v in schema.items()} for name,schema in SCHEMAS.items()})
    counts=defaultdict(int)
    def rows(name):
        for frame in facade.frames(name):
            c.tick(check);c.exact({k:str(v) for k,v in frame.schema.items()},facade.schemas[name])
            if not 1<=frame.height<=65536:raise ArithmeticError('persisted frame row bound')
            for row in frame.iter_rows(named=True):
                c.tick(check)
                for key in JSON_CELLS.get(name,()):row[key]=json.loads(row[key])
                if name=='journal':
                    debit=row.pop('debit');credit=row.pop('credit');tx=row.pop('transaction_id')
                    if not all(math.isfinite(v) and v>=0. for v in (debit,credit)) or (debit and credit):raise ArithmeticError('persisted debit/credit domain')
                    num=tx.rsplit(':',1)[-1]
                    if not num.isdigit() or tx!=row['scenario']+':'+str(int(num)):raise ArithmeticError('exact transaction identity')
                    row['transaction_number']=int(num);row['value']=debit-credit
                yield row
    cursors={name:iter(rows(name)) for name in SCHEMAS}
    def emit(name,want):
        c.tick(check);actual=next(cursors[name],None)
        if actual is None:raise ArithmeticError('missing ordered row '+name)
        c.equal(actual,want,name+'/'+str(counts[name]));counts[name]+=1
        if name=='deposit_principal_lineage' or (name=='journal' and want['event']=='source_principal_basis_reclassification'):c.exact(actual,want)
    inputs=control._tables
    rest=_replay(*inputs[:7],check,inputs[7],emit)
    for name,values in rest.items():
        if name in ('reported_daily_balance','reporting_rights','acquisition_lineage','daily_balance'):continue
        for row in values:emit(name,row)
    for name,cursor in cursors.items():
        if next(cursor,None) is not None:raise ArithmeticError('extra persisted row '+name)
    c.tick(check);c.exact(facade.completion,dict(complete=True,protocol=3));c.exact(raw,control._raw)
    if control._pin!=_seal(control._raw,control._tables):raise ArithmeticError('control changed during replay')
    return dict(physical_deposit_portfolio_sources_recomputed=True,original_yield_continuations_recomputed=True,
        full_unweighted_gross_origins_verified=True,ordered_pooled_same_sim_journal_replayed=True,
        persisted_partitions_replayed=True,accounting_standard_approval=False,empirical_validation=False,
        installed_adoption=False,saved_book_adoption=False,capital_ftp_eligible=False)
