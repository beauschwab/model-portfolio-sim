"""Independent raw-source verification of one native mixed tagged installment/card GL.

This pure Python oracle is a publication gate, never a production financial
backend. All raw sources and reviewed policy are retained and reconstructed.
"""
from copy import deepcopy
import json
import math

from portfolio_risk.analytics import product_mixed_reference as dated

from portfolio_risk.analytics import consumer_credit_reference as credit
from portfolio_risk.analytics import card_credit_reference as card
from portfolio_risk.analytics import consumer_mixed_reference as shared

SCHEMA='product-mixed-product-credit-1'
PHASE='opening_acquisition_dated_recognition_consumer_events_dated_settlement_close-1'
POLICY={'ownership':'consumer_product_event_intents-1','phase':PHASE,'tail':'complete_closed_claims-1',
        'cash':'end_of_day_no_replacement_funding-1','initial_allowance':'explicit_reviewed_non_pcd_expense-1',
        'generic_cashflows':'reject','generic_credit':'reject','generic_marks':'reject'}
GL={'cash':'cash','live_principal':'asset_principal','default_principal':'credit_default_principal',
    'live_book_adjustment':'book_adjustment','default_book_adjustment':'credit_default_book_adjustment',
    'allowance':'allowance','interest_income':'pnl:consumer_interest_income',
    'credit_loss_expense':'pnl:consumer_credit_loss_expense','basis_recognition_expense':'pnl:consumer_basis_recognition_expense',
    'recovery_income':'pnl:consumer_recovery_income'}


def _text(s):
    credit.text(s)


def _near_zero(v):
    credit.equal(v,0.,'mixed-consumer/complete-right')


def _empty_owned():
    return dict(live=[credit._empty_live() for _ in range(3)],allowance=0.,claims=[])


def _close_owned(owned):
    # Closure is per unweighted source, before any allocation or path pooling.
    for live in owned['live']:
        for value in live.values():_near_zero(value)
    _near_zero(owned['allowance'])
    for claim in owned['claims']:
        for key in ('outstanding_principal','outstanding_accrued_interest','outstanding_book_adjustment'):
            _near_zero(claim[key])
        if claim['remaining_recoveries']:
            raise ArithmeticError('mixed consumer unretired complete recovery tail')


def _product_bounds(raw,check):
    """Conservative 1080-day native source/replay reservation before projection."""
    if not isinstance(raw['product_owners'],list) or not 1<=len(raw['product_owners'])<=64:
        raise ArithmeticError('mixed consumer bounded owner inventory')
    work=capacity=count=0
    for owner in raw['product_owners']:
        dated._check(check)
        credit.fields(owner,'unit_id position_id account_id unit_face allocated_face accounting_category source_identity original_yield_anchor initial_allowance_review runs')
        if not isinstance(owner['runs'],list) or not 1<=len(owner['runs'])<=9:raise ArithmeticError('mixed consumer bounded run coverage')
        for run in owner['runs']:
            credit.fields(run,'ledger_run_id scenario_id severity paths')
            if not isinstance(run['paths'],list) or not 1<=len(run['paths'])<=32:raise ArithmeticError('mixed consumer bounded path coverage')
            for path in run['paths']:
                dated._check(check);credit.fields(path,'path_id source_weight request');request=path['request']
                credit.fields(request,'kind request');kind=request['kind'];request=request['request']
                if kind not in ('installment','card'):raise ArithmeticError('explicit native product kind required')
                (credit if kind=='installment' else card)._admit(request)
                if dated.candidate._size(path['request'])>1024*1024:raise ArithmeticError('bounded raw primitive wire')
                c=request['contract'];days=c['horizon_days'];reviews=len(request['allowance_reviews'])+len(request['chargeoffs'])
                if not 1<=days<=1080 or len(c['steps'])+len(c['payment_dates'] if kind=='installment' else c['cycles'])>4096 or reviews>64:
                    raise ArithmeticError('mixed consumer bounded raw primitive work domain')
                upper=days+2*(len(c['payment_dates'] if kind=='installment' else c['cycles'])+len(c['steps']))+6*reviews+32
                actions=len(c['steps'])+len(c['payment_dates'] if kind=='installment' else c['cycles'])
                primitive=(days+1)*(actions+reviews+72)
                if primitive>2_000_000:raise ArithmeticError('native primitive pre-generation work')
                work+=primitive+days*(len(c['steps'])+len(c['payment_dates'] if kind=='installment' else c['cycles'])+reviews+64)+upper*32
                width=_escaped_strings([owner['unit_id'],owner['position_id'],owner['account_id'],path['path_id'],path['request']],check)+8192
                capacity+=(upper*14+(days+1)*16)*width;count+=1
                if work>8_000_000 or capacity>128*1024*1024:
                    raise ArithmeticError('mixed consumer aggregate work/escaped capacity exceeded before projection')
    work+=1081*count*64;capacity+=1081*count*8192
    if work>8_000_000 or capacity>128*1024*1024:raise ArithmeticError('mixed consumer aggregate reservation exceeded before projection')
    return dict(work=work,retained_bytes=capacity,full_end_ordinal=raw['performing']['backbook']['dated']['as_of_ordinal']+1080,paths=count)


def _escaped_strings(value,check):
    dated._check(check)
    if isinstance(value,str):return len(json.dumps(value,ensure_ascii=False,separators=(',',':')).encode())
    if isinstance(value,list):return sum(_escaped_strings(x,check) for x in value)
    if isinstance(value,dict):return sum(_escaped_strings(x,check) for x in value.values())
    return 0


def _performing_capacity(raw, preparation, spec, events, end, check=lambda:False):
    """Escape-aware complete metadata admission; never use amounts as truth."""
    asof = raw['backbook']['dated']['as_of_ordinal']; days = end-asof
    runs = raw['backbook']['dated']['runs']; selection = raw['candidate']; total = 0
    for ui,u in enumerate(preparation['preparation']['units']):
        dated._check(check)
        for income in u['income']:
            raw_income = income['native_income']['raw_income_request']
            rows = income['native_income']['native_income_result']['rows']; last = rows[-1]['payment_ordinal']; d = last-u['purchase_ordinal']
            source = next(s for s in u['complete_purchase_result']['native_sources'] if s['role']=='physical' and s['scenario_id']==income['scenario_id'] and s['path_id']==income['path_id'])
            projection=source['projection']
            if 'interest' in projection:
                origins=[f"deposit:{e['segment_id']}:{e['start_ordinal']}:{e['end_ordinal']}" for e in projection['interest']]
            else:origins=[e['origin_id'] for e in projection['recognition']]
            cash_ids=[e['event_id'] for e in projection['cashflows']]
            metadata = [selection['owners'][ui],selection['policy'],raw['policy'],selection['run_mapping'],u['unit_id'],income['scenario_id'],income['path_id'],source['run_id'],origins,cash_ids,raw_income['contract'],raw_income['policy'],raw_income['original_yield_anchor']]
            width = 4*dated.candidate._size(metadata)+8192
            total += (d*24+days*2+32)*width
    owners = raw['backbook']['dated']['owners']; ranges = {}; cursor = 0
    for plan in raw['backbook']['dated']['accrual_plans']:
        selector = dated._selector(plan['run'],{r['run_id']:r for r in runs}); n = len(plan['segments'])
        ranges[plan['position_id'],plan['event_id'],selector] = [cursor,cursor+n]; cursor += n
    for index,e in enumerate(events):
        if index % 256 == 0:
            dated._check(check)
        event = e['event']; oi = next(i for i,o in enumerate(owners) if o['position_id']==event['position_id'])
        ri = None if e['selector'] is None else next(i for i,r in enumerate(runs) if r['run_id']==e['selector'])
        stored = dict(index=index,owner_index=oi,event_id=event['event_id'],run_index=ri,payment_ordinal=event['payment_ordinal'],
            day_offset=event['day_offset'],full_contract_end_ordinal=end,accrual_period=event['accrual_period'],components=event['components'],
            measure=event['measure'],provenance=event['provenance'],recognition_range=ranges.get((event['position_id'],event['event_id'],e['selector']),[0,0]))
        count = 8+sum(max(0,min(s['end_ordinal'],end)-max(s['start_ordinal'],asof)) for s in e['segments'])
        width = 4*dated.candidate._size([stored,owners[oi],runs,raw['backbook']['recognition_policy']])+4096
        total += count*12*(len(runs) if ri is None else 1)*width
    for p in spec['positions']:
        total += (days+8)*len(runs)*(4*dated.candidate._size([p,runs])+2048)
    for a in spec['accounts']:
        total += (days+8)*len(runs)*(4*dated.candidate._size([a,runs])+8192)
    total += 4*(dated.candidate._size(raw)+dated.candidate._size(preparation))
    if total > 128*1024*1024:
        raise ArithmeticError('mixed aggregate escaped metadata capacity exceeded before daily generation')
    return total


def _admit(raw,performing_preparation,check):
    credit.fields(raw,'schema performing combined_specification product_owners policy')
    if raw['schema']!=SCHEMA:raise ArithmeticError('mixed consumer exact schema required')
    credit.fields(raw['policy'],'identity ownership phase tail cash initial_allowance generic_cashflows generic_credit generic_marks')
    credit.identity(raw['policy']['identity'])
    for key,value in POLICY.items():
        if raw['policy'][key]!=value:raise ArithmeticError('mixed consumer explicit exclusive policy required')
    if dated.candidate._size(raw)>32*1024*1024:raise ArithmeticError('mixed product raw wire admission')
    reservation=_product_bounds(raw,check)
    base=raw['performing']
    base_spec,owners,runs,events,capture,base_end,context=dated._admit(base,performing_preparation,check)
    combined=dated._normalize(raw['combined_specification'])
    credit.equal({k:v for k,v in combined.items() if k!='positions'},
                 {k:v for k,v in base_spec.items() if k!='positions'},'mixed-consumer/actual-opening')
    base_ids={p['id'] for p in base_spec['positions']}
    credit.equal([p for p in combined['positions'] if p['id'] in base_ids],base_spec['positions'],'mixed-consumer/original-positions')
    extra={p['id']:p for p in combined['positions'] if p['id'] not in base_ids}
    if not isinstance(raw['product_owners'],list) or not raw['product_owners'] or len(raw['product_owners'])>64 or len(combined['positions'])>512:
        raise ArithmeticError('mixed consumer bounded nonempty owner inventory required')
    scenes=[(r['run_id'],r['scenario_id'],r['severity']) for r in base['backbook']['dated']['runs']]
    if any(severity!=1. for _,_,severity in scenes):raise ArithmeticError('mixed consumer protocol3 reverse severity unsupported')
    start=context['as_of_ordinal'];report=context['reporting_end_ordinal'];end=base_end
    seen=set();units=set();paths=[]
    for oi,owner in enumerate(raw['product_owners']):
        dated._check(check)
        credit.fields(owner,'unit_id position_id account_id unit_face allocated_face accounting_category source_identity original_yield_anchor initial_allowance_review runs')
        for key in ('unit_id','position_id','account_id','accounting_category'):_text(owner[key])
        for key in ('source_identity','original_yield_anchor'):credit.identity(owner[key])
        if owner['initial_allowance_review'] is not None:
            credit.fields(owner['initial_allowance_review'],'review amount')
            credit.identity(owner['initial_allowance_review']['review']);credit.amount(owner['initial_allowance_review']['amount'])
        credit.amount(owner['unit_face']);credit.amount(owner['allocated_face'])
        if owner['unit_face']==0. or owner['position_id'] in seen or owner['unit_id'] in units or owner['position_id'] not in extra:
            raise ArithmeticError('mixed consumer duplicate or missing exclusive owner')
        seen.add(owner['position_id']);units.add(owner['unit_id']);pos=extra[owner['position_id']]
        if (pos['account']!=owner['account_id'] or pos['kind']!='loan' or pos['classification']!=owner['accounting_category']
                or owner['accounting_category']!='ac' or pos['opening_market_price']!=1. or pos['collateral_pool'] or pos['collateral_position']
                or any(pos[k]!=0. for k in ('balance','opening_accrued','book_adjustment','allowance','commitment','annual_pd','migration_rate','draw_fraction','monthly_runoff','watch_fraction','rate','maturity_day','start_day',
                    'floating_beta','rollover','eligible_fraction','encumbered_fraction','pledged_face','risk_weight','rsf_weight','asf_weight','hqla_weight','lcr_outflow_weight','lcr_inflow_weight'))):
            raise ArithmeticError('mixed consumer exact zero-opening exclusive asset placeholder required')
        account=next((a for a in combined['accounts'] if a['id']==owner['account_id']),None)
        if account is None or any(account[k]!=0. for k in ('cet1_floor','leverage_floor','lcr_floor','nsfr_floor')) or account['htm_asset_limit']!=1.:
            raise ArithmeticError('mixed consumer interim regulatory/liquidity constraint classification unsupported')
        coverage=[]
        for run in owner['runs']:
            credit.finite(run['severity'],0.,20.);coverage.append((run['ledger_run_id'],run['scenario_id'],run['severity']))
        if len(set(coverage))!=len(coverage) or set(coverage)!=set(scenes):
            raise ArithmeticError('mixed consumer complete native run/scenario coverage required')
        anchor=None
        for ri,run in enumerate(owner['runs']):
            credit.fields(run,'ledger_run_id scenario_id severity paths')
            if not run['paths'] or len(run['paths'])>32:raise ArithmeticError('mixed consumer bounded complete physical path list')
            weights=0.;keys=set()
            for pi,path in enumerate(run['paths']):
                dated._check(check);credit.fields(path,'path_id source_weight request');_text(path['path_id'])
                credit.finite(path['source_weight'],0.,1.);weights+=path['source_weight']
                if path['path_id'] in keys:raise ArithmeticError('mixed consumer duplicate physical path key')
                keys.add(path['path_id']);credit.fields(path['request'],'kind request');kind=path['request']['kind'];request=path['request']['request']
                if kind not in ('installment','card'):raise ArithmeticError('explicit native product kind required')
                (credit if kind=='installment' else card)._admit(request);c=request['contract'];a=request['acquisition']
                if (request['path_id']!=path['path_id'] or c['identity']['position_id']!=owner['position_id']
                        or c['identity']['run_id']!=run['ledger_run_id'] or request['economic_source']!=owner['source_identity']
                        or a['original_yield_anchor']!=owner['original_yield_anchor'] or a['principal']!=owner['unit_face']
                        or c['identity']['currency']!=next(a['currency'] for a in combined['accounts'] if a['id']==owner['account_id'])
                        or a['purchase_ordinal']<start or request['reporting_end_ordinal']!=min(max(report,a['purchase_ordinal']),c['as_of_ordinal']+c['horizon_days'])):
                    raise ArithmeticError('mixed consumer original source/anchor/calendar/binding mismatch')
                current=(kind,_terms(kind,c),a,c['identity']['original_loan_id'],c['identity']['source_revision'],request['economic_source'],request['policy'],request['opening_allowance'])
                if anchor is None:anchor=current
                elif current!=anchor:raise ArithmeticError('mixed consumer path/run original purchase anchor changed')
                if request['opening_allowance']!=0. and owner['initial_allowance_review'] is None:
                    raise ArithmeticError('mixed consumer initial allowance requires explicit recognition review')
                if owner['initial_allowance_review'] is not None:
                    credit.equal(owner['initial_allowance_review']['amount'],float(request['opening_allowance']),'mixed-consumer/reviewed-initial-allowance')
                individual_end=c['as_of_ordinal']+c['horizon_days'];end=max(end,individual_end)
                if end-start>1080:raise ArithmeticError('mixed consumer complete calendar admission exceeded')
                factor=owner['allocated_face']/owner['unit_face']*path['source_weight'];credit.finite(factor,0.,1e15)
                paths.append(dict(owner_index=oi,run_index=ri,path_index=pi,owner=owner,run=run,path=dict(path,request=request),kind=kind,factor=factor,full_end_ordinal=individual_end))
    if set(extra)!=seen:raise ArithmeticError('mixed consumer unowned extra or duplicate original position')
    for owner in raw['product_owners']:
        for run in owner['runs']:
            credit.equal(sum((p['source_weight'] for p in run['paths']),0.),1.,'mixed-consumer/absolute-physical-mass')
    if len(json.dumps(raw,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())>32*1024*1024:
        raise ArithmeticError('mixed consumer bounded raw source JSON required')
    derived,acquisitions,declared,base_events,pools,signs=dated._combined(base,owners,runs,events,capture,end)
    context=dict(context,reservation=reservation,original_spec=base_spec,back_events=events,capture=capture,runs=runs,derived=derived,acquisitions=acquisitions)
    fullspec=deepcopy(raw['combined_specification']);fullspec['horizon_days']=end-start;derived['specification']=fullspec
    # Reserve native generation and all daily tables before projecting any credit.
    _,candidate_work=dated.candidate._journal_admission(context,performing_preparation['preparation'],
        [p['full_end_ordinal'] for p in capture['paths']],capture['admitted_daily_rows'],capture['admitted_work'])
    performing_fullspec=deepcopy(base_spec);performing_fullspec['horizon_days']=end-start
    performing_bytes=_performing_capacity(base,performing_preparation,performing_fullspec,events,end,check)
    if performing_bytes+reservation['retained_bytes']>128*1024*1024:
        raise ArithmeticError('mixed joined escaped capacity reservation exceeded before credit projection')
    indexing=(len(events)+sum(len(e['segments']) for e in events))*len(runs)
    total_work=candidate_work+indexing+capture['admitted_daily_rows']*24+dated._dated_work(performing_fullspec,base_events,end,len(runs),acquisitions['events'])+reservation['work']
    if total_work>8_000_000:raise ArithmeticError('mixed consumer complete aggregate work exceeded before projection')
    context['admitted_work']=total_work
    source_bytes=0
    for path in paths:
        dated._check(check);source=(credit if path['kind']=='installment' else card).reconstruct(path['path']['request'],check);_close_owned(source['closing']);source_bytes+=dated.candidate._size(dict(kind=path['kind'],result=source))
        if source_bytes>reservation['retained_bytes']:raise ArithmeticError('retained typed native source wire exceeded reservation')
        path['original_source']=source;path['source']=deepcopy(source)
        for line in path['source']['lineage']:line['product_event_ids']=deepcopy(line.get('product_event_ids',line.get('source_event_ids',[])))
    return combined,base_events,acquisitions['events'],pools,signs,paths,end,context,declared


def _receipts(path,ordinal):
    source=path['original_source'];request=path['path']['request'];day=min(ordinal-request['acquisition']['purchase_ordinal'],len(source['daily'])-1)
    row=deepcopy(source['daily'][day]);out=dict(kind=path['kind'],daily=row)
    if path['kind']=='card':
        out.update(statement_lineage=[deepcopy(s) for s in source['statement_lineage'] if s['ordinal']==ordinal],
            commitment_source=deepcopy(source['commitment_source']),termination=deepcopy(request['termination']))
    return out


def _terms(kind,c):
    if kind=='installment':return None
    out=deepcopy(c)
    for k in ('steps','horizon_days','collection_fraction','collection_cost','recovery_delay_days'):out.pop(k)
    out['identity'].pop('run_id')
    return out



_assert_channels=shared._assert_channels
_mapped_intent=shared._mapped_intent
_observe_full=shared._observe_full
_closing_gl=shared._closing_gl

def replay_events(spec,base,events,acquisitions,pools,signs,paths,end,report,check=lambda:False,detailed=False):
    """Independent raw EVENTS -> one chronological journal/pooled GL, never outputs."""
    start=base['backbook']['dated']['as_of_ordinal'];out={k:[] for k in ('journal','trial_balance','daily_accounts','reporting_accounts','closing_accounts','product_daily','product_lineage')}
    if detailed:
        out.update({k:[] for k in ('closing_statements','daily_balance','dated_rights','event_lineage','acquisition_lineage','exposures','ledger','reporting_rights','reported_daily_balance')})
        out['product_acquisition_lineage']=[]
    scenarios=['baseline']+[s['name'] for s in spec['scenarios']]
    run_by_scene={r['scenario_id']:r for r in base['backbook']['dated']['runs']}
    for scene in scenarios:
        dated._check(check);runid=run_by_scene[scene]['run_id'];accounts=deepcopy(spec['accounts']);by_account={a['id']:a for a in accounts}
        positions=deepcopy(spec['positions']);by_position={p['id']:p for p in positions};gl={};seq=0;aggregates={};aggregate_order=[]
        selected=[e for e in events if e['selector'] in {None,runid}];acq=[a for a in acquisitions if a['run_id']==runid]
        selected_paths=[p for p in paths if p['run']['ledger_run_id']==runid]
        rights={p['id']:[0.,0.,0.,0.] for p in positions};event_state=[];pool_state={a['event_id']:0. for a in acq}
        for e in selected:
            earned=dated._cumulative(e['segments'],start,'accrued_interest');book=dated._cumulative(e['segments'],start,'book_amortization')
            event_state.append(dict(earned=earned,book=book,settled=False))
            for i,v in enumerate(dated._rights(earned,0.,False)):rights[e['event']['position_id']][i]+=v
        channels={};acquired=set();intent_by_day={};purchase_by_day={}
        for n,path in enumerate(selected_paths):
            channels[n]={};request=path['path']['request'];purchase=request['acquisition']['purchase_ordinal']
            purchase_by_day.setdefault(purchase,[]).append((n,path))
            for index,intent in enumerate(path['source']['intents']):intent_by_day.setdefault(intent['recognition_ordinal'],[]).append((n,path,index,intent))

        def post(day,account,event,lines,cash=0.,earnings=0.):
            nonlocal seq
            rows=[(g,i,v) for g,i,v in lines if v!=0.]
            if abs(math.fsum(v for _,_,v in rows))>1e-9*max(1.,math.fsum(abs(v) for _,_,v in rows)):
                raise ArithmeticError('mixed consumer raw-event transaction imbalance')
            if rows:
                seq+=1
                for g,i,v in rows:
                    key=account,g,i;gl[key]=gl.get(key,0.)+v;credit.finite(gl[key],-1e15,1e15)
                    out['journal'].append(dict(transaction_number=seq,scenario=scene,day=day,account=account,event=event,gl_account=g,instrument_id=i,value=v))
            a=by_account[account];a['cash']+=cash;a['equity']+=earnings
            if event!='opening':
                if (account,event) not in aggregate_order:aggregate_order.append((account,event))
                sums=aggregates.setdefault((account,event),[0.,0.]);sums[0]+=cash;sums[1]+=earnings
            return seq if rows else None

        for a in accounts:
            rows=[('cash','',a['cash']),('opening_equity','',-a['equity'])]
            for p in positions:
                if p['account']==a['id']:
                    rows.extend([(dated._principal(p),p['id'],dated._side(p)*p['balance']),('book_adjustment',p['id'],dated._side(p)*p['book_adjustment']),('allowance',p['id'],-p['allowance'])])
                    rows.extend((g,p['id'],v) for g,v in zip(dated.RIGHTS,rights[p['id']],strict=True))
            post(0,a['id'],'opening',rows)

        def dated_event(index,ordinal,interest,book,settlement):
            e=selected[index];ev=e['event'];v=event_state[index];pos=by_position[ev['position_id']];pool=pools.get((runid,ev['event_id']))
            before=dated._path_rights(dated._side(pos),signs[pool],pool_state[pool]) if pool is not None else dated._rights(v['earned'],ev['components']['cash_interest'],v['settled'])
            if settlement and v['settled']:raise ArithmeticError('mixed consumer duplicate dated settlement')
            v['earned']+=interest;v['book']+=book
            if settlement:v['settled']=True
            if pool is not None:
                pool_state[pool]+=interest-(ev['components']['cash_interest'] if settlement else 0.)
                after=dated._path_rights(dated._side(pos),signs[pool],pool_state[pool])
            else:after=dated._rights(v['earned'],ev['components']['cash_interest'],v['settled'])
            delta=[a-b for a,b in zip(after,before,strict=True)]
            for i,x in enumerate(delta):rights[pos['id']][i]+=x
            pc=ev['components']['principal'] if settlement else 0.;ic=ev['components']['cash_interest'] if settlement else 0.
            if abs(pc)>pos['balance']+1e-9:raise ArithmeticError('mixed consumer dated settlement exceeds owned principal')
            pos['balance']-=abs(pc);pos['book_adjustment']+=dated._side(pos)*book
            lines=[(dated._principal(pos),pos['id'],-dated._side(pos)*abs(pc)),('book_adjustment',pos['id'],book)]
            lines.extend((g,pos['id'],x) for g,x in zip(dated.RIGHTS,delta,strict=True))
            lines.extend([('cash','',pc+ic),('pnl:dated_settlement' if settlement else 'pnl:dated_recognition',pos['id'],-interest-book)])
            tx=post(ordinal-start,pos['account'],'dated_settlement' if settlement else 'dated_recognition',lines,pc+ic,interest+book)
            if detailed:
                owner=e['owner'];run=run_by_scene[scene]
                out['event_lineage'].append(dict(scenario=scene,run_id=runid,input_revision=run['input_revision'],day=ordinal-start,
                    transaction_number=tx,position_id=pos['id'],event_id=ev['event_id'],phase='settlement' if settlement else 'recognition',
                    source_id=owner['source_id'],source_revision=owner['source_revision'],
                    provenance_json=json.dumps(ev['provenance'],separators=(',',':'),ensure_ascii=False),measure=ev['measure'],ownership=owner['ownership'],
                    policy_id=base['backbook']['recognition_policy']['policy_id'],policy_revision=base['backbook']['recognition_policy']['policy_revision'],
                    principal=pc,cash_interest=ic,accrued_interest=interest,book_amortization=book,fees=0.))

        for ordinal in range(start,end+1):
            dated._check(check);day=ordinal-start
            for a in acq:
                if a['ordinal']!=ordinal:continue
                p=by_position[a['position_id']];side=dated._side(p);basis=a['cost']-a['principal'];p['balance']+=a['principal'];p['book_adjustment']+=basis
                tx=post(day,p['account'],'dated_acquisition',[(dated._principal(p),p['id'],side*a['principal']),('book_adjustment',p['id'],side*basis),('cash','',-side*a['cost'])],-side*a['cost'])
                if detailed:
                    out['acquisition_lineage'].append(dict(scenario=scene,severity=1.,run_id=runid,day=day,ordinal=ordinal,
                        position_id=p['id'],account=p['account'],event_id=a['event_id'],source=a['source'],path_id=a['path_id'],
                        source_weight=a['source_weight'],allocation_factor=a['allocation_factor'],original_yield_anchor=a['original_yield_anchor'],
                        measure='physical',policy=base.get('policy',base['candidate']['policy'])['identity'],principal=a['principal'],cost=a['cost'],basis=basis,transaction_number=tx))
            for n,path in purchase_by_day.get(ordinal,[]):
                req=path['path']['request'];a=req['acquisition'];owner=path['owner'];f=path['factor'];pos=owner['position_id']
                if n in acquired:raise ArithmeticError('mixed consumer duplicate acquisition')
                acquired.add(n);channels[n]={('live','principal'):a['principal'],('live','basis'):a['cost']-a['principal'],('allowance',):0.}
                purchase_tx=post(day,owner['account_id'],'consumer_acquisition',[('asset_principal',pos,a['principal']*f),('book_adjustment',pos,(a['cost']-a['principal'])*f),('cash','',-a['cost']*f)],-a['cost']*f)
                allowance=req['opening_allowance']
                allowance_tx=None
                if owner['initial_allowance_review'] is not None:
                    channels[n]['allowance',]=allowance
                    allowance_tx=post(day,owner['account_id'],'consumer_reviewed_initial_allowance',[('pnl:consumer_credit_loss_expense',pos,allowance*f),('allowance',pos,-allowance*f)],earnings=-allowance*f)
                if detailed:
                    out['product_acquisition_lineage'].append(dict(scenario=scene,run_id=runid,ordinal=ordinal,position_id=pos,path_id=path['path']['path_id'],
                        source_weight=path['path']['source_weight'],allocation_factor=owner['allocated_face']/owner['unit_face'],economic_source=deepcopy(owner['source_identity']),
                        acquisition=deepcopy(a),original_yield_anchor=deepcopy(owner['original_yield_anchor']),initial_allowance_review=deepcopy(owner['initial_allowance_review']),
                        purchase_transaction_number=purchase_tx,allowance_transaction_number=allowance_tx))
            if day:
                for i,e in enumerate(selected):
                    interest=book=0.;active=False
                    for segment in e['segments']:
                        if segment['start_ordinal']<=ordinal-1<segment['end_ordinal']:
                            length=segment['end_ordinal']-segment['start_ordinal'];f0=(ordinal-1-segment['start_ordinal'])/length;f1=(ordinal-segment['start_ordinal'])/length
                            interest+=segment['accrued_interest']*f1-segment['accrued_interest']*f0
                            book+=segment['book_amortization']*f1-segment['book_amortization']*f0;active=True
                    if active:dated_event(i,ordinal,interest,book,False)
            for n,path,index,intent in intent_by_day.get(ordinal,[]):
                if n not in acquired:raise ArithmeticError('mixed consumer intent precedes actual purchase')
                next_state,lines,cash,earnings=_mapped_intent(intent,channels[n],path['factor']);owner=path['owner']
                tx=post(day,owner['account_id'],intent['event'],[(g,'' if g=='cash' else owner['position_id'],v) for g,v in lines],cash,earnings)
                channels[n]=next_state
                out['product_lineage'].append(dict(scenario=scene,run_id=runid,ordinal=ordinal,position_id=owner['position_id'],path_id=path['path']['path_id'],
                    source_weight=path['path']['source_weight'],allocation_factor=owner['allocated_face']/owner['unit_face'],intent_index=index,
                    transaction_number=tx,claim_id=intent['claim_id'],policy=deepcopy(intent['policy']),source=deepcopy(intent['source']),
                    original_yield_anchor=deepcopy(owner['original_yield_anchor']),native_lineage=deepcopy(path['original_source']['lineage'][index])))
            for i,e in enumerate(selected):
                if e['event']['payment_ordinal']==ordinal:dated_event(i,ordinal,0.,0.,True)
            for n,path in enumerate(selected_paths):
                purchase=path['path']['request']['acquisition']['purchase_ordinal']
                owned=path['source']['daily'][min(ordinal-purchase,path['full_end_ordinal']-purchase)]['owned'] if ordinal>=purchase else _empty_owned()
                _assert_channels(channels[n],owned)
                out['product_daily'].append(dict(scenario=scene,run_id=runid,ordinal=ordinal,position_id=path['owner']['position_id'],path_id=path['path']['path_id'],
                    acquired=n in acquired,source_weight=path['path']['source_weight'],allocation_factor=path['owner']['allocated_face']/path['owner']['unit_face'],unweighted_owned=deepcopy(owned),product_receipts=_receipts(path,ordinal) if n in acquired else None))
            if detailed:
                summaries={}
                for n,path in enumerate(selected_paths):
                    owner=path['owner'];summary=summaries.setdefault(owner['position_id'],[0.]*9);ch=channels[n];f=path['factor']
                    value=ch.get(('live','accrued'),0.)
                    v=[ch.get(('live','principal'),0.),ch.get(('live','basis'),0.),ch.get(('allowance',),0.),max(value,0.),min(value,0.),0.,0.,0.,0.]
                    for key,x in ch.items():
                        if key[0]!='claim':continue
                        if key[2]=='principal':v[5]+=x
                        elif key[2]=='basis':v[6]+=x
                        elif key[2]=='accrued':v[7]+=max(x,0.);v[8]+=min(x,0.)
                    for j,x in enumerate(v):summary[j]+=x*f
                for position,value in summaries.items():
                    pos=by_position[position];pos['balance']=value[0];pos['book_adjustment']=value[1];pos['allowance']=value[2]
                rows=_observe_full(spec,positions,accounts,rights,summaries,scene,day)
                if day<=30 or day%30==0 or ordinal in {end,report}:out['daily_balance'].extend(rows)
                if ordinal==report:out['reported_daily_balance'].extend(deepcopy(rows))
                if day==0 or day%30==0 or ordinal==end:
                    for p in positions:
                        accrued=sum(rights[p['id']])*dated._side(p) if p['id'] not in summaries else summaries[p['id']][3]+summaries[p['id']][4]
                        out['exposures'].append(dict(scenario=scene,day=day,account=p['account'],position=p['id'],kind=p['kind'],
                            balance=p['balance'],allowance=p['allowance'],watch_fraction=p['watch_fraction'],commitment=p['commitment'],
                            market_factor=1.,encumbered_fraction=p['encumbered_fraction'],accrued_interest=accrued))
                for account,event in aggregate_order:
                    value=aggregates.get((account,event),[0.,0.])
                    if any(x!=0. for x in value):out['ledger'].append(dict(scenario=scene,day=day,account=account,event=event,cash=value[0],earnings=value[1],aoci=0.,memo_amount=0.))
            aggregates.clear();aggregate_order.clear()
            for a in accounts:
                assets=liabilities=0.
                funding={(account,instrument) for account,code,instrument in gl if code=='funding_principal'}
                for (account,code,instrument),value in gl.items():
                    if account!=a['id']:continue
                    if (code in {'funding_principal','dated_interest_payable','dated_deferred_income','credit_default_deferred_income'}
                            or code in {'book_adjustment','accrued_interest'} and (account,instrument) in funding):liabilities-=value
                    elif code!='opening_equity' and not code.startswith('pnl:'):assets+=value
                credit.equal(assets-liabilities,a['equity'],'mixed-consumer/daily-pooled-GL')
                if a['cash']<a['cash_floor']-1e-9:raise ArithmeticError('mixed consumer daily pooled cash floor')
                row=dict(scenario=scene,run_id=runid,account=a['id'],ordinal=ordinal,cash=a['cash'],assets=assets,liabilities=liabilities,equity=a['equity'])
                out['daily_accounts'].append(row)
                if ordinal==report:out['reporting_accounts'].append(deepcopy(row))
                if ordinal==end:out['closing_accounts'].append(deepcopy(row))
        for n,path in enumerate(selected_paths):_close_owned(path['source']['closing'])
        for p in positions:
            _near_zero(p['balance']);_near_zero(p['book_adjustment'])
            for value in rights[p['id']]:_near_zero(value)
        out['trial_balance'].extend(dict(scenario=scene,account=a,gl_account=g,instrument_id=i,balance=v) for (a,g,i),v in sorted(gl.items()))
        if detailed:
            out['dated_rights'].extend(dated._rights_rows(selected,event_state,scene,runid,end,pool_state,pools,acq))
            out['closing_statements'].extend(_closing_gl(gl,spec['accounts'],scene))
    if detailed:
        report_source=deepcopy(base);report_source['backbook']['dated']['reporting_horizon_days']=report-start
        out['reporting_rights']=dated._reporting_rights(report_source,events,acquisitions,pools,signs)
    return out



def _check_equal(actual, expected, path):
    """Financial tolerances never admit a floating point date/count identity."""
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys()!=expected.keys():raise ArithmeticError(path+': exact fields differ')
        for key,value in expected.items():_check_equal(actual[key],value,path+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': ordered rows differ')
        for i,value in enumerate(expected):_check_equal(actual[i],value,path+'/'+str(i))
    elif type(expected) is int and (path.rsplit('/',1)[-1] in {
            'day','day_offset','transaction_number','owner_index','run_index','path_index','intent_index',
            'horizon_days','reporting_horizon_days','admitted_work','protocol','work','retained_bytes','paths',
            'purchase_transaction_number','allowance_transaction_number','source_index','admitted_bytes','recovery_delay_days'} or path.rsplit('/',1)[-1].endswith('ordinal') or 'source_recognition_indices' in path.split('/')):
        if type(actual) is not int or actual!=expected:raise ArithmeticError(path+': exact integer differs')
    else:dated._equal(actual,expected,path)


SCOPE='one native full-tail expected dated/backbook/candidate/installment/card product-event GL; reviewed original anchors and non-PCD allowance/default/recovery policies; exclusive owners and exactly-once physical weights; no observed-settlement, accounting-standard, empirical, joint-stochastic, pricing/solver credit adoption or independent persisted verification claim; interim capital research weights remain supplied and do not infer default RWA'
TABLES=('journal','trial_balance','closing_statements','daily_balance','dated_rights','event_lineage','acquisition_lineage','exposures','ledger','reporting_rights','reported_daily_balance','product_daily','product_lineage','product_acquisition_lineage')


def reconstruct(raw,native_preparation,check=lambda:False):
    """Raw product sources and reviewed policy -> one complete event/GL replay."""
    dated._check(check)
    spec,events,acq,pools,signs,paths,end,context,declared=_admit(raw,native_preparation,check)
    base=raw['performing'];report=context['reporting_end_ordinal']
    replay=replay_events(spec,base,events,acq,pools,signs,paths,end,report,check,detailed=True)
    performing=dict(raw_request=deepcopy(base),native_preparation=deepcopy(native_preparation),candidate_capture=context['capture'],
        combined_dated_request=context['derived'],acquisitions=context['acquisitions'],backbook_source_ends=declared,
        reporting_end_ordinal=report,full_end_ordinal=end,admitted_work=context['admitted_work'])
    request=dict(schema='product-event-owner-1',reporting_end_ordinal=report,owners=deepcopy(raw['product_owners']))
    result={k:replay[k] for k in TABLES}
    result.update(schema=SCHEMA,preparation=dict(raw_request=deepcopy(raw),performing_preparation=performing,product_request=request,product_reservation=context['reservation']),
        product_sources=[dict(owner_index=p['owner_index'],run_index=p['run_index'],path_index=p['path_index'],native=dict(kind=p['kind'],result=deepcopy(p['original_source']))) for p in paths],
        protocol=3,complete=True,reporting_end_ordinal=report,full_end_ordinal=end,admitted_work=context['admitted_work'],mixed_ledger_adoption=True,
        independent_persisted_verification=False,dynamic_mixed_acceptance=False,interim_default_capital_adoption=False,scope=SCOPE)
    if len(json.dumps(result,ensure_ascii=False,separators=(',',':'),allow_nan=False).encode())>32*1024*1024:raise ArithmeticError('mixed consumer complete result byte admission exceeded')
    dated._check(check);return result


def verify(raw,result,check=lambda:False):
    """Independent publication gate. Native source arrays/flags are never truth."""
    dated._check(check)
    if not isinstance(result,dict) or not isinstance(result.get('preparation'),dict):raise ArithmeticError('mixed consumer complete native preparation absent')
    p=result['preparation']
    if not isinstance(p.get('performing_preparation'),dict) or not isinstance(p['performing_preparation'].get('native_preparation'),dict):raise ArithmeticError('mixed consumer retained native product source absent')
    expected=reconstruct(raw,p['performing_preparation']['native_preparation'],check)
    # JSON provenance key order is not economic order; parse its complete typed
    # identity without altering ordered source events, journal rows or reductions.
    actual=deepcopy(result)
    for output in (actual,expected):
        for row in output['event_lineage']:
            dated._check(check);row['provenance_json']=json.loads(row['provenance_json'])
    _check_equal(actual,expected,'mixed-consumer');dated._check(check)
    return dict(verified=True,mixed_product_credit_only=True,ordered_mixed_journal_recomputed=True,
        complete_cash_rights_basis_tail_verified=True,dynamic_credit_acceptance=False,empirical_validation=False)

