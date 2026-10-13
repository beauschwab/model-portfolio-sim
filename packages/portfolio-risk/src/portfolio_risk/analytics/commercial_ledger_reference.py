"""Raw commercial preparation and persisted shared-journal publication gate."""
from collections import defaultdict
from copy import deepcopy
import json
import math
from dataclasses import fields
from types import SimpleNamespace
from .commercial_source_reference import compare, verify_source, project
from .commercial_credit_reference import replay, typed, full_end
from .commercial_accounting_reference import groups
from .modeled_json_identity import exact_json_identity
from .balance_stream import replay_partitions

SCHEMA='commercial-credit-ledger-1'
DATE_POLICY='actual_credit_clock_order_day_end_journal_no_retiming_source-1'


def identity(expected,actual):
    if not exact_json_identity(typed(expected),actual):raise ArithmeticError('retained commercial raw/DTO identity differs')


def verify_draw_sources(raw,actual,check=lambda:False,*,original_acquisition=None,measure='physical'):
    """Independently replay every declared draw source, including inactive paths."""
    expected=raw.get('performing_draw_tranches',[]);received=actual.get('draw_sources',[])
    if len(expected)!=len(received):raise ArithmeticError('commercial draw source coverage differs')
    for terms,draw in zip(expected,received):
        check();identity(terms,draw['terms'])
        for role in ('original','current'):
            native=draw[role];identity(terms['source'][role],native['raw']);compare(raw['factor_policy'],native['policy'],check=check);verify_source(terms['source'][role],native,check)
        projection,_=project(terms['source']['original'],check);acquisition=terms['source']['acquisition']
        quote=sum((e['components']['principal']+e['components']['cash_interest']+e['components']['fees'])*math.pow(1.+acquisition['original_annual_effective_rate'],-e['day_offset']/365.) for e in projection['schedule']['events'])
        if measure=='physical':
            if original_acquisition is not None:
                proof=next((p for p in original_acquisition['conditional_tranche_proofs']if p['tranche_id']==terms['review']['id']),None)
                if proof is None or not exact_json_identity(typed(acquisition),proof['acquisition'])or proof['draw_ordinal']!=terms['draw_ordinal']:raise ArithmeticError('conditional draw original acquisition authority absent')
                quote=proof['conditional_cash_quote']
            compare(float(acquisition['cost']),float(quote),'draw original acquisition quote',check)


def compile_original_acquisition(histories,stops_by_history,check=lambda:False,*,compact_generated=False):
    """Private independent complete stopped Physical acquisition authority."""
    if not histories or len(histories)!=len(stops_by_history):raise ArithmeticError('complete original Physical acquisition histories required')
    from .commercial_acquisition_reference import partition,conditional
    acquisition=typed(histories[0]['source']['source']['acquisition']);weights=[];projections=[];bindings=[];partitions=[];recipes=[];quote=0.;seen=set()
    for history,stops in zip(histories,stops_by_history):
        check();raw=history['source'];weight=history['weight'];paths=raw['credit']['paths']
        if len(paths)!=1 or paths[0]['path_key'] in seen or type(weight)not in (int,float) or not math.isfinite(weight) or not 0<weight<=1:raise ArithmeticError('distinct complete weighted original Physical paths required')
        seen.add(paths[0]['path_key'])
        if not exact_json_identity(typed(raw['source']['original']),typed(raw['source']['current'])) or not exact_json_identity(typed(raw['source']['acquisition']),acquisition):raise ArithmeticError('original acquired source/anchors changed')
        for draw in raw.get('performing_draw_tranches',[]):
            if not exact_json_identity(typed(draw['source']['original']),typed(draw['source']['current'])):raise ArithmeticError('original tranche current source changed')
        projection=replay(raw,check,stops=stops);parts=partition(raw,projection,stops,check);partitions.append(parts)
        cash_quote=sum((r['amount']*math.pow(1.+acquisition['original_annual_effective_rate'],-r['year'])for r in parts['base']),0.)
        quote+=weight*cash_quote;weights.append(float(weight))
        if compact_generated:recipes.append(dict(raw_request=typed(raw),native_stops=typed(list(stops))))
        else:projections.append(projection)
        bindings.append(dict(path_key=paths[0]['path_key'],original=typed(raw['source']['original']),draws=typed(raw.get('performing_draw_tranches',[]))))
    if abs(sum(weights)-1.)>1e-12:raise ArithmeticError('complete original Physical probability required')
    compare(float(acquisition['cost']),float(quote),'original stopped Physical acquisition quote',check)
    result=dict(original_stopped_cash_quote=float(quote),acquisition=acquisition,weights=weights,stopped_physical=projections,conditional_tranche_proofs=conditional(histories,weights,partitions,check),acquired_cash_partitions=partitions,original_bindings=bindings)
    if compact_generated:result['generated_original_physical']=recipes
    return result


def verify_compiler(raw,actual,check=lambda:False,*,stops=(),original_acquisition=None):
    """Recompute both coupons, original quote, actual CTMC and accounting books."""
    check();identity(raw,actual['raw_request'])
    identity(list(stops),actual.get('native_stops',[]))
    if actual['schema']!='commercial-source-credit-accounting-proof-1':raise ArithmeticError('commercial compiler schema differs')
    for name in ('original_anchor_refitted','mixed_ledger_adoption','empirical_adoption','accounting_approval'):
        if actual.get(name) is not False:raise ArithmeticError('commercial compiler eligibility differs')
    if actual.get('prospective_accounting_rate_policy') is not (raw['source']['policy']['reset_income']=='reviewed_prospective_at_period_start-1'):
        raise ArithmeticError('commercial accounting policy identity differs')
    for role in ('original','current'):
        source=actual[role+'_source'];identity(raw['source'][role],source['raw']);compare(raw['factor_policy'],source['policy'],check=check);verify_source(raw['source'][role],source,check)
    verify_draw_sources(raw,actual,check,original_acquisition=original_acquisition)
    original,_=project(raw['source']['original'],check);a=raw['source']['acquisition'];quote=sum((e['components']['principal']+e['components']['cash_interest']+e['components']['fees'])*math.pow(1.+a['original_annual_effective_rate'],-e['day_offset']/365.) for e in original['schedule']['events'])
    if original_acquisition is not None:
        if not exact_json_identity(typed(raw['source']['acquisition']),original_acquisition['acquisition']) or not any(b['path_key']==raw['credit']['paths'][0]['path_key'] and exact_json_identity(b['original'],typed(raw['source']['original'])) for b in original_acquisition['original_bindings']):raise ArithmeticError('current Physical acquisition does not bind original stopped source/anchors')
        binding=next(b for b in original_acquisition['original_bindings']if b['path_key']==raw['credit']['paths'][0]['path_key'])
        current=typed(raw.get('performing_draw_tranches',[]));original_draws=binding.get('draws',[])
        if len(current)!=len(original_draws):raise ArithmeticError('current original conditional tranche coverage differs')
        for now,old in zip(current,original_draws,strict=True):
            for field in ('review','draw_ordinal','eligible_origins'):
                if not exact_json_identity(now[field],old[field]):raise ArithmeticError('current tranche original date/eligibility identity differs')
            for field in ('original','acquisition'):
                if not exact_json_identity(now['source'][field],old['source'][field]):raise ArithmeticError('current tranche original source/acquisition identity differs')
        quote=original_acquisition['original_stopped_cash_quote']
    compare(float(quote),actual['original_cash_quote'],check=check)
    expected=replay(raw,check,stops=stops);identity(expected['prepared'],actual['prepared_credit_request']);carry=actual['carry'];identity(expected['prepared'],carry['raw_request'])
    if carry.get('schema')!='credit-path-income-proof-1':raise ArithmeticError('commercial carry schema differs')
    for name in ('original_anchor_refitted','corporate_source_binding','mixed_ledger_adoption','accounting_approval','complete_coupon_settlement_ownership'):
        if carry.get(name) is not False:raise ArithmeticError('commercial carry eligibility differs')
    for name in ('native_factor_provider_selected','default_interest_stopped'):
        if carry.get(name) is not True:raise ArithmeticError('commercial carry source ownership differs')
    for key in ('credit','carry_points','earned_origin_contributions','complete_recovery_receipts'):
        compare(expected[key],carry[key],f'$.carry.{key}',check)
    for key in ('native_source_cash','native_coupon_recoveries'):
        compare(expected[key],carry.get(key,[]),f'$.carry.{key}',check)
    gs,closing=groups(expected,check);book=actual['accounting'];compare(expected['policy'],book['policy'],check=check);compare(gs,book['groups'],check=check);compare(closing,book['closing'],check=check)
    if book['schema']!='commercial-coupon-accounting-proof-1' or book.get('complete_native_recovery_tail_retained') is not True:
        raise ArithmeticError('commercial accounting source/tail differs')
    for name in ('actual_risk_source_binding','mixed_ledger_adoption','accounting_approval'):
        if book.get(name) is not False:raise ArithmeticError('commercial accounting eligibility differs')
    native_closed=all(c['principal']==0. and c['book_adjustment']==0. and c['legal_interest']==0. and c['recognized_interest']==0. and (not c.get('reviewed_credit_state') or c['reviewed_credit_state']['legal_principal']==0.) for c in closing)
    if type(book.get('claims_fully_closed')) is not bool or book['claims_fully_closed']!=native_closed:raise ArithmeticError('commercial claims closure differs')
    # Admission is integer bounded evidence, never permission to trust money.
    for admission in (actual['admission'],carry['admission'],actual['original_source']['admission'],actual['current_source']['admission']):
        for key,value in admission.items():
            if key=='carry':continue
            if key=='full_credit_end_year':compare(float(full_end(raw)),value,check=check);continue
            if type(value) is not int or value<0:raise ArithmeticError('commercial admission integer identity differs')
    if len(gs)>actual['admission']['carry_points_upper'] or actual['admission']['joined_work_upper']>8_000_000 or actual['admission']['joined_bytes_upper']>128<<20:
        raise ArithmeticError('commercial observed source exceeded admission')
    return dict(raw=deepcopy(raw),replay=expected,groups=gs,closing=closing)


def verify_preparation(raw,prepared,receipt,check=lambda:False,*,expected_protocol=6):
    check()
    if not exact_json_identity(raw,prepared) or not exact_json_identity(raw,receipt['original_request']):raise ArithmeticError('commercial immutable raw request differs')
    if receipt.get('schema')!=SCHEMA or type(receipt.get('protocol'))is not int or receipt['protocol']!=expected_protocol or receipt.get('shared_daily_sim_adoption') is not True or receipt.get('mixed_ledger_adoption') is not False:
        raise ArithmeticError('commercial single Sim preparation identity differs')
    for name,limit in (('admitted_work_upper',8_000_000),('retained_bytes_upper',128<<20)):
        if type(receipt.get(name))is not int or not 0<receipt[name]<=limit:raise ArithmeticError('commercial preparation admission integer/bound differs')
    for name in ('pricing_credit_adoption','independent_persisted_verification','empirical_validation'):
        if receipt.get(name) is not False:raise ArithmeticError('commercial preparation eligibility differs')
    if len(raw['commercial'])!=len(receipt['sources']):raise ArithmeticError('commercial source coverage differs')
    if any(r['credit'].get('instrument_kind','commercial_loan')=='issuer_credit_debt' for r in raw['commercial']):raise ArithmeticError('commercial ledger loan route cannot classify issuer securities')
    sources=[verify_compiler(r,s,check) for r,s in zip(raw['commercial'],receipt['sources'])]
    report=raw['spec'].get('horizon_days',360)
    if type(report) is not int or type(receipt.get('reporting_horizon_days'))is not int or receipt['reporting_horizon_days']!=report:raise ArithmeticError('commercial original reporting horizon differs')
    def full_days(r):
        year=full_end(r);days=year*365.;rounded=round(days)
        day=rounded if year==rounded/365. else math.ceil(days)
        return day+1 if year>day/365. else day
    full=max(report,max(full_days(r) for r in raw['commercial']))
    resolved=deepcopy(raw['spec']);resolved['horizon_days']=full
    from ._deposit_continuation_calendar import _normalize
    from ._deposit_continuation_models import Account,Position,Scenario
    normalized=_normalize(resolved)
    for key,cls in (('accounts',Account),('positions',Position),('scenarios',Scenario)):
        for row in normalized[key]:
            for field in fields(cls):
                if field.type is float:row[field.name]=float(row[field.name])
    spec={k:[SimpleNamespace(**v) for v in vs] if k in ('accounts','positions','scenarios') else deepcopy(vs) for k,vs in normalized.items()}
    from .commercial_reports_reference import scope
    scope(spec,[a['position_id'] for a in raw['allocations']])
    compare(normalized,receipt['resolved_specification'],check=check)
    return dict(raw=deepcopy(raw),sources=sources,spec=spec,horizon=full,reporting_horizon=report)


GL={'cash':'cash','live_principal':'asset_principal','default_principal':'credit_default_principal','live_book_adjustment':'book_adjustment','default_book_adjustment':'credit_default_book_adjustment',
    'allowance':'allowance','credit_loss_expense':'pnl:commercial_credit_loss_expense','recovery_income':'pnl:commercial_recovery_income','interest_income':'pnl:commercial_effective_interest_income','basis_recognition_expense':'pnl:commercial_basis_realization_expense'}


def journal_lines(group,tracking,factor):
    """Gross sign classification and financial/memo lines in source order."""
    stocks,legal=tracking;lines=[]
    def append(gl,value):
        if value!=0.:lines.append((gl,value))
    for owned in group['postings']:
        p=owned['posting'];code=p['account'];key=(code,owned['coupon_id'],owned['earning_origin_index']);old=stocks.get(key,0.);new=old+p['amount'];stocks[key]=new
        if code in ('live_accrued_interest','default_accrued_interest'):
            prefix='credit_default_' if code.startswith('default') else 'dated_'
            append(prefix+'interest_receivable',(max(new,0.)-max(old,0.))*factor);append(prefix+'deferred_income',(min(new,0.)-min(old,0.))*factor)
        else:append(GL[code],p['amount']*factor)
    for lot in group['next']['lots']:
        key=(lot['coupon_id'],lot['earning_origin_index']);delta=(lot['legal_remaining']-legal.get(key,0.))*factor;append('memo:commercial_legal_coupon_right',delta);append('memo:commercial_legal_coupon_contra',-delta);legal[key]=lot['legal_remaining']
    if group['next'].get('reviewed_credit_state') is not None:
        principal=group['next']['reviewed_credit_state']['legal_principal'];delta=(principal-legal.get((None,None),0.))*factor;append('memo:commercial_legal_principal',delta);append('memo:commercial_legal_principal_contra',-delta);legal[(None,None)]=principal
    return lines


def _rows(writer,table,check):
    for frame in writer.frames(table):
        check()
        for row in frame.iter_rows(named=True):check();yield row


def verify_stream(raw,prepared,writer,check=lambda:False):
    """Replay ordered saved source groups, journal lines and daily gross books."""
    if not exact_json_identity(writer.completion,dict(complete=True,protocol=6)):raise ArithmeticError('commercial producer completion differs')
    specs=prepared['spec'];positions={p.id:p for p in specs['positions']};alloc={a['position_id']:a['allocated_face'] for a in raw['allocations']};weights={(p['scenario'],p['position_id'],p['path_key']):p['source_weight'] for p in raw['participations']}
    expected=[];bindings=[]
    for si,source in enumerate(prepared['sources']):
        current=source['raw']['source']['current'];scenario=current['scenario_id'];position=current['position_id'];origin_day=source.get('origin_day',0);source_class=source.get('source_class','commercial_loan-1');allocation=alloc[position]/current['opening_principal']
        for p in source['raw']['credit']['paths']:
            key=(scenario,position,p['path_key']);weight=weights[key];factor_=allocation*weight;tracking=({},{});last=None;daily={}
            for gi,g in enumerate(source['groups']):
                if g['path_key']!=p['path_key']:continue
                days=g['year']*365.;rounded=round(days);elapsed=rounded if g['year']==rounded/365. else math.ceil(days)
                if g['year']>elapsed/365.:elapsed+=1
                day=origin_day+elapsed
                lines=journal_lines(g,tracking,factor_);expected.append((scenario,day,origin_day/365.+g['year'],len(bindings),gi,dict(scenario=scenario,day=day,native_credit_year=g['year'],source_class=source_class,source_origin_day=origin_day,global_native_credit_year=origin_day/365.+g['year'],date_policy=DATE_POLICY,position_id=position,path_key=p['path_key'],source_weight=float(weight),allocation_factor=float(allocation),native_group_index=gi,native_atomic_group=g),lines));daily[day]=g['next'];last=g['next']
            bindings.append(dict(source_class=source_class,origin_day=origin_day,key=key,allocation=float(allocation),weight=float(weight),daily=daily,closing=last))
    order={name:i for i,name in enumerate(['baseline']+[s.name for s in specs['scenarios']])};expected.sort(key=lambda e:(order[e[0]],e[1],e[2],e[3],e[4]))
    lineage=list(_rows(writer,'commercial_source_lineage',check))
    if len(lineage)!=len(expected):raise ArithmeticError('commercial persisted group coverage differs')
    transactions={};sequences=defaultdict(int)
    for row,item in zip(lineage,expected):
        scenario,day,_,_,_,want,lines=item;group=json.loads(row.pop('native_atomic_group_json'));tx=row.pop('transaction_number');row['native_atomic_group']=group;compare(want,row,check=check)
        if lines:
            if type(tx) is not int or tx<=sequences[scenario]:raise ArithmeticError('commercial persisted transaction order differs')
            sequences[scenario]=tx;transactions[(scenario,tx)]=(day,positions[want['position_id']].account,want['native_atomic_group']['phase'],want['position_id'],lines)
        elif tx is not None:raise ArithmeticError('empty commercial source created transaction')
    current=None;observed=[];seen=set();journal_count=0
    def flush():
        if current in transactions:
            day,account,event,position,lines=transactions[current]
            want=[dict(day=day,account=account,event=event,gl_account=gl,instrument_id='' if gl=='cash' else position,debit=max(v,0.),credit=max(-v,0.)) for gl,v in lines]
            compare(want,observed,check=check);seen.add(current)
        elif any(r['event'] in {g['phase'] for s in prepared['sources'] for g in s['groups']} or r['instrument_id'] in alloc for r in observed):raise ArithmeticError('unbound commercial persisted journal')
    for row in _rows(writer,'journal',check):
        key=(row['scenario'],int(row['transaction_id'].rsplit(':',1)[1]));journal_count+=1
        if current!=key:
            if current is not None:flush()
            current=key;observed=[]
        observed.append({k:row[k] for k in ('day','account','event','gl_account','instrument_id','debit','credit')})
    if current is not None:flush()
    if seen!=set(transactions):raise ArithmeticError('missing commercial persisted journal source')
    expected_daily=[];expected_closing=[]
    for scenario in order:
        for day in range(prepared['horizon']+1):
            for b in bindings:
                if b['key'][0]!=scenario:continue
                if day in b['daily']:b['current']=b['daily'][day]
                if 'current' not in b:continue
                value=dict(scenario=scenario,day=day,position_id=b['key'][1],path_key=b['key'][2],source_weight=b['weight'],allocation_factor=b['allocation'],source_class=b['source_class'],source_origin_day=b['origin_day'],unweighted_book=b['current'],capital_ftp_eligible=False)
                expected_daily.append(value)
                if day==prepared['horizon']:expected_closing.append(value)
    for table,wanted in (('commercial_daily',expected_daily),('commercial_closing',expected_closing)):
        rows=[]
        for row in _rows(writer,table,check):row['unweighted_book']=json.loads(row.pop('unweighted_book_json'));rows.append(row)
        compare(wanted,rows,check=check)
    gl,count=replay_partitions(writer.frames('journal'));trial=list(_rows(writer,'trial_balance',check));seen=set()
    for r in trial:
        key=tuple(r[k] for k in ('scenario','account','gl_account','instrument_id'))
        if key in seen or key not in gl:raise ArithmeticError('commercial persisted closing GL identity differs')
        compare(float(gl[key]),r['balance'],check=check);seen.add(key)
    if seen!=set(gl):raise ArithmeticError('commercial persisted closing GL missing')
    from .commercial_reports_reference import verify as verify_reports
    verify_reports(prepared,expected,writer,check)
    return dict(verified=True,journal_replayed=True,journal_rows=count,commercial_source_recomputed=True,commercial_credit_clock_recomputed=True,
        original_yield_anchors_retained=True,gross_coupon_origins_reconciled=True,ordered_same_sim_journal_replayed=True,dated_source_reconciled=True,
        complete_unweighted_tail_verified=True,persisted_partitions_replayed=True,accounting_standard_approval=False,empirical_validation=False,capital_ftp_eligible=False)
