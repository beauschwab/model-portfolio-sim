"""Independent actual-path fee source and single-Sim journal/report replay."""
from copy import deepcopy
from collections import defaultdict
import math
from .commercial_source_reference import compare
from .commercial_ledger_reference import verify_preparation,journal_lines
from .commercial_reports_reference import reports
from .nonintegral_fee_reference import replay as fee_replay,verify as fee_verify
from .modeled_json_identity import exact_json_identity

SCHEMA='commercial-fee-journal-1'
GL={'cash':'cash','fee_receivable':'commercial_fee_receivable','fee_payable':'commercial_fee_payable',
    'deferred_fee_income':'commercial_deferred_fee_income','prepaid_fee_expense':'commercial_prepaid_fee_expense',
    'fee_income':'pnl:commercial_nonintegral_fee_income','fee_writeoff_expense':'pnl:commercial_fee_writeoff_expense',
    'fee_recovery_income':'pnl:commercial_fee_recovery_income'}

def _day(year):
    v=year*365.;i=round(v);d=i if i/365.==year else math.ceil(v)
    return d+1 if year>d/365. else d

def bind(policy,raw,source,check):
    origin=raw['source']['current']['asof_ordinal'];current=raw['source']['current'];out=[]
    def request(events):
        return dict(schema='nonintegral-fee-accounting-1',measure='physical',currency=current['currency'],monetary_unit=current['monetary_unit'],policy=deepcopy(policy['policy']),
            asof_ordinal=origin,reporting_end_ordinal=current['reporting_end_ordinal'],complete_end_ordinal=policy['complete_end_ordinal'],fees=deepcopy(policy['fees']),events=events)
    for path in source['replay']['credit']['result']['paths']:
        check();exit=None
        for e in path['events']:
            if e['destination']in('default','prepaid'):
                exit=dict(native_event_id=e['event_id'],native_year=e['year'],reason='default'if e['destination']=='default'else'other_reviewed');break
        if exit is None:
            e=next((e for e in path['events']if e['kind']=='maturity'),None)
            if e is not None:exit=dict(native_event_id=e['event_id'],native_year=e['year'],reason='maturity')
        # This acquired-loan route contains no conditional-policy caller stops.
        timeline=[];prefix=[deepcopy(e)for e in policy['events']if exit is None or (e['ordinal']-origin)/365.<exit['native_year']]
        before=fee_replay(request(prefix),check)
        if exit is not None:
            rule=policy[dict(default='default',other_reviewed='prepayment',maturity='maturity',exercise='exercise')[exit['reason']]]
            for fi,(fee,snap)in enumerate(zip(policy['fees'],before['closing'],strict=True)):
                s=snap['state'];sign=1. if fee['entitlement']> 0. else -1.
                def emit(label,action,amount,year,reason):
                    if amount!=0.:
                        timeline.append((year,dict(event_id=f"native-fee:{path['path_key']}:{fi}:{label}",fee_id=fee['fee_id'],source=deepcopy(fee['source']),review=deepcopy(rule['review']),ordinal=origin+_day(year),sequence=0,action=action,reason=reason,amount=amount)))
                year=exit['native_year'];reason=exit['reason']
                if rule['cancel_unearned_entitlement']:emit('cancel','cancel_unearned',s['unearned_entitlement'],year,reason)
                refunded=s.get('refunded_earned',0.)
                waived=max((s['earned']-refunded-s['waived_earned']-s['net_service_cash'])*sign,0.)*sign*rule['waive_unpaid_earned_fraction']
                emit('waive','waive_unpaid_earned',waived,year,reason)
                refund=max((s['net_service_cash']-s['earned']+refunded+s['waived_earned'])*sign,0.)*sign*rule['refund_prepaid_fraction']
                emit('refund','refund_prepaid',refund,year,'settlement')
                collected=max(min((s['earned']-refunded-s['waived_earned'])*sign,s['net_service_cash']*sign),0.)*sign
                emit('earned-refund','refund_earned',collected*rule.get('refund_earned_fraction',0.),year,reason)
                for i,v in enumerate(rule['recoveries']):emit(f'recovery:{i}','recover_waived',waived*v['fraction'],year+v['lag_days']/365.,reason)
            for e in policy['events']:
                year=(e['ordinal']-origin)/365.
                if year>=exit['native_year'] and(e['action']=='earn'and rule['cancel_future_service']or e['action']=='cash'and rule['cancel_future_cash']):continue
                timeline.append((year,deepcopy(e)))
        else:timeline=[((e['ordinal']-origin)/365.,deepcopy(e))for e in policy['events']]
        timeline.sort(key=lambda e:e[0]);events=[];years=[]
        for i,(year,e)in enumerate(timeline):e['sequence']=i;events.append(e);years.append(year)
        out.append(dict(path_key=path['path_key'],position_id=current['position_id'],scenario=current['scenario_id'],native_years=years,
            source=fee_replay(request(events),check),exit=exit,native_credit_event_binding=True,loan_carrying_value_changed=False))
    return out

def verify(raw,result,check=lambda:False):
    check()
    if not exact_json_identity(raw,result.get('raw_source'))or raw['schema']!=SCHEMA or result.get('schema')!=SCHEMA:raise ValueError('fee journal original source differs')
    for flag in('native_credit_event_binding','shared_daily_sim_adoption'):
        if result.get(flag)is not True:raise ValueError('fee native ownership differs')
    for flag in('loan_carrying_value_changed','pricing_unit_ftp_adoption','empirical_validation','accounting_approval'):
        if result.get(flag)is not False:raise ValueError('fee adoption boundary differs')
    if not exact_json_identity(result.get('completion'),dict(complete=True,protocol=7)):raise ValueError('fee protocol completion differs')
    loan=deepcopy(raw['loan']);report=loan['spec'].get('horizon_days',360)
    loan['spec']['horizon_days']=max([report]+[p['complete_end_ordinal']-s['source']['current']['asof_ordinal']for p,s in zip(raw['fees'],loan['commercial'],strict=True)])
    if result['reporting_horizon_days']!=report:raise ValueError('fee original reporting cutoff differs')
    # The nested typed DTO retains exact immutable financial request identity.
    from .commercial_dto import typed
    nested=result['loan_receipt'];expected=typed(loan)
    prepared=verify_preparation(expected,expected,nested,check,expected_protocol=7)
    cells=sum((len(p['events'])+len(p['fees'])*(3+max(len(p[k]['recoveries'])for k in('default','prepayment','exercise','maturity'))+int(any(p[k].get('refund_earned_fraction',0.)>0. for k in('default','prepayment','exercise','maturity')))))*len(s['credit']['paths'])for p,s in zip(raw['fees'],loan['commercial'],strict=True))
    bounds=dict(admitted_work_upper=nested['admitted_work_upper']+cells*128+cells*8*(prepared['horizon']+1)*16,
        retained_bytes_upper=nested['retained_bytes_upper']+cells*4096+cells*8*4096+(32<<20))
    for name,wanted in bounds.items():
        if type(result.get(name))is not int or result[name]!=wanted:raise ValueError('joined fee admission differs')
    fee_sources=[bind(p,r,s,check)for p,r,s in zip(raw['fees'],raw['loan']['commercial'],prepared['sources'],strict=True)]
    compare(fee_sources,result['fee_sources'],check=check)
    for paths in result['fee_sources']:
        for f in paths:fee_verify(f['source']['raw_request'],f['source'],check)
    positions={p.id:p for p in prepared['spec']['positions']};alloc={a['position_id']:a['allocated_face']for a in loan['allocations']}
    weights={(p['scenario'],p['position_id'],p['path_key']):p['source_weight']for p in loan['participations']}
    events=[];loan_lineage=[];fee_lineage=[];daily=[];fee_daily=[];binding=0;fee_binding=0;binding_order={}
    for si,s in enumerate(prepared['sources']):
        current=s['raw']['source']['current'];scenario=current['scenario_id'];position=current['position_id'];allocation=alloc[position]/current['opening_principal']
        for path,fs in zip(s['raw']['credit']['paths'],fee_sources[si],strict=True):
            binding_order[scenario,position,path['path_key']]=binding
            weight=weights[scenario,position,path['path_key']];factor=allocation*weight;tracking=({},{});last_by_day={}
            for i,g in enumerate(s['groups']):
                if g['path_key']!=path['path_key']:continue
                day=_day(g['year']);lines=journal_lines(g,tracking,factor)
                meta=dict(scenario=scenario,day=day,native_credit_year=g['year'],source_origin_day=0,source_class='commercial_loan-1',global_native_credit_year=g['year'],date_policy='actual_credit_clock_order_day_end_journal_no_retiming_source-1',position_id=position,path_key=path['path_key'],source_weight=float(weight),allocation_factor=float(allocation),native_group_index=i,native_atomic_group=g)
                events.append((scenario,day,g['year'],0,binding,i,meta,lines));last_by_day[day]=g['next']
            latest=None
            for day in range(prepared['horizon']+1):
                if day in last_by_day:latest=last_by_day[day]
                if latest is not None:daily.append(dict(scenario=scenario,day=day,position_id=position,path_key=path['path_key'],source_weight=float(weight),allocation_factor=float(allocation),source_origin_day=0,source_class='commercial_loan-1',unweighted_book=latest,capital_ftp_eligible=False))
            by_day=defaultdict(list)
            for i,(year,g)in enumerate(zip(fs['native_years'],fs['source']['groups'],strict=True)):
                day=_day(year);lines=[(GL[p['account']],p['amount']*factor)for p in g['postings']if p['amount']*factor!=0.]
                meta=dict(scenario=scenario,day=day,position_id=position,path_key=path['path_key'],native_year=year,source_weight=float(weight),allocation_factor=float(allocation),native_group_index=i,native_fee_group=g)
                events.append((scenario,day,year,1,fee_binding,i,meta,lines));by_day[day].append(g)
            states={}
            for day in range(prepared['horizon']+1):
                for g in by_day[day]:states[g['event']['fee_id']]=g['next']
                for fid in sorted(states):fee_daily.append(dict(scenario=scenario,day=day,position_id=position,path_key=path['path_key'],fee_id=fid,source_weight=float(weight),allocation_factor=float(allocation),native_fee_snapshot=states[fid]))
            binding+=1;fee_binding+=1
    order={name:i for i,name in enumerate(['baseline']+[s.name for s in prepared['spec']['scenarios']])};events.sort(key=lambda e:(order[e[0]],e[1],e[2],e[3],e[4],e[5]));converted=[];sequence=defaultdict(lambda:sum(a.cash != 0. or a.equity != 0. for a in prepared['spec']['accounts']))
    for scenario,day,year,kind,bi,i,meta,lines in events:
        if lines:sequence[scenario]+=1
        meta['transaction_number']=sequence[scenario]if lines else None
        (fee_lineage if kind else loan_lineage).append(meta)
        group=dict(phase='commercial_nonintegral_fee'if kind else meta['native_atomic_group']['phase'])
        converted.append((scenario,day,year,bi,i,dict(position_id=meta['position_id'],native_atomic_group=group),lines))
    wanted=reports(prepared,converted,check)
    tables=result['tables']
    allowed=set(wanted)|{'commercial_source_lineage','commercial_fee_lineage','commercial_daily','commercial_closing','commercial_fee_daily','commercial_fee_closing'}
    if set(tables)-allowed:raise ValueError('fee journal table ownership differs')
    for table,rows in wanted.items():
        actual=deepcopy(tables.get(table,[]))
        if table=='journal':
            for row in actual:v=row.pop('value');n=row.pop('transaction_number');row.update(transaction_id=f"{row['scenario']}:{n}",debit=max(v,0.),credit=max(-v,0.))
        compare(rows,actual,f'fee full reports.{table}',check)
    compare(loan_lineage,tables.get('commercial_source_lineage',[]),check=check);compare(fee_lineage,tables.get('commercial_fee_lineage',[]),check=check)
    # Native snapshot ordering is scenario/day/binding/fee ID.
    daily.sort(key=lambda r:(order[r['scenario']],r['day'],binding_order[r['scenario'],r['position_id'],r['path_key']]))
    fee_daily.sort(key=lambda r:(order[r['scenario']],r['day'],binding_order[r['scenario'],r['position_id'],r['path_key']],r['fee_id']))
    for name,rows in(('commercial_daily',daily),('commercial_fee_daily',fee_daily)):
        compare(rows,tables.get(name,[]),check=check)
        compare([r for r in rows if r['day']==prepared['horizon']],tables.get(name.replace('_daily','_closing'),[]),check=check)
    check();return dict(verified=True,raw_loan_and_fee_recomputed=True,native_credit_event_binding=True,single_sim_ordered_journal_replayed=True,gross_fee_rights_reconciled=True,
        complete_tail_reports_replayed=True,loan_carrying_value_changed=False,pricing_unit_ftp_adoption=False,empirical_validation=False,accounting_approval=False)
