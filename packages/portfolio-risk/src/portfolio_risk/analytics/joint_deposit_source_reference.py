"""Independent source2 equations and exact dated/FIFO reconstruction for joint owner.
No producer journal, native callback or standalone-ledger receipt is authority.
"""

from copy import deepcopy
from dataclasses import fields
from fractions import Fraction
import json,math,struct
from . import _deposit_portfolio_income_reference as c
from . import _deposit_portfolio_principal_reference as source
from ._deposit_portfolio_calendar import _normalize,_backbook,Account,Position,Scenario

from .deposit_portfolio_reference import source_plan,portfolio_admission,gross,fifo

def reconstruct(raw,prepared_native,sources,usage,admission,check=lambda:False):
    c.tick(check);c.keys(raw,'schema runs positions recognition_policy interest_rights_policy specification')
    c.exact(raw['schema'],'deposit-performing-portfolio-ledger-2');c.exact(raw['interest_rights_policy'],'native_earned_counted_fifo_cash_origin-2')
    receipt=dict(sources,usage=usage,admission=admission)
    c.keys(sources,'owners cash_origins');c.exact(admission,portfolio_admission(raw))
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
            native=source.reconstruct(src,check);actual=deepcopy(emitted['paths'][pi]['native']['native']);actual.pop('admission');expected=deepcopy(native);expected.pop('admission');c.exact(actual['raw_request'],expected['raw_request']);c.equal(actual,expected)
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
    c.equal(prepared_native,prepared);c.exact(prepared_native['specification'],raw['specification']);c.exact(prepared_native['recognition_policy'],raw['recognition_policy']);c.equal(receipt['cash_origins'],origins)
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
    return inputs
