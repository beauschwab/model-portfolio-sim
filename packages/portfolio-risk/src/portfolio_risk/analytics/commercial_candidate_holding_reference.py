"""Independent complete-origin candidate loan/fee holding-window replay."""
from collections import defaultdict
from copy import deepcopy
import json

from .commercial_candidate_reference import verify as verify_candidate
from .commercial_continuation_reference import prefix as loan_prefix
from .commercial_dto import graph_typed
from .commercial_holding_reference import _window
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity

SCHEMA='commercial-candidate-holding-journal-1'


def same(a,b):
    if not exact_json_identity(graph_typed(a),graph_typed(b)):
        raise ArithmeticError('candidate holding immutable original anchor or observed prefix changed')


def raw_prefix(raw,check=lambda:False):
    old=raw['completed_candidate'];new=raw['revised_candidate'];cut=raw['cut_ordinal']
    a=old['commercial'];b=deepcopy(new['commercial'])
    if len(a['units'])!=len(b['units']):raise ArithmeticError('candidate holding unit coverage changed')
    for x,y in zip(a['units'],b['units'],strict=True):
        if len(x['graph']['positions'])!=len(y['graph']['positions']):raise ArithmeticError('candidate holding position coverage changed')
        for p,q in zip(x['graph']['positions'],y['graph']['positions'],strict=True):
            if len(p['histories'])!=len(q['histories']):raise ArithmeticError('candidate holding scenario coverage changed')
            for hs,js in zip(p['histories'],q['histories'],strict=True):
                same(hs['scenario_id'],js['scenario_id'])
                for role in('physical','pricing','pricing_down','pricing_up'):
                    if len(hs[role])!=len(js[role]):raise ArithmeticError('candidate holding path coverage changed')
                    for h,j in zip(hs[role],js[role],strict=True):
                        check();same([h['path_id'],h['weight']],[j['path_id'],j['weight']])
                        if cut<=h['source']['source']['current']['asof_ordinal']:same(h['source'],j['source'])
                        else:loan_prefix(dict(schema='commercial-holding-continuation-1',cut_ordinal=cut,completed_source=h['source'],revised_source=j['source']),check)
            q['histories']=deepcopy(p['histories'])
            f=p.get('fees');g=q.get('fees')
            if (f is None)!=(g is None):raise ArithmeticError('candidate holding fee ownership changed')
            if f is not None:
                if len(f['histories'])!=len(g['histories']):raise ArithmeticError('candidate holding fee scenario coverage changed')
                for hs,js in zip(f['histories'],g['histories'],strict=True):
                    same(hs['scenario_id'],js['scenario_id'])
                    for role in('physical','pricing','pricing_down','pricing_up'):
                        if len(hs[role])!=len(js[role]):raise ArithmeticError('candidate holding fee path coverage changed')
                        for h,j in zip(hs[role],js[role],strict=True):
                            same(h['path_id'],j['path_id']);policy=deepcopy(j['policy']);policy['events']=deepcopy(h['policy']['events']);same(h['policy'],policy)
                            for key in('events','acquisition_releases'):
                                left=h['policy']if key=='events'else h;right=j['policy']if key=='events'else j
                                same([e for e in left.get(key,[])if e['ordinal']<=cut],[e for e in right.get(key,[])if e['ordinal']<=cut])
                g['histories']=deepcopy(f['histories'])
    same(a,b);same({k:v for k,v in old.items()if k!='commercial'},{k:v for k,v in new.items()if k!='commercial'})


def monetary_prefix(a,b,cut,check):
    for name in set(a)|set(b):
        check();rows=lambda tables:[r for r in tables.get(name,[])if type(r.get('day'))is int and r['day']<=cut]
        if not exact_json_identity(rows(a),rows(b)):raise ArithmeticError('candidate holding exact ordered monetary prefix changed')


def verify(raw,actual,check=lambda:False):
    check()
    names='schema raw_source origin_asof_ordinal cut_ordinal cut_day full_end_ordinal completed_candidate revised_candidate window_tables admitted_work_upper retained_bytes_upper component_output_limit_bytes complete_origin_daily_sim_adoption exact_observed_monetary_prefix_retained original_credit_clock_restarted original_loan_anchor_refitted original_fee_anchor_refitted second_acquisition_posted prior_publication_verified cut_started_mixed_sim_adoption secondary_acquisition_adoption empirical_validation scope'
    if set(actual)!=set(names.split())or raw['schema']!=SCHEMA or actual['schema']!=SCHEMA or not exact_json_identity(raw,actual['raw_source']):raise ArithmeticError('candidate holding exact raw/result identity differs')
    raw_prefix(raw,check)
    a=raw['completed_candidate'];b=raw['revised_candidate'];old=actual['completed_candidate'];new=actual['revised_candidate']
    if type(actual['component_output_limit_bytes'])is not int or actual['component_output_limit_bytes']!=8<<20:raise ArithmeticError('candidate holding bounded component tier differs')
    for component in(old,new,actual['window_tables']):
        if len(json.dumps(component,allow_nan=False,separators=(',',':')).encode())>8<<20:raise ArithmeticError('candidate holding materialized component exceeds8MiB')
    verify_candidate(a,old,check);verify_candidate(b,new,check)
    origin=a['as_of_ordinal'];cut=raw['cut_ordinal']-origin
    if cut<=0 or raw['cut_ordinal']>min(old['full_end_ordinal'],new['full_end_ordinal']):raise ArithmeticError('candidate holding actual calendar differs')
    for key,value in(('origin_asof_ordinal',origin),('cut_ordinal',raw['cut_ordinal']),('cut_day',cut),('full_end_ordinal',new['full_end_ordinal'])):
        if type(actual[key])is not int or actual[key]!=value:raise ArithmeticError('candidate holding exact calendar differs')
    for key in('complete_origin_daily_sim_adoption','exact_observed_monetary_prefix_retained'):
        if actual[key]is not True:raise ArithmeticError('candidate holding complete native owner absent')
    for key in('original_credit_clock_restarted','original_loan_anchor_refitted','original_fee_anchor_refitted','second_acquisition_posted','prior_publication_verified','cut_started_mixed_sim_adoption','secondary_acquisition_adoption','empirical_validation'):
        if actual[key]is not False:raise ArithmeticError('candidate holding adoption evidence differs')
    for key,limit in(('admitted_work_upper',32_000_000),('retained_bytes_upper',128<<20)):
        if type(actual[key])is not int or not 0<actual[key]<=limit:raise ArithmeticError('candidate holding joined bounds differ')
    if type(actual['scope'])is not str or not actual['scope']:raise ArithmeticError('candidate holding disclosure absent')
    monetary_prefix(old['tables'],new['tables'],cut,check)
    expected=_window(new['tables'],new['preparation']['resolved_specification'],cut,check)
    for name in('commercial_fee_daily','commercial_fee_closing'):
        if name in new['tables']:expected[name]=[deepcopy(r)for r in new['tables'][name]if r['day']>=cut]
    compare(expected,actual['window_tables'],check=check)
    balance=defaultdict(float)
    for row in actual['window_tables']['journal']:
        check();balance[tuple(row[k]for k in('scenario','account','gl_account','instrument_id'))]+=row['value']
    trial={tuple(row[k]for k in('scenario','account','gl_account','instrument_id')):row['balance']for row in actual['window_tables']['trial_balance']}
    if set(balance)!=set(trial):raise ArithmeticError('candidate holding window GL coverage differs')
    for key,value in balance.items():compare(value,trial[key],check=check)
    return dict(verified=True,complete_origin_candidate_journals_replayed=True,raw_observed_prefix_recomputed=True,
        exact_ordered_monetary_prefix_retained=True,loan_and_fee_original_anchors_retained=True,
        window_gross_rights_basis_replayed=True,prior_pnl_rolled_into_opening=True,once_only_future_postings_replayed=True,
        persisted_partitions_replayed=True,prior_publication_verified=False,cut_started_mixed_sim_adoption=False,
        secondary_acquisition_adoption=False,empirical_validation=False)
