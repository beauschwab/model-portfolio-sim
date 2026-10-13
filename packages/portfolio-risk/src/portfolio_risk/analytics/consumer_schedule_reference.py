"""Independent simple-interest consumer schedule oracle for explicit dated adoption.
This module is a publication verifier, never an execution backend.
"""
from copy import deepcopy
import calendar
import datetime as dt
from .credit_path_verification import equal

def year_fraction(start,end,convention):
    if convention=='act360':return (end-start)/360.
    if convention=='act365_fixed':return (end-start)/365.
    if convention!='act_act_isda':raise ArithmeticError('unsupported consumer reference accrual')
    total=0.;cursor=start
    while cursor<end:
        year=dt.date.fromordinal(cursor).year
        boundary=min(end,dt.date(year+1,1,1).toordinal()) if year<9999 else end
        total+=(boundary-cursor)/(366. if calendar.isleap(year) else 365.);cursor=boundary
    return total

def checkpoint(check):
    if check():raise InterruptedError('consumer source verification cancelled')

def project(contract,history,scenario,check=lambda:False):
    if contract['initial_state']!='performing' or any(contract[k]!=0 for k in ('opening_accrued_interest','opening_unrecognized_interest','opening_fees')):
        raise ArithmeticError('unsupported scheduled consumer opening')
    if contract['day_count'] not in {'act360','act365_fixed'}:
        raise ArithmeticError('scheduled consumer ledger requires ACT360 or ACT365F')
    dates=[s['ordinal'] for s in history['steps']]
    if dates!=sorted(set(dates)):raise ArithmeticError('consumer action dates must be unique and ordered')
    actions={s['ordinal']:s['action'] for s in history['steps']}
    if any(a['kind'] not in {'payment','payoff','change_payment'} for a in actions.values()):raise ArithmeticError('unsupported scheduled consumer action')
    balance=contract['opening_principal'];interest=0.;payment=contract['original_payment'];collected=0.;state='performing'
    previous=contract['as_of_ordinal'];end=previous+contract['horizon_days'];cash=[];origins=[];points=[]
    dues=set(contract['payment_dates'])
    for day in sorted(dues|set(actions)|{end}):
        checkpoint(check)
        if not contract['as_of_ordinal']<day<=end:raise ArithmeticError('consumer date outside admitted horizon')
        if day>previous and balance>0 and state!='paid_off':
            earned=balance*contract['note_rate']*year_fraction(previous,day,contract['day_count'])
            interest+=earned
            if earned:origins.append(dict(origin_id='interest-'+str(len(origins)),start_ordinal=previous,end_ordinal=day,interest=earned))
        previous=day;action=actions.get(day);amount=None;payoff=False
        if action:
            if action['kind']=='payment':amount=action['amount']
            elif action['kind']=='payoff':amount=balance+interest;payoff=True
            elif action['kind']=='change_payment':payment=action['amount']
        if amount is None and day in dues and state=='performing':
            amount=balance+interest if day==contract['payment_dates'][-1] else min(payment,balance+interest)
        if amount is not None:
            paid_interest=min(amount,interest);interest-=paid_interest
            principal=min(max(amount-paid_interest,0.),balance);balance-=principal;collected+=principal
            fee=principal*contract['prepayment_penalty_fraction'] if payoff else 0.
            if fee:raise ArithmeticError('unsupported scheduled consumer fee')
            if amount>0:cash.append(dict(event_id='payment-'+str(len(cash)),ordinal=day,kind='prepayment' if payoff else 'payment',principal=principal,interest=paid_interest,fees=0.))
            if balance==0 and interest==0:state='paid_off'
        points.append(dict(ordinal=day,state=state,principal=balance,accrued_interest=interest,unrecognized_interest=0.,fees=0.,scheduled_payment=payment))
    identity=dict(contract['identity'],measure='physical',assumption_id=history['assumption_id'],run_id=scenario['run_id'])
    return dict(schema='consumer-installment-1',identity=identity,original_principal=contract['original_principal'],original_payment=contract['original_payment'],
        disclosed_apr=contract['disclosed_apr'],payment_dates=contract['payment_dates'],cashflows=cash,recognition=origins,points=points,default=None,
        principal_collected=collected,closing_principal=balance,closing_accrued_interest=interest,closing_unrecognized_interest=0.,closing_fees=0.)

def verify_preparation(raw,prepared,receipt,check=lambda:False):
    checkpoint(check)
    if receipt['raw_request']!=raw:raise ArithmeticError('consumer exact raw source identity differs')
    if (raw['schema']!='graph-dated-ledger-1' or receipt['schema']!='graph-dated-ledger-source-1' or
            receipt['normalization_policy']!='native_par_opening_anchors-1'):
        raise ArithmeticError('consumer source schema/policy identity differs')
    graph=raw['graph'];capture=receipt['capture'];owners={o['position_id']:o for o in raw['owners']}
    if (graph['schema']!='advanced-product-graph-1' or receipt['graph']['schema']!=graph['schema'] or
            receipt['graph']['identity']!=graph['identity'] or capture['schema']!='advanced-scheduled-capture-1'):
        raise ArithmeticError('consumer graph/capture identity differs')
    positions=graph['positions'];scenarios=graph['scenarios']
    if (len(owners)!=len(raw['owners']) or set(owners)!={p['position_id'] for p in positions} or
            len({o['source_id'] for o in raw['owners']})!=len(owners)):
        raise ArithmeticError('consumer complete unique ownership differs')
    for owner in owners.values():
        if (owner['ownership']!='fully_scenario_generated' or owner['direction']!='receipt' or
                owner['source_revision']!=graph['identity']['input_revision']):
            raise ArithmeticError('consumer source ownership policy differs')
    policy=raw['capture_policy']
    if (policy['recognition_model']!='consumer_simple_interest_par-1' or
            policy['book_basis']!='par_note_simple_interest' or policy['fees']!='reject'):
        raise ArithmeticError('consumer unsupported capture policy')
    mapping={m['run_id']:m['ledger_scenario_id'] for m in raw['run_mapping']}
    if len(mapping)!=len(raw['run_mapping']) or set(mapping)!={s['run_id'] for s in scenarios}:
        raise ArithmeticError('consumer complete unique run mapping differs')
    for scenario in scenarios:
        base=scenario['scenario_id']==graph['base_scenario_id']
        if (mapping[scenario['run_id']]=='baseline')!=base or (base and scenario['severity']!=1.):
            raise ArithmeticError('consumer base must map exclusively to baseline severity one')
    physical=[s for s in receipt['graph']['native_sources'] if s['role']=='physical']
    expected_sources=[];expected_plans=[];openings=[];expected_physical=[]
    for position in graph['positions']:
        checkpoint(check);product=position['product'];contract=product['contract'];owner=owners[position['position_id']]
        events=[];coverage=[]
        for scenario in graph['scenarios']:
            histories=[h for h in product['histories'] if h['scenario_id']==scenario['scenario_id']]
            if len(histories)!=1 or len(histories[0]['physical'])!=1 or histories[0]['physical'][0]['weight']!=1.:
                raise ArithmeticError('consumer scheduled capture requires one physical path weight one')
            history=histories[0]['physical'][0]
            output=project(contract,history,scenario,check)
            expected_physical.append(dict(position_id=position['position_id'],scenario_id=scenario['scenario_id'],run_id=scenario['run_id'],path_id=history['path_id'],weight=1.,role='physical',projection=output))
            selector=dict(kind='named',scenario_id=scenario['run_id'])
            coverage.append(dict(position_id=position['position_id'],run=selector,status=dict(kind='complete_event_set') if output['cashflows'] else dict(kind='no_events',rationale='complete native physical projection has no cash events')))
            remaining=[r['interest'] for r in output['recognition']];cursor=0
            for cash in output['cashflows']:
                checkpoint(check)
                needed=cash['interest'];segments=[]
                while needed>0 and cursor<len(remaining):
                    origin=output['recognition'][cursor];allocated=min(needed,remaining[cursor])
                    if allocated>0:segments.append(dict(origin_id=origin['origin_id'],start_ordinal=origin['start_ordinal'],end_ordinal=origin['end_ordinal'],accrued_interest=allocated,book_amortization=0.))
                    remaining[cursor]-=allocated;needed-=allocated
                    if remaining[cursor]==0:cursor+=1
                if needed>max(1e-9,cash['interest']*1e-12):raise ArithmeticError('consumer cash lacks independently earned origins')
                if segments:segments[-1]['accrued_interest']+=cash['interest']-sum(s['accrued_interest'] for s in segments)
                event_id=cash['kind']+':'+str(cash['ordinal'])
                period=dict(start_ordinal=segments[0]['start_ordinal'],end_ordinal=segments[-1]['end_ordinal']) if segments else None
                events.append(dict(position_id=position['position_id'],event_id=event_id,scenario=selector,payment_ordinal=cash['ordinal'],day_offset=cash['ordinal']-graph['as_of_ordinal'],
                    accrual_period=period,currency=graph['currency'],components=dict(principal=cash['principal'],cash_interest=cash['interest'],accrued_interest=cash['interest'],book_amortization=0.,fees=0.),measure='physical',
                    provenance=dict(kind='modeled',model_id='consumer-installment-1',model_version='consumer-installment-1',input_revision=graph['identity']['input_revision']),adjustment=dict(kind='none')))
                if segments:expected_plans.append(dict(position_id=position['position_id'],event_id=event_id,run=selector,segments=segments))
            if sum(remaining)>1e-9:raise ArithmeticError('consumer unsettled rights omitted')
        events.sort(key=lambda e:e['payment_ordinal'])
        expected_sources.append(dict(source_id=owner['source_id'],source_revision=owner['source_revision'],schedule=dict(schema='dated-cashflow-1',as_of_ordinal=graph['as_of_ordinal'],horizon_days=contract['horizon_days'],scenarios=[s['run_id'] for s in graph['scenarios']],events=events),coverage=coverage))
        openings.append(dict(position_id=position['position_id'],principal=contract['opening_principal'],accrued_interest=0.,gross_carrying_value=contract['opening_principal']))
    equal(physical,expected_physical,'consumer/physical_sources')
    for key,want in [('identity',graph['identity']),('policy',raw['capture_policy']),('owners',raw['owners']),('scenarios',graph['scenarios']),('openings',openings),('sources',expected_sources),('accrual_plans',expected_plans)]:equal(capture[key],want,'consumer/capture/'+key)
    specification=deepcopy(raw['specification']);anchors={o['position_id']:o for o in openings}
    ids=[p['id'] for p in specification['positions']]
    if len(ids)!=len(set(ids)) or not anchors.keys()<=set(ids):
        raise ArithmeticError('consumer missing or duplicate ledger position mapping')
    for p in specification['positions']:
        if p['id'] in anchors:
            owner=owners[p['id']];a=anchors[p['id']]
            if p.get('kind')!='loan' or p.get('account')!=owner['account_id']:
                raise ArithmeticError('consumer explicit ledger loan/account mapping differs')
            additions=dict(balance=a['principal'],opening_accrued=0.,book_adjustment=0.,source_id=owner['source_id'])
            if any(key in p and p[key]!=value for key,value in additions.items()):
                raise ArithmeticError('consumer explicit ledger opening/source anchor conflicts')
            p.update(additions)
    runs=[dict(run_id=s['run_id'],scenario_id=mapping[s['run_id']],severity=s['severity'],input_revision=graph['identity']['input_revision']) for s in graph['scenarios']]
    dated=dict(schema='dated-store-1',as_of_ordinal=graph['as_of_ordinal'],reporting_horizon_days=graph['reporting_ordinals'][-1]-graph['as_of_ordinal'],runs=runs,
        owners=[dict(o,currency=graph['currency']) for o in raw['owners']],sources=expected_sources,
        accrual_plans=[dict(p,segments=[{k:v for k,v in s.items() if k!='origin_id'} for s in p['segments']]) for p in expected_plans])
    equal(prepared,dict(schema='dated-ledger-1',monetary_unit=raw['monetary_unit'],recognition_policy=raw['recognition_policy'],specification=specification,dated=dated),'consumer/prepared')
    return dict(physical_consumer_source_recomputed=True,accrual_origins_retained=True,pricing_graph_independently_verified=False,empirical_validation=False)
