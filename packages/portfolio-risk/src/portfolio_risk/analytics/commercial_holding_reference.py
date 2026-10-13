"""Raw source, complete daily journal and prospective holding-window verification."""
from collections import defaultdict
from copy import deepcopy
import math
from .commercial_candidate_reference import Frames
from .commercial_continuation_reference import verify as verify_holding
from .commercial_ledger_reference import verify_preparation, verify_stream, identity
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

SCHEMA='commercial-holding-journal-1'


def _window(tables, specification, cut, check):
    """Independent signed GL replay; producer opening/trial values are never inputs."""
    journal=tables.get('journal',[]);prefix=defaultdict(float);last={}
    keys=set('transaction_number scenario day account event gl_account instrument_id value'.split())
    for row in journal:
        check()
        if set(row)!=keys or type(row['value'])is not float or not math.isfinite(row['value']):raise ArithmeticError('holding full journal shape/money differs')
        for name in ('day','transaction_number'):
            if type(row[name])is not int or row[name]<0:raise ArithmeticError('holding full journal integer differs')
        state=(row['day'],row['transaction_number']);before=last.get(row['scenario'])
        if before is not None and(state[0]<before[0] or state[1]<before[1]):raise ArithmeticError('holding full native order differs')
        last[row['scenario']]=state
        if row['day']<=cut:
            key=tuple(row[k]for k in ('scenario','account','gl_account','instrument_id'));prefix[key]+=row['value']
    result={k:[]for k in ('journal','opening','retained_prior_pnl','trial_balance','commercial_daily','commercial_closing')};closing={}
    for scenario in ['baseline']+[s['name']for s in specification['scenarios']]:
        sequence=0
        for account in specification['accounts']:
            check();account=account['id'];stocks={};prior=0.
            for (s,a,g,i),value in sorted(prefix.items()):
                if s!=scenario or a!=account:continue
                if g.startswith('pnl:'):prior+=value
                else:stocks[g,i]=value
            stocks['opening_equity','']=stocks.get(('opening_equity',''),0.)+prior
            result['retained_prior_pnl'].append(dict(scenario=scenario,account=account,day=cut,signed_prior_pnl=prior))
            if len(stocks)==1 and all(v==0. for v in stocks.values()):continue
            signed=sum(stocks[k]for k in sorted(stocks));gross=sum(abs(stocks[k])for k in sorted(stocks))
            if not math.isfinite(signed) or abs(signed)>1e-9+1e-12*gross:raise ArithmeticError('holding independently derived opening unbalanced')
            sequence+=1
            for (gl,instrument),value in sorted(stocks.items()):
                row=dict(scenario=scenario,account=account,day=cut,gl_account=gl,instrument_id=instrument)
                result['opening'].append(dict(row,balance=value));closing[scenario,account,gl,instrument]=value
                result['journal'].append(dict(row,transaction_number=sequence,event='native_holding_opening',value=value,origin_transaction_number=None))
        previous=None
        for row in journal:
            if row['scenario']!=scenario or row['day']<=cut:continue
            check()
            if row['event']in('opening','reviewed_opening_snapshot'):raise ArithmeticError('holding window posted a second acquisition')
            if row['transaction_number']!=previous:sequence+=1;previous=row['transaction_number']
            new=dict(row,transaction_number=sequence,origin_transaction_number=row['transaction_number']);result['journal'].append(new)
            key=tuple(row[k]for k in('scenario','account','gl_account','instrument_id'));closing[key]=closing.get(key,0.)+row['value']
    for (scenario,account,gl,instrument),value in sorted(closing.items()):
        result['trial_balance'].append(dict(scenario=scenario,account=account,gl_account=gl,instrument_id=instrument,balance=value))
    for name in ('commercial_daily','commercial_closing'):
        result[name]=[deepcopy(row)for row in tables.get(name,[])if row['day']>=cut]
    # Check all transactions and original/future separation independently.
    transactions=defaultdict(list)
    for row in result['journal']:transactions[row['scenario'],row['transaction_number']].append(row)
    for rows in transactions.values():
        check()
        if len({(r['day'],r['account'],r['event'])for r in rows})!=1:raise ArithmeticError('holding transaction mixed identities')
        signed=math.fsum(r['value']for r in rows);gross=math.fsum(abs(r['value'])for r in rows)
        if abs(signed)>1e-9+1e-12*gross:raise ArithmeticError('holding transaction unbalanced')
    return result


def verify(raw, actual, check=lambda:False):
    check()
    names='schema raw_source origin_asof_ordinal cut_ordinal cut_day full_end_ordinal source_receipt prefix_proofs full_tables window_tables producer_completion admitted_work_upper retained_bytes_upper complete_origin_daily_sim_adoption original_credit_clock_restarted second_acquisition_posted prior_publication_verified cut_started_mixed_sim_adoption secondary_acquisition_adoption empirical_validation scope'
    if set(actual)!=set(names.split()) or raw['schema']!=SCHEMA or actual['schema']!=SCHEMA or not exact_json_identity(raw,actual['raw_source']):raise ArithmeticError('holding exact raw/result identity differs')
    if actual['complete_origin_daily_sim_adoption']is not True:raise ArithmeticError('holding native Sim adoption absent')
    for name in ('original_credit_clock_restarted','second_acquisition_posted','prior_publication_verified','cut_started_mixed_sim_adoption','secondary_acquisition_adoption','empirical_validation'):
        if actual[name]is not False:raise ArithmeticError('holding adoption evidence differs')
    if type(actual['scope'])is not str or not actual['scope']:raise ArithmeticError('holding disclosure absent')
    for name,limit in (('admitted_work_upper',32_000_000),('retained_bytes_upper',128<<20)):
        if type(actual[name])is not int or not 0<actual[name]<=limit:raise ArithmeticError('holding joined bounds differ')
    revised=deepcopy(raw['origin_ledger']);revised['commercial']=deepcopy(raw['revised_sources'])
    receipt=actual['source_receipt'];prepared=verify_preparation(revised,revised,receipt,check)
    origin=raw['origin_ledger']['commercial'][0]['source']['current']['asof_ordinal'];cut=raw['cut_ordinal']-origin
    if cut<=0 or any(type(actual[k])is not int or actual[k]!=v for k,v in [('cut_day',cut),('cut_ordinal',raw['cut_ordinal']),('origin_asof_ordinal',origin),('full_end_ordinal',origin+prepared['horizon'])]):raise ArithmeticError('holding actual calendar differs')
    if len(raw['origin_ledger']['commercial'])!=len(raw['revised_sources']) or len(actual['prefix_proofs'])!=len(raw['revised_sources']):raise ArithmeticError('holding complete prefix coverage differs')
    for old,new,proof,source in zip(raw['origin_ledger']['commercial'],raw['revised_sources'],actual['prefix_proofs'],receipt['sources'],strict=True):
        check()
        pair=dict(schema='commercial-holding-continuation-1',cut_ordinal=raw['cut_ordinal'],completed_source=old,revised_source=new)
        if set(proof)!=set('raw_source completed openings cut_year admitted_work_upper retained_bytes_upper'.split()):raise ArithmeticError('holding prefix receipt shape differs')
        identity(pair,proof['raw_source'])
        child=dict(proof,schema=pair['schema'],revised=source,complete_origin_replayed=True,credit_clock_restarted=False,
            original_anchor_refitted=False,second_acquisition_posted=False,mixed_ledger_adoption=False,verified_prior_publication=False,secondary_acquisition_adoption=False,empirical_validation=False)
        verify_holding(pair,child,check)
    # Full-source books, once-only actual postings and every ordinary daily/
    # consolidated report are replayed from raw histories before window checking.
    frames=Frames(actual['full_tables'],actual['producer_completion'],check)
    if not exact_json_identity(actual['producer_completion'],dict(complete=True,protocol=6)):raise ArithmeticError('holding producer completion differs')
    verified=verify_stream(revised,prepared,frames,check)
    expected=_window(actual['full_tables'],receipt['resolved_specification'],cut,check)
    compare(expected,actual['window_tables'],check=check)
    # Independent replay of window postings must equal each published closing.
    replay=defaultdict(float)
    for row in actual['window_tables']['journal']:
        key=tuple(row[k]for k in('scenario','account','gl_account','instrument_id'));replay[key]+=row['value']
    trial={tuple(r[k]for k in('scenario','account','gl_account','instrument_id')):r['balance']for r in actual['window_tables']['trial_balance']}
    if set(replay)!=set(trial):raise ArithmeticError('holding window GL coverage differs')
    for key,value in replay.items():compare(value,trial[key],check=check)
    return dict(verified=True,raw_source_recomputed=True,complete_origin_journal_replayed=True,completed_prefix_recomputed=True,
        window_opening_recomputed=True,window_postings_recomputed=True,window_trial_balance_replayed=True,
        original_yield_anchors_retained=verified['original_yield_anchors_retained'],prior_publication_verified=False,
        cut_started_mixed_sim_adoption=False,secondary_acquisition_adoption=False,empirical_validation=False)
