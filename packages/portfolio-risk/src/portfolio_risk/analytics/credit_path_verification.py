"""Independent persisted protocol 5 source and ordered journal verification."""
from collections import defaultdict
import json
import math

from .credit_path_reference import account_path
from .credit_path_tables import JSON_FIELDS,SCHEMAS

CORE_ACCOUNTS=('cash','live_principal','default_principal','live_accrued_interest','default_accrued_interest',
    'live_book_adjustment','default_book_adjustment','allowance','interest_income','credit_loss_expense','basis_recognition_expense','recovery_income','equity_opening')
GL_ACCOUNTS=('cash','asset_principal','credit_default_principal','accrued_interest','credit_default_accrued_interest',
    'book_adjustment','credit_default_book_adjustment','allowance','interest_income','credit_loss_expense','basis_recognition_expense','recovery_income','equity_opening')


def equal(actual,expected,path=''):
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():raise ArithmeticError(f'credit path source keys differ: {path}')
        for key in expected:equal(actual[key],expected[key],path+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(f'credit path source length differs: {path}')
        for i,(a,b) in enumerate(zip(actual,expected)):equal(a,b,path+'/'+str(i))
    elif isinstance(expected,(float,int)) and not isinstance(expected,bool):
        exact_integer=isinstance(actual,int) and isinstance(expected,int)
        if not isinstance(actual,(float,int)) or isinstance(actual,bool) or not math.isfinite(actual) or (actual!=expected if exact_integer else not math.isclose(actual,expected,rel_tol=1e-12,abs_tol=1e-9)):
            raise ArithmeticError(f'credit path source amount differs: {path}')
    elif actual!=expected:raise ArithmeticError(f'credit path source identity differs: {path}')


def verify_sources(raw,spec,writer,check):
    """Rebuild every raw path and policy step without invoking a native pricer."""
    counts=defaultdict(int)
    def rows(table):
        for frame in writer.frames(table):
            check()
            for row in frame.iter_rows(named=True):
                for key in JSON_FIELDS.get(table,()):row[key]=json.loads(row[key])
                yield row
    streams={name:iter(rows(name)) for name in SCHEMAS}
    def take(table,expected):
        row=next(streams[table],None)
        if row is None:raise ArithmeticError(f'missing credit path {table} evidence')
        equal(row,expected,table);counts[table]+=1
    accounts=[vars(a) for a in spec['accounts']];positions=[vars(p) for p in spec['positions']]
    index={a['id']:i for i,a in enumerate(accounts)};asof=raw['as_of_ordinal'];horizon=spec['horizon_days'];end=asof+horizon
    transaction=0
    for run in raw['runs']:
        check();scope=dict(run_id=run['run_id'],scenario=run['scenario_id'],severity=run['severity'])
        gl=defaultdict(float);opening=[[0.]*13 for _ in accounts];deltas=defaultdict(lambda:[0.]*12);sources=defaultdict(lambda:[0.]*8)
        def labels(ai):
            a=accounts[ai];return dict(scope,account=a['id'],entity=a['entity'],currency=a['currency'])
        def post(ai,day,ordinal,key,event,lines):
            nonlocal transaction
            lines=[(code,instrument,float(value)) for code,instrument,value in lines if value!=0.]
            if not lines:return None
            transaction+=1
            if not math.isclose(math.fsum(value for _,_,value in lines),0.,rel_tol=0.,abs_tol=1e-9*max(1.,math.fsum(abs(value) for _,_,value in lines))):
                raise ArithmeticError('independent path journal unbalanced')
            for code,instrument,value in lines:
                take('journal',dict(labels(ai),day=day,ordinal=ordinal,transaction_number=transaction,transaction_key=key,
                    event=event,gl_account=GL_ACCOUNTS[code],instrument_id=instrument,value=value))
                gl[ai,code,instrument]+=value
                if day:deltas[day,ai][code]+=value
            return transaction
        def key(parts):return json.dumps(parts,ensure_ascii=False,separators=(',',':'))
        for ai,a in enumerate(accounts):
            lines=[(0,'@cash',a['cash']),(12,'@equity',-a['equity'])]
            for position in positions:
                if position['account']!=a['id']:continue
                ident=str(position['id'])
                lines.extend([(1,ident,position['balance']),(3,ident,position['opening_accrued']),
                    (5,ident,position['book_adjustment']),(7,ident,-position['allowance'])])
            for code,_,value in lines:opening[ai][code]+=value
            post(ai,0,asof,key(['opening',run['run_id'],a['id']]),'opening',lines)
        for batch in run['batches']:
            check();request=batch['request'];bindings={b['path_key']:b for b in request['bindings']}
            mapping={p['position_id']:index[p['account_id']] for p in batch['account_by_position']}
            for path in request['path']['paths']:
                check();binding=bindings[path['path_key']];position=binding['position_id'];ai=mapping[position]
                result=account_path(request,path,binding,check)
                lineage=dict(labels(ai),position=position,path_key=path['path_key'])
                source=dict(source_id=request['path']['source_id'],source_revision=request['path']['source_revision'])
                take('credit_path_sources',dict(lineage,**source,batch_id=batch['batch_id'],claim_id=binding['claim_id'],
                    measure='physical',monetary_unit='currency_units',binding=binding,economic=result['economic']))
                for i,timed in enumerate(result['intents']):
                    check();intent=timed['intent'];day=intent['recognition_ordinal']-asof
                    lines=[]
                    for posting in intent['postings']:
                        code=CORE_ACCOUNTS.index(posting['account'])
                        instrument='@cash' if code==0 else ((intent['claim_id'] or position) if code in (2,4,6) else position)
                        lines.append((code,instrument,posting['amount']))
                    transaction_key=key(['path',run['run_id'],batch['batch_id'],position,i])
                    number=post(ai,day,intent['recognition_ordinal'],transaction_key,intent['event'],lines)
                    take('credit_path_lineage',dict(lineage,batch_id=batch['batch_id'],day=day,ordinal=intent['recognition_ordinal'],
                        transaction_number=number,transaction_key=transaction_key,event=intent['event'],claim_id=intent['claim_id'],
                        source_id=intent['source']['id'],source_revision=intent['source']['revision'],policy_id=intent['policy']['id'],policy_revision=intent['policy']['revision'],
                        **{name:timed[name] for name in ('exact_event','accrual','event_ids','contractual_policy_id','timing')}))
                for row in result['daily']:
                    day=row['recognition_ordinal']-asof
                    values=[sum(s['principal'] for s in row['live']),row['default_principal'],sum(s['accrued_interest'] for s in row['live']),
                        row['default_accrued_interest'],sum(s['book_adjustment'] for s in row['live']),row['default_book_adjustment'],-row['allowance']]
                    for i,value in enumerate(values):sources[day,ai][i]+=value
                    sources[day,ai][7]+=-sum(min(s['accrued_interest'],0.) for s in row['live'])-min(row['default_accrued_interest'],0.)
                    take('credit_path_daily',dict(lineage,batch_id=batch['batch_id'],claim_id=binding['claim_id'],day=day,ordinal=row['recognition_ordinal'],state=row))
                for recovery in result['complete_recoveries']:
                    take('credit_path_recoveries',dict(lineage,batch_id=batch['batch_id'],claim_id=binding['claim_id'],
                        source_id=binding['recovery_schedule_review']['id'],source_revision=binding['recovery_schedule_review']['revision'],recovery=recovery))
                for claim in result['closing']['claims']:take('credit_path_claims',dict(lineage,ordinal=end,claim=claim))
        balances=opening;errors=[0.]*len(accounts)
        for day in range(1,horizon+1):
            check()
            for ai,a in enumerate(accounts):
                for i,value in enumerate(deltas[day,ai]):balances[ai][i]+=value
                b=balances[ai]
                for i,value in enumerate(sources[day,ai][:7]):
                    equal(b[i+1],value,'daily independent GL/source');errors[ai]=max(errors[ai],abs(b[i+1]-value))
                net=sum(b[:8]);liabilities=sources[day,ai][7];equity=a['equity']-sum(b[8:12])
                equal(net,equity,'daily independent net assets');errors[ai]=max(errors[ai],abs(net-equity))
                take('daily_balance',dict(labels(ai),day=day,ordinal=asof+day,cash=b[0],assets=net+liabilities,liabilities=liabilities,
                    net_assets=net,equity=equity,allowance=-b[7],reconciliation_error=errors[ai]))
        for (ai,code,instrument),balance in sorted(gl.items()):
            take('trial_balance',dict(labels(ai),gl_account=GL_ACCOUNTS[code],instrument_id=instrument,balance=balance))
        for ai,a in enumerate(accounts):
            b=balances[ai];net=sum(b[:8]);liabilities=sources[horizon,ai][7]
            take('summary',dict(labels(ai),final_cash=b[0],final_assets=net+liabilities,final_liabilities=liabilities,
                final_net_assets=net,final_equity=a['equity']-sum(b[8:12]),max_reconciliation_error=errors[ai]))
    for name,stream in streams.items():
        check()
        if next(stream,None) is not None:raise ArithmeticError(f'extra credit path {name} evidence')
    return dict(journal_replayed=True,credit_path_source_reconciled=True,physical_paths_recomputed=True,
        reference='Independent scalar hazard, realized-rights and reviewed-accounting replay',counts=dict(counts),
        empirical_validation=False,regulatory_compliance=False)
