"""Independent actual credit-clock and original-yield commercial replay.

The oracle starts at raw sources and CRN. Producer monetary checkpoints,
entitlements, cash applications and recognized-interest books are never inputs.
"""
from copy import deepcopy
import math
from .credit_path_reference import _pair, STATES
from .commercial_source_reference import project, factor, compare, validate_native_availability

from .commercial_dto import typed


def full_end(raw):
    start=raw['source']['current']['asof_ordinal']
    contractual=(max([raw['source']['current']['tail_end_ordinal']]+[d['source']['current']['tail_end_ordinal'] for d in raw.get('performing_draw_tranches',[])])-start)/365.
    lag=max((r['lag_years'] for i in raw['credit']['intervals'] for tail in i['complete_recovery_tails'] for r in tail),default=0.)
    end=contractual+lag
    actions=raw.get('reviewed_credit_actions') or {}
    dates=[a['review']['recognition_ordinal'] for field in ('allowance_reviews','chargeoffs') for a in actions.get(field,[])]+[a['recognition_ordinal'] for a in actions.get('legal_waivers',[])]
    for date in dates:
        if type(date)is not int or date<start:raise ArithmeticError('reviewed action precedes native acquisition or has invalid date')
        end=max(end,(date-start)/365.)
    return end


def prepared(raw,stops=(),check=lambda:False):
    """Reconstruct native compiler's daily split and explicit accounting policy."""
    for owner in [raw['source']]+[draw['source']for draw in raw.get('performing_draw_tranches',[])]:
        for role in ('original','current'):validate_native_availability(owner[role],check)
    source=raw['source']['current'];acq=raw['source']['acquisition'];credit=raw['credit']
    contract_end=(max([source['tail_end_ordinal']]+[d['source']['current']['tail_end_ordinal'] for d in raw.get('performing_draw_tranches',[])])-source['asof_ordinal'])/365.;end=full_end(raw)
    cuts=sorted(set([i['end_year'] for i in credit['intervals']]+[d/365. for d in range(1,math.ceil(end*365.)+1) if d/365.<end]+[end]))
    terms=[];reviews=[];recognition=[]
    for index,cut in enumerate(cuts):
        original=next((j for j,i in enumerate(credit['intervals']) if i['end_year']>=cut),len(credit['intervals'])-1)
        i=credit['intervals'][original];term={k:deepcopy(i[k]) for k in ('end_year','hazards_per_year','loss_given_default','credit_conversion_factor','interest_recovery_fraction','complete_recovery_tails')}
        term['end_year']=cut;term['accrual_rates']=[0.]*3
        if cut>contract_end:term['hazards_per_year']=[[0.]*5 for _ in range(3)]
        terms.append(term)
        review=deepcopy(raw['accounting_rates'][original]);review['interval_index']=index
        mode=deepcopy(raw['accounting']['recognition'][original]);mode['interval_index']=index
        if cut>contract_end:review['annual_effective_rates']=[0.]*3;mode['by_origin']=['legal_only_income_stopped']*3
        reviews.append(review);recognition.append(mode)
    boundaries=[dict(event_id=p['period_id'],policy_id=raw['accounting']['identity']['id'],year=(p['payment_ordinal']-source['asof_ordinal'])/365.,
        action=dict(kind='repayment',principal_fraction=[0.]*3,interest_fraction=[0.]*3,cancel_undrawn_fraction=[0.]*3)) for p in source['periods']]
    for draw in raw.get('performing_draw_tranches',[]):
        boundaries.append(dict(event_id='native-draw:'+draw['review']['id'],policy_id=draw['review']['id'],year=(draw['draw_ordinal']-source['asof_ordinal'])/365.,action=dict(kind='draw',undrawn_fraction=[0.]*3)))
        boundaries.extend(dict(event_id=p['period_id'],policy_id=draw['review']['id'],year=(p['payment_ordinal']-source['asof_ordinal'])/365.,action=dict(kind='repayment',principal_fraction=[0.]*3,interest_fraction=[0.]*3,cancel_undrawn_fraction=[0.]*3)) for p in draw['source']['current']['periods'])
    boundaries.extend(dict(event_id=s['event_id'],policy_id=s['review']['id'],year=(s['ordinal']-source['asof_ordinal'])/365.,action=dict(kind='maturity')) for s in stops)
    if raw.get('maturity_basis_policy'):
        maturity=raw['maturity_basis_policy'];boundaries.append(dict(event_id='native-maturity-basis:'+maturity['review']['id'],policy_id=maturity['review']['id'],year=contract_end,action=dict(kind='maturity')))
    def boundary_order(boundary):
        stop=next((s for s in stops if s['event_id']==boundary['event_id']),None)
        priority=0 if stop and stop.get('timing')=='before_scheduled_cash' else 2 if stop else 3 if boundary['action']['kind']=='maturity' else 1
        return boundary['year'],priority
    boundaries.sort(key=boundary_order)
    path=dict(schema='credit-path-1',measure='physical',monetary_unit=source['monetary_unit'],source_id=source['contract']['id'],source_revision=source['contract']['revision'],
        model_policy_id=credit['identity']['id'],origin_time=dict(ordinal=source['asof_ordinal'],seconds_of_day=0),max_jumps_per_path=credit['max_jumps_per_path'],
        intervals=terms,boundaries=boundaries,draw_stream=deepcopy(credit['draw_stream']),paths=[dict(**p,principal=float(source['opening_principal']),
        undrawn_commitment=float(credit.get('opening_undrawn_commitment',0.)),accrued_interest=0.,book_adjustment=acq['cost']-source['opening_principal']) for p in credit['paths']])
    result=dict(schema='credit-path-income-proof-1',raw_credit_path=path,original_anchors=[dict(path_key=p['path_key'],original_yield_anchor=acq['original_yield_anchor']['id'],
        acquisition_cost=float(acq['cost']),original_annual_effective_rate=float(acq['original_annual_effective_rate'])) for p in credit['paths']],recognition_rates=reviews,
        policy_review=raw['accounting']['identity']['id'],partial_payment_policy='cash_reduces_gross_no_proportional_basis_disposal-1',default_policy='stop_income_transfer_actual_gross_retain_nominal_claims-1')
    policy=deepcopy(raw['accounting']);policy['recognition']=recognition
    return result,policy


def _clock(source,year):
    days=year*365.;rounded=round(days)
    if year==rounded/365.:return source['asof_ordinal']+rounded,0.
    whole=math.floor(days);return source['asof_ordinal']+whole,days-whole


def replay(raw, check=lambda:False, *, stops=()):
    """All actual P/A/G, cash, CRN events and coupon-origin contributions."""
    p,policy=prepared(raw,stops,check);source=raw['source']['current'];contract,ss=project(source,check);path=p['raw_credit_path']
    cash=list(contract['schedule']['events']);audits=list(contract['audit']);ss=deepcopy(ss);views=[(source,None,0,len(audits),0,len(ss))]
    for draw in raw.get('performing_draw_tranches',[]):
        dsource=draw['source']['current'];projection,segments=project(dsource,check);ai=len(audits);si=len(ss)
        for segment in deepcopy(segments):segment['native_audit_index']+=ai;ss.append(segment)
        audits.extend(projection['audit']);cash.extend(projection['schedule']['events']);views.append((dsource,draw['review']['id'],ai,len(audits),si,len(ss)))
    cash_index={e['event_id']:i for i,e in enumerate(cash)}
    points=[];contributions=[];source_cash=[];recoveries=[];coupon_recoveries=[];paths=[];sojourns=[];work=0
    horizon=path['intervals'][-1]['end_year']
    for opening in path['paths']:
        check();state=STATES.index(opening['initial_live_state']);principal=opening['principal'];undrawn=opening['undrawn_commitment'];accrued=0.;gross=raw['source']['acquisition']['cost']
        out=dict(path_id=opening['path_id'],path_key=opening['path_key'],outcome=STATES[state],earned_interest=0.,earned_interest_by_origin=[0.]*3,cash_received=0.,cash_funded=0.,book_adjustment_recognized=0.,random_pairs_used=0,events=[],default=None)
        canonical=[];active=set();tranches=[]
        def owned_view(audit_index):return next(v for v in views if v[2]<=audit_index<v[3])
        def canonicalize(year):
            nonlocal accrued,canonical
            canonical=[]
            for ai,audit in enumerate(audits):
                if owned_view(ai)[1] is not None and owned_view(ai)[1] not in active:continue
                coupon=audit['period_id'];value=0.
                for segment in (s for s in ss if s['native_audit_index']==ai):
                    start=(segment['interest_start_ordinal']-source['asof_ordinal'])/365.;end=(segment['interest_end_ordinal']-source['asof_ordinal'])/365.
                    if year<=start:break
                    day,fraction=_clock(source,min(year,end));value=factor(segment,day,fraction)
                    if year<end:break
                earned=audit['opening_principal']*value
                collected=sum((r['cash']['interest'] for r in source_cash if r['path_key']==opening['path_key'] and r['cash']['coupon_id']==coupon),0.)
                canonical.append(dict(coupon_id=coupon,native_audit_index=ai,native_cash_index=cash_index[coupon],opening_entitlement_principal=audit['opening_principal'],cumulative_factor=value,contractual_earned=earned,actual_interest_collected=collected,legal_remaining=earned-collected))
            accrued=sum((r['legal_remaining'] for r in canonical),0.)
        def point(year,kind,origin=None,rate=None,eir=0.,paid=0.,basis=0.):
            value=dict(path_key=opening['path_key'],year=year,kind=kind,origin_index=origin,principal=principal,contractual_accrued_interest=accrued,
                book_adjustment=gross-principal-accrued,gross_carry=gross,accounting_rate=rate,effective_interest=eir,cash=paid,basis_realization=basis)
            if canonical:value['canonical_legal_coupons']=deepcopy(canonical)
            if tranches:value['draw_tranches']=deepcopy(tranches)
            value['undrawn_commitment']=undrawn
            points.append(value)
        point(0.,'reviewed_opening_snapshot');time=0.;interval=boundary=jumps=0;waiting=None
        while time<horizon or boundary<len(path['boundaries']) and path['boundaries'][boundary]['year']==time:
            check();work+=1
            if work>2_000_000:raise ArithmeticError('independent commercial path work exceeded')
            terms=path['intervals'][interval];end=min(terms['end_year'],path['boundaries'][boundary]['year'] if boundary<len(path['boundaries']) else horizon)
            hazard=sum(terms['hazards_per_year'][state])
            if hazard>0. and waiting is None:
                u,v=_pair(path,opening['path_key'],out['random_pairs_used']);waiting=(-math.log(u),v);out['random_pairs_used']+=1
            capacity=hazard*(end-time);jump=waiting is not None and hazard>0. and waiting[0]<=capacity
            next_time=min(time+waiting[0]/hazard,end) if jump else end
            interest=0.
            for si,s in enumerate(ss):
                view=owned_view(s['native_audit_index'])
                if view[1] is not None and view[1] not in active:continue
                check();left=max(time,(s['interest_start_ordinal']-source['asof_ordinal'])/365.);right=min(next_time,(s['interest_end_ordinal']-source['asof_ordinal'])/365.)
                if right<=left:continue
                ld,lf=_clock(source,left);rd,rf=_clock(source,right);delta=factor(s,rd,rf)-factor(s,ld,lf);ai=s['native_audit_index'];ci=cash_index[s['period_id']];entitlement=audits[ai]['opening_principal'];earned=entitlement*delta;interest+=earned
                contributions.append(dict(path_key=opening['path_key'],interval_index=interval,origin_index=state,start_year=time,end_year=next_time,coupon_id=s['period_id'],source_event_id=s['period_id'],
                    source_id=view[0]['contract']['id'],source_revision=view[0]['contract']['revision'],captured_segment_index=si,delta_factor=delta,actual_principal=entitlement,contractual_earned=earned,
                    native_entitlement=dict(native_audit_index=ai,native_cash_index=ci,opening_entitlement_principal=entitlement)))
            if next_time>time:sojourns.append(dict(path_key=opening['path_key'],start_year=time,end_year=next_time,origin=STATES[state],principal=principal,undrawn_commitment=undrawn,
                opening_accrued_interest=accrued,book_adjustment=gross-principal-accrued,accrual_rate=0.,earned_interest=interest))
            accrued+=interest;out['earned_interest']+=interest;out['earned_interest_by_origin'][state]+=interest
            annual=p['recognition_rates'][interval]['annual_effective_rates'][state];eir=(gross-sum((t['gross_carry'] for t in tranches),0.))*math.expm1(math.log1p(annual)*(next_time-time))
            for tranche in tranches:
                selected=0. if policy['recognition'][interval]['by_origin'][state]=='legal_only_income_stopped' else tranche['original_annual_effective_rate']
                tranche_source=next(d for d in raw['performing_draw_tranches'] if d['review']['id']==tranche['tranche_id'])['source']
                if policy['recognition'][interval]['by_origin'][state]!='legal_only_income_stopped' and tranche_source['policy']['reset_income']=='reviewed_prospective_at_period_start-1':
                    interval_start=path['intervals'][interval-1]['end_year'] if interval else 0.
                    date_start=source['asof_ordinal']+round(interval_start*365.)
                    applicable=[r for r in tranche_source.get('current_accounting_rate_reviews',[]) if r['reset_ordinal']<=date_start]
                    if applicable:selected=applicable[-1]['annual_effective_rate']
                earned=tranche['gross_carry']*math.expm1(math.log1p(selected)*(next_time-time));tranche['gross_carry']+=earned;eir+=earned
            gross+=eir;time=next_time
            canonicalize(time);point(time,'live_sojourn',state,annual,eir)
            if jump:
                jumps+=1
                if jumps>path['max_jumps_per_path']:raise ArithmeticError('independent commercial jump bound')
                threshold=waiting[1]*hazard;cum=0.
                for destination,rate in enumerate(terms['hazards_per_year'][state]):
                    cum+=rate
                    if threshold<cum:break
                waiting=None;event=dict(year=time,event_id=f'jump:{jumps}',policy_id=path['model_policy_id'],origin=STATES[state],destination=STATES[destination],kind='transition',cash=0.,principal=0.,interest=0.,book_adjustment_recognition=0.)
                if destination==3:
                    draw=undrawn*terms['credit_conversion_factor'][state];principal+=draw;undrawn=0.;gross+=draw;out['cash_funded']+=draw;point(time,'funded_draw',paid=-draw)
                    event.update(cash=-draw,principal=principal,interest=accrued)
                    claim=dict(year=time,origin=STATES[state],principal=principal,accrued_interest=accrued,book_adjustment=gross-principal-accrued,contingent_draw=draw,economic_principal_loss=principal*terms['loss_given_default'][state],recoveries=[])
                    # Raw earned/coupon cash build gross legal claims before recovery.
                    lots=legal_lots([c for c in contributions if c['path_key']==opening['path_key']], [c for c in source_cash if c['path_key']==opening['path_key']],out['events'])
                    for ri,r in enumerate(terms['complete_recovery_tails'][state]):
                        check();work+=1;year=time+r['lag_years'];icash=0.
                        for lot in lots:
                            interest_paid=lot['legal_remaining']*terms['interest_recovery_fraction'][state]*r['weight'];icash+=interest_paid
                            coupon_recoveries.append(dict(path_key=opening['path_key'],default_year=time,recovery_index=ri,year=year,coupon_id=lot['coupon_id'],source_event_id=lot['source_event_id'],native_audit_index=lot['native_audit_index'],earning_origin_index=lot['earning_origin_index'],interest=interest_paid,applied_in_horizon=year<=horizon))
                        claim['recoveries'].append(dict(year=year,principal=principal*(1.-terms['loss_given_default'][state])*r['weight'],interest=icash))
                    canonical=[];tranches=[];point(time,'default_gross_transfer');paidp=paida=0.
                    for ri,r in enumerate(claim['recoveries']):
                        recoveries.append(dict(path_key=opening['path_key'],default_year=time,recovery_index=ri,**r,applied_in_horizon=r['year']<=horizon))
                        if r['year']<=horizon:
                            paidp+=r['principal'];paida+=r['interest'];principal=claim['principal']-paidp;accrued=claim['accrued_interest']-paida;gross-=r['principal']+r['interest'];out['cash_received']+=r['principal']+r['interest'];point(r['year'],'default_actual_cash',paid=r['principal']+r['interest'])
                    out['default']=claim;out['outcome']='default'
                elif destination==4:
                    premium=principal*raw['prepayment_terms']['principal_premium_fraction'] if raw.get('prepayment_terms') else 0.
                    event.update(kind='prepayment',principal=principal,interest=accrued,cash=principal+accrued+premium,book_adjustment_recognition=gross-principal-accrued-premium)
                    out['cash_received']+=event['cash'];out['book_adjustment_recognized']+=event['book_adjustment_recognition'];principal=accrued=gross=undrawn=0.;canonical=[];tranches=[];out['outcome']='prepaid';point(time,'full_live_exit',paid=event['cash'],basis=event['book_adjustment_recognition'])
                else:state=destination;out['outcome']=STATES[state]
                out['events'].append(event)
                if destination>=3:break
            else:
                if waiting is not None:waiting=(waiting[0]-capacity,waiting[1])
                while boundary<len(path['boundaries']) and path['boundaries'][boundary]['year']==time:
                    check();work+=1;b=path['boundaries'][boundary]
                    if b['action']['kind']=='draw':
                        draw=next(d for d in raw['performing_draw_tranches'] if 'native-draw:'+d['review']['id']==b['event_id']);boundary+=1
                        if not draw['eligible_origins'][state]:continue
                        acquisition=draw['source']['acquisition'];nominal=draw['source']['current']['opening_principal'];cost=acquisition['cost'];active.add(draw['review']['id'])
                        tranches.append(dict(tranche_id=draw['review']['id'],original_annual_effective_rate=acquisition['original_annual_effective_rate'],nominal_draw=nominal,acquisition_cost=cost,gross_carry=cost))
                        principal+=nominal;undrawn-=nominal;gross+=cost;out['cash_funded']+=cost;canonicalize(time);point(time,'funded_draw',paid=-cost)
                        out['events'].append(dict(year=time,event_id=b['event_id'],policy_id=b['policy_id'],origin=STATES[state],destination=None,kind='draw',cash=-cost,principal=nominal,interest=0.,book_adjustment_recognition=0.));continue
                    if b['action']['kind']=='maturity':
                        stop=next((s for s in stops if s['event_id']==b['event_id']),None);boundary+=1
                        if stop is not None and stop['path_key']!=opening['path_key']:continue
                        if stop is None:
                            if not raw.get('maturity_basis_policy') or principal!=0. or accrued!=0. or any(c['legal_remaining']!=0. for c in canonical):raise ArithmeticError('reviewed maturity disposal requires exact legal close')
                            paid=0.
                        else:paid=principal*stop['principal_clean_price_factor']+accrued+stop['fee']
                        basis=gross-paid
                        out['events'].append(dict(year=time,event_id=b['event_id'],policy_id=b['policy_id'],origin=STATES[state],destination=None,kind='maturity',cash=paid,principal=principal,interest=accrued,book_adjustment_recognition=basis))
                        out['cash_received']+=paid;out['book_adjustment_recognized']+=basis;principal=accrued=gross=undrawn=0.;canonical=[];tranches=[];out['outcome']='matured';point(time,'full_live_exit',paid=paid,basis=basis);break
                    ci=cash_index[b['event_id']];e=cash[ci]['components'];fee=e['fees'];pcash=e['principal'];icash=e['cash_interest'];paid=pcash+icash+fee
                    tranche_id=next((v[1] for v in views if any(a['period_id']==b['event_id'] for a in audits[v[2]:v[3]])),None)
                    if tranche_id is not None and tranche_id not in active:boundary+=1;continue
                    receipt=dict(coupon_id=b['event_id'],source_event_id=b['event_id'],native_cash_index=ci,principal=pcash,interest=icash)
                    if fee!=0.:receipt['fee']=fee
                    if tranche_id is not None:
                        receipt['tranche_id']=tranche_id;next(t for t in tranches if t['tranche_id']==tranche_id)['gross_carry']-=paid
                    source_cash.append(dict(path_key=opening['path_key'],year=time,cash=receipt))
                    principal-=pcash;accrued-=icash;gross-=paid;out['cash_received']+=paid;canonicalize(time);point(time,'contractual_cash',paid=paid)
                    out['events'].append(dict(year=time,event_id=b['event_id'],policy_id=b['policy_id'],origin=STATES[state],destination=None,kind='repayment',cash=paid,principal=pcash,interest=icash,book_adjustment_recognition=0.));boundary+=1
                if out['outcome']=='matured':break
                if time==terms['end_year'] and interval+1<len(path['intervals']):interval+=1
                if time==horizon:break
        out.update(closing_principal=principal,closing_undrawn_commitment=undrawn,closing_accrued_interest=accrued,closing_book_adjustment=gross-principal-accrued);point(horizon,'closing_claims_retained');paths.append(out)
    stream=path['draw_stream'];result=dict(schema='credit-path-1',measure='physical',monetary_unit=path['monetary_unit'],source_id=path['source_id'],source_revision=path['source_revision'],model_policy_id=path['model_policy_id'],origin_time=path['origin_time'],clock='act_365_elapsed_seconds',stream_id=stream['stream_id'],stream_revision=stream['stream_revision'],work_units=work,paths=paths)
    return dict(reviewed_credit_actions=deepcopy(raw.get('reviewed_credit_actions')),asof_ordinal=source['asof_ordinal'],prepared=p,policy=policy,credit=dict(result=result,sojourns=sojourns),carry_points=points,earned_origin_contributions=contributions,complete_recovery_receipts=recoveries,native_source_cash=source_cash,native_coupon_recoveries=coupon_recoveries)


def sync(lot):
    lot['legal_remaining']=lot['contractual_earned']-lot['legal_cash_collected'];lot['recognized_remaining']=lot['recognized_earned']-lot['recognized_cash_applied']


def add_earned(lots,c,recognized):
    lot=next((l for l in lots if l['coupon_id']==c['coupon_id'] and l['earning_origin_index']==c['origin_index']),None)
    if lot is None:
        lot=dict(coupon_id=c['coupon_id'],source_event_id=c['source_event_id'],source_id=c['source_id'],source_revision=c['source_revision'],native_audit_index=c['native_entitlement']['native_audit_index'],native_cash_index=c['native_entitlement']['native_cash_index'],earning_origin_index=c['origin_index'],contractual_earned=0.,legal_cash_collected=0.,legal_remaining=0.,recognized_earned=0.,recognized_cash_applied=0.,recognized_remaining=0.,default_claim=False);lots.append(lot)
    lot['contractual_earned']+=c['contractual_earned']
    if recognized:lot['recognized_earned']+=c['contractual_earned']
    sync(lot)


def apply_cash(lots,c,origin):
    remaining=c['cash']['interest'];selected=[l for l in lots if l['coupon_id']==c['cash']['coupon_id']]
    for lot in selected:
        right=lot['legal_remaining'];used=math.copysign(min(abs(right),abs(remaining)),remaining) if math.copysign(1.,right)==math.copysign(1.,remaining) else 0.
        lot['legal_cash_collected']+=used;lot['recognized_cash_applied']+=used;remaining-=used;sync(lot)
    if remaining!=0.:
        lot=next(l for l in selected if l['earning_origin_index']==origin);lot['legal_cash_collected']+=remaining;lot['recognized_cash_applied']+=remaining;sync(lot)


def legal_lots(contributions,cash,events):
    lots=[];ci=0
    for paid in cash:
        while ci<len(contributions) and contributions[ci]['end_year']<=paid['year']:add_earned(lots,contributions[ci],False);ci+=1
        event=next(e for e in events if e['event_id']==paid['cash']['source_event_id']);apply_cash(lots,paid,STATES.index(event['origin']))
    for c in contributions[ci:]:add_earned(lots,c,False)
    return lots
