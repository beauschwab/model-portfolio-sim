"""Independent joint finite Bellman replay with separate loan/fee discounts."""
from copy import deepcopy
import math
from .commercial_credit_reference import replay
from .commercial_exercise_reference import information as loan_information,_state,_surviving_rights,_key,compile_policy as loan_compile
from .commercial_fee_reference import bind
from .commercial_dto import graph_typed
from .commercial_reference_math import _cash,_df
from .nonintegral_fee_reference import FIELDS
from .commercial_fee_adoption_reference import require_complete


def bound(policy,raw,projection,check,stops=()):
    rows=bind(policy,raw,dict(replay=projection),check,stops)
    if len(rows)!=1:raise ArithmeticError('one independent joint fee path required')
    require_complete(rows[0],check)
    return rows[0]


def fee_cash(owner):
    result=[]
    for year,g in zip(owner['native_years'],owner['source']['groups'],strict=True):
        amount=math.fsum(p['amount']for p in g['postings']if p['account']=='cash')
        if amount!=0.:result.append(dict(year=year,amount=amount,native=g['event']['event_id'].startswith('native-fee:')))
    return result


def information(raw,projection,fee_policy,ordinal,timing,check=lambda:False,owner=None):
    owner=owner or bound(fee_policy,raw,projection,check);policy=graph_typed(fee_policy)
    info=loan_information(raw,projection,ordinal,timing);year=(ordinal-raw['source']['current']['asof_ordinal'])/365.
    states={f['fee_id']:dict.fromkeys(FIELDS,0.)for f in policy['fees']}
    for f in policy['fees']:states[f['fee_id']]['unearned_entitlement']=f['entitlement']
    events=[]
    for t,g in zip(owner['native_years'],owner['source']['groups'],strict=True):
        if t<year or t==year and timing=='after_scheduled_cash':
            states[g['event']['fee_id']]=deepcopy(g['next'])
            events.append(dict(year=t,**{k:deepcopy(g['event'][k])for k in('fee_id','action','reason','amount')}))
    info['separate_fees']=dict(contracts=policy['fees'],policy=policy['policy'],**{k:policy[k]for k in('default','prepayment','exercise','maturity','coincident_order')},states=states,events=events)
    return info


def make_stop(policy,raw,date,action,rights_only):
    result=dict(path_key=raw['credit']['paths'][0]['path_key'],event_id=f"native-exercise:{policy['identity']['id']}:{date['ordinal']}",ordinal=date['ordinal'],action=action,principal_clean_price_factor=date[action+'_clean_price_factor'],fee=date[action+'_fee'],review=deepcopy(policy['identity']),timing=date['timing'])
    if rights_only:result['rights_only_review']=deepcopy(policy['rights_only_termination']['review'])
    return result


def compile_policy(policy,histories,fee_policies,curve,oas,spread,bump=0.,check=lambda:False):
    policy=loan_compile(policy,histories,curve,oas,bump,check)['result']['policy']
    if len(fee_policies)!=len(histories)or any(p['coincident_order']!='native_exit_follows_native_exercise_timing-1'for p in fee_policies):raise ArithmeticError('explicit complete joint exercise clock policies required')
    raws=[h['source']for h in histories];weights=[h['weight']for h in histories];projections=[replay(r,check)for r in raws]
    owners=[bound(p,r,v,check)for p,r,v in zip(fee_policies,raws,projections,strict=True)]
    cash=[_cash(p['credit'])for p in projections];fees=[fee_cash(p)for p in owners]
    asof=raws[0]['source']['current']['asof_ordinal'];root=_key(information(raws[0],projections[0],fee_policies[0],asof,'after_scheduled_cash',check,owners[0]))
    if any(_key(information(r,p,f,asof,'after_scheduled_cash',check,b))!=root for r,p,f,b in zip(raws,projections,fee_policies,owners,strict=True)):raise ArithmeticError('joint initial observable information differs')
    def df(t,rate):return _df(curve,t)*math.exp(-(rate+bump)*t)
    next_values=[(0.,0.)for _ in raws];next_time=None;lookup={};nodes=[]
    for date in reversed(policy['exercise_dates']):
        check();year=(date['ordinal']-asof)/365.;ld=df(year,oas);fd=df(year,spread);groups={}
        for i,(r,p,f,b)in enumerate(zip(raws,projections,fee_policies,owners,strict=True)):
            info=information(r,p,f,date['ordinal'],date['timing'],check,b);groups.setdefault(_key(info),(info,[]))[1].append(i)
        values=[(0.,0.)for _ in raws]
        def after(t):return t>year or t==year and date['timing']=='before_scheduled_cash'
        def before(t):return next_time is None or t<next_time[0]or t==next_time[0]and next_time[1]=='after_scheduled_cash'
        for key,(info,members)in sorted(groups.items()):
            probability=sum(weights[i]for i in members);continuation=[0.,0.]
            for i in members:
                loan,fee=(next_values[i][0]*df(next_time[0],oas)/ld,next_values[i][1]*df(next_time[0],spread)/fd)if next_time else(0.,0.)
                loan+=sum(((c['principal']+c['interest']+c['fees'])*df(c['year'],oas)/ld for c in cash[i]if after(c['year'])and before(c['year'])),0.)
                fee+=sum((c['amount']*df(c['year'],spread)/fd for c in fees[i]if after(c['year'])and before(c['year'])),0.)
                continuation[0]+=weights[i]/probability*loan;continuation[1]+=weights[i]/probability*fee
            state,principal,legal=_state(raws[members[0]],projections[members[0]],year,date['timing']);rights_only=principal==0. and _surviving_rights(raws[members[0]],projections[members[0]],date['ordinal'],date['timing'])
            if state in('performing','watch','delinquent')and rights_only and not policy.get('rights_only_termination'):raise ArithmeticError('explicit joint rights-only owner required')
            chosen=tuple(continuation);decision='continue'
            if state in('performing','watch','delinquent')and(principal>0. or rights_only):
                for action in(('put','call')if policy['simultaneous_priority']=='issuer'else('call','put')):
                    if date[action+'_clean_price_factor']is None:continue
                    payoff=[0.,0.]
                    for i in members:
                        stop=make_stop(policy,raws[i],date,action,rights_only);stopped=replay(raws[i],check,stops=[stop]);full=_cash(stopped['credit'])
                        lv=math.fsum((c['principal']+c['interest']+c['fees'])*df(c['year'],oas)/ld for c in full if c['year']>year or c['year']==year and(date['timing']=='before_scheduled_cash'or c['event_id']==stop['event_id']))
                        fb=bound(fee_policies[i],raws[i],stopped,check,[stop])
                        fv=math.fsum(c['amount']*df(c['year'],spread)/fd for c in fee_cash(fb)if c['year']>year or c['year']==year and(date['timing']=='before_scheduled_cash'or c['native']))
                        payoff[0]+=weights[i]/probability*lv;payoff[1]+=weights[i]/probability*fv
                    # Compare the actual time-zero conditional alternatives.
                    # This independent oracle does not rely on the native
                    # loan-numeraire conversion used for node reporting.
                    root_payoff=ld*payoff[0]+fd*payoff[1];root_chosen=ld*chosen[0]+fd*chosen[1]
                    if action=='call'and root_payoff<root_chosen or action=='put'and root_payoff>root_chosen:chosen=tuple(payoff);decision=action
            for i in members:values[i]=chosen
            node=dict(ordinal=date['ordinal'],timing=date['timing'],information=info,probability=float(probability),continuation_value=float((ld*continuation[0]+fd*continuation[1])/ld),value=float((ld*chosen[0]+fd*chosen[1])/ld),decision=decision,live_principal=principal,legal_interest=legal)
            lookup[date['ordinal'],key]=node;nodes.append(node)
        next_values=values;next_time=(year,date['timing'])
    total=[0.,0.]
    for i,w in enumerate(weights):
        loan,fee=(next_values[i][0]*df(next_time[0],oas),next_values[i][1]*df(next_time[0],spread))if next_time else(0.,0.)
        before=lambda t:next_time is None or t<next_time[0]or t==next_time[0]and next_time[1]=='after_scheduled_cash'
        loan+=sum(((c['principal']+c['interest']+c['fees'])*df(c['year'],oas)for c in cash[i]if c['year']>0. and before(c['year'])),0.)
        fee+=sum((c['amount']*df(c['year'],spread)for c in fees[i]if c['year']>0. and before(c['year'])),0.)
        total[0]+=w*loan;total[1]+=w*fee
    result=dict(policy=deepcopy(policy),value=float(sum(total)),nodes=nodes,fixed_base_oas=float(oas),parallel_bump=float(bump),finite_distribution=True,continuous_model_convergence=False,joint_fee_values=dict(loan_value=float(total[0]),fee_value=float(total[1]),fee_discount_spread=float(spread),decision_criterion='root_total_loan_and_fee_in_loan_numeraire-1'))
    return dict(result=result,lookup=lookup,asof=asof,position_id=raws[0]['source']['current']['position_id'])


def stops_for(compiled,raw,fee_policy,check=lambda:False):
    projection=replay(raw,check);policy=compiled['result']['policy'];owner=bound(fee_policy,raw,projection,check)
    if raw['source']['current']['asof_ordinal']!=compiled['asof']or raw['source']['current']['position_id']!=compiled['position_id']:raise ArithmeticError('joint Physical source identity differs')
    for date in policy['exercise_dates']:
        year=(date['ordinal']-compiled['asof'])/365.;state,principal,_=_state(raw,projection,year,date['timing']);rights_only=principal==0. and _surviving_rights(raw,projection,date['ordinal'],date['timing'])
        if state not in('performing','watch','delinquent')or principal==0. and not rights_only:continue
        info=information(raw,projection,fee_policy,date['ordinal'],date['timing'],check,owner)
        node=compiled['lookup'].get((date['ordinal'],_key(info)))
        if node is None:raise ArithmeticError('Physical loan/fee observable state absent from Pricing policy')
        if node['decision']!='continue':return [make_stop(policy,raw,date,node['decision'],rights_only)]
    return []
