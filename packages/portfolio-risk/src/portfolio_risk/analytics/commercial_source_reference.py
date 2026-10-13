"""Independent supplied commercial coupon mathematics for publication only.

This module never dispatches native financial code. Dates and inputs come from
the retained raw contract; producer cash, accumulator and entitlement are checked.
"""
from copy import deepcopy
from datetime import date
import calendar
import math


def compare(expected, actual, path='$', check=lambda: False):
    check()
    if isinstance(expected, dict):
        if type(actual) is not dict or expected.keys() != actual.keys():
            raise ArithmeticError(f'{path}: source shape differs')
        for key in expected: compare(expected[key], actual[key], f'{path}.{key}', check)
    elif isinstance(expected, list):
        if type(actual) is not list or len(expected) != len(actual):
            raise ArithmeticError(f'{path}: source count differs')
        for i, (left, right) in enumerate(zip(expected, actual)): compare(left, right, f'{path}[{i}]', check)
    elif type(expected) is float:
        clock=path.rsplit('.',1)[-1] in ('year','end_year','start_year','native_credit_year','default_year','full_credit_end_year')
        dimensionless=clock or path.rsplit('.',1)[-1] in ('delta_factor','full_factor','source_weight','allocation_factor','accounting_rate','accrual_rate','probability','weight')
        limit=max(1e-14,1e-12*max(abs(expected),abs(actual))) if dimensionless and type(actual) in (float,int) else max(1e-9,64*math.ulp(1.)*max(abs(expected),abs(actual))) if type(actual) in (float,int) else 0.
        if type(actual) not in (float, int) or not math.isfinite(actual) or abs(expected-actual)>limit:
            raise ArithmeticError(f'{path}: calculated amount differs')
    elif type(actual) is not type(expected) or expected != actual:
        raise ArithmeticError(f'{path}: source identity differs')


def tau(start, end, basis):
    if basis == 'act360': return (end-start)/360.
    if basis == 'act365_fixed': return (end-start)/365.
    if basis != 'act_act_isda': raise ValueError('unsupported independent day count')
    total=0.
    while start<end:
        year=date.fromordinal(int(start)).year
        limit=min(end,float(date(year+1,1,1).toordinal())) if year<9999 else end
        total+=(limit-start)/(366. if calendar.isleap(year) else 365.)
        start=limit
    return total


def bounded(rate, limits):
    if limits.get('floor') is not None: rate=max(rate,limits['floor'])
    if limits.get('cap') is not None: rate=min(rate,limits['cap'])
    return rate


def validate_native_availability(raw,check=lambda:False):
    """Scoped midnight actual-cash/continuous-factor ownership admission.

    Pure supplied indexed arithmetic remains separately usable through project;
    this guard applies to the source owner adopted by commercial cash/books.
    """
    midnight=lambda ordinal:(ordinal-719163)*86400
    retained=raw['principal_plan']['kind']=='retained_payment';entitlement_known=midnight(raw['asof_ordinal'])
    for period in raw['periods']:
        check();coupon=period['coupon']
        if retained and midnight(period['payment_ordinal'])<entitlement_known:raise ArithmeticError('native retained-payment cash precedes inherited indexed entitlement availability')
        if coupon['kind']=='fixed':continue
        reset=coupon.get('reset_cutoff_unix_seconds')
        if reset is not None and type(reset)is not int:raise ArithmeticError('native reset cutoff integer required')
        for observation in coupon['observations']:
            check();source=observation['source'];kind=source.get('kind')
            availability=source.get('publication_unix_seconds')if kind=='observed'else source.get('information_unix_seconds')if kind=='conditional_projection'else None
            cutoff=observation['selection_cutoff_unix_seconds'];fixing=observation['fixing_ordinal']
            if any(type(v)is not int for v in (availability,cutoff,fixing)):raise ArithmeticError('native fixing availability integer authority required')
            if fixing>period['payment_ordinal']or availability>midnight(period['payment_ordinal'])or cutoff>midnight(period['payment_ordinal']):raise ArithmeticError('native actual cash precedes indexed fixing availability')
            if availability>midnight(observation['interest_start_ordinal'])or cutoff>midnight(observation['interest_start_ordinal'])or(reset is not None and reset>midnight(period['start_ordinal'])):raise ArithmeticError('native continuous source unavailable at interest-segment start midnight')
            if retained:entitlement_known=max(entitlement_known,availability,cutoff,midnight(fixing),reset if reset is not None else entitlement_known)


def segments(period, audit_index):
    c=period['coupon'];result=[]
    if c['kind']=='fixed':
        start,end=period['start_ordinal'],period['end_ordinal']
        return [dict(period_id=period['period_id'],native_source_event_id=period['period_id'],native_audit_index=audit_index,
            interest_start_ordinal=start,interest_end_ordinal=end,observation_start_ordinal=start,observation_end_ordinal=end,
            day_count=period['day_count'],observation_shift=False,full_factor=float(c['annual_rate'])*tau(start,end,period['day_count']),
            source=None,native_factor_kind='fixed',selected_all_in=None,margin=None,calculation=None,margin_treatment=None,
            before_accumulator=None,fixed_annual_rate=float(c['annual_rate']))]
    simple,product,margin_time=0.,1.,0.
    for o in c['observations']:
        all_in=bounded(bounded(o['rate'],c['index_bounds'])+c['margin'],c['all_in_bounds'])
        before=[simple,math.log(product),margin_time]
        it=tau(o['interest_start_ordinal'],o['interest_end_ordinal'],period['day_count'])
        weight=tau(o['observation_start_ordinal'],o['observation_end_ordinal'],period['day_count']) if c['observation_shift'] else it
        if c['calculation']=='daily_compounded':
            rate=all_in if c['margin_treatment']=='compounded' else all_in-c['margin']
            product*=1.+rate*weight;margin_time+=it
            factor=product-1.+(c['margin']*margin_time if c['margin_treatment']=='simple' else 0.)
        else:
            simple+=(all_in-c['margin'])*weight+c['margin']*it;factor=simple
        result.append(dict(period_id=period['period_id'],native_source_event_id=period['period_id'],native_audit_index=audit_index,
            interest_start_ordinal=o['interest_start_ordinal'],interest_end_ordinal=o['interest_end_ordinal'],
            observation_start_ordinal=o['observation_start_ordinal'],observation_end_ordinal=o['observation_end_ordinal'],
            day_count=period['day_count'],observation_shift=c['observation_shift'],full_factor=factor,source=deepcopy(o['source']),
            native_factor_kind='indexed',selected_all_in=float(all_in),margin=float(c['margin']),calculation=c['calculation'],
            margin_treatment=c['margin_treatment'],before_accumulator=before,fixed_annual_rate=None))
    return result


def factor(segment, ordinal, fraction=0.):
    left,right=segment['interest_start_ordinal'],segment['interest_end_ordinal']
    ordinal=min(max(ordinal,left),right)
    if ordinal==right:return segment['full_factor']
    denominator=360. if segment['day_count']=='act360' else 366. if segment['day_count']=='act_act_isda' and calendar.isleap(date.fromordinal(ordinal).year) else 365.
    it=tau(left,ordinal,segment['day_count'])+fraction/denominator
    if segment['native_factor_kind']=='fixed':return segment['fixed_annual_rate']*it
    observation=segment['observation_start_ordinal']+ordinal-left
    weight=tau(segment['observation_start_ordinal'],observation,segment['day_count'])+fraction/denominator if segment['observation_shift'] else it
    simple,log_product,margin_time=segment['before_accumulator'];product=math.exp(log_product);margin=segment['margin'];all_in=segment['selected_all_in']
    if segment['calculation']=='daily_compounded':
        rate=all_in if segment['margin_treatment']=='compounded' else all_in-margin
        return product*(1.+rate*weight)-1.+(margin*(margin_time+it) if segment['margin_treatment']=='simple' else 0.)
    return simple+(all_in-margin)*weight+margin*it


def project(raw, check=lambda:False):
    """Full schedule, period entitlement and continuous factor descriptors."""
    balance=float(raw['opening_principal']);after=balance;events=[];audits=[];all_segments=[]
    sign=1. if raw['direction']=='receipt' else -1.
    for i,p in enumerate(raw['periods']):
        check();ss=segments(p,i);all_segments.extend(ss);interest=balance*ss[-1]['full_factor']
        balloon=0.;plan=raw['principal_plan']
        if plan['kind']=='supplied':principal=float(plan['principal_payments'][i])
        else:
            regular=plan['original_payment']-interest
            if i+1==len(raw['periods']):principal=balance;balloon=max(balance-regular,0.)
            else:principal=regular
        principal=min(principal,balance);closing=balance-principal
        audits.append(dict(period_id=p['period_id'],opening_principal=balance,interest=interest,principal_paid=principal,
            closing_principal=closing,terminal_balloon=balloon,calendar_mapping_verified=p['coupon']['kind']=='fixed',
            selected_rate_sources=[] if p['coupon']['kind']=='fixed' else [deepcopy(o['source']) for o in p['coupon']['observations']]))
        events.append(dict(position_id=raw['position_id'],event_id=p['period_id'],scenario=dict(kind='named',scenario_id=raw['scenario_id']),
            payment_ordinal=p['payment_ordinal'],day_offset=p['payment_ordinal']-raw['asof_ordinal'],
            accrual_period=dict(start_ordinal=p['start_ordinal'],end_ordinal=p['end_ordinal']),currency=raw['currency'],
            components=dict(principal=sign*principal,cash_interest=sign*interest,accrued_interest=sign*interest,book_amortization=0.,fees=sign*float(p['fee'])),
            measure=raw['measure'],provenance=dict(kind='modeled',model_id=raw['assumption']['id'],model_version='term-contracts-1',input_revision=raw['assumption']['revision']),
            adjustment=dict(kind='none')))
        if p['payment_ordinal']<=raw['reporting_end_ordinal']:after-=principal
        balance=closing
    events.sort(key=lambda e:e['payment_ordinal'])
    result=dict(schema='term-enhancements-1',model_version='term-contracts-1',contract=deepcopy(raw['contract']),assumption=deepcopy(raw['assumption']),path=deepcopy(raw['path']),
        schedule=dict(schema='dated-cashflow-1',as_of_ordinal=raw['asof_ordinal'],horizon_days=raw['tail_end_ordinal']-raw['asof_ordinal'],scenarios=[raw['scenario_id']],events=events),
        audit=audits,principal_after_reporting=max(after,0.))
    return result,all_segments


def verify_source(raw, receipt, check=lambda:False):
    validate_native_availability(raw,check)
    expected,ss=project(raw,check)
    got=deepcopy(receipt['source']);disclosure=got.pop('disclosure',None)
    if type(disclosure) is not str or not disclosure:raise ArithmeticError('source disclosure missing')
    compare(expected,got,check=check);compare(ss,receipt['segments'],check=check)
    return expected,ss
