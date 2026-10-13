"""Independent purchased commercial graph and complete ordered journal gate."""
from copy import deepcopy
from dataclasses import fields
from types import SimpleNamespace
import json
import math
import polars as pl
from .commercial_graph_reference import verify_units,verify_graph
from .commercial_ledger_reference import verify_stream
from .commercial_credit_reference import full_end
from .commercial_source_reference import compare
from .commercial_ledger_transport import SCHEMAS
from .modeled_json_identity import exact_json_identity

SCHEMA='commercial-candidate-journal-1'


class Frames:
    def __init__(self,tables,completion,check):
        if not set(tables)<=set(SCHEMAS):raise ArithmeticError('candidate complete table schema required')
        self.tables={};self.completion=completion
        for name in SCHEMAS:
            rows=tables.get(name,[])
            check();converted=deepcopy(rows)
            if name=='journal':
                for row in converted:
                    value=row.pop('value');number=row.pop('transaction_number')
                    if type(value)is not float or not math.isfinite(value)or type(number)is not int:raise ArithmeticError('candidate strict raw journal money/transaction')
                    row.update(transaction_id=f"{row['scenario']}:{number}",debit=max(value,0.),credit=max(-value,0.))
            if name.startswith('commercial_'):
                field='native_atomic_group'if name=='commercial_source_lineage'else'unweighted_book'
                for row in converted:row[field+'_json']=json.dumps(row.pop(field),allow_nan=False,separators=(',',':'))
            for row in converted:
                if set(row)!=set(SCHEMAS[name]):raise ArithmeticError('candidate table column identity differs')
                for key,dtype in SCHEMAS[name].items():
                    value=row[key]
                    if value is None:continue
                    if dtype==pl.Float64 and(type(value)is not float or not math.isfinite(value)):raise ArithmeticError('candidate strict table monetary scalar')
                    if dtype in(pl.Int64,pl.UInt64)and type(value)is not int:raise ArithmeticError('candidate strict integer table scalar')
                    if dtype==pl.Boolean and type(value)is not bool:raise ArithmeticError('candidate strict boolean table scalar')
            self.tables[name]=pl.DataFrame(converted,schema=SCHEMAS[name])
    def frames(self,name):return iter([self.tables[name]])


def allocation(source,allocations,check):
    s,n,m,h=source['shape'];faces=[];face=cost=0.;scenarios=[];worst=float('inf')
    if len(allocations)!=n:raise ArithmeticError('candidate ordered allocation coverage differs')
    for a,u in zip(allocations,source['units'],strict=True):
        x=a['face']
        if a['unit_id']!=u['unit_id']or type(x)not in(float,int)or not math.isfinite(x)or not 0<=x<=1e15:raise ArithmeticError('candidate raw allocation domain differs')
        x=float(x);faces.append(x);face+=x;cost+=x*u['quote_price_per_face_unit']
    funding=source['funding'];f=len(funding['ordinals'])
    for si,scenario in enumerate(source['scenario_ids']):
        check();values=[0.]*(m*h)
        for ui,x in enumerate(faces):
            for mi in range(m):
                for hi in range(h):values[mi*h+hi]+=x*source['values'][((si*n+ui)*m+mi)*h+hi]
        nii=sum(values[:h],0.);worst=min(worst,nii);cash=[];total=0.
        for di,day in enumerate(funding['ordinals']):
            amounts=[0.]*4
            for e in funding['cash_events']:
                if e['scenario_id']==scenario and e['ordinal']==day:
                    for i,name in enumerate(('purchase_cash','principal_cash','interest_cash','fee_cash')):amounts[i]+=faces[e['unit_index']]*e[name]
            total=total+amounts[0]+amounts[1]+amounts[2]+amounts[3];demand=sum((x*funding['demand'][(si*n+ui)*f+di]for ui,x in enumerate(faces)),0.)
            cash.append(dict(ordinal=day,purchase_cash=amounts[0],principal_cash=amounts[1],interest_cash=amounts[2],fee_cash=amounts[3],cumulative_bank_cash=total,funding_demand=demand))
        scenarios.append(dict(scenario_id=scenario,shape=[m,h],values=values,incremental_external_nii=nii,cash=cash))
    return dict(schema='commercial-strategy-replay-1',units=source['units'],allocated_face=faces,total_asset_face=face,total_asset_purchase_cost=cost,worst_incremental_external_nii=worst,scenarios=scenarios,source_cash_replayed=True)


def verify(raw,actual,check=lambda:False):
    names='schema preparation tables producer_completion reporting_end_ordinal full_end_ordinal shared_daily_sim_adoption graph_exercise_source_adoption independent_persisted_verification opening_identity_verified mixed_ledger_adoption empirical_validation scope'
    if set(actual)!=set(names.split())or raw['schema']!=SCHEMA or actual['schema']!=SCHEMA:raise ArithmeticError('candidate strict schema differs')
    prep=actual['preparation']
    if set(prep)!=set('raw_request native_preparation allocation_replay resolved_specification admitted_work_upper retained_bytes_upper'.split())or not exact_json_identity(raw,prep['raw_request'])or set(prep['native_preparation'])!={'source'}:raise ArithmeticError('candidate immutable raw/preparation differs')
    units=raw['commercial'];source=prep['native_preparation']['source'];verify_units(units,source,check)
    expected=allocation(source,raw['allocation'],check);received=deepcopy(prep['allocation_replay']);scope=received.pop('scope',None)
    if type(scope)is not str or not scope:raise ArithmeticError('candidate allocation disclosure absent')
    compare(expected,received,check=check)
    full=max(30,raw['reporting_end_ordinal']-raw['as_of_ordinal']);mapped={r['source_scenario_id']:r for r in raw['run_mapping']};controls=[];alloc=[];participations=[]
    if len(raw['owners'])!=len(units['units']):raise ArithmeticError('candidate ownership coverage differs')
    for ui,(unit,owner)in enumerate(zip(units['units'],raw['owners'],strict=True)):
        check();graph=verify_graph(unit['graph'],source['native_graphs'][ui],check);origin=unit['graph']['as_of_ordinal']-raw['as_of_ordinal']
        if type(origin)is not int or origin<0 or owner['unit_id']!=unit['unit_id']or owner['ownership']!='exclusive_native_commercial_graph-1'or any(owner[k]!=0. for k in('pre_purchase_principal','pre_purchase_accrued','pre_purchase_basis'))or owner['accounting_category']!=unit['accounting_category']or owner['ledger_classification']not in('ac','htm','afs'):raise ArithmeticError('candidate acquired ownership identity differs')
        alloc.append(dict(position_id=owner['position_id'],allocated_face=raw['allocation'][ui]['face']))
        for native in graph['sources']:
            if native['role']!='physical':continue
            mapping=mapped[native['scenario_id']];scenario=next(s for s in unit['graph']['scenarios']if s['scenario_id']==native['scenario_id'])
            if mapping['severity']!=scenario['severity']:raise ArithmeticError('candidate run severity differs')
            issuer=native['source_request']['credit'].get('instrument_kind','commercial_loan')=='issuer_credit_debt'
            if issuer and(native['source_request'].get('performing_draw_tranches')or native['source_request']['credit'].get('opening_undrawn_commitment',0.)!=0. or any(x!=0. for t in native['source_request']['credit']['intervals']for x in t['credit_conversion_factor'])):raise ArithmeticError('issuer security cannot own a loan funding facility')
            request=deepcopy(native['source_request']);request['source']['current']['scenario_id']=mapping['ledger_scenario_id'];request['source']['current']['position_id']=owner['position_id'];projection=native['reference'];groups=native['groups']
            controls.append(dict(raw=request,replay=projection,groups=groups,closing=[g['next']for g in groups if g['phase']=='closing_claims_retained'],origin_day=origin,source_class='issuer_ac_security-1'if issuer else'commercial_loan-1'))
            participation=dict(scenario=mapping['ledger_scenario_id'],position_id=owner['position_id'],path_key=native['path_key'],source_weight=native['weight'])
            participations.append(participation)
            year=full_end(native['source_request']);days=year*365.;rounded=round(days);elapsed=rounded if rounded/365.==year else math.ceil(days)
            if year>elapsed/365.:elapsed+=1
            full=max(full,origin+elapsed)
    from ._deposit_continuation_calendar import _normalize
    from ._deposit_continuation_models import Account,Position,Scenario
    specification=deepcopy(raw['specification']);specification['horizon_days']=full;normalized=_normalize(specification)
    for key,cls in(('accounts',Account),('positions',Position),('scenarios',Scenario)):
        for row in normalized[key]:
            for f in fields(cls):
                if f.type is float:row[f.name]=float(row[f.name])
    compare(normalized,prep['resolved_specification'],check=check)
    positions={p['id']:p for p in normalized['positions']};runs={'baseline'}|{s['name']for s in normalized['scenarios']}
    if set(positions)!={o['position_id']for o in raw['owners']}or{m['ledger_scenario_id']for m in raw['run_mapping']}!=runs:raise ArithmeticError('candidate complete ledger mapping differs')
    for owner in raw['owners']:
        p=positions[owner['position_id']]
        kinds={s['source_class']for s in controls if s['raw']['source']['current']['position_id']==owner['position_id']}
        wanted_kind='security'if kinds=={'issuer_ac_security-1'}else'loan'if kinds=={'commercial_loan-1'}else None
        if p['kind']!=wanted_kind or p['duration']!=0. or p['opening_market_price']!=1. or p['classification']!='ac'or owner['accounting_category']!='AC'or p['account']!=owner['account_id']or p['classification']!=owner['ledger_classification']:raise ArithmeticError('candidate position/account/classification differs')
    spec={k:[SimpleNamespace(**v)for v in vs]if k in('accounts','positions','scenarios')else deepcopy(vs)for k,vs in normalized.items()}
    from .commercial_reports_reference import scope as verify_scope
    verify_scope(spec,[a['position_id']for a in alloc])
    derived=dict(allocations=alloc,participations=participations);control=dict(spec=spec,horizon=full,sources=controls)
    receipt=verify_stream(derived,control,Frames(actual['tables'],actual['producer_completion'],check),check)
    for name,value in [('reporting_end_ordinal',raw['reporting_end_ordinal']),('full_end_ordinal',raw['as_of_ordinal']+full),('shared_daily_sim_adoption',True),('graph_exercise_source_adoption',True),('independent_persisted_verification',False),('opening_identity_verified',False),('mixed_ledger_adoption',False),('empirical_validation',False)]:
        if not exact_json_identity(value,actual[name]):raise ArithmeticError('candidate eligibility/date differs/'+name)
    for name,limit in [('admitted_work_upper',32_000_000),('retained_bytes_upper',128<<20)]:
        if type(prep[name])is not int or not 0<prep[name]<=limit:raise ArithmeticError('candidate integer admission bound differs')
    return dict(verified=True,raw_graph_units_recomputed=True,allocation_replayed=True,purchased_journal_replayed=True,complete_tail_replayed=True,persisted_partitions_replayed=True,opening_identity_verified=False,mixed_ledger_adoption=False,empirical_validation=False)
