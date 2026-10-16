"""Independent finite complete-tail census and observed row inventory.

Raw contracts and complete supplied tails determine every upper; native monetary
checkpoints never determine capacity. This is explicit verification only.
"""
import json
from .commercial_factor_work_reference import FINITE_POLICY,summary
from .commercial_source_reference import observable_policy

def structural(value):
    if value is None:return 4
    if type(value)is bool:return 5
    if type(value)is int:return len(str(value))
    if type(value)is float:return 4 if value==0. else 8 if -10.<=value<=10. and round(value*10.)/10.==value else 32
    if isinstance(value,str):return len(json.dumps(value,ensure_ascii=False,separators=(',',':')).encode())
    if isinstance(value,list):return 2+sum(1+structural(v)for v in value)
    if isinstance(value,dict):return 2+sum(2+len(json.dumps(k,ensure_ascii=False).encode())+(32 if k in("accrued_interest","book_adjustment")else structural(v))for k,v in value.items())
    raise ArithmeticError('unsupported finite tail structural cell')

def census(raw,prepared,expected):
    path=prepared['raw_credit_path'];n=len(path['paths']);i=len(path['intervals']);b=len(path['boundaries']);j=path['max_jumps_per_path'];s=i+b+j+1
    tails=[len(t)for interval in path['intervals']for t in interval['complete_recovery_tails']]
    if not tails or min(tails)<1 or max(tails)>32:raise ArithmeticError('complete finite1..32 tails required')
    r=max(tails);source=raw['source']['current'];p=len(source['periods']);m=summary(source,0,observable_policy(raw['factor_policy']))['upper']['segments'];t=m if raw['factor_policy'].get('terminal_obligations')else 0
    c=p+len(source['principal_plan'].get('principal_events',[]));l=4*p+t;base=s+b+j+4
    per=dict(points=base+r+t*(1+r),contributions=s*m+l+t,scheduled_source_cash=c,terminal_source_cash=t*r,recoveries=r,coupon_recoveries=l*r,terminal_path_events=t*r)
    rows=n*(base+2*r+c+l*(1+r)+t*(2+2*r)+s*m)
    request_bytes=structural(prepared);binding=4*(request_bytes+4096);unit=max(8192,2*(512+p*1024+m*32));points=n*per['points'];events=t*(1+r)
    temporary=m*4096+l*4096+p*4096+t*8192+events*256
    rebuild=(points+3)*unit if t else 0
    trace=n*(s*512+(b+j)*2048+r*256+t*r*2048)
    body=binding+points*(unit-8192)+temporary+rebuild+trace
    total_cash=n*(c+t*r);canonical=points*p*(total_cash+64);defaults=n*((s*m+l)*(l+1)+c*(b+j+l+1));filters=n*(n*per['contributions']+total_cash)
    work=request_bytes*16+(canonical+defaults+filters)*64+n*events*(max(events,1).bit_length()+1)*16
    terminal=sum(e['kind']=='pending_coupon_cash'for p in expected['credit']['result']['paths']for e in p['events'])
    observed=dict(points=len(expected['carry_points']),contributions=len(expected['earned_origin_contributions']),scheduled_source_cash=len(expected['native_source_cash'])-terminal,terminal_source_cash=terminal,recoveries=len(expected['complete_recovery_receipts']),coupon_recoveries=len(expected['native_coupon_recoveries']),terminal_path_events=terminal)
    for name,value in observed.items():
        if not 0<=value<=n*per[name]:raise ArithmeticError('independent finite tail category overrun')
    return dict(policy=FINITE_POLICY,measure=path['measure'],maximum_recoveries=r,intervals=i,boundaries=b,maximum_jumps=j,paths=n,maximum_atoms=m,maximum_terminal_steps=t,source_cash_per_path=c,gross_coupon_claims_per_path=l,source_shape=dict(canonical_coupons=p,pending_indices=m,draw_tranches=0),per_path=per,row_upper=rows,request_bytes_upper=request_bytes,binding_capacity_upper=binding,additional_work_upper=work,additional_body_bytes_upper=body,observed=observed)

def verify(raw,actual,expected,check=lambda:False):
    check();selected=(raw['source'].get('transfer')or{}).get('source_work_policy')
    carry=actual['carry'];got=carry.get('tail_census')
    if selected is None:
        if actual.get('selected_accounting_owner')is not None:raise ArithmeticError('unselected accounting phase receipt')
        if got is not None:raise ArithmeticError('unselected finite tail census receipt')
        return
    if selected!=FINITE_POLICY:raise ArithmeticError('unknown finite tail policy')
    wanted=census(raw,expected['prepared'],expected)
    full='accounting'in actual
    if actual.get('selected_accounting_owner')!=('reviewed_coupon_book-1'if full else'economic_carry_only-1'):raise ArithmeticError('selected accounting owner differs')
    if not full and (actual['admission']['book_work_upper']!=0 or actual['admission']['book_bytes_upper']!=0):raise ArithmeticError('economic carry reserved an absent accounting Book')
    def exact(a,b):
        if type(a)is not type(b):return False
        if isinstance(a,dict):return a.keys()==b.keys()and all(exact(v,b[k])for k,v in a.items())
        return a==b
    if not exact(wanted,got):raise ArithmeticError('independent finite tail census differs')
    admitted=carry['admission'];source=actual['current_source']['admission']
    if admitted['row_upper']!=wanted['row_upper']or admitted['work_upper']!=4*admitted['raw_bytes']+128*wanted['row_upper']+source['work_upper']+wanted['additional_work_upper']or admitted['retained_bytes_upper']!=20*admitted['raw_bytes']+8192*wanted['row_upper']+source['retained_bytes_upper']+wanted['additional_body_bytes_upper']:
        raise ArithmeticError('selected finite carry admission differs')
