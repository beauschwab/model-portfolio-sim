"""Rust dated-ledger transport and independent persisted-source verification.

Financial scheduling/recognition executes in Rust. The Python calculations in
``verify_dated_sources`` are an independent publication gate over persisted
evidence, never an execution backend or a model-parameter fallback.
"""
from collections import defaultdict
from copy import deepcopy
import json
import math
import os
from pathlib import Path
import tempfile
import time
import uuid

import polars as pl

from portfolio_risk.analytics.balance_stream import SCHEMAS
from portfolio_risk.analytics.journal import statements
from .saved_derivative_identity import exact_identity

MODEL_VERSION = 'dated-ledger-1'
RIGHTS_GL = ('dated_interest_receivable', 'dated_interest_payable', 'dated_prepaid_expense', 'dated_deferred_income')
DATED_SCHEMAS = dict(SCHEMAS,
    event_lineage={
        'scenario':pl.String,'run_id':pl.String,'input_revision':pl.String,'day':pl.Int64,
        'transaction_number':pl.Int64,'position_id':pl.String,'event_id':pl.String,'phase':pl.String,
        'source_id':pl.String,'source_revision':pl.String,'provenance_json':pl.String,'measure':pl.String,
        'ownership':pl.String,'policy_id':pl.String,'policy_revision':pl.String,
        **{k:pl.Float64 for k in ('principal','cash_interest','accrued_interest','book_amortization','fees')},
    },
    dated_rights={
        **{k:pl.String for k in ('scenario','run_id','account','position_id','event_id','source_id','source_revision')},
        'payment_ordinal':pl.Int64,'full_contract_end_ordinal':pl.Int64,'cash_settled':pl.Boolean,
        **{k:pl.Float64 for k in ('earned_interest','cash_interest_settled','signed_interest_right',
            'recognized_book_amortization','remaining_principal_settlement','remaining_cash_interest',
            'remaining_fees','remaining_interest_recognition','remaining_book_recognition')},
    })


def _close(a, b):
    return a is not None and b is not None and math.isfinite(a) and math.isfinite(b) and math.isclose(a,b,rel_tol=1e-12,abs_tol=1e-9)


def _selector(selector):
    return None if selector['kind']=='all' else selector['scenario_id']


def _cumulative(segments, date, field):
    # Independent full source calculation; production uses a native active cursor.
    return math.fsum(s[field]*max(0,min(date,s['end_ordinal'])-s['start_ordinal']) /
                     (s['end_ordinal']-s['start_ordinal']) for s in segments)


def _rights(earned, cash, settled):
    value=earned-(cash if settled else 0.)
    return (0.,0.,max(value,0.),min(value,0.)) if settled else (max(value,0.),min(value,0.),0.,0.)


def verify_dated_sources(raw, spec, writer, check=lambda:False):
    """Reconcile immutable source events, once-only lineage, postings and rights.

    Memory retains source/event state and closing GL, not a day-by-event result
    tensor or every transaction. Lineage and journal partitions are merged as
    ordered streams. This verifies the admitted scheduled-recognition contract;
    it does not validate the producer's empirical behavior or credit forecasts.
    """
    dated=raw['dated'];asof=dated['as_of_ordinal'];end=asof+spec['horizon_days']
    owners={o['position_id']:o for o in dated['owners']}
    run_cases={r['scenario_id']:r for r in dated['runs'] if r['severity']==1.}
    scenarios=['baseline']+[s.name for s in spec['scenarios']]
    plans={(p['position_id'],p['event_id'],_selector(p['run'])):p['segments'] for p in dated['accrual_plans']}
    positions={p.id:p for p in spec['positions']}
    states={};expected_principal={};expected_book={}
    for scenario in scenarios:
        run=run_cases[scenario]
        for position in owners:
            p=positions[position];sign=1. if p.kind in {'loan','security','reverse_repo'} else -1.
            expected_principal[scenario,position]=sign*p.balance
            expected_book[scenario,position]=sign*p.book_adjustment
        for source in dated['sources']:
            for event in source['schedule']['events']:
                selector=_selector(event['scenario'])
                if selector is not None and selector!=run['run_id']:continue
                key=scenario,event['position_id'],event['event_id']
                if key in states:raise ArithmeticError('duplicate dated source event')
                segments=plans.get((event['position_id'],event['event_id'],selector),[])
                states[key]=dict(source=source,event=event,segments=segments,run=run,
                    earned=_cumulative(segments,asof,'accrued_interest'),
                    book=_cumulative(segments,asof,'book_amortization'),settled=False,
                    recognition_count=0,last_recognition_day=0,settlement_count=0)
    lineage_count=0
    def records():
        nonlocal lineage_count
        for frame in writer.frames('event_lineage'):
            check()
            for row in frame.iter_rows(named=True):
                lineage_count+=1
                if lineage_count%256==0:check()
                key=row['scenario'],row['position_id'],row['event_id']
                state=states.get(key)
                if state is None:raise ArithmeticError('lineage names unknown dated source event')
                event,run,source=state['event'],state['run'],state['source'];owner=owners[row['position_id']]
                for field,want in dict(run_id=run['run_id'],input_revision=run['input_revision'],
                    source_id=source['source_id'],source_revision=source['source_revision'],
                    measure=event['measure'],ownership=owner['ownership'],
                    policy_id=raw['recognition_policy']['policy_id'],policy_revision=raw['recognition_policy']['policy_revision']).items():
                    if not exact_identity(row[field],want):raise ArithmeticError(f'dated lineage identity mismatch: {field}')
                if not exact_identity(json.loads(row['provenance_json']),event['provenance']):
                    raise ArithmeticError('dated lineage provenance mismatch')
                day=row['day'];date=asof+day;phase=row['phase'];values=dict.fromkeys(event['components'],0.)
                old=_rights(state['earned'],event['components']['cash_interest'],state['settled'])
                if phase=='recognition':
                    if not 1<=day<=spec['horizon_days'] or day<=state['last_recognition_day']:
                        raise ArithmeticError('duplicate/unordered dated recognition')
                    segments=state['segments']
                    if not any(s['start_ordinal']<date and s['end_ordinal']>=date for s in segments):
                        raise ArithmeticError('recognition outside supplied calendar segments')
                    for field in ('accrued_interest','book_amortization'):
                        values[field]=_cumulative(segments,date,field)-_cumulative(segments,date-1,field)
                    state['earned']+=values['accrued_interest'];state['book']+=values['book_amortization']
                    state['last_recognition_day']=day;state['recognition_count']+=1
                elif phase=='settlement':
                    if day!=event['day_offset'] or not 0<=day<=spec['horizon_days'] or state['settled']:
                        raise ArithmeticError('duplicate/wrong-date dated settlement')
                    for field in ('principal','cash_interest','fees'):values[field]=event['components'][field]
                    state['settled']=True;state['settlement_count']+=1
                else:raise ArithmeticError('unknown dated lineage phase')
                for field,want in values.items():
                    if not _close(row[field],want):raise ArithmeticError(f'dated source amount mismatch: {field}')
                new=_rights(state['earned'],event['components']['cash_interest'],state['settled'])
                position=row['position_id'];p=positions[position]
                gl='asset_principal' if p.kind in {'loan','security','reverse_repo'} else 'funding_principal'
                amounts={(gl,position):-values['principal'],('book_adjustment',position):values['book_amortization'],
                    ('cash',''):values['principal']+values['cash_interest']+values['fees'],
                    ('pnl:dated_'+phase,position):-(values['accrued_interest']+values['book_amortization']+values['fees'])}
                amounts.update({(gl,position):b-a for gl,a,b in zip(RIGHTS_GL,old,new)})
                expected_principal[row['scenario'],position]-=values['principal']
                expected_book[row['scenario'],position]+=values['book_amortization']
                amounts={k:v for k,v in amounts.items() if v!=0.}
                tx=row['transaction_number']
                if amounts:
                    if type(tx) is not int or tx<1:raise ArithmeticError('dated posting lacks transaction identity')
                    yield row['scenario'],f"{row['scenario']}:{tx}",day,owner['account_id'],'dated_'+phase,amounts
                elif tx is not None:raise ArithmeticError('empty dated event declares a posting')
    expected=iter(records());wanted=next(expected,None)
    def groups():
        current=None;amounts=defaultdict(list)
        for frame in writer.frames('journal'):
            check()
            for row in frame.iter_rows(named=True):
                if row['event'] not in {'dated_recognition','dated_settlement'}:continue
                key=tuple(row[k] for k in ('scenario','transaction_id','day','account','event'))
                if current is not None and key!=current:
                    yield (*current,{k:math.fsum(v) for k,v in amounts.items()});amounts=defaultdict(list)
                current=key;amounts[row['gl_account'],row['instrument_id']].append(row['debit']-row['credit'])
        if current is not None:yield (*current,{k:math.fsum(v) for k,v in amounts.items()})
    for got in groups():
        if wanted is None or got[:5]!=wanted[:5]:
            raise ArithmeticError('dated journal/lineage transaction mismatch')
        keys = got[5].keys() | wanted[5].keys()
        # Exact source arithmetic and incremental native accumulation can cross
        # mathematical zero on different sides at the final recognition boundary.
        # Preserve every actual gross GL amount. Only declared rights accounts
        # may have a zero counterpart under the ORIGINAL monetary tolerance;
        # arbitrary balanced extra accounts remain an ownership failure.
        for key in got[5].keys() ^ wanted[5].keys():
            value = got[5].get(key, wanted[5].get(key))
            if key[0] not in RIGHTS_GL or not _close(value, 0.):
                raise ArithmeticError('dated journal/lineage gross ownership mismatch')
        if any(not _close(got[5].get(k, 0.), wanted[5].get(k, 0.)) for k in keys):
            raise ArithmeticError('dated journal differs from independent source postings')
        wanted=next(expected,None)
    if wanted is not None:raise ArithmeticError('dated lineage posting missing from journal')
    expected_rights=defaultdict(lambda:[0.]*4)
    for key,state in states.items():
        event=state['event'];period=event.get('accrual_period')
        count=max(0,min(period['end_ordinal'],end)-max(period['start_ordinal'],asof)) if state['segments'] else 0
        if state['recognition_count']!=count or state['settlement_count']!=int(event['day_offset']<=spec['horizon_days']):
            raise ArithmeticError('dated source phases missing from persisted lineage')
        if not _close(state['earned'],_cumulative(state['segments'],end,'accrued_interest')):
            raise ArithmeticError('dated earned-interest source reconciliation failed')
        for j,value in enumerate(_rights(state['earned'],event['components']['cash_interest'],state['settled'])):
            expected_rights[key[0],key[1]][j]+=value
    seen=set()
    for frame in writer.frames('dated_rights'):
        check()
        for row in frame.iter_rows(named=True):
            key=row['scenario'],row['position_id'],row['event_id'];state=states.get(key)
            if key in seen or state is None:raise ArithmeticError('duplicate/unknown closing dated right')
            seen.add(key);event=state['event'];settled=state['settled'];owner=owners[row['position_id']]
            for field,want in dict(run_id=state['run']['run_id'],account=owner['account_id'],
                source_id=state['source']['source_id'],source_revision=state['source']['source_revision'],
                payment_ordinal=event['payment_ordinal'],full_contract_end_ordinal=asof+state['source']['schedule']['horizon_days'],cash_settled=settled).items():
                if not exact_identity(row[field],want):raise ArithmeticError('closing dated right identity mismatch')
            amounts=dict(earned_interest=state['earned'],cash_interest_settled=event['components']['cash_interest'] if settled else 0.,
                signed_interest_right=state['earned']-(event['components']['cash_interest'] if settled else 0.),recognized_book_amortization=state['book'],
                remaining_principal_settlement=0. if settled else event['components']['principal'],
                remaining_cash_interest=0. if settled else event['components']['cash_interest'],remaining_fees=0. if settled else event['components']['fees'],
                remaining_interest_recognition=event['components']['accrued_interest']-state['earned'],
                remaining_book_recognition=event['components']['book_amortization']-state['book'])
            if any(not _close(row[k],v) for k,v in amounts.items()):raise ArithmeticError('closing dated right amount mismatch')
    if len(seen)!=len(states):raise ArithmeticError('missing closing dated event/tail')
    trial=[row for frame in writer.frames('trial_balance') for row in frame.iter_rows(named=True)]
    balances={(r['scenario'],r['instrument_id'],r['gl_account']):r['balance'] for r in trial if r['instrument_id'] in owners}
    for key,value in expected_principal.items():
        p=positions[key[1]];gl='asset_principal' if p.kind in {'loan','security','reverse_repo'} else 'funding_principal'
        if not _close(balances.get((*key,gl),0.),value) or not _close(balances.get((*key,'book_adjustment'),0.),expected_book[key]):
            raise ArithmeticError('dated closing principal/basis differs from source roll-forward')
        for gl,want in zip(RIGHTS_GL,expected_rights[key]):
            if not _close(balances.get((*key,gl),0.),want):raise ArithmeticError('dated GL right differs from independent source balance')
    closing,consolidated=statements(trial,{a.id:a for a in spec['accounts']},scenarios)
    for table,expected_rows,key_fields in [('closing_statements',closing,('scenario','account')),('consolidated',consolidated,('scenario','currency'))]:
        expected_map={tuple(r[k] for k in key_fields):r for r in expected_rows};actual=set()
        for frame in writer.frames(table):
            check()
            for row in frame.iter_rows(named=True):
                key=tuple(row[k] for k in key_fields)
                if key in actual or key not in expected_map:raise ArithmeticError('dated statement identity mismatch')
                actual.add(key)
                for field,want in expected_map[key].items():
                    if isinstance(want,(int,float)):
                        if not _close(row[field],want):raise ArithmeticError('dated statement differs from independently classified GL')
                    elif not exact_identity(row[field],want):raise ArithmeticError('dated statement label mismatch')
        if actual!=expected_map.keys():raise ArithmeticError('missing independently verified dated statement')
    return dict(dated_source_reconciled=True,dated_events=len(states),lineage_rows=lineage_count,
                dated_tail_retained=True,producer_empirically_validated=False)

