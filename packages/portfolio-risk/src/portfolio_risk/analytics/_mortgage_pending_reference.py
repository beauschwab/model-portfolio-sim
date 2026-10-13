"""Artifact-only independent cached daily transport and complete-source oracle.

Raw original/current source is reconstructed by the frozen independent source
oracle. SciPy exponentials supply checkpoint economics; producer actions, money,
flags, and committed states never supply financial truth.
"""
from copy import deepcopy
from pathlib import Path
import json
import math
import hashlib
import sys

from portfolio_risk.analytics import mortgage_credit_reference as sequential
from portfolio_risk.analytics import mortgage_credit_math_reference as model
from ._mortgage_pending_math_reference import propagate, ordered_sum
from ._mortgage_generator_binding import EXPECTED_DIGEST, EXPECTED_BYTES

SCHEMA='mortgage-pending-accounting-1'
POLICY=dict(checkpoint='cached_native_interval_checkpoint-1',
 pending='pending_exit_assets_until_native_boundary-1',income='original_eir_reward_only-1',
 boundary='source_boundary_dispositions_only-1',unsupported='reject_added_cost_nonaccrual_allowance-1')


def admit(raw,check=lambda:False):
    model.checkpoint(check);model.fields(raw,'schema source policy');model.fields(raw['policy'],'identity '+ ' '.join(POLICY));model.identity(raw['policy']['identity'])
    if raw['schema']!=SCHEMA or any(raw['policy'][k]!=v for k,v in POLICY.items()):raise ArithmeticError('explicit pending source policy')
    source,a=sequential.admit(raw['source'],check);r=deepcopy(raw);r['source']=source
    for contract in (source['contract'],source['original_contract']):
        values=[contract['original_principal'],contract['opening_contractual_principal'],contract['original_payment'],contract['current_payment'],ordered_sum(contract['opening_borrower_principal']),ordered_sum(contract['opening_investor_principal'])]
        if any(abs(x)>10000. for x in values):raise ArithmeticError('component money admission exceeds10000')
    growth=math.exp(max(math.log1p(source['acquisition']['original_annual_effective_rate']),0.)*max(source['contract']['horizon_days'],source['original_contract']['horizon_days'])/365.)
    if source['acquisition']['actual_cost']*growth>10000.:raise ArithmeticError('component growth admission exceeds10000')
    lag=max(source['policy']['economic_principal_disposal_lag_days'],source['policy']['interest']['disposal_lag_days'])
    end=source['contract']['as_of_ordinal']+max(source['contract']['horizon_days'],source['original_contract']['horizon_days'])+lag
    days=end-source['contract']['as_of_ordinal'];model.ordinal(end)
    raw_bytes=len(sequential.canonical_bytes(r))
    tails=sum(len(x) for x in source['contract']['complete_recovery_tails'])+sum(len(x) for x in source['policy']['interest']['complete_collection_tails'])
    groups=a['interval_count']*(23+tails)+days+1
    work=a['work_bound']+a['interval_count']*750000+4096*(days+groups)
    per=raw_bytes+a['interval_count']*3*512+8192
    capacity=a['retained_byte_bound']+(groups+days)*per+a['interval_count']*16384+32768
    if not 1<=days<=64 or raw_bytes>1024*1024 or work>60000000 or capacity>32*1024*1024:raise ArithmeticError('joined pending admission')
    model.checkpoint(check)
    return r,dict(reserved_end_ordinal=end,days=days,work=work,reserved_bytes=capacity,max_groups=groups,interval_count=a['interval_count'])


def empty_channel():return dict(principal=0.,recognized_interest=0.,basis=0.)
def carrying(channel):return channel['principal']+channel['recognized_interest']+channel['basis']
def book_total(book):return ordered_sum(carrying(x) for x in book['slots'])+ordered_sum(carrying(x['book']) for x in book['claims'])
def line(account,amount,slot=None,claim=None,index=None):return dict(account=account,slot=slot,claim_id=claim,source_action_index=index,amount=float(amount))
def channel_lines(values,slot=None,claim=None):return [line(k,v,slot,claim) for k,v in zip(('principal','recognized_interest','basis'),values,strict=True)]


def reconstruct(raw,check=lambda:False):
    r,admission=admit(raw,check);s=sequential.reconstruct(r['source'],check);c=r['source']['contract'];policy=r['source']['policy'];acq=r['source']['acquisition'];start=c['as_of_ordinal']
    if max(s['full_end_ordinal'],s['original_full_end_ordinal'])>admission['reserved_end_ordinal']:raise ArithmeticError('complete source tail')
    book=dict(ordinal=start,slots=[empty_channel() for _ in range(7)],borrower_principal=[0.]*7,borrower_contract_interest=[0.]*7,claims=[],cash=0.,original_eir_income=0.,reviewed_income=0.)
    groups=[];daily=[];checkpoints=[]
    def validate(b):
        model.checkpoint(check)
        for value in [b['cash'],b['original_eir_income'],b['reviewed_income'],book_total(b),b['original_eir_income']+b['reviewed_income']]+b['borrower_principal']+b['borrower_contract_interest']:model.finite(value,-10000.,10000.)
        for ch in b['slots']+[x['book'] for x in b['claims']]:
            for k in ch:model.finite(ch[k],-10000.,10000.)
            model.finite(carrying(ch),-10000.,10000.)
            if min(ch['principal'],ch['recognized_interest'],carrying(ch))< -1e-9:raise ArithmeticError('negative pending gross book')
        for claim in b['claims']:
            for key in ('economic_principal','economic_contract_interest'):
                model.finite(claim[key],-10000.,10000.)
                if claim[key]<-1e-9:raise ArithmeticError('negative economic claim')
        model.equal(book_total(b)+b['cash'],b['original_eir_income']+b['reviewed_income'],'atomic conservation')
    def commit(kind,source,review,lines,next_=None):
        nonlocal book
        model.checkpoint(check);next_=deepcopy(book if next_ is None else next_)
        model.equal(ordered_sum(x['amount'] for x in lines),0.,'atomic balance')
        for row in lines:
            model.finite(row['amount'],-10000.,10000.);a=row['account']
            if a in ('principal','recognized_interest','basis'):
                ch=next_['slots'][row['slot']] if row['slot'] is not None else next(x['book'] for x in next_['claims'] if x['claim_id']==row['claim_id']);ch[a]+=row['amount']
            elif a=='cash':next_['cash']+=row['amount']
            elif a in ('original_eir','reviewed_income'):next_['original_eir_income' if a=='original_eir' else 'reviewed_income']-=row['amount']
            else:raise ArithmeticError('unknown account')
        validate(next_)
        if len(groups)>=admission['max_groups']:raise ArithmeticError('group bound')
        indices=[]
        for row in lines:
            if row['source_action_index'] is not None and (not indices or indices[-1]!=row['source_action_index']):indices.append(row['source_action_index'])
        groups.append(dict(source_action_indices=indices,ordinal=next_['ordinal'],kind=kind,source_id=source,review=deepcopy(review),lines=deepcopy(lines),committed=deepcopy(next_)));book=next_
    opening=deepcopy(book);opening['borrower_principal'][:3]=c['opening_borrower_principal'];lines=[line('cash',-acq['actual_cost'])]
    for origin in range(3):lines+=channel_lines([c['opening_investor_principal'][origin],0.,acq['opening_gross_carrying'][origin]-c['opening_investor_principal'][origin]],origin)
    commit('acquisition',acq['identity']['id'],acq['identity'],lines,opening);daily.append(deepcopy(book))
    links={x['entitlement_id']:x['cash_event_id'] for x in s['source']['capture']['cash_links']};actions={}
    for index,a in enumerate(s['actions']):actions.setdefault(a['ordinal'],[]).append(index)
    consumed=set()
    last_interval=None;last_reward=0.
    for day in range(start+1,s['full_end_ordinal']+1):
        model.checkpoint(check);book['ordinal']=day
        active=next(((i,t) for i,t in enumerate(s['source']['intervals']) if t['start_ordinal']<day<=t['end_ordinal']),None)
        if active:
            index,receipt=active
            if last_interval!=index:last_interval=index;last_reward=0.
            interval=s['intervals'][index];request=deepcopy(interval['native_carry_request']);request['duration_years']=(day-receipt['start_ordinal'])/365.
            transported=model.carry_apply(request,interval['opening'],check);state=transported['closing'];new_eir=transported['total_interval_earned_eir']-last_reward;last_reward=transported['total_interval_earned_eir']
            opening=interval['opening'];mathbook=dict(principal=opening['principal'],accrued=opening['accrued_interest'],gross=opening['gross_carrying'],earned=opening['earned_eir'],loan_principal=receipt['opening_borrower_principal']+[0.]*4,loan_interest=receipt['opening_borrower_interest']+[0.]*4,probability=receipt['opening_probability'])
            loan,_=propagate(mathbook,request['generator'],request['recognized_contractual_rates'],receipt['loan_operator_request']['accrual_rates'],acq['original_annual_effective_rate'],request['duration_years'])
            borrower=dict(schema='credit-operator-state-1',measure='physical',monetary_unit=c['monetary_unit'],probability=loan['probability'],principal=loan['loan_principal'],undrawn_commitment=[0.]*7,accrued_interest=loan['loan_interest'],book_adjustment=[0.]*7)
            next_=deepcopy(book);next_['borrower_principal']=loan['loan_principal'];next_['borrower_contract_interest']=loan['loan_interest'];lines=[]
            for slot in range(7):
                old=book['slots'][slot];p=state['principal'][slot];a=state['accrued_interest'][slot];b=state['gross_carrying'][slot]-p-a
                lines+=channel_lines([p-old['principal'],a-old['recognized_interest'],b-old['basis']],slot)
            lines.append(line('original_eir',-new_eir));commit('exact_checkpoint',f'interval:{index}:day:{day}',acq['income_review'],lines,next_)
            checkpoints.append(dict(ordinal=day,interval_index=index,duration_years=request['duration_years'],generator=request['generator'],recognized_contractual_rates=request['recognized_contractual_rates'],exact_investor=deepcopy(state),exact_borrower=borrower,new_original_eir=new_eir,method='retained_exact_source_boundary' if day==receipt['end_ordinal'] else 'cached_one_day_recurrence',one_day_duration_years=1./365.))
        for ai in actions.get(day,[]):
            model.checkpoint(check)
            if ai in consumed:continue
            action=s['actions'][ai];boundary=s['source']['capture']['boundaries'][action['interval_index']];kind=action['kind'];sid=action['source_id'];next_=deepcopy(book);lines=[];compound=False;review=action['review']
            if kind=='default_accrued_reversal':
                close=next(x for x in s['default_closeouts'] if x['claim_id']==sid)
                if any(x['claim_id']==sid for x in book['claims']):raise ArithmeticError('duplicate source default')
                next_['claims'].append(dict(claim_id=sid,origin=close['origin'],birth_ordinal=day,book=empty_channel(),economic_principal=close['gross_principal'],economic_contract_interest=close['gross_contract_interest']))
                values=[close['gross_principal'],close['recognized_interest'],close['signed_default_basis']]
                lines=channel_lines([-v for v in values],3+close['origin'])+channel_lines(values,claim=sid)
                for offset,(name,account) in enumerate([('default_accrued_reversal','recognized_interest'),('default_signed_basis_disposal','basis'),('default_native_principal_loss_book_chargeoff','principal')]):
                    part=s['actions'][ai+offset]
                    if part['kind']!=name or any(part[key]!=action[key] for key in ('source_id','ordinal','interval_index','origin')):raise ArithmeticError('default component source ordering')
                    lines += [line(account,-part['gross_carry_disposed'],claim=sid,index=ai+offset),line('reviewed_income',-part['income'],index=ai+offset)]
                    consumed.add(ai+offset)
                compound=True;kind='native_default_claim_and_disposition';review=policy['identity']
            elif kind=='native_default_principal_cash':
                close=next(x for x in s['default_closeouts'] if sid in x['native_principal_cash_ids']);cid=close['claim_id'];claim=next(x for x in next_['claims'] if x['claim_id']==cid);claim['economic_principal']-=action['actual_cash']
                lines=[line('cash',action['actual_cash']),line('principal',-action['actual_cash'],claim=cid)]
            elif kind=='reviewed_default_interest_cash_income':
                cash=next(x for x in s['reviewed_interest_cash'] if x['cash_id']==sid);claim=next(x for x in next_['claims'] if x['claim_id']==cash['claim_id']);claim['economic_contract_interest']-=cash['amount']
                lines=[line('cash',action['actual_cash']),line('reviewed_income',-action['income'])]
            elif kind=='scheduled_native_cash':
                item=next(x for x in boundary['scheduled'] if x['entitlement_id'] is not None and links[x['entitlement_id']]==sid)
                lines=channel_lines([-item['principal'],-item['recognized_interest'],item['recognized_interest']-item['interest']],item['origin'])+[line('cash',action['actual_cash'])]
                exit=s['actions'][ai+1] if ai+1<len(s['actions']) else None
                if exit and exit['kind']=='scheduled_full_exit_signed_basis' and all(exit[key]==action[key] for key in ('origin','ordinal','interval_index')):
                    for row in lines:row['source_action_index']=ai
                    lines += [line('basis',-exit['gross_carry_disposed'],exit['origin'],index=ai+1),line('reviewed_income',-exit['income'],index=ai+1)]
                    consumed.add(ai+1);compound=True;kind='scheduled_full_exit_cash_and_basis'
            elif kind=='scheduled_full_exit_signed_basis':lines=[line('basis',-action['gross_carry_disposed'],action['origin']),line('reviewed_income',-action['income'])]
            elif kind.endswith('_exact_exit'):
                item=next(x for x in boundary['prepayments'] if x['entitlement_id'] is not None and links[x['entitlement_id']]==sid)
                lines=channel_lines([-item['principal'],-item['recognized_interest'],-(action['gross_carry_disposed']-item['principal']-item['recognized_interest'])],6)+[line('cash',action['actual_cash']),line('reviewed_income',-action['income'])]
            else:raise ArithmeticError('unsupported source action')
            if not compound:
                for row in lines:row['source_action_index']=ai
            commit(kind,sid,review,lines,next_)
        for close in s['default_closeouts']:
            for prefix,field in [('principal','economic_principal'),('interest','economic_contract_interest')]:
                if close[prefix+'_disposal_ordinal']==day:
                    next_=deepcopy(book);claim=next(x for x in next_['claims'] if x['claim_id']==close['claim_id']);claim[field]-=close[prefix+'_economic_disposal']
                    commit('economic_'+prefix+'_waiver',close['claim_id'],close[prefix+'_disposal_review'],[],next_)
        boundary=next((x for x in s['source']['capture']['boundaries'] if x['ordinal']==day),None)
        if boundary:
            point=next(x for x in s['source']['native_projection']['points'] if x['ordinal']==day);book['borrower_principal']=point['borrower_principal']+[0.]*4;book['borrower_contract_interest']=point['borrower_interest']+[0.]*4
            for origin in range(3):
                model.equal(book['slots'][origin]['principal'],boundary['closing_investor_principal'][origin],'boundary P');model.equal(book['slots'][origin]['recognized_interest'],boundary['closing_investor_interest'][origin],'boundary A');model.equal(carrying(book['slots'][origin]),s['intervals'][boundary['interval_index']]['closing_live_gross_carrying'][origin],'boundary G')
            for slot in book['slots'][3:]:
                for value in (slot['principal'],slot['recognized_interest'],carrying(slot)):model.equal(value,0.,'pending exit retirement')
        validate(book);daily.append(deepcopy(book))
    for ch in book['slots']+[x['book'] for x in book['claims']]:
        for value in ch.values():model.equal(value,0.,'gross book full closure')
    for claim in book['claims']:
        for key in ('economic_principal','economic_contract_interest'):model.equal(claim[key],0.,'economic full closure')
    model.equal(book['original_eir_income'],s['total_earned_eir'],'EIR once');model.equal(book['original_eir_income']+book['reviewed_income'],s['total_accounting_income'],'total income');model.equal(book['cash']+acq['actual_cost'],s['total_actual_cash'],'actual cash once')
    interval_count=len(s['intervals']);active_days=len(checkpoints)
    experiment=dict(source_work_bound=s['admission']['work_bound'],cached_compile_work_bound=interval_count*750000,daily_apply_work_bound=admission['days']*4096,group_work_bound=admission['max_groups']*4096,coupled_compilations=interval_count,loan_compilations=interval_count,coupled_matrix_multiplications=0,coupled_applications=active_days,loan_applications=active_days,retained_full_boundary_controls=interval_count,actual_groups=len(groups),final_output_byte_cap=32*1024*1024,source_and_checkpoint_under_8m=admission['work']<=8000000,joined_ledger_admission_verified=False)
    return dict(schema=SCHEMA,policy=r['policy'],admission=admission,experiment=experiment,source=s,checkpoints=checkpoints,groups=groups,daily=daily,closing=deepcopy(book),production_adoption=False,mixed_ledger_adoption=False,accounting_standard_approval=False)


def verify(raw,actual,check=lambda:False):
    expected=reconstruct(raw,check)
    # Verify producer source completely against raw before accepting its exact
    # byte hashes/work counters (native uniformization has different last bits).
    verify_source(raw['source'],actual['source'],check);expected['source']=deepcopy(actual['source'])
    # Numerical backend work counters are bounded diagnostics, never finance truth.
    count=actual.get('experiment',{}).get('coupled_matrix_multiplications')
    interval_count=expected['experiment']['coupled_compilations']
    if type(count) is not int or not 0<=count<=140*interval_count:raise ArithmeticError('bounded compiled matrix metadata')
    if raw['source']['acquisition']['original_annual_effective_rate']==0. and count!=0:raise ArithmeticError('zero-yield specialized compile metadata')
    expected['experiment']['coupled_matrix_multiplications']=count
    if len(sequential.canonical_bytes(actual))>min(expected['admission']['reserved_bytes'],32*1024*1024):raise ArithmeticError('complete escaped output capacity')
    sequential.checked_equal(actual,expected,check,'pending')
    model.checkpoint(check)
    return dict(verified=True,raw_source_recomputed=True,exact_daily_pending_rights_recomputed=True,atomic_groups_recomputed=True,complete_gross_claim_tail_verified=True,production_adoption=False,mixed_ledger_adoption=False,empirical_validation=False,joined_ledger_admission_verified=False)


def verify_source(raw,actual,check):
    """Installed pure original/current source oracle; no native reader callback.

    Public financial hash binds the existing nine-file generator; executable and
    new pending-owner identities are pinned separately by the contained tests.
    """
    if model.SOURCE_SHA!=EXPECTED_DIGEST or model.GENERATOR_CODE_HASH_BYTES!=EXPECTED_BYTES:
        raise ArithmeticError('public financial generator review identity')
    typed=sequential.typed(raw)
    immutable(actual['raw_request'],typed,'retained original/current raw source')
    for name,contract in (('source',typed['contract']),('original_source',typed['original_contract'])):
        projection=actual[name]['native_projection']
        for key in ('identity','ownership','monetary_unit','original_principal','original_payment','origination_ordinal'):
            immutable(projection[key],contract[key],'projection immutable/'+name+'/'+key)
    for interval in actual['intervals']:
        request=interval['native_carry_request']
        immutable(request['original_annual_effective_rate'],typed['acquisition']['original_annual_effective_rate'],'original yield request')
    sequential.verify(raw,actual,check)


def immutable(actual,expected,path):
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():raise ArithmeticError(path+': immutable fields')
        for key,value in expected.items():immutable(actual[key],value,path+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': immutable sequence')
        for i,value in enumerate(expected):immutable(actual[i],value,path+'/'+str(i))
    elif type(actual) is not type(expected) or actual!=expected or (type(expected) is float and
            (not math.isfinite(actual) or actual.hex()!=expected.hex())):
        raise ArithmeticError(path+': immutable value')
