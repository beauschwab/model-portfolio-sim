"""Independent supplied-curve/index/swap analytical source verifier.

Test/reference only. Never a product execution backend or native callback.
"""
from copy import deepcopy
import copy
import json
import math
import hashlib
import unicodedata

SCHEMA='market-indexed-adapter-1'
BRIDGE='market-product-source-1'
MAX_AMOUNT=1e15
def df(curve,t):
    times=curve["times_years"];dfs=curve["discount_factors"]
    for i,x in enumerate(times):
        if t==x:return dfs[i],False
        if t<x:
            w=(t-times[i-1])/(x-times[i-1])
            # Independently evaluated documented log-linear curve policy.
            return math.exp(math.log(dfs[i-1])+(math.log(dfs[i])-math.log(dfs[i-1]))*w),False
    if curve["right_tail"]=="flat_zero":return math.exp(math.log(dfs[-1])*t/times[-1]),True
    if curve["right_tail"]=="flat_forward":return math.exp(math.log(dfs[-1])+(math.log(dfs[-1])-math.log(dfs[-2]))*(t-times[-1])/(times[-1]-times[-2])),True
    raise ValueError("tail")

def expected(request,market,c_shift=0.,i_shift=0.):
    c=next(c for c in market["curves"] if c["curve_id"]==request["binding"]["collateral_curve_id"])
    i=next(c for c in market["curves"] if c["curve_id"]==request["binding"]["index_curve_id"])
    known=request["contract"];side=1 if known["direction"]=="payer_fixed" else -1
    rows=[];pv=annuity=float_pv=0.
    for p in known["periods"]:
        t0=(p["start_ordinal"]-request["identity"]["as_of_ordinal"])/365.
        t1=(p["end_ordinal"]-request["identity"]["as_of_ordinal"])/365.
        tp=(p["payment_ordinal"]-request["identity"]["as_of_ordinal"])/365.
        ti=(p["end_ordinal"]-p["start_ordinal"])/(360. if known["index_day_count"]=="actual360" else 365.)
        tf=(p["end_ordinal"]-p["start_ordinal"])/(360. if known["fixed_day_count"]=="actual360" else 365.)
        d0,e0=df(i,t0);d1,e1=df(i,t1);dc,ec=df(c,tp)
        forward=math.expm1(math.log(d0*math.exp(-i_shift*t0))-math.log(d1*math.exp(-i_shift*t1)))/ti
        dc*=math.exp(-c_shift*tp)
        discount=dc*math.exp(-request["original"]["locked_oas"]*tp)
        fc=known["notional"]*(forward+known["margin"])*ti
        fixed=known["notional"]*known["fixed_rate"]*tf
        cash=side*(fc-fixed)
        rows.append(dict(index_forward=forward,index_accrual_fraction=ti,fixed_accrual_fraction=tf,
                         fixed_cash=-side*fixed,floating_cash=side*fc,net_cash=cash,
                         collateral_discount_factor=dc,oas_discount_factor=discount,discounted_cash=cash*discount,
                         index_tail_used=e0 or e1,collateral_tail_used=ec))
        pv+=cash*discount;annuity+=tf*discount;float_pv+=(forward+known["margin"])*ti*discount
    return dict(pv=pv,fixed_annuity=annuity,par_fixed_rate=float_pv/annuity,projections=rows)

def expected_dtos(r,market,scenario,values):
    b=r["bridge"];k=b["contract"];identity=lambda name,rev:dict(id=name,revision=rev)
    path=identity(scenario,market["identity"]["source_revision"])
    periods=[];fixed=[];points={b["identity"]["as_of_ordinal"]:1.}
    for p,val in zip(k["periods"],values["projections"]):
        source=dict(kind="conditional_projection",model=r["projection_model"],path=path,
                    information_unix_seconds=market["information_unix_seconds"],measure="pricing")
        observation=dict(interest_start_ordinal=p["start_ordinal"],interest_end_ordinal=p["end_ordinal"],
                         observation_start_ordinal=p["start_ordinal"],observation_end_ordinal=p["end_ordinal"],
                         fixing_ordinal=p["reset_ordinal"],selection_cutoff_unix_seconds=p["reset_cutoff_unix_seconds"],
                         rate=val["index_forward"],source=source)
        day=lambda name:"act360" if k[name]=="actual360" else "act365_fixed"
        coupon=dict(kind="indexed",index=dict(index_id=b["binding"]["index_id"],kind="term",tenor_months=r["index_tenor_months"],average_calendar_days=None),
                    calendar=r["calendar"],complete_calendar_mapping=True,calculation="term_simple",margin_treatment="simple",margin=k["margin"],
                    index_bounds=dict(floor=None,cap=None),all_in_bounds=dict(floor=None,cap=None),reset_cutoff_unix_seconds=p["reset_cutoff_unix_seconds"],
                    lookback_business_days=0,observation_shift=False,observations=[observation])
        basic=dict(period_id=p["period_id"],start_ordinal=p["start_ordinal"],end_ordinal=p["end_ordinal"],payment_ordinal=p["payment_ordinal"])
        periods.append(dict(**basic,day_count=day("index_day_count"),coupon=coupon,fee=0.))
        fixed.append(dict(**basic,day_count=day("fixed_day_count")))
        points[p["payment_ordinal"]]=val["collateral_discount_factor"]
    common=dict(contract=identity(k["contract_id"],k["contract_revision"]),position_id=k["contract_id"],scenario_id=scenario,
                currency=b["identity"]["currency"],monetary_unit="currency_units",asof_ordinal=b["identity"]["as_of_ordinal"],
                tail_end_ordinal=max(p["payment_ordinal"] for p in k["periods"]))
    indexed=dict(**common,schema="term-enhancements-1",model_version="term-contracts-1",assumption=r["index_basis"],path=path,
                 direction="receipt",measure="pricing",reporting_end_ordinal=b["identity"]["as_of_ordinal"],
                 observed_publication_cutoff_unix_seconds=market["information_unix_seconds"],opening_principal=k["notional"],
                 principal_plan=dict(kind="supplied",principal_payments=[0.]*(len(periods)-1)+[k["notional"]]),periods=periods)
    swap=dict(**common,index_basis=r["index_basis"],rate_path=path,valuation_cutoff_unix_seconds=market["information_unix_seconds"],
              payment_timing="after_scheduled_payments",side="payer" if k["direction"]=="payer_fixed" else "receiver",notional=k["notional"],
              fixed_rate=k["fixed_rate"],fixed_periods=fixed,floating_periods=periods,
              discount_curve=dict(source=identity(market["identity"]["source_id"],market["identity"]["source_revision"]),
                                  collateral_discount_policy=r["collateral_policy"],currency=b["identity"]["currency"],asof_ordinal=b["identity"]["as_of_ordinal"],
                                  information_unix_seconds=market["information_unix_seconds"],basis="collateralized_clean",
                                  points=[dict(ordinal=o,discount_factor=d) for o,d in sorted(points.items())]))
    return indexed,swap

# Appended to the proposed reference module; this file is artifact build input only.
def _check(check):
    if check():raise InterruptedError('market indexed source verification cancelled')

def _fields(v,names):
    if not isinstance(v,dict) or set(v)!=set(names.split()):raise ArithmeticError('market indexed raw fields differ')
def _text(v,n=128):
    if not isinstance(v,str) or not v.strip() or len(v.encode())>n or any(unicodedata.category(c)=='Cc' for c in v):raise ArithmeticError('bounded market indexed identity required')
def _int(v,lo,hi):
    if type(v) is not int or not lo<=v<=hi:raise ArithmeticError('market indexed integer/date domain')
def _float(v,lo,hi):
    if type(v) not in (int,float) or not math.isfinite(v) or not lo<=v<=hi:raise ArithmeticError('market indexed financial numeric domain')
    return float(v)
def _id(v):
    _fields(v,'id revision');_text(v['id']);_text(v['revision'])
def _size(v):
    if isinstance(v,dict):return 2+max(0,len(v)-1)+sum(_size(k)+1+_size(x) for k,x in v.items())
    if isinstance(v,list):return 2+max(0,len(v)-1)+sum(_size(x) for x in v)
    if isinstance(v,str):return len(json.dumps(v,ensure_ascii=False).encode())
    if type(v) is float:
        s=repr(v)
        if 'e' in s:
            mantissa,exp=s.split('e');n=int(exp)
            if -5<=n<0:
                sign='-' if mantissa.startswith('-') else '';digits=mantissa.lstrip('-').replace('.','')
                s=sign+'0.'+'0'*(-n-1)+digits
            else:s=mantissa+'e'+('-' if n<0 else '+')+str(abs(n))
        return len(s)
    return len(json.dumps(v,allow_nan=False,separators=(',',':')).encode())
def _amount(v):
    if not math.isfinite(v) or abs(v)>MAX_AMOUNT:raise ArithmeticError('native product monetary domain')
    return v

def admit(raw,check=lambda:False):
    _check(check)
    r=deepcopy(raw);_fields(r,'schema index_tenor_months calendar projection_model index_basis collateral_policy original_oas_review bridge')
    if r['schema']!=SCHEMA:raise ArithmeticError('market indexed schema mismatch')
    _int(r['index_tenor_months'],1,1200)
    for key in ['calendar','projection_model','index_basis','collateral_policy','original_oas_review']:_id(r[key])
    b=r['bridge'];_fields(b,'schema model_version identity measure projection_policy curve_time_policy binding contract original current_market risk_bump')
    if b['schema']!=BRIDGE or b['model_version']!=BRIDGE or b['measure']!='pricing' or b['projection_policy']!='deterministic_asof_simple_forward-1' or b['curve_time_policy']!='actual365_fixed_from_asof-1':raise ArithmeticError('unsupported bridge policy/measure')
    def identity(v):
        _fields(v,'source_id source_revision input_revision model_version currency as_of_ordinal')
        for k in ['source_id','source_revision','input_revision','currency']:_text(v[k],256)
        if v['model_version']!=BRIDGE:raise ArithmeticError('market source version')
        _int(v['as_of_ordinal'],719163,3652059)
    identity(b['identity']);asof=b['identity']['as_of_ordinal'];day=(asof-719163)*86400
    binding=b['binding'];_fields(binding,'index_id index_curve_id collateral_curve_id collateral_index calendar_identity complete_supplied_date_mapping')
    for k in ['index_id','index_curve_id','collateral_curve_id','collateral_index','calendar_identity']:_text(binding[k],256)
    if binding['complete_supplied_date_mapping'] is not True or binding['index_curve_id']==binding['collateral_curve_id']:raise ArithmeticError('explicit distinct complete mapping')
    original=b['original'];_fields(original,'review_identity locked_oas market');_text(original['review_identity'],256)
    original['locked_oas']=_float(original['locked_oas'],-1.,1.)
    if r['calendar']['id']!=binding['calendar_identity'] or r['projection_model']!={'id':b['projection_policy'],'revision':BRIDGE} or r['original_oas_review']['id']!=original['review_identity']:raise ArithmeticError('adapter review/calendar/model identity mismatch')
    k=b['contract'];_fields(k,'contract_id contract_revision maturity_ordinal direction monetary_unit notional fixed_rate margin fixed_day_count index_day_count periods')
    for key in ['contract_id','contract_revision']:_text(k[key])
    if k['direction'] not in ['payer_fixed','receiver_fixed'] or k['monetary_unit']!='actual_currency' or k['fixed_day_count'] not in ['actual360','actual365_fixed'] or k['index_day_count'] not in ['actual360','actual365_fixed']:raise ArithmeticError('contract policy')
    currency=b['identity']['currency']
    if len(currency)!=3 or not currency.isascii() or not currency.isupper() or not currency.isalpha():raise ArithmeticError('actual product currency')
    k['notional']=_float(k['notional'],1e-12,1e12);k['fixed_rate']=_float(k['fixed_rate'],-10.,10.);k['margin']=_float(k['margin'],-10.,10.)
    b['risk_bump']=_float(b['risk_bump'],1e-6,.05)
    if not isinstance(k['periods'],list) or not 1<=len(k['periods'])<=512:raise ArithmeticError('period cardinality')
    seen=set();prior=None
    for p in k['periods']:
        _check(check);_fields(p,'period_id start_ordinal end_ordinal payment_ordinal reset_ordinal reset_cutoff_unix_seconds');_text(p['period_id'])
        for key in ['start_ordinal','end_ordinal','payment_ordinal','reset_ordinal']:_int(p[key],1,3652059)
        _int(p['reset_cutoff_unix_seconds'],0,253402300799)
        reset=(p['reset_ordinal']-719163)*86400
        if p['period_id'] in seen or not asof<=p['reset_ordinal']<=p['start_ordinal']<p['end_ordinal']<=p['payment_ordinal']<=asof+36500 or prior is not None and prior!=p['start_ordinal'] or not reset<=p['reset_cutoff_unix_seconds']<reset+86400:raise ArithmeticError('dated reset/period/maturity domain')
        prior=p['end_ordinal'];seen.add(p['period_id'])
    _int(k['maturity_ordinal'],1,3652059)
    if prior!=k['maturity_ordinal']:raise ArithmeticError('complete maturity tail')
    nodes=0
    for m in [original['market'],b['current_market']]:
        _check(check);_fields(m,'identity available_unix_seconds information_unix_seconds curves');identity(m['identity'])
        for key in ['source_id','source_revision']:_text(m['identity'][key])
        if m['identity']['currency']!=currency or m['identity']['as_of_ordinal']!=asof:raise ArithmeticError('market source pair')
        _int(m['available_unix_seconds'],(1-719163)*86400,253402300799);_int(m['information_unix_seconds'],0,253402300799)
        if not m['available_unix_seconds']<=m['information_unix_seconds'] or not day<=m['information_unix_seconds']<day+86400:raise ArithmeticError('source unavailable')
        if not isinstance(m['curves'],list) or not 1<=len(m['curves'])<=16:raise ArithmeticError('curve count')
        ids={}
        for c in m['curves']:
            _check(check);_fields(c,'curve_id source_id source_revision currency role times_years discount_factors right_tail')
            for name in ['curve_id','source_id','source_revision','currency']:_text(c[name],256)
            role=c['role']
            if not isinstance(role,dict) or role.get('kind') not in ['collateral','index']:raise ArithmeticError('named role')
            _fields(role,'kind '+('collateral_index' if role['kind']=='collateral' else 'index_id'));_text(next(v for n,v in role.items() if n!='kind'),256)
            if c['currency']!=currency or c['curve_id'] in ids or c['right_tail'] not in ['reject','flat_zero','flat_forward']:raise ArithmeticError('curve currency/identity/tail')
            if not isinstance(c['times_years'],list) or not isinstance(c['discount_factors'],list) or not 2<=len(c['times_years'])<=4096 or len(c['times_years'])!=len(c['discount_factors']):raise ArithmeticError('curve nodes')
            c['times_years']=[_float(x,0.,100.) for x in c['times_years']];c['discount_factors']=[_float(x,1e-100,1e100) for x in c['discount_factors']]
            if c['times_years'][0]!=0. or c['discount_factors'][0]!=1. or any(a>=z for a,z in zip(c['times_years'],c['times_years'][1:])):raise ArithmeticError('curve anchor/order')
            ids[c['curve_id']]=c;nodes+=len(c['times_years'])
        if ids.get(binding['index_curve_id'],{}).get('role')!={'kind':'index','index_id':binding['index_id']} or ids.get(binding['collateral_curve_id'],{}).get('role')!={'kind':'collateral','collateral_index':binding['collateral_index']}:raise ArithmeticError('named binding')
        for p in k['periods']:
            if m['information_unix_seconds']>p['reset_cutoff_unix_seconds']:raise ArithmeticError('reset information unavailable')
            for curve,o in [(ids[binding['index_curve_id']],p['start_ordinal']),(ids[binding['index_curve_id']],p['end_ordinal']),(ids[binding['collateral_curve_id']],p['payment_ordinal'])]:df(curve,(o-asof)/365.)
    size=_size(r);bridge_size=_size(b);rows=8*len(k['periods']);work=4*nodes+320*rows
    retained=8*rows*(size+16384)+4*size+65536;bridge_retained=rows*(bridge_size+4096)+bridge_size+16384
    if size>1048576 or bridge_size>1048576 or work>2000000 or retained>33554432 or bridge_retained>8388608:raise ArithmeticError('aggregate capacity before generation')
    return r,dict(retained_bytes=retained,numerical_work=work,case_count=8),dict(input_bytes=bridge_size,retained_bytes=bridge_retained,admitted_work=4*nodes+64*rows,projection_rows=rows)

def _event(r,p,scenario,event_id,cash,principal,model_id,version,revision):
    return dict(position_id=r['bridge']['contract']['contract_id'],event_id=event_id,scenario=dict(kind='named',scenario_id=scenario),payment_ordinal=p['payment_ordinal'],day_offset=p['payment_ordinal']-r['bridge']['identity']['as_of_ordinal'],accrual_period=dict(start_ordinal=p['start_ordinal'],end_ordinal=p['end_ordinal']),currency=r['bridge']['identity']['currency'],components=dict(principal=principal,cash_interest=cash,accrued_interest=cash,book_amortization=0.,fees=0.),measure='pricing',provenance=dict(kind='modeled',model_id=model_id,model_version=version,input_revision=revision),adjustment=dict(kind='none'))

def reconstruct(raw,check=lambda:False):
    r,admission,bridge_admission=admit(raw,check);b=r['bridge'];k=b['contract'];n=k['notional'];asof=b['identity']['as_of_ordinal'];bump=b['risk_bump']
    definitions=[('original',b['original']['market'],0.,0.),('current',b['current_market'],0.,0.)]
    for domain,c,i in [('collateral',bump,0.),('index',0.,bump),('parallel',bump,bump)]:definitions.extend([(domain+'_down',b['current_market'],-c,-i),(domain+'_up',b['current_market'],c,i)])
    valuations=[];cases=[]
    for scenario,market,c,i in definitions:
        _check(check);v=expected(b,market,c,i);indexed,swap=expected_dtos(r,market,scenario,v)
        bridge_rows=[];term_events=[];audit=[];fixed_events=[];float_events=[];fixedpv=floatpv=annuity=spread=0.
        for j,(p,o) in enumerate(zip(k['periods'],v['projections'])):
            _check(check);float_cash=n*(o['index_forward']+k['margin'])*o['index_accrual_fraction'];_amount(float_cash)
            if (o['index_forward']+k['margin'])*o['index_accrual_fraction']<=-1.:raise ArithmeticError('complete floating coupon factor')
            end=j+1==len(k['periods']);principal=n if end else 0.
            term_events.append(_event(r,p,scenario,p['period_id'],float_cash,principal,r['index_basis']['id'],'term-contracts-1',r['index_basis']['revision']))
            audit.append(dict(period_id=p['period_id'],opening_principal=n,interest=float_cash,principal_paid=principal,closing_principal=0. if end else n,terminal_balloon=0.,calendar_mapping_verified=False,selected_rate_sources=[indexed['periods'][j]['coupon']['observations'][0]['source']]))
            fixed_events.append(_event(r,p,scenario,'fixed:'+p['period_id'],o['fixed_cash'],0.,'derivative-clean-swap','derivative-contracts-1',k['contract_revision']))
            float_events.append(_event(r,p,scenario,'floating:'+p['period_id'],o['floating_cash'],0.,r['index_basis']['id'],'term-contracts-1',r['index_basis']['revision']))
            dc=o['collateral_discount_factor'];fixedpv=_amount(fixedpv+n*k['fixed_rate']*o['fixed_accrual_fraction']*dc);floatpv=_amount(floatpv+float_cash*dc);annuity=_amount(annuity+n*o['fixed_accrual_fraction']*dc)
            row=deepcopy(p);row['scenario_id']=scenario
            row.update(o);row.update(source_kind='conditional_projection',information_unix_seconds=market['information_unix_seconds'],source=market['identity'],index_id=b['binding']['index_id'],calendar_mapping_verified=False)
            bridge_rows.append(row)
        valuation=dict(scenario_id=scenario,pv=v['pv'],par_fixed_rate=v['par_fixed_rate'],fixed_annuity=v['fixed_annuity'],projections=bridge_rows);valuations.append(valuation)
        tail=swap['tail_end_ordinal'];schedule=lambda events:dict(schema='dated-cashflow-1',as_of_ordinal=asof,horizon_days=tail-asof,scenarios=[scenario],events=sorted(events,key=lambda event:event['payment_ordinal']))
        term=dict(schema='term-enhancements-1',model_version='term-contracts-1',contract=indexed['contract'],assumption=indexed['assumption'],path=indexed['path'],schedule=schedule(term_events),audit=audit,principal_after_reporting=n,disclosure='Explicit supplied conditional index path, not calibrated projection. Calendar identities/lookbacks and interval coverage retained; business-day calendar mapping is supplied and not independently verified. No default, prepayment, option exercise, OAS solve or book amortization. Complete maturity tail retained beyond report horizon.')
        clean=_amount((1. if k['direction']=='receiver_fixed' else -1.)*(fixedpv-floatpv))
        swap_result=dict(position_id=k['contract_id'],contract=swap['contract'],index_basis=swap['index_basis'],rate_path=swap['rate_path'],collateral_discount_source=swap['discount_curve']['source'],fixed_leg_pv=fixedpv,floating_leg_pv=floatpv,fixed_annuity=annuity,par_rate=floatpv/annuity,clean_value=clean,schedule=schedule(fixed_events+float_events),floating_audit=audit,disclosure='Clean constant-notional swap: actual dated fixed/indexed legs and separately supplied collateral discount/index basis. Projected coupon information cannot follow valuation cutoff. No notional exchange, OAS recalibration, XVA, accounting designation, curve interpolation or future rate-path generation inferred. Same-day paid coupons follow explicit before/after timing; indexed calendar mapping is supplied/unverified.')
        points={p['ordinal']:p['discount_factor'] for p in swap['discount_curve']['points']};cashclean=0.
        for event in swap_result['schedule']['events']:
            _check(check);cash=event['components']['cash_interest'];dc=points[event['payment_ordinal']];cashclean=_amount(cashclean+cash*dc);spread+=cash*dc*math.exp(-b['original']['locked_oas']*(event['payment_ordinal']-asof)/365.)
        coupons=[]
        for period in swap['floating_periods']:
            coupon=deepcopy(period['coupon']);coupon.pop('observations');coupons.append([period['period_id'],period['start_ordinal'],period['end_ordinal'],period['payment_ordinal'],period['day_count'],coupon])
        descriptor=[swap['contract'],swap['index_basis'],swap['currency'],swap['monetary_unit'],swap['side'],n,k['fixed_rate'],[[p['period_id'],p['start_ordinal'],p['end_ordinal'],p['payment_ordinal'],p['day_count']] for p in swap['fixed_periods']],coupons]
        cases.append(dict(scenario_id=scenario,rate_source=market['identity'],indexed_request=indexed,indexed_result=term,swap_request=swap,swap_result=swap_result,original_locked_oas=b['original']['locked_oas'],original_oas_review=r['original_oas_review'],spread_adjusted_swap_pv=spread,original_parent=b['original'],native_contract_descriptor=json.dumps(descriptor,separators=(',',':'),sort_keys=True)))
    risks=[]
    for j,domain in enumerate(['collateral','index','parallel']):
        down,up=valuations[2+j*2:4+j*2];risks.append(dict(domain=domain,down=down,up=up,dollar_pv01=(down['pv']-up['pv'])*.0001/(2*bump)))
    bridge_result=dict(schema=BRIDGE,model_version=BRIDGE,measure='pricing',source_status='supplied_curves_deterministic_asof_projection',cache_parent=b,admission=bridge_admission,original=valuations[0],current=valuations[1],risks=risks,observed_fixings=False,stochastic_dynamics=False,smile_adoption=False,physical_cash=False,production_adoption=False,calendar_mapping_verified=False,paired_history_policy='same_contract_dates_deterministic_bumps_no_random_draws-1')
    return dict(schema=SCHEMA,cache_parent=r,admission=admission,bridge_receipt=bridge_result,cases=cases,production_adoption=False,observed_fixings=False,physical_accounting=False,joint_dynamics=False,calendar_mapping_verified=False,accounting_adoption=False,capital_ftp_eligible=False,empirical_validation=False)

def _equal(actual,expect,check,path='result'):
    _check(check)
    if isinstance(expect,dict):
        if not isinstance(actual,dict) or set(actual)!=set(expect):raise ArithmeticError(path+' fields differ')
        for k,v in expect.items():_equal(actual[k],v,check,path+'.'+k)
    elif isinstance(expect,list):
        if not isinstance(actual,list) or len(actual)!=len(expect):raise ArithmeticError(path+' length differs')
        for j,(a,b) in enumerate(zip(actual,expect)):_equal(a,b,check,f'{path}[{j}]')
    elif type(expect) is float:
        if type(actual) not in (int,float) or not math.isfinite(actual) or not math.isclose(actual,expect,abs_tol=1e-9,rel_tol=1e-12):raise ArithmeticError(path+' financial amount differs')
    elif path.endswith('.native_contract_descriptor'):
        _source_equal(json.loads(actual),json.loads(expect),check,path+'.parsed')
    elif type(actual) is not type(expect) or actual!=expect:raise ArithmeticError(path+' identity/date/policy differs')

def _source_equal(actual,expected,check,path):
    """Exact typed identity for retained raw/source fields, never a financial tolerance."""
    _check(check)
    if type(actual) is not type(expected):raise ArithmeticError(path+' source type differs')
    if isinstance(expected,dict):
        if set(actual)!=set(expected):raise ArithmeticError(path+' source fields differ')
        for key,value in expected.items():_source_equal(actual[key],value,check,path+'.'+key)
    elif isinstance(expected,list):
        if len(actual)!=len(expected):raise ArithmeticError(path+' source length differs')
        for i,(left,right) in enumerate(zip(actual,expected)):_source_equal(left,right,check,f'{path}[{i}]')
    elif actual!=expected or (isinstance(expected,float) and expected==0.0 and math.copysign(1.0,actual)!=math.copysign(1.0,expected)):
        raise ArithmeticError(path+' source identity differs')

def verify(raw,result,check=lambda:False):
    expected_result=reconstruct(raw,check)
    _source_equal(result.get('cache_parent'),expected_result['cache_parent'],check,'result.cache_parent')
    _source_equal(result['bridge_receipt']['cache_parent'],expected_result['bridge_receipt']['cache_parent'],check,'result.bridge_receipt.cache_parent')
    if len(result['cases'])!=len(expected_result['cases']):raise ArithmeticError('case count differs')
    for i,(case,expected) in enumerate(zip(result['cases'],expected_result['cases'])):
        for field in ['original_parent','original_locked_oas','original_oas_review','rate_source']:
            _source_equal(case[field],expected[field],check,f'result.cases[{i}].{field}')
    _equal(result,expected_result,check)
    _check(check)
    return dict(verified=True,analytical_market_indexed_only=True,raw_named_curves_recomputed=True,actual_product_cash_recomputed=True,original_locked_oas_preserved=True,calendar_mapping_verified=False,observed_fixings=False,joint_dynamics=False,physical_accounting=False,accounting_adoption=False,production_adoption=False,capital_ftp_eligible=False,empirical_validation=False,persisted_journal_verified=False)
