"""Independent source projector with the unchanged Core monetary domain.

Derived from the frozen independently implemented raw mortgage projector. The
old interval proof stays frozen; this separate function admits large amounts
using the actual Core scale-aware conservation bound, without tolerance changes
in complete financial comparisons or clipping/normalizing recovery cash.
"""
from copy import deepcopy
import math
import numpy as np
from .mortgage_credit_math_reference import (typed_contract,validate_contract,checkpoint,finite,
    ordinal,tau,boundaries,hazard_generator,coefficients,SOURCE_SHA,ORDER)

def project(c, check=lambda:False):
    """Raw path generator, including interval-end approximation and full cash tails."""
    c=typed_contract(c);validate_contract(c); checkpoint(check)
    start=c['as_of_ordinal']; end=start+c['horizon_days']; owner=c['ownership']; agency=owner['kind']=='agency'
    prob=np.array(c['opening_probability']+[0.]*4); b=np.array(c['opening_borrower_principal'],float); ba=np.array(c['opening_borrower_interest'],float)
    p=np.array(c['opening_investor_principal'],float); a=np.array(c['opening_investor_interest'],float)
    note=c['current_note_rate']; payment=c['current_payment']; contractual=c['opening_contractual_principal']; ci=c['opening_contractual_interest']; burnout=c['opening_burnout_incentive_years']; matured=0.; previous=start
    reset_count=0 if c['rate']['kind']=='fixed' else c['rate']['terms']['prior_reset_count']
    resets={} if c['rate']['kind']=='fixed' else {r['ordinal']:r for r in c['rate']['resets']}
    out={k:deepcopy(c[k]) for k in ['schema','identity','ownership','monetary_unit','original_principal','original_payment','origination_ordinal']}
    out.update({k:[] for k in ['borrower_cashflows','cashflows','entitlements','recognition','points','defaults','resets']})
    totals=['borrower_scheduled_principal','borrower_turnover_principal','borrower_refinance_principal','investor_scheduled_principal','investor_turnover_principal','investor_refinance_principal','guarantee_principal_advance','guarantee_interest_advance']
    out.update({k:0. for k in totals}); out.update(calendar_mapping_verified=False,work_units=0)
    intervals=[]
    def cash(rows,day,kind,principal,interest):
        for v in [principal,interest,principal+interest]: finite(v)
        ordinal(day)
        if principal or interest: rows.append(dict(event_id=f'{kind}-{len(rows)}',ordinal=day,kind=kind,principal=float(principal),interest=float(interest),fees=0.))
    def entitlement(day,kind,principal,interest):
        ordinal(day);ordinal(day+owner['remittance_lag_days'])
        for v in [principal,interest,principal+interest]: finite(v)
        rows=out['entitlements']
        if principal or interest: rows.append(dict(event_id=f'{kind}-{len(rows)}',entitlement_ordinal=day,remittance_ordinal=day+owner['remittance_lag_days'],kind=kind,principal=float(principal),interest=float(interest)))
    def recognition(prefix,left,right,value):
        if value: out['recognition'].append(dict(origin_id=f'{prefix}-{len(out["recognition"])}',start_ordinal=left,end_ordinal=right,interest=float(value)))
    def buyout(day,principal,interest,coupon):
        if not agency:return None
        final=day+owner['default_buyout_lag_days'];ordinal(final);since=day
        if day in c['payment_dates'] and final>day:
            entitlement(day,'agency_default_guaranteed_coupon',0.,interest);interest=0.
        for due in [d for d in c['payment_dates'] if day<d<final]:
            out['work_units']+=1; interest+=principal*coupon*tau(since,due,c['day_count'])
            entitlement(due,'agency_default_guaranteed_coupon',0.,interest)
            if since<end:recognition('agency-default-earned',since,min(due,end),principal*coupon*tau(since,min(due,end),c['day_count']))
            interest=0.;since=due
        interest+=principal*coupon*tau(since,final,c['day_count'])
        if since<end and final>since:recognition('agency-default-earned',since,min(final,end),principal*coupon*tau(since,min(final,end),c['day_count']))
        entitlement(final,'agency_default_buyout',principal,interest)
        return final
    for day in boundaries(c):
        checkpoint(check); out['work_units']+=2+len(out['defaults'])
        path=max((x for x in c['path'] if x['start_ordinal']<=previous),key=lambda x:x['start_ordinal'])
        years=(day-previous)/365.; fraction=tau(previous,day,c['day_count']); ci+=contractual*note*fraction;finite(ci)
        age=previous-c['origination_ordinal'];ref=c['refinance'];incentive=max(note-path['refinancing_market_rate']-ref['incentive_threshold'],0.)
        ramp=lambda n: min(age/n,1.) if n else 1.
        turnover=path['turnover_hazard_per_year']*ramp(ref['turnover_seasoning_days'])
        refinancing=ref['incentive_slope']*incentive*ramp(ref['refinance_seasoning_days'])*math.exp(-ref['burnout_sensitivity']*burnout)
        finite(refinancing,0.,100.)
        h=deepcopy(path['credit_hazards_per_year'])
        for i in range(3):h[i][4]=(turnover+refinancing)*ref['prepay_eligibility'][i]
        q=hazard_generator(h);coupon=note-(owner['servicing_and_guarantee_spread'] if agency else 0.);finite(coupon,0.,2.)
        factors=[1.]*3 if agency else owner['recognized_accrual_fraction']
        lr=[note*fraction/years]*3;ir=[coupon*f*fraction/years for f in factors]
        t,lf=coefficients(q,lr,years);it,iff=coefficients(q,ir,years)
        live=np.array(list(prob[:3])+[0.]*4)
        op=lambda rates:dict(schema='credit-operator-1',measure=c['identity']['measure'],duration_years=years,transition_hazards_per_year=h,accrual_rates=rates)
        intervals.append(dict(schema='mortgage-loop-interval-1',financial_source_sha256=SOURCE_SHA,identity=deepcopy(c['identity']),ownership=deepcopy(owner),interval_index=len(intervals),start_ordinal=previous,end_ordinal=day,path_start_ordinal=path['start_ordinal'],duration_years_act365f=years,contractual_day_fraction=fraction,note_rate_before_boundary=note,investor_coupon_before_boundary=coupon,payment_before_boundary=payment,reset_count_before_boundary=reset_count,burnout_before_interval=burnout,loan_operator_request=op(lr),investor_operator_request=op(ir),loan_transition=t.tolist(),loan_earned_interest=lf.tolist(),investor_transition=it.tolist(),investor_earned_interest=iff.tolist(),opening_probability=live.tolist(),opening_borrower_principal=b.tolist(),opening_borrower_interest=ba.tolist(),opening_investor_principal=p.tolist(),opening_investor_interest=a.tolist(),default_claim_start_index=len(out['defaults']),payment_is_boundary=day in c['payment_dates'],arm_reset_is_boundary=day in resets,event_order=ORDER))
        bp=np.r_[b,[0.]*4]; bi=np.r_[ba,[0.]*4]; ip=np.r_[p,[0.]*4]; ia=np.r_[a,[0.]*4]
        lp=bp@t;la=bi@t+bp@lf;owned=ip@it;oi=ia@it+ip@iff
        recognition('mortgage-earned',previous,day,float(sum(ip@iff)))
        newprob=live@t; prob[:3]=newprob[:3];prob[3:]+=newprob[3:]
        b=lp[:3].copy();ba=la[:3].copy();p=owned[:3].copy();a=oi[:3].copy()
        for i in range(3):
            j=i+3
            if lp[j]==la[j]==owned[j]==oi[j]==0.:continue
            gross=lp[j];loss=gross*c['loss_given_default'][i];tail=[];out['work_units']+=len(c['complete_recovery_tails'][i])+1
            for w in c['complete_recovery_tails'][i]:
                due=day+w['lag_days'];amount=(gross-loss)*w['weight'];tail.append(dict(ordinal=due,principal=float(amount)))
                cash(out['borrower_cashflows'],due,'borrower_default_recovery',amount,0.)
                if not agency:entitlement(due,'whole_loan_default_recovery',amount,0.)
            bought=buyout(day,owned[j],oi[j],coupon)
            out['defaults'].append(dict(claim_id=f'mortgage-default-{len(out["defaults"])}',origin=i,default_ordinal=day,borrower_principal=float(gross),borrower_contract_interest=float(la[j]),investor_principal=float(owned[j]),investor_accrued_interest=float(oi[j]),borrower_principal_loss=float(loss),complete_recovery_tail=tail,investor_buyout_ordinal=bought))
        if turnover+refinancing>0.:
            for kind,part in [('turnover',turnover/(turnover+refinancing)),('refinance',refinancing/(turnover+refinancing))]:
                lb=lp[6]*part;li=la[6]*part;ib=owned[6]*part;ii=oi[6]*part if agency else li
                cash(out['borrower_cashflows'],day,kind,lb,li);entitlement(day,kind,ib,ii)
                if not agency:recognition('nonaccrual-payoff',day,day,li-oi[6]*part)
                out[f'borrower_{kind}_principal']+=float(lb);out[f'investor_{kind}_principal']+=float(ib)
        burnout+=incentive*years;finite(burnout,0.,200.)
        if day in c['payment_dates']:
            if payment+1e-9<ci:raise ArithmeticError('unsupported negative amortization')
            final=day==c['payment_dates'][-1];scheduled=contractual if final else min(payment-ci,contractual);contractual-=scheduled
            for i in range(3):
                available=(b[i]+ba[i])*c['borrower_collection_fraction'][i] if final else payment*prob[i]*c['borrower_collection_fraction'][i]
                actual_i=min(available,ba[i]);actual_p=min(available-actual_i,b[i]);b[i]-=actual_p;ba[i]-=actual_i
                cash(out['borrower_cashflows'],day,'scheduled_borrower_collection',actual_p,actual_i)
                owner_p=(p[i] if final else min(scheduled*prob[i],p[i])) if agency else actual_p;owner_i=a[i] if agency else actual_i
                if agency:out['guarantee_principal_advance']+=float(max(owner_p-actual_p,0.));out['guarantee_interest_advance']+=float(max(a[i]-actual_i,0.))
                recognized=min(owner_i,a[i]);p[i]-=owner_p;a[i]-=recognized
                entitlement(day,'scheduled_investor_entitlement',owner_p,owner_i)
                if not agency and owner_i>recognized:recognition('nonaccrual-collection',day,day,owner_i-recognized)
                out['borrower_scheduled_principal']+=float(actual_p);out['investor_scheduled_principal']+=float(owner_p)
                if final and b[i]==ba[i]==p[i]==a[i]==0.:matured+=float(prob[i]);prob[i]=0.
            ci=0.
        if day in resets:
            reset=resets[day];terms=c['rate']['terms'];fix=reset['input']['fixing'];old=note
            up=terms['initial_up_cap'] if not reset_count else terms['subsequent_up_cap'];down=terms['initial_down_cap'] if not reset_count else terms['subsequent_down_cap']
            lo=max(terms['original_note_rate']-terms['lifetime_down_cap'],terms['note_floor'],note-down);hi=min(terms['original_note_rate']+terms['lifetime_up_cap'],note+up)
            if lo>hi:raise ArithmeticError('nonintersecting reset caps')
            note=min(max(fix['rate']+terms['margin'],lo),hi);finite(note-(owner['servicing_and_guarantee_spread'] if agency else 0.),0.,2.);reset_count+=1
            if terms['payment_recast']=='retain_original':payment=c['original_payment']
            elif contractual>0.:
                remaining=[d for d in c['payment_dates'] if d>day]
                if not remaining:raise ArithmeticError('complete recast tail required')
                dates=[day]+remaining;fractions=[tau(l,r,c['day_count']) for l,r in zip(dates,dates[1:])]
                discounts=np.cumprod([1/(1+note*f) for f in fractions])
                payment=(contractual+ci/(1+note*fractions[0]))/float(sum(discounts))
            out['resets'].append(dict(ordinal=day,previous_rate=old,selected_index_rate=fix['rate'],new_rate=note,new_payment=payment,fixing_id=fix['fixing_id'],fixing_revision=fix['revision'],projected=reset['input']['kind']=='projected'))
        borrower_default=sum((d['borrower_principal']-sum((w['principal'] for w in d['complete_recovery_tail'] if w['ordinal']<=day),0.) for d in out['defaults']),0.)
        investor_default=sum((d['investor_principal'] for d in out['defaults'] if d['investor_buyout_ordinal']>day),0.) if agency else borrower_default
        for v in list(b)+list(ba)+list(p)+list(a):finite(float(v))
        out['points'].append(dict(ordinal=day,seasoning_days=day-c['origination_ordinal'],burnout_incentive_years=burnout,note_rate=note,current_payment=payment,contractual_principal=contractual,probability=prob.tolist(),matured_probability=matured,borrower_principal=b.tolist(),borrower_interest=ba.tolist(),investor_principal=p.tolist(),investor_interest=a.tolist(),turnover_hazard_per_year=turnover,refinance_hazard_per_year=refinancing,borrower_default_principal=borrower_default,investor_default_principal=investor_default));previous=day
    out['borrower_cashflows'].sort(key=lambda r:(r['ordinal'],r['event_id']));out['entitlements'].sort(key=lambda r:(r['entitlement_ordinal'],r['event_id']))
    for e in out['entitlements']:cash(out['cashflows'],e['remittance_ordinal'],e['kind'],e['principal'],e['interest'])
    out['cashflows'].sort(key=lambda r:(r['ordinal'],r['event_id']))
    out['borrower_principal_conservation_error']=sum(c['opening_borrower_principal'])-sum(e['principal'] for e in out['borrower_cashflows'] if e['ordinal']<=end)-sum(b)-borrower_default
    out['investor_principal_conservation_error']=sum(c['opening_investor_principal'])-sum(e['principal'] for e in out['entitlements'] if e['entitlement_ordinal']<=end)-sum(p)-investor_default
    for k in ['borrower_principal_conservation_error','investor_principal_conservation_error']:
        opening=sum(c['opening_borrower_principal' if k.startswith('borrower') else 'opening_investor_principal'])
        if abs(out[k])>max(1e-8,256.*np.finfo(float).eps*abs(opening)):raise ArithmeticError('principal conservation')
    if out['work_units']>2_000_000:raise ArithmeticError('source work bound')
    return dict(raw_request=deepcopy(c),native_projection=out,intervals=intervals,production_adoption=False,installed_independent_verification=False)
