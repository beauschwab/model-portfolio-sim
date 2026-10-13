"""Independent acquired cash ownership and conditional tranche purchase quotes."""
from copy import deepcopy
import math
from .commercial_credit_reference import STATES
from .commercial_source_reference import project,compare
from .commercial_dto import typed


def partition(raw,projection,stops,check=lambda:False):
    paths=projection['credit']['result']['paths']
    if len(paths)!=1:raise ArithmeticError('one complete acquired Physical path required')
    path=paths[0];terms=raw.get('performing_draw_tranches',[]);n=len(terms);result=dict(base=[],tranches=[[]for _ in terms],acquired=[False]*n);principal=[0.]*n;default_principal=[0.]*n
    offsets=[];audit=len(project(raw['source']['current'],check)[0]['audit'])
    for d in terms:
        count=len(project(d['source']['current'],check)[0]['audit']);offsets.append((audit,audit+count));audit+=count
    by_id={d['review']['id']:i for i,d in enumerate(terms)};prior=None;source_index=recovery_index=0
    def push(rows,year,amount):
        if amount!=0.:rows.append(dict(year=year,amount=float(amount)))
    for point in projection['carry_points']:
        check();kind=point['kind'];year=point['year']
        if kind=='funded_draw':
            new=next((t for t in point.get('draw_tranches',[])if not result['acquired'][by_id[t['tranche_id']]]),None)
            if new is None:push(result['base'],year,point['cash'])
            else:i=by_id[new['tranche_id']];result['acquired'][i]=True;principal[i]=new['nominal_draw']
        elif kind=='contractual_cash':
            paid=projection['native_source_cash'][source_index]['cash'];source_index+=1
            if paid.get('tranche_id')is None:push(result['base'],year,point['cash'])
            else:i=by_id[paid['tranche_id']];principal[i]-=paid['principal'];push(result['tranches'][i],year,point['cash'])
        elif kind=='full_live_exit':
            stop=next((s for s in stops if s['path_key']==path['path_key']and(s['ordinal']-raw['source']['current']['asof_ordinal'])/365.==year),None)
            factor=stop['principal_clean_price_factor']if stop else 1.+raw.get('prepayment_terms',{}).get('principal_premium_fraction',0.)if any(e['year']==year and e['kind']=='prepayment'for e in path['events'])else 1.
            drawn=0.
            for i in range(n):
                if not result['acquired'][i]:continue
                legal=sum((c['legal_remaining']for c in prior.get('canonical_legal_coupons',[])if offsets[i][0]<=c['native_audit_index']<offsets[i][1]),0.);cash=principal[i]*factor+legal;push(result['tranches'][i],year,cash);drawn+=cash;principal[i]=0.
            push(result['base'],year,point['cash']-drawn)
        elif kind=='default_gross_transfer':default_principal=principal.copy()
        elif kind=='default_actual_cash':
            default=path['default'];recovery=default['recoveries'][recovery_index];origin=STATES.index(default['origin']);interval=next(t for t in projection['prepared']['raw_credit_path']['intervals']if default['year']<=t['end_year']);weight=interval['complete_recovery_tails'][origin][recovery_index]['weight'];drawn=0.
            for i in range(n):
                if not result['acquired'][i]:continue
                interest=sum((r['interest']for r in projection['native_coupon_recoveries']if r['recovery_index']==recovery_index and offsets[i][0]<=r['native_audit_index']<offsets[i][1]),0.);cash=default_principal[i]*(1.-interval['loss_given_default'][origin])*weight+interest;push(result['tranches'][i],year,cash);drawn+=cash
            push(result['base'],year,recovery['principal']+recovery['interest']-drawn);recovery_index+=1
        prior=point
    return result


def conditional(histories,weights,parts,check=lambda:False):
    terms=histories[0]['source'].get('performing_draw_tranches',[]);result=[];base_asof=histories[0]['source']['source']['current']['asof_ordinal']
    for i,d in enumerate(terms):
        check();a=typed(d['source']['acquisition']);probability=sum((w for w,p in zip(weights,parts,strict=True)if p['acquired'][i]),0.)
        if probability<=0.:raise ArithmeticError('declared original tranche lacks activation evidence')
        draw_year=(d['draw_ordinal']-base_asof)/365.;quote=0.
        for h,w,p in zip(histories,weights,parts,strict=True):
            other=h['source'].get('performing_draw_tranches',[])
            if len(other)!=len(terms):raise ArithmeticError('original conditional tranche coverage differs')
            current=other[i]
            if any(typed(current[k])!=typed(d[k])for k in('review','draw_ordinal','eligible_origins'))or current['source']['original']['opening_principal']!=d['source']['original']['opening_principal']or typed(current['source']['acquisition'])!=a:raise ArithmeticError('original conditional tranche acquisition anchors differ')
            quote+=w*sum((r['amount']*math.pow(1.+a['original_annual_effective_rate'],-(r['year']-draw_year))for r in p['tranches'][i]),0.)
        quote/=probability;compare(float(a['cost']),float(quote),'original conditional tranche quote',check)
        result.append(dict(tranche_id=d['review']['id'],acquisition=deepcopy(a),draw_ordinal=d['draw_ordinal'],activation_probability=probability,conditional_cash_quote=quote,original_annual_effective_rate=a['original_annual_effective_rate'],allocation_policy='native_source_principal_coupon_ownership_base_exit_fee-1'))
    return result
