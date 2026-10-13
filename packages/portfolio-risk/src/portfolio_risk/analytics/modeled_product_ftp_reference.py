"""Independent raw-source/whole-GL/funding/charge acceptance. Never an execution backend."""
from copy import deepcopy
from datetime import date
import math
from . import modeled_product_ftp_source_reference as source
from . import modeled_product_funding_reference as funding
from portfolio_risk.analytics import mortgage_closing_capital_reference as capital
from portfolio_risk.analytics import mortgage_mixed_reference as mixed
from portfolio_risk.analytics import _mortgage_mixed_financial_reference as financial
from portfolio_risk.analytics import mortgage_mixed_transport as mt

def key(p):return tuple(p[k] for k in ('source_id','owner_index','run_index','path_index'))
def interpolate(x,xs,ys):
    if x<=xs[0]:return ys[0]
    if x>=xs[-1]:return ys[-1]
    for lo,hi,a,b in zip(xs,xs[1:],ys,ys[1:]):
        if x<=hi:return a+(b-a)*(x-lo)/(hi-lo)
    raise ArithmeticError('curve')

def financial_number(x):
    if type(x)not in(int,float) or not math.isfinite(x):raise ArithmeticError('finite declared f64 required')
    return float(x)

def admit_raw(raw):
    financial.keys(raw,'schema source as_of policy_review curves charges capital capital_source_id dirty_unit_state acquisition_phase_policy')
    if raw['schema']!='modeled-product-ftp-preparation-1' or raw['dirty_unit_state']!='clean_frozen_units-1' or raw['acquisition_phase_policy']!='actual_owned_purchase_and_initial_allowance_once-1':raise ArithmeticError('explicit raw source/charge policy')
    def review(v):financial.keys(v,'id revision');financial.domain.identity(v)
    def binding(v):
        financial.keys(v,'source_id owner_index run_index path_index')
        if type(v['source_id'])is not str or not v['source_id']:raise ArithmeticError('source identity')
        if any(type(v[k])is not int or v[k]<0 for k in ('owner_index','run_index','path_index')):raise ArithmeticError('integer binding')
    review(raw['policy_review'])
    if type(raw['curves'])is not list or not 0<len(raw['curves'])<=16:raise ArithmeticError('bounded explicit curves')
    curves=set()
    for c in raw['curves']:
        financial.keys(c,'id entity currency as_of tenors reference_rates liquidity_spreads')
        for k in ('id','entity','currency','as_of'):
            if type(c[k])is not str or not c[k].strip()or len(c[k].encode('utf-8'))>200:raise ArithmeticError('curve label domain')
        if c['id']in curves:raise ArithmeticError('duplicate curve')
        curves.add(c['id'])
        if c['as_of']!=raw['as_of']:raise ArithmeticError('trade curve vintage')
        xs=c['tenors']
        if type(xs)is not list or not 1<=len(xs)<=256 or len(c['reference_rates'])!=len(xs)or len(c['liquidity_spreads'])!=len(xs):raise ArithmeticError('curve coverage')
        xs=[financial_number(x)for x in xs]
        if xs[0]<0 or any(x>1e18 for x in xs)or any(b<=a for a,b in zip(xs,xs[1:])):raise ArithmeticError('strict tenor sequence')
        for x in c['reference_rates']+c['liquidity_spreads']:
            if abs(financial_number(x))>1e18:raise ArithmeticError('curve rate domain')
    if type(raw['charges'])is not list or len(raw['charges'])>16:raise ArithmeticError('bounded complete charges')
    for c in raw['charges']:
        financial.keys(c,'binding review curve_id option contingent other_earnings cost_review daily_operating_cost allocated_capital annual_cost_of_capital')
        binding(c['binding']);review(c['review']);review(c['cost_review'])
        if c['curve_id']not in curves:raise ArithmeticError('curve identity')
        for name in ('daily_operating_cost','allocated_capital','annual_cost_of_capital'):
            if not 0<=financial_number(c[name])<= (1. if name=='annual_cost_of_capital'else 10000.):raise ArithmeticError('explicit cost domain')
        for name in ('option','contingent'):
            d=c[name];financial.keys(d,'review benchmark methodology annual_rate rate_vintage');review(d['review']);binding(d['benchmark'])
            if not 0<=financial_number(d['annual_rate'])<=1:raise ArithmeticError('explicit rate domain')

def path_expectations(raw,actual,check):
    # Whole raw/economic/ordered journal gate first. Fresh independent source math
    # below does not consume the producer's channel or funding values.
    source.verify(raw,dict(raw_input=raw,result=actual),check)
    controls={s['source_id']:source.checked_source(s,a['raw_native_result'],check) for s,a in zip(raw['sources'],actual['sources'],strict=True)}
    paths={}
    for s in raw['sources']:
        for review in raw['reviews']:
            if review['source_id']!=s['source_id']:continue
            request,opening,cash=source.projected_path(s,controls[s['source_id']],review,check)
            position,account,run,_=source.binding(s,review)
            points=None
            if s['input']['kind']=='product':
                o,r,p=(review[k]for k in ('owner_index','run_index','path_index'))
                original=s['input']['request']['product_owners'][o]['runs'][r]['paths'][p]
                if original['request']['kind']=='card_v2':points=controls[s['source_id']]['sources'][o,r,p]['economic']['points']
            prepared=funding.prepare(request,check,independent_source_residuals=True)
            commitment=[]
            for i,state in enumerate(prepared['raw_request']['states']):
                if points is not None:
                    index,point=next((j,p)for j,p in enumerate(points)if p['ordinal']==state['ordinal'])
                    commitment.append(dict(ordinal=state['ordinal'],native_economic_point_index=index,limit=float(point['limit']),undrawn=float(point['undrawn']),source='native_card_contract_commitment-1'))
                else:commitment.append(dict(ordinal=state['ordinal'],native_economic_point_index=i,limit=None,undrawn=float(next(x['amount']for x in state['channels']if x['channel']=='undrawn_memo')),source='native_owned_accounting_undrawn-1'))
            paths[key(review)]=dict(preparation=prepared,position_id=position,account_id=account,ledger_run_id=run,opening_earnings=opening,opening_source_cash=cash,commitment=commitment)
    # Native source coordinator emits stable explicit binding-key order, not the
    # caller's review-list order. Derive that order from raw binding identities.
    source_order={s['source_id']:i for i,s in enumerate(raw['sources'])}
    return dict(sorted(paths.items(),key=lambda p:(source_order[p[0][0]],p[0][1:])))

def matched(p,b,k,bk):
    if k[0]!=bk[0] or k[1]!=bk[1]:raise ArithmeticError('same reviewed original owner required')
    for name in ('entity','currency_unit','side','start_ordinal','full_end_ordinal','source_weight','unit_face','allocated_face','original_yield_anchor','policy','resets'):
        source.exact(p['raw_request'][name],b['raw_request'][name],'benchmark exact '+name)

def undrawn(p,points,i):
    r=p['raw_request'];a=points[i]['undrawn']
    if r['policy']['averaging']=='trapezoid_daily':
        z=points[i+1]['undrawn'];a=(a+z)/2
    return a*p['scale']

def expected_charges(raw,paths,check=lambda:False):
    positions=[];lineage=[];bykey={key(c):c for c in raw['source']['reviews']}
    keys=set(paths);seen=set()
    for pkey,p in paths.items():
        source.probe(check);rules=[c for c in raw['charges'] if key(c['binding'])==pkey]
        if len(rules)!=1:raise ArithmeticError('complete once-only charge coverage')
        rule=rules[0];seen.add(pkey)
        if rule['other_earnings']!='retain_seven_lanes_no_second_credit_loss-1':raise ArithmeticError('second credit loss')
        for name,method in (('option','positive_incremental_daily_funding-1'),('contingent','positive_incremental_native_undrawn-1')):
            c=rule[name];financial.domain.identity(c['review'])
            if c['methodology']!=method or key(c['benchmark'])not in keys or c['rate_vintage']!=raw['as_of']:raise ArithmeticError('explicit comparative rate policy')
            rate=financial_number(c['annual_rate'])
            if not 0<=rate<=1:raise ArithmeticError('rate')
        r=p['preparation'];bk=key(rule['option']['benchmark']);ck=key(rule['contingent']['benchmark']);b=paths[bk]['preparation'];c=paths[ck]['preparation'];matched(r,b,pkey,bk);matched(r,c,pkey,ck)
        for i,d in enumerate(r['days']):
            source.probe(check)
            if d['scaled_average_funding_balance']<0:raise ArithmeticError('signed Treasury exposure unsupported')
            inc=max(d['scaled_average_funding_balance']-b['days'][i]['scaled_average_funding_balance'],0.)
            un=max(undrawn(r,p['commitment'],i)-undrawn(c,paths[ck]['commitment'],i),0.)
            option=inc*rule['option']['annual_rate']*d['accrual_fraction'];contingent=un*rule['contingent']['annual_rate']*d['accrual_fraction'];basis=d['scaled_average_funding_balance']*d['accrual_fraction']
            if basis==0 and option!=0:raise ArithmeticError('nonzero option at zero average')
            positions.append(dict(id=':'.join(map(str,(*pkey,d['ordinal']))),loan_id=p['position_id'],cohort_id=None,entity=r['raw_request']['entity'],currency=r['raw_request']['currency_unit'],scenario=r['raw_request']['scenario_id'],period=date.fromordinal(d['ordinal']).isoformat(),curve_id=rule['curve_id'],side=r['raw_request']['side'],average_balance=d['scaled_average_funding_balance'],accrual_fraction=d['accrual_fraction'],repricing_years=d['reference_years'],funding_years=d['liquidity_years'],external_interest=d['original_eir'],fees=0.,operating_cost=rule['daily_operating_cost']*r['scale'],expected_loss=0.,allocated_capital=rule['allocated_capital']*r['scale'],cost_of_capital=float(rule['annual_cost_of_capital']),option_spread=option/basis if basis else 0.,contingent_liquidity_charge=contingent))
            lineage.append(dict(binding=rule['binding'],ordinal=d['ordinal'],benchmark=rule['option']['benchmark'],contingent_benchmark=rule['contingent']['benchmark'],actual_cash_difference=d['bank_cash']-b['days'][i]['bank_cash'],incremental_funding=inc,incremental_undrawn=un,option_amount=option,contingent_amount=contingent,retained_earnings=d['earnings_by_channel'],native_opening_undrawn=p['commitment'][i]['undrawn'],native_closing_undrawn=p['commitment'][i+1]['undrawn'],benchmark_opening_undrawn=paths[ck]['commitment'][i]['undrawn'],benchmark_closing_undrawn=paths[ck]['commitment'][i+1]['undrawn']))
    if seen!=keys or len(raw['charges'])!=len(keys):raise ArithmeticError('complete all paths required')
    return positions,lineage

def treasury_expected(request):
    # Direct independent management-accounting equations, no native callbacks.
    rows=[];totals={};curves={c['id']:c for c in request['curves']}
    for p in request['positions']:
        c=curves[p['curve_id']];reference=interpolate(p['repricing_years'],c['tenors'],c['reference_rates']);liquidity=interpolate(p['funding_years'],c['tenors'],c['liquidity_spreads']);basis=p['average_balance']*p['accrual_fraction'];side=1. if p['side']=='asset'else -1.
        rc=side*basis*reference;lc=side*basis*liquidity;oc=basis*p['option_spread'];charge=rc+lc+oc+p['contingent_liquidity_charge'];external=p['external_interest']+p['fees']-p['operating_cost']-p['expected_loss'];profit=external-charge;cc=p['allocated_capital']*p['cost_of_capital']*p['accrual_fraction']
        rows.append({k:p[k]for k in ('id','loan_id','cohort_id','entity','currency','scenario','period','curve_id')}|dict(reference_rate=reference,liquidity_spread=liquidity,reference_charge=rc,liquidity_charge=lc,option_charge=oc,contingent_liquidity_charge=p['contingent_liquidity_charge'],ftp_charge=charge,external_profit=external,business_profit=profit,capital_cost=cc,economic_profit=profit-cc,raroc=profit/p['allocated_capital']/p['accrual_fraction']if p['allocated_capital']>0 else None,flat_tail_used=any(x<c['tenors'][0]or x>c['tenors'][-1]for x in (p['repricing_years'],p['funding_years']))))
        k=tuple(p[x]for x in ('entity','currency','scenario','period'));a=totals.setdefault(k,[0.,0.,0.,0.]);a[0]+=external;a[1]+=profit;a[2]+=charge;a[3]+=cc
    reconciliation=[dict(zip(('entity','currency','scenario','period'),k))|dict(external_profit=v[0],business_profit=v[1],treasury_profit=v[2],consolidated_profit=v[1]+v[2],capital_cost=v[3],elimination_error=v[1]+v[2]-v[0])for k,v in sorted(totals.items())]
    return rows,reconciliation

def acquisition_expected(raw,actual,paths,check):
    out=[]
    for selected in actual['paths']:
        k=key(selected);p=paths[k];q=p['preparation']['raw_request'];s=next(s for s in raw['sources']if s['source_id']==k[0]);r=s['input']['request'];native=next(x for x in actual['sources']if x['source_id']==k[0])['raw_native_result'];kind=s['input']['kind'];acq=None;receipt=None;cost=allowance=0.;purchase=[];provision=[]
        if kind=='product':
            owner=r['product_owners'][k[1]];path=owner['runs'][k[2]]['paths'][k[3]];request=path['request']['request']
            if path['request']['kind']=='installment':acq=deepcopy(request['acquisition']);cost=float(acq['cost'])
            else:
                review=request['review'];cost=float(review['cost']);acq=dict(accounting_basis=review['accounting_basis'],cost=cost,identity=review['identity'],income_review=review['identity'],original_annual_effective_rate=float(review['original_annual_effective_rate']),original_yield_anchor=review['original_yield_anchor'],principal=float(review['principal']),purchase_ordinal=review['acquisition_ordinal'])
            # Metadata topology is used ONLY after complete independent source and
            # ordered journal verification. Financial expectations come from raw.
            receipt=next(x for x in native['product_acquisition_lineage']if x['run_id']==p['ledger_run_id']and x['position_id']==p['position_id']and x['path_id']==q['path_id'])
            allowance=float(owner['initial_allowance_review']['amount'])if owner['initial_allowance_review']is not None else 0.
            rows=native['journal'];tx=receipt['purchase_transaction_number'];atx=receipt['allowance_transaction_number']
        elif kind=='mortgage':
            owner=r['owners'][k[1]];path=owner['runs'][k[2]]['paths'][k[3]];request=next(x for x in r['sources']if x['source_id']==path['source_id'])['request'];acq=deepcopy(request['source']['acquisition']);cost=float(acq['actual_cost'])
            for name in ('actual_cost','original_annual_effective_rate'):acq[name]=float(acq[name])
            acq['opening_gross_carrying']=[float(x)for x in acq['opening_gross_carrying']]
            receipt=next(x for m in native['ledger_messages']if m.get('table')=='mortgage_lineage'for x in m['rows']if x['run_id']==p['ledger_run_id']and x['position_id']==p['position_id']and x['path_id']==q['path_id']and x['native_group_index']==0)
            rows=[];dictionary=[]
            for m in native['ledger_messages']:
                if m.get('table')!='journal':continue
                dictionary+=m['dictionary'];c=m['columns']
                for i,value in enumerate(c['value']):rows.append({name:(dictionary[col[i]]if name in mixed.STR_COLUMNS else col[i])for name,col in c.items()})
            tx=receipt['transaction_number'];atx=None
        if kind not in ('deposit','deposit_continuation','deposit_portfolio'):
            for name in ('cost','principal','original_annual_effective_rate'):
                if name in acq:acq[name]=float(acq[name])
            selected_rows=lambda n:[deepcopy(j)for j in rows if n is not None and j['scenario']==q['scenario_id']and j['account']==p['account_id']and j['transaction_number']==n]
            purchase=selected_rows(tx);provision=selected_rows(atx)
            source.exact(receipt['source_weight'],q['source_weight'],'once acquisition source weight');source.exact(receipt['allocation_factor'],q['allocated_face']/q['unit_face'],'once face');source.exact(receipt['ordinal'],q['start_ordinal'],'actual purchase date')
            # All journal rows already independently checked. Check raw purchase
            # cost and reviewed allowance separately again, including zero paths.
            source.near(sum(j['value']for j in purchase if j['gl_account']=='cash'),-cost*p['preparation']['scale'],'raw owned purchase cash')
            source.near(-sum(j['value']for j in provision if j['gl_account']=='pnl:consumer_credit_loss_expense'),-allowance*p['preparation']['scale'],'reviewed initial expense once')
        cash=-cost*p['preparation']['scale'];expense=-allowance*p['preparation']['scale'];opening=deepcopy(p['opening_earnings']);next(x for x in opening if x['channel']=='initial_allowance')['amount']+=expense
        out.append(dict(binding=dict(zip(('source_id','owner_index','run_index','path_index'),k)),kind='existing_opening_anchor_no_new_acquisition-1'if kind in ('deposit','deposit_continuation','deposit_portfolio')else 'actual_owned_acquisition-1',ordinal=q['start_ordinal'],position_id=p['position_id'],account_id=p['account_id'],ledger_run_id=p['ledger_run_id'],scenario=q['scenario_id'],native_path_id=q['path_id'],entity=q['entity'],currency=q['currency_unit'],source_weight=q['source_weight'],allocation_factor=q['allocated_face']/q['unit_face'],unweighted_acquisition=acq,purchase_cash=cash,initial_allowance_expense=expense,complete_opening_source_cash=p['opening_source_cash']+cash,complete_opening_earnings=opening,native_acquisition_receipt=receipt,purchase_postings=purchase,initial_allowance_postings=provision,cash_excluded_from_funding_rollforward=True,initial_allowance_excluded_from_recurring_external_interest=True))
    return out

def verify(raw,out,check=lambda:False):
    source.probe(check);admit_raw(raw);source.exact(out['raw_input'],raw,'original raw selected Value');result=out['result'];source.exact(result['raw_request'],raw,'immutable successor raw');source.exact(result['schema'],'modeled-product-ftp-preparation-1','schema')
    paths=path_expectations(raw['source'],result['source_preparation'],check);positions,lineage=expected_charges(raw,paths,check)
    phases=acquisition_expected(raw['source'],result['source_preparation'],paths,check)
    source.compare(result['selected_acquisition_phases'],phases,'complete owned opening phases')
    for actual,expected in zip(result['selected_acquisition_phases'],phases,strict=True):
        for field in ('binding','kind','ordinal','position_id','account_id','ledger_run_id','scenario','native_path_id','entity','currency','source_weight','allocation_factor','unweighted_acquisition','native_acquisition_receipt','purchase_postings','initial_allowance_postings','cash_excluded_from_funding_rollforward','initial_allowance_excluded_from_recurring_external_interest'):
            source.exact(actual[field],expected[field],'strict acquisition '+field)
    commitments=[dict(binding=dict(zip(('source_id','owner_index','run_index','path_index'),k)),points=p['commitment'])for k,p in sorted(paths.items())]
    source.compare(result['source_commitment_receipts'],commitments,'independent raw commitment source')
    for a,e in zip(result['source_commitment_receipts'],commitments,strict=True):
        source.exact(a['binding'],e['binding'],'commitment binding')
        for x,y in zip(a['points'],e['points'],strict=True):
            for field in ('ordinal','native_economic_point_index','source'):source.exact(x[field],y[field],'commitment identity')
    source.compare(result['charges'],lineage,'independent scenario cost')
    # All supplied identities/rates in Treasury input retain normalized declared
    # DTO fields separately from raw Value. No calculated amount may alter scope.
    request=result['treasury_request'];expected=dict(as_of=raw['as_of'],policy_id=raw['policy_review']['id'],capital=[],curves=deepcopy(raw['curves']),positions=positions,strategy_units=[],capital_deltas=[])
    for curve in expected['curves']:
        for field in ('tenors','reference_rates','liquidity_spreads'):curve[field]=[financial_number(x)for x in curve[field]]
    source.compare(request,expected,'native Treasury preparation')
    source.exact(request['curves'],expected['curves'],'exact independently normalized supplied curves')
    for a,e in zip(request['positions'],positions,strict=True):
        for k in ('id','loan_id','cohort_id','entity','currency','scenario','period','curve_id','side','cost_of_capital','repricing_years','funding_years'):source.exact(a[k],e[k],'exact position '+k)
    for a,e in zip(result['charges'],lineage,strict=True):
        for k in ('binding','ordinal','benchmark','contingent_benchmark'):source.exact(a[k],e[k],'exact charge lineage')
    rows,reconciliation=treasury_expected(expected);source.compare(result['treasury_result']['ftp_positions'],rows,'independent charges');source.compare(result['treasury_result']['ftp_reconciliation'],reconciliation,'signed scoped offsets')
    accounting=[]
    for phase in phases:
        income=sum(x['amount']for x in phase['complete_opening_earnings'])
        accounting.append(dict(binding=phase['binding'],ordinal=phase['ordinal'],phase='opening_acquisition-1',source_earnings=income,operating_cost=0.,full_external_earnings=income,business_profit=income,treasury_profit=0.,consolidated_earnings=income,capital_cost=0.))
    for charge,row,position in zip(lineage,rows,positions,strict=True):
        income=sum(x['amount']for x in charge['retained_earnings']);recurring=next(x['amount']for x in charge['retained_earnings']if x['channel']=='original_eir');profit=row['business_profit']+(income-recurring);total=income-position['operating_cost']
        accounting.append(dict(binding=charge['binding'],ordinal=charge['ordinal'],phase='earned_day-1',source_earnings=income,operating_cost=position['operating_cost'],full_external_earnings=total,business_profit=profit,treasury_profit=row['ftp_charge'],consolidated_earnings=profit+row['ftp_charge'],capital_cost=row['capital_cost']))
    source.compare(result['accounting_scope_reconciliation'],accounting,'seven lanes reconciled exactly once')
    for a,e in zip(result['accounting_scope_reconciliation'],accounting,strict=True):
        for field in ('binding','ordinal','phase'):source.exact(a[field],e[field],'accounting scope identity')
    for field in ('capital_metrics','capital_limits'):source.exact(result['treasury_result'][field],[],'FTP never changes capital')
    source.exact(result['retained_capacity_bound'],result['source_preparation']['retained_capacity_bound']+64*1024*1024+source.portfolio_funding.states(raw['source'])*4096+(64*1024*1024 if raw['capital']is not None else 0),'joined conservative capacity')
    if result['retained_capacity_bound']>384*1024*1024:raise ArithmeticError('joined retained capacity')
    charge_work=source.portfolio_funding.states(raw['source'])*1024+len(raw['curves'])*256*64
    phase_bound=sum(s['work_bound']for s in result['source_preparation']['sources']);source.exact(result['phase_work_bound'],phase_bound,'separate acquisition scan reservation')
    phase_work=128*len(paths)
    for item in result['source_preparation']['sources']:
        native=item['raw_native_result']
        if 'journal'in native:phase_work+=32*len(native['journal'])
        else:
            for m in native['ledger_messages']:
                if m.get('table')=='journal':phase_work+=32*len(m['columns']['value'])+sum(len(s.encode('utf-8'))//8+1 for s in m['dictionary'])
    source.exact(result['phase_work'],phase_work,'ordered whole journal acquisition scan charge')
    if type(result['phase_work'])is not int or not 0<result['phase_work']<=phase_bound:raise ArithmeticError('bounded actual phase scan')
    source.exact(result['charge_work_bound'],charge_work,'raw charge upper bound');source.exact(result['joined_work_bound'],result['source_preparation']['joined_work_bound']+phase_bound+charge_work+(10_000_000 if raw['capital']is not None else 0),'explicit additional native capital replay charge')
    if raw['capital']is not None:
        selected=next(s for s in raw['source']['sources']if s['source_id']==raw['capital_source_id']);source.exact(raw['capital']['raw_mixed'],selected['input']['request'],'capital separate raw binding')
        actual=next(s for s in result['source_preparation']['sources']if s['source_id']==selected['source_id'])['raw_native_result'];normalized,*_=financial.admit(selected['input']['request'],check)
        receipt=dict(schema='mortgage-mixed-preparation-1',raw_request=normalized,admission=actual['admission'],sources=actual['sources'],source_groups_sole_postings=True,memo_checkpoint_policy='native_complete_daily_boundary',**{k:False for k in mixed.PERMISSIONS})
        control=mixed.verify_preparation(selected['input']['request'],normalized['dated'],receipt,check);facade=source.Frames(actual['ledger_messages'],mt.SCHEMAS,mt.JSON_COLUMNS);facade.schemas=mt.SCHEMAS;verified=mixed.verify_stream(selected['input']['request'],control,facade,check)
        cc=capital.ClosingControl(control,verified,mixed.digest(verified),capital.SEAL)
        cap_request=deepcopy(raw['capital']);cap_request['raw_mixed']=deepcopy(control.raw);cap_request['policies']=[capital._financial_policy(p)for p in cap_request['policies']];cap_request['amount_multiplier']=financial_number(cap_request['amount_multiplier']);cap_request['treasury'].setdefault('strategy_units',[]);cap_request['treasury'].setdefault('capital_deltas',[])
        capital.verify_capital(cap_request,result['closing_capital'],cc,check)
    else:source.exact(result['closing_capital'],None,'no inferred capital')
    for flag in ('source_ftp_adoption','independent_persisted_verification','regulatory_approval','empirical_validation','interim_capital_eligible','capital_ftp_eligible','pricing_measure_option_valuation'):source.exact(result[flag],False,flag)
    source.exact(result['charge_methodology'],'reviewed_physical_scenario_incremental_cost-1','no inferred option valuation')
    source.probe(check)
    return dict(verified=True,raw_source_and_ordered_journal_verified=True,daily_charge_and_treasury_offsets_recomputed=True,external_nii_unchanged=True,public_ftp_adoption=False,regulatory_approval=False,empirical_validation=False)
