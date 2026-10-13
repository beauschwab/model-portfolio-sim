"""Independent dated GL loop with an explicit asof revision phase.

Derived from the existing independent calendar replay recipe, with source and
catchup computed only from raw deposit inputs. No native finance callbacks.
"""
from copy import deepcopy
from . import liquidity_policy_reference as liquidity_policy
import json
import math
import re
from portfolio_risk.analytics.product_mixed_reference import (
    _check,_cumulative,_rights,_side,_principal,_path_rights,_observe,
    _rights_rows,_reporting_rights,RIGHTS,Account,_decode)
from portfolio_risk.analytics import product_strategy_reference as product
from ._deposit_revision_source_reference import equal as _equal
from ._deposit_revision_source_reference import reconstruct_source, cash_origin_allocations, near, amount, tick
from ._deposit_revision_source_reference import typed_source as _typed
from portfolio_risk.analytics.product_mixed_reference import _normalize, _backbook

def preparation(raw,check=lambda:False):
    """Build supplied-calendar ownership only from independent raw economics."""
    from ._deposit_revision_source_reference import checks
    checks.keys(raw,'schema position_id account_id source recognition_policy interest_rights_policy specification')
    if raw['schema']!='deposit-revised-income-ledger-1' or raw['interest_rights_policy']!='native_earned_fifo_cash_origin-1':
        raise ArithmeticError('explicit revised deposit cash-origin ownership required')
    source=reconstruct_source(raw['source'],check);spec=_normalize(raw['specification'])
    if len(spec['positions'])!=1 or len(spec['accounts'])!=1:raise ArithmeticError('one liability/account required')
    p=spec['positions'][0];a=spec['accounts'][0];anchor=raw['source']['anchor']
    if (p['id']!=raw['position_id'] or p['account']!=raw['account_id'] or a['id']!=raw['account_id']
        or p['kind']!='deposit' or p['classification']!='ac' or a['entity']!=raw['source']['original']['identity']['entity_id']
        or a['currency']!=raw['source']['original']['identity']['currency']):raise ArithmeticError('original liability owner identity differs')
    for k in 'start_day rate annual_pd allowance commitment monthly_runoff floating_beta migration_rate draw_fraction opening_accrued'.split():
        if p[k]!=0.:raise ArithmeticError('legacy liability financial overlay '+k)
    for k in 'annual_fees annual_costs annual_dividends tax_rate'.split():
        if a[k]!=0.:raise ArithmeticError('generic account financial overlay '+k)
    for x in (p['balance'],p['book_adjustment'],a['cash'],a['equity']):amount(x)
    near(p['balance'],anchor['original_principal']);near(p['book_adjustment'],anchor['original_gross_carrying_value']-anchor['original_principal'])
    near(a['cash']-p['balance']-p['book_adjustment'],a['equity'],'original opening equity')
    required={'baseline'}|{x['name'] for x in spec['scenarios']}
    if {x['scenario_id'] for x in source['paths']}!=required:raise ArithmeticError('complete explicit baseline/named source runs')
    original=raw['source']['original'];start=original['as_of_ordinal'];h=original['horizon_days']
    if spec['horizon_days']!=h:raise ArithmeticError('complete source horizon required')
    events=[];plans=[];coverage=[];runs=[];origins=[]
    for pi,path in enumerate(source['paths']):
        tick(check);selector=dict(kind='named',scenario_id=path['run_id']);revision=path['source']['identity']['source_revision']
        runs.append(dict(run_id=path['run_id'],scenario_id=path['scenario_id'],severity=path['severity'],input_revision=revision))
        coverage.append(dict(position_id=p['id'],run=selector,status=dict(kind='complete_event_set')))
        provenance=dict(kind='modeled',model_id='deposit_state_original_yield_revision',model_version='deposit-revised-income-source-1',input_revision=revision)
        def add(eventid,date,components,segments):
            period=dict(start_ordinal=segments[0]['start_ordinal'],end_ordinal=segments[-1]['end_ordinal']) if segments else None
            events.append(dict(position_id=p['id'],event_id=eventid,scenario=deepcopy(selector),payment_ordinal=date,day_offset=date-start,accrual_period=period,currency=a['currency'],components=components,measure='physical',provenance=deepcopy(provenance),adjustment=dict(kind='none')))
            if segments:plans.append(dict(position_id=p['id'],event_id=eventid,run=deepcopy(selector),segments=segments))
        for ci,allocation in enumerate(cash_origin_allocations(path['source'],check)):
            cash=path['source']['cashflows'][ci];eventid=f'cash:{pi}:{ci}'
            segments=[dict(start_ordinal=x['start_ordinal'],end_ordinal=x['end_ordinal'],accrued_interest=-x['amount'],book_amortization=0.) for x in allocation['lots']]
            add(eventid,cash['ordinal'],dict(principal=cash['principal'],cash_interest=cash['cash_interest'],accrued_interest=sum((x['accrued_interest'] for x in segments),0.),book_amortization=0.,fees=0.),segments)
            origins.append(dict(run_id=path['run_id'],event_id=eventid,native_event_id=cash['event_id'],native_kind=cash['kind'],full_native_accrued_settlement=allocation['retirement']=='complete_eligible_origins',allocations=[dict(native_interest_index=x['origin_index'],amount=x['amount']) for x in allocation['lots'] if x['origin_index'] is not None]))
        for i,row in enumerate(path['income']['rows'][1:],1):
            date=row['payment_ordinal'];book=-row['basis_amortization']
            add(f'book:{pi}:{i}',date,dict(principal=0.,cash_interest=0.,accrued_interest=0.,book_amortization=book,fees=0.),[dict(start_ordinal=date-1,end_ordinal=date,accrued_interest=0.,book_amortization=book)])
    events.sort(key=lambda x:x['payment_ordinal'])
    identity=raw['source']['contract']
    owner=dict(position_id=p['id'],account_id=a['id'],currency=a['currency'],direction='payment',ownership='fully_scenario_generated',source_id=identity['id'],source_revision=identity['revision'])
    schedule=dict(schema='dated-cashflow-1',as_of_ordinal=start,horizon_days=h,scenarios=[x['run_id'] for x in source['paths']],events=events)
    dated=dict(schema='dated-store-1',as_of_ordinal=start,reporting_horizon_days=h,runs=runs,owners=[owner],sources=[dict(source_id=identity['id'],source_revision=identity['revision'],schedule=schedule,coverage=coverage)],accrual_plans=plans)
    request=dict(schema='dated-ledger-1',monetary_unit='currency_units',recognition_policy=deepcopy(raw['recognition_policy']),specification=deepcopy(raw['specification']),dated=dated)
    _,_,admitted,_=_backbook(request,spec,check)
    for e in admitted:e['owner']=owner
    return request,source,spec,admitted,origins

def reconstruct(raw,check=lambda:False):
    request,source,spec,events,origins=preparation(raw,check)
    output=_replay(dict(backbook=request,revision_review=raw['source']['policy']['review']),spec,events,[],request['dated']['as_of_ordinal']+spec['horizon_days'],{},{},check,{x['run_id']:x for x in source['paths']})
    return dict(prepared_request=request,source=source,cash_origins=origins,tables=output)

def _encoded_size(value):
    # JSON token lengths only; source identity equality is verified separately.
    s=json.dumps(value,separators=(',',':'),ensure_ascii=False,allow_nan=False)
    s=re.sub(r'e([+-])0+(\d+)',r'e\1\2',s)
    s=re.sub(r'e\+(\d+)',r'e\1',s)
    return len(s.encode('utf8'))

def verify_preparation(raw,workflow,check=lambda:False):
    from ._deposit_revision_source_reference import checks,verify_source
    from portfolio_risk.analytics.product_mixed_reference import _dated_work
    tick(check);checks.keys(workflow,'schema raw_request prepared_request source_receipt')
    checks.exact(workflow['schema'],raw['schema'],'workflow schema');checks.exact(workflow['raw_request'],raw,'retained original raw')
    expected=reconstruct(raw,check);_equal(workflow['prepared_request'],expected['prepared_request'],'derived calendar')
    checks.exact(workflow['prepared_request']['specification'],raw['specification'],'raw original opening')
    for k in ('runs','owners'):
        checks.exact(workflow['prepared_request']['dated'][k],expected['prepared_request']['dated'][k],'immutable prepared '+k)
    checks.exact(workflow['prepared_request']['recognition_policy'],raw['recognition_policy'],'reviewed recognition identity')
    receipt=workflow['source_receipt']
    checks.keys(receipt,'original_source original_pv paths revisions admission cash_origins interest_rights_policy original_anchors revision_policy accounting_standard_approval installed_adoption capital_ftp_eligible')
    verify_source(raw['source'],receipt,check)
    for k in ('accounting_standard_approval','installed_adoption','capital_ftp_eligible'):checks.exact(receipt[k],False,'analytical receipt '+k)
    for k,sourcekey in [('original_anchors','anchor'),('revision_policy','policy')]:checks.exact(receipt[k],_typed(raw['source'][sourcekey]),'immutable '+k)
    checks.exact(receipt['interest_rights_policy'],raw['interest_rights_policy'],'reviewed rights ownership')
    checks.exact(receipt['original_source']['identity'],raw['source']['original']['identity'],'original physical identity')
    for i,path in enumerate(receipt['paths']):
        for k in ('run_id','scenario_id','severity'):
            checks.exact(path[k],expected['source']['paths'][i][k],'immutable path '+k)
        checks.exact(path['source']['identity'],raw['source']['runs'][i]['revised']['identity'],'revised physical identity')
        for k in ('contract','original_yield_anchor','policy','original_annual_effective_rate','opening_annual_effective_rate'):
            checks.exact(path['income_request'][k],expected['source']['paths'][i]['income_request'][k],'original EIR policy '+k)
    _equal(receipt['cash_origins'],expected['cash_origins'],'exact cash-origin order')
    anchor=raw['source']['anchor'];revisions=[]
    for i,path in enumerate(expected['source']['paths']):
        runindex=0 if path['scenario_id']=='baseline' else 1+[s['name'] for s in raw['specification']['scenarios']].index(path['scenario_id'])
        revisions.append(dict(run_index=runindex,position_index=0,account_index=0,original_basis=anchor['original_gross_carrying_value']-anchor['original_principal'],revised_basis=path['revised_carrying_value']-anchor['original_principal'],amount=path['basis_revision'],review=deepcopy(raw['source']['policy']['review'])))
    _equal(receipt['revisions'],revisions,'reviewed once revision')
    h=raw['source']['original']['horizon_days'];n=len(revisions);size=_encoded_size(_typed(raw['source']))
    sourcework=(n+1)*h*4096+size*8;reserve=(n+1)*(h+2)*32768+size*12+n*(h+1)*65536
    request=expected['prepared_request'];spec=_normalize(raw['specification']);_,_,events,end=_backbook(request,spec,check)
    ledgerwork=n*(h+1)*8192+_dated_work(spec,events,end,n)
    admission=dict(work_units=sourcework+ledgerwork,reserved_bytes=reserve,native_source_work=sourcework,ledger_work=ledgerwork)
    if admission['work_units']>8000000 or reserve>32*1024*1024:raise ArithmeticError('independent joined8M/32MiB admission')
    checks.exact(receipt['admission'],admission,'checked source+one-GL work')
    return expected

def verify_stream(raw,workflow,messages,check=lambda:False):
    """Check finite artifact stream; this proof is not a production backend."""
    from ._deposit_revision_source_reference import checks
    expected=verify_preparation(raw,workflow,check);tables=expected['tables']
    tables={('path' if k=='daily_balance' else k):v for k,v in tables.items() if k not in {'reported_daily_balance','reporting_rights','acquisition_lineage'}}
    cursors={k:0 for k in tables};dictionary=[];complete=False;outputbytes=_encoded_size(workflow)
    for message in messages:
        tick(check);outputbytes+=_encoded_size(message)+1
        if outputbytes>32*1024*1024:raise ArithmeticError('bounded source+stream32MiB')
        if complete:raise ArithmeticError('extra messages after actual completion')
        if 'complete' in message:
            checks.exact(message,dict(complete=True,protocol=3),'actual native completion');complete=True;continue
        table=message.get('table')
        if table not in tables:raise ArithmeticError('unknown or unsupported financial table '+str(table))
        if table=='journal':
            checks.keys(message,'table dictionary columns');dictionary.extend(message['dictionary']);columns=message['columns']
            checks.keys(columns,'transaction_number scenario day account event gl_account instrument_id value');length=len(columns['day'])
            if any(type(v) is not list or len(v)!=length for v in columns.values()):raise ArithmeticError('ragged journal columns')
            rows=[]
            for i in range(length):
                tick(check);row={}
                for k,v in columns.items():
                    x=v[i]
                    if k in {'scenario','account','event','gl_account','instrument_id'}:
                        if type(x) is not int or not 0<=x<len(dictionary):raise ArithmeticError('journal dictionary index')
                        x=dictionary[x]
                    row[k]=x
                rows.append(row)
        else:
            checks.keys(message,'table rows');rows=message['rows']
        if type(rows) is not list:raise ArithmeticError('bounded rows required')
        start=cursors[table];_equal(rows,tables[table][start:start+len(rows)],'ordered single-GL stream/'+table);cursors[table]+=len(rows)
    if not complete or any(cursors[k]!=len(v) for k,v in tables.items()):raise ArithmeticError('incomplete producer/tables')
    return dict(verified=True,physical_deposit_source_recomputed=True,original_yield_revision_recomputed=True,ordered_deposit_journal_recomputed=True,complete_gross_cash_origin_rights_verified=True,accounting_standard_approval=False,capital_ftp_eligible=False,installed_adoption=False)

def _replay(raw, spec, events, acquisitions, end, pool_for_event, coupon_signs, check, income_paths):
    """Own the complete journal ordering; producer postings are never inputs."""
    dated = raw['backbook']['dated']; asof = dated['as_of_ordinal']; horizon = end-asof
    run_by_scene = {r['scenario_id']:r for r in dated['runs']}
    scenarios = ['baseline']+[s['name'] for s in spec['scenarios']]
    output = {k:[] for k in ('journal', 'trial_balance', 'closing_statements', 'daily_balance',
        'dated_rights', 'event_lineage', 'acquisition_lineage', 'exposures', 'ledger',
        'reporting_rights', 'reported_daily_balance')}
    all_observations=[]
    from portfolio_risk.analytics.journal import statements
    for scenario in scenarios:
        _check(check); run = run_by_scene[scenario]; runid = run['run_id']
        positions = deepcopy(spec['positions']); by_id = {p['id']:p for p in positions}
        accounts = deepcopy(spec['accounts']); account_by_id = {a['id']:a for a in accounts}
        selected = [e for e in events if e['selector'] in {None, runid}]
        selected_acq = [a for a in acquisitions if a['run_id'] == runid]
        rights = {p['id']:[0., 0., 0., 0.] for p in positions}; accrued = {p['id']:p['opening_accrued'] for p in positions}
        state = []; pools = {a['event_id']:0. for a in selected_acq}; gl = {}; sequence = 0
        aggregates = {}
        for e in selected:
            interest = _cumulative(e['segments'], asof, 'accrued_interest'); book = _cumulative(e['segments'], asof, 'book_amortization')
            state.append(dict(earned=interest, book=book, settled=False))
            values = _rights(interest, 0., False)
            for i, value in enumerate(values):
                rights[e['event']['position_id']][i] += value
        for p in positions:
            _equal(sum(rights[p['id']]), _side(p)*p['opening_accrued'], 'mixed/opening_source_rights')

        def post(day, account, event, lines, cash=0., earnings=0., position='', aggregate=True):
            nonlocal sequence
            lines = list(lines)
            if aggregate:
                lines += [('cash', '', cash), ('pnl:'+event, position, -earnings)]
            lines = [line for line in lines if line[2] != 0.]
            if lines:
                if abs(math.fsum(l[2] for l in lines)) > 1e-9*max(1., sum(abs(l[2]) for l in lines)):
                    raise ArithmeticError('mixed independent journal event fails balance')
                sequence += 1
                for code, instrument, value in lines:
                    key = account, code, instrument
                    gl[key] = product._finite(gl.get(key, 0.)+value)
                    output['journal'].append(dict(transaction_number=sequence, scenario=scenario, day=day,
                        account=account, event=event, gl_account=code, instrument_id=instrument, value=value))
            if aggregate:
                a = account_by_id[account]; a['cash'] += cash; a['equity'] += earnings
                sums = aggregates.setdefault((account, event), [0., 0.]); sums[0] += cash; sums[1] += earnings
            return sequence if lines else None

        for a in accounts:
            lines = [('cash', '', a['cash']), ('opening_equity', '', -a['equity'])]
            for p in positions:
                if p['account'] == a['id']:
                    lines += [(_principal(p), p['id'], _side(p)*p['balance']),
                              ('book_adjustment', p['id'], _side(p)*p['book_adjustment']),
                              ('allowance', p['id'], -p['allowance'])]
                    lines += [(code, p['id'], value) for code, value in zip(RIGHTS, rights[p['id']], strict=True)]
            post(0, a['id'], 'opening', lines, aggregate=False)

        # Reviewed instantaneous catchup follows the ORIGINAL opening journal.
        revision=income_paths[runid]['basis_revision']
        owner_position=positions[0]
        post(0,owner_position['account'],'deposit_basis_revision',
             [('book_adjustment',owner_position['id'],-revision)],
             earnings=-revision,position=owner_position['id'])
        owner_position['book_adjustment']+=revision
        output.setdefault('deposit_revision',[]).append(dict(scenario=scenario,run_id=runid,
            position_id=owner_position['id'],ordinal=asof,
            original_basis=spec['positions'][0]['book_adjustment'],
            revised_basis=owner_position['book_adjustment'],basis_revision=revision,
            review=deepcopy(raw['revision_review']),cash=0.,original_yield_retained=True,
            accounting_standard_approval=False,capital_ftp_eligible=False))

        def apply_event(index, day, interest, book, settlement):
            e = selected[index]; event = e['event']; v = state[index]; pos = by_id[event['position_id']]
            pool = pool_for_event.get((runid, event['event_id']))
            before = (_path_rights(_side(pos), coupon_signs[pool], pools[pool]) if pool is not None
                      else _rights(v['earned'], event['components']['cash_interest'], v['settled']))
            if settlement and v['settled']:
                raise ArithmeticError('mixed duplicate settlement')
            v['earned'] += interest; v['book'] += book
            if settlement:
                v['settled'] = True
            if pool is not None:
                pools[pool] += interest-(event['components']['cash_interest'] if settlement else 0.)
                after = _path_rights(_side(pos), coupon_signs[pool], pools[pool])
            else:
                after = _rights(v['earned'], event['components']['cash_interest'], v['settled'])
            delta = [a-b for a, b in zip(after, before, strict=True)]
            for i, value in enumerate(delta):
                rights[pos['id']][i] += value
            accrued[pos['id']] = _side(pos)*sum(rights[pos['id']])
            pc = event['components']['principal'] if settlement else 0.; ic = event['components']['cash_interest'] if settlement else 0.
            if abs(pc) > pos['balance']+1e-9:
                raise ArithmeticError('mixed settlement exceeds live principal')
            pos['balance'] = max(pos['balance']-abs(pc), 0.); pos['book_adjustment'] += _side(pos)*book
            lines = [(_principal(pos), pos['id'], -_side(pos)*abs(pc)), ('book_adjustment', pos['id'], book)]
            lines += [(code, pos['id'], value) for code, value in zip(RIGHTS, delta, strict=True)]
            tx = post(day, pos['account'], 'dated_settlement' if settlement else 'dated_recognition', lines,
                      cash=pc+ic, earnings=interest+book, position=pos['id'])
            owner = e['owner']; provenance = event['provenance']
            output['event_lineage'].append(dict(scenario=scenario, run_id=runid, input_revision=run['input_revision'], day=day,
                transaction_number=tx, position_id=pos['id'], event_id=event['event_id'], phase='settlement' if settlement else 'recognition',
                source_id=owner['source_id'], source_revision=owner['source_revision'],
                provenance_json=json.dumps(provenance, separators=(',', ':'), ensure_ascii=False), measure=event['measure'], ownership=owner['ownership'],
                policy_id=raw['backbook']['recognition_policy']['policy_id'], policy_revision=raw['backbook']['recognition_policy']['policy_revision'],
                principal=pc, cash_interest=ic, accrued_interest=interest, book_amortization=book, fees=0.))

        for day in range(horizon+1):
            _check(check); ordinal = asof+day
            for a in selected_acq:
                if a['ordinal'] != ordinal:
                    continue
                pos = by_id[a['position_id']]; side = _side(pos); basis = a['cost']-a['principal']
                pos['balance'] += a['principal']; pos['book_adjustment'] += basis
                tx = post(day, pos['account'], 'dated_acquisition',
                          [(_principal(pos), pos['id'], side*a['principal']), ('book_adjustment', pos['id'], side*basis)],
                          cash=-side*a['cost'], position=pos['id'])
                output['acquisition_lineage'].append(dict(scenario=scenario, severity=1., run_id=runid, day=day, ordinal=ordinal,
                    position_id=pos['id'], account=pos['account'], event_id=a['event_id'], source=a['source'], path_id=a['path_id'],
                    source_weight=a['source_weight'], allocation_factor=a['allocation_factor'], original_yield_anchor=a['original_yield_anchor'],
                    measure='physical', policy=raw.get('policy',raw['candidate']['policy'])['identity'], principal=a['principal'], cost=a['cost'], basis=basis, transaction_number=tx))
            if day:
                for i, e in enumerate(selected):
                    interest = book = 0.; active = False
                    for s in e['segments']:
                        if s['start_ordinal'] <= ordinal-1 < s['end_ordinal']:
                            n = s['end_ordinal']-s['start_ordinal']; f0 = (ordinal-1-s['start_ordinal'])/n; f1 = (ordinal-s['start_ordinal'])/n
                            interest += s['accrued_interest']*f1-s['accrued_interest']*f0
                            book += s['book_amortization']*f1-s['book_amortization']*f0; active = True
                    if active:
                        apply_event(i, day, interest, book, False)
            for i, e in enumerate(selected):
                if e['event']['payment_ordinal'] == ordinal:
                    apply_event(i, day, 0., 0., True)
            for account in accounts:
                if account['cash'] < account['cash_floor']-1e-9:
                    raise ArithmeticError('end-of-day cash floor breached; replacement funding unsupported')
            rows = _observe(spec, positions, accounts, rights, accrued, scenario, day)
            all_observations.extend(rows)
            if day <= 30 or day % 30 == 0 or day == horizon or day == spec['horizon_days']:
                output['daily_balance'] += rows
            if ordinal == asof+spec['horizon_days']:
                output['reported_daily_balance'] += rows
            if day == 0 or day % 30 == 0 or day == horizon:
                for p in positions:
                    output['exposures'].append(dict(scenario=scenario, day=day, account=p['account'], position=p['id'], kind=p['kind'],
                        balance=p['balance'], allowance=p['allowance'], watch_fraction=p['watch_fraction'], commitment=p['commitment'],
                        market_factor=1., encumbered_fraction=p['encumbered_fraction'], accrued_interest=accrued[p['id']]))
            for (account, event), sums in aggregates.items():
                if any(v != 0. for v in sums):
                    output['ledger'].append(dict(scenario=scenario, day=day, account=account, event=event, cash=sums[0], earnings=sums[1], aoci=0., memo_amount=0.))
            aggregates.clear()
        trial = [dict(scenario=scenario, account=a, gl_account=g, instrument_id=i, balance=v) for (a, g, i), v in sorted(gl.items())]
        output['dated_rights'] += _rights_rows(selected,state,scenario,runid,end,pools,pool_for_event,selected_acq)
        output['trial_balance'] += trial
        closing, _ = statements(trial, {a['id']:_decode(Account, a) for a in spec['accounts']}, [scenario])
        output['closing_statements'] += closing
        for p in positions:
            _equal(p['balance'], 0., 'mixed/complete_principal_tail')
            _equal(p['book_adjustment'], 0., 'mixed/complete_basis_tail')
            for value in rights[p['id']]:
                _equal(value, 0., 'mixed/complete_gross_right_tail')
    output['reporting_rights'] = _reporting_rights(raw,events,acquisitions,pool_for_event,coupon_signs)
    output['summary']=[]
    output['breaches']=[]
    for scenario in scenarios:
        first=set()
        for row in all_observations:
            if row['scenario']!=scenario:continue
            for metric in ('cash','cet1','leverage','lcr','nsfr','htm'):
                key=(row['account'],metric)
                if row[metric+'_headroom'] is not None and row[metric+'_headroom'] < -1e-9 and key not in first:
                    first.add(key);output['breaches'].append(dict(scenario=scenario,account=row['account'],day=row['day'],metric=metric))
    for scenario in scenarios:
        for account in spec['accounts']:
            rows=[x for x in all_observations if x['scenario']==scenario and x['account']==account['id']]
            last=rows[-1];minimum=min([account['cash']]+[x['cash'] for x in rows])
            summary=dict(scenario=scenario,account=account['id'],entity=account['entity'],currency=account['currency'],final_cash=last['cash'],final_equity=last['equity'],minimum_cash=minimum,peak_cash_shortfall=max(0.,account['cash_floor']-minimum),max_reconciliation_error=max(abs(x['reconciliation_error']) for x in rows))
            for label,metric in [('cash','cash'),('capital','cet1'),('leverage','leverage'),('lcr','lcr'),('nsfr','nsfr'),('htm','htm')]:
                summary['first_'+label+'_breach_day']=next((x['day'] for x in rows if x[metric+'_headroom'] is not None and x[metric+'_headroom'] < -1e-9),None)
            liquidity_policy.apply_summary(summary, account, rows)
            output['summary'].append(summary)
    output['consolidated']=[{k:x[k] for k in ('scenario','currency','assets','liabilities','equity','cash','restricted_cash')} for x in output['closing_statements']]
    totals={}
    for row in output['ledger']:
        sums=totals.setdefault((row['scenario'],row['account'],row['event']),[0.,0.,0.])
        for i,k in enumerate(('cash','earnings','aoci')):sums[i]+=row[k]
    output['attribution']=[]
    keys=sorted({(a,e) for _,a,e in totals})
    for scenario in scenarios[1:]:
        for a,event in keys:
            old=totals.get(('baseline',a,event),[0.,0.,0.]);new=totals.get((scenario,a,event),[0.,0.,0.])
            if ('baseline',a,event) in totals or (scenario,a,event) in totals:
                output['attribution'].append(dict(scenario=scenario,account=a,event=event,cash_delta=new[0]-old[0],earnings_delta=new[1]-old[1],aoci_delta=new[2]-old[2]))
    return output
