"""Independent supplied deposit runoff source verifier; never an execution backend.

Gross earned expense and bank cash settlement are reconstructed separately.

"""

from copy import deepcopy

import math

from ._deposit_continuation_income_reference import equal



def checkpoint(check):

    if check():raise InterruptedError('deposit source verification cancelled')



def project(raw,check=lambda:False,receipts=None):
    """Reviewed physical dated-ledger boundary; no signed-rate accounting inferred."""
    if any(s['opening_paid_rate']<0. or s['repricing']['floor']<0. for s in raw['segments']):raise ArithmeticError('deposit negative-rate accounting unsupported')
    return project_declared(raw,measure='physical',check=check,receipts=receipts)

def project_declared(raw,*,measure,check=lambda:False,receipts=None):
    """Independent complete supplied history for analysis/product-unit verification.

    The measure is a required declaration, never converted from P to Q. Signed
    earned interest is supported here; the ledger wrapper retains its own guard.
    ACT/ACT, migrations, external inflows and opening accrued remain unsupported.
    """
    checkpoint(check)
    if measure not in {'physical','pricing'} or raw['schema']!='deposit-state-1' or raw['identity']['measure']!=measure:raise ArithmeticError('deposit declared source measure required')
    if raw['migrations'] or any(any(v!=0. for v in d['external_inflows']) for d in raw['drivers']):raise ArithmeticError('deposit migration/inflow ownership unsupported')

    segments=raw['segments'];n=len(segments);asof=raw['as_of_ordinal'];end=asof+raw['horizon_days']

    if raw['day_count'] not in {'act360','act365_fixed'}:raise ArithmeticError('deposit actual accrual convention unsupported')

    divisor=360. if raw['day_count']=='act360' else 365.

    if any(s['opening_accrued_interest']!=0. for s in segments):raise ArithmeticError('deposit opening rights unsupported')
    if raw['terminal']['kind'] not in {'retain_residual','assumed_liquidation'}:raise ArithmeticError('deposit terminal value unsupported')

    observations={d['ordinal']:d for d in raw['drivers']}

    if len(observations)!=len(raw['drivers']):raise ArithmeticError('deposit duplicate observation date')

    balance=[s['opening_balance'] for s in segments];accrued=[0.]*n;rates=[s['opening_paid_rate'] for s in segments]

    cash=[];interest=[];points=[];withdrawals=closures=maturities=terminal=0.

    def point(day,outside=None,assessed=None):return dict(ordinal=day,balances=balance.copy(),accrued_interest=accrued.copy(),paid_rates=rates.copy(),outside_supported_rate_regime=outside or [False]*n,rate_regime_assessed=assessed or [False]*n)

    def pay(day,i,kind,p,c):

        if p!=0. or c!=0.:cash.append(dict(event_id=f"{segments[i]['segment_id']}-{kind}-{day}",ordinal=day,segment_id=segments[i]['segment_id'],kind=kind,principal=p,cash_interest=c))

    def atom(day,i,kind,before,after,cash_start,exited=None,w=None,c=None):
        if receipts is not None:receipts.append(dict(allocation_index=len(receipts),ordinal=day,segment_id=segments[i]['segment_id'],kind=kind,principal_before=before,principal_after=after,exited=exited,withdrawal_intensity=w,closure_intensity=c,cash_event_ids=[x['event_id'] for x in cash[cash_start:]]))
    points.append(point(asof))

    for day in range(asof,end):

        checkpoint(check);driver=observations[day];active=[s['maturity_ordinal'] is None or s['maturity_ordinal']>day for s in segments];outside=[False]*n;assessed=[False]*n

        for i,s in enumerate(segments):

            if s['maturity_ordinal']==day:

                before=balance[i];cs=len(cash);maturities+=balance[i];pay(day,i,'contractual_maturity',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.;atom(day,i,'contractual_maturity',before,balance[i],cs)

            policy=s['repricing'];observed=observations[day-policy['observation_lag_days']];reference=observed['reference_rate'];competitor=observed['competitor_rates'][i];rule=policy['target']

            if rule['kind']=='linear':beta=rule['beta']

            elif rule['kind']=='logistic_beta':

                x=rule['steepness']*(reference-rule['midpoint']);sigmoid=1./(1.+math.exp(-x)) if x>=0. else math.exp(x)/(1.+math.exp(x));beta=rule['beta_min']+(rule['beta_max']-rule['beta_min'])*sigmoid

            else:raise ArithmeticError('deposit target rule unsupported')

            desired=min(max(rule['intercept']+beta*reference+rule['competitor_weight']*(competitor-reference)+observed['offered_spread_stress'][i],policy['floor']),policy['ceiling'])

            half=policy['rising_half_life_days'] if desired>=rates[i] else policy['falling_half_life_days'];adjustment=1. if half==0. else -math.expm1(-math.log(2.)/half)

            rates[i]=min(max(rates[i]+adjustment*(desired-rates[i]),policy['floor']),policy['ceiling'])

            regime=policy['supported_observation_range']

            if regime is not None:assessed[i]=True;outside[i]=not regime['minimum_reference_rate']<=reference<=regime['maximum_reference_rate']

        new_balance=[0.]*n;new_accrued=[0.]*n

        for i,s in enumerate(segments):

            if not active[i]:continue

            p=s['retention'];c=s['characteristics'];gap=max(driver['competitor_rates'][i]-rates[i],0.);velocity=abs(driver['reference_rate']-observations[day-1]['reference_rate'])

            exposure=p['uninsured_stress_sensitivity']*c['uninsured_fraction']+p['concentration_stress_sensitivity']*c['concentration']+p['digital_stress_sensitivity']*c['digital_fraction']-p['relationship_protection']*c['relationship_fraction']-p['operational_protection']*c['operational_fraction']

            withdrawal=max(p['withdrawal_per_year']+p['opportunity_gap_sensitivity']*gap+p['absolute_daily_velocity_sensitivity']*velocity+driver['confidence_stress'][i]*exposure,0.)

            closure=p['closure_per_year']+p['closure_stress_sensitivity']*driver['confidence_stress'][i];total=withdrawal+closure

            if not math.isfinite(total) or not 0.<=total<=100.:raise ArithmeticError('deposit competing intensity outside bounds')

            exited=-math.expm1(-total/365.) if total else 0.;share=exited/total if total else 0.

            wd=balance[i]*withdrawal*share;cl=balance[i]*closure*share;wi=accrued[i]*withdrawal*share;ci=accrued[i]*closure*share

            cs=len(cash);withdrawals+=wd;closures+=cl;pay(day,i,'withdrawal',-wd,-wi);pay(day,i,'closure',-cl,-ci)

            new_balance[i]+=balance[i]*(1.-exited);new_accrued[i]+=accrued[i]*(1.-exited);atom(day,i,'competing_withdrawal_closure',balance[i],new_balance[i],cs,exited,withdrawal,closure)

        for i,s in enumerate(segments):

            earned=new_balance[i]*rates[i]/divisor;new_accrued[i]+=earned

            if earned:interest.append(dict(start_ordinal=day,end_ordinal=day+1,segment_id=s['segment_id'],amount=earned))

            if day+1 in s['interest_payment_ordinals']:pay(day+1,i,'interest_payment',0.,-new_accrued[i]);new_accrued[i]=0.

        balance=new_balance;accrued=new_accrued;points.append(point(day+1,outside,assessed))

    for i,s in enumerate(segments):

        if s['maturity_ordinal']==end:before=balance[i];cs=len(cash);maturities+=balance[i];pay(end,i,'contractual_maturity',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.;atom(end,i,'contractual_maturity',before,balance[i],cs)

    if raw['terminal']['kind']=='assumed_liquidation':

        for i in range(n):terminal+=balance[i];pay(end,i,'assumed_terminal_payout',-balance[i],-accrued[i]);balance[i]=accrued[i]=0.

    if any(v!=0. for v in balance+accrued):raise ArithmeticError('deposit source requires complete zero tail')

    points[-1]['balances']=balance.copy();points[-1]['accrued_interest']=accrued.copy();cash.sort(key=lambda e:e['ordinal'])

    opening=sum(s['opening_balance'] for s in segments);expense=sum(v['amount'] for v in interest);paid=sum(v['cash_interest'] for v in cash)

    equal(opening,withdrawals+closures+maturities+terminal,'deposit/principal_conservation');equal(expense+paid,0.,'deposit/interest_conservation')

    return dict(schema='deposit-state-1',identity=deepcopy(raw['identity']),parameter_status='supplied_assumption',segment_ids=[s['segment_id'] for s in segments],points=points,cashflows=cash,migrations=[],interest=interest,total_external_inflows=0.,total_withdrawals=withdrawals,total_closures=closures,total_contractual_maturities=maturities,total_assumed_terminal_payout=terminal,closing_balance=sum(balance),closing_accrued_interest=sum(accrued),total_interest_expense=expense,total_bank_cash_interest=paid,supplied_franchise_value=None)


