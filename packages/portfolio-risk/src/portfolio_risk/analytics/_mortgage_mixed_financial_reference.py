"""Artifact-only independent immutable-source and pooled mortgage GL oracle.

No production DLL, pricing callback or producer posting enters reconstruction.
Frozen public-pending SciPy/source controls reconstruct each raw registry entry.
This file is a review oracle, never an execution backend or adoption receipt.
"""
from collections import defaultdict
from copy import deepcopy
from pathlib import Path
import json
import math
import sys
from fractions import Fraction

from . import _mortgage_pending_reference as pending
from portfolio_risk.analytics import product_mixed_reference as dated
from portfolio_risk.analytics import mortgage_credit_reference as source
from portfolio_risk.analytics import mortgage_credit_math_reference as domain

SCHEMA = 'mortgage-mixed-accounting-1'
MAX_WORK = 8_000_000
MAX_BYTES = 32*1024*1024
MAX_GROSS_MONEY = 10000.
POLICY = dict(phase='native_pending_groups_before_dated_settlement-1',
              ownership='exclusive_native_seven_slot_owner-1',
              tail='complete_unweighted_tail-1',
              source_usage='explicit_immutable_source_registry-1',
              money_domain='combined_gross_state_10k-1')


def check_cancel(check):
    domain.checkpoint(check)


def equal(actual, expected, path='mixed'):
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise ArithmeticError(path+': exact fields differ')
        for key, value in expected.items():
            equal(actual[key], value, path+'/'+key)
    elif isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise ArithmeticError(path+': complete ordered rows differ')
        for i, value in enumerate(expected):
            equal(actual[i], value, path+'/'+str(i))
    elif type(expected) is bool or type(expected) is int or expected is None or isinstance(expected, str):
        if type(actual) is not type(expected) or actual != expected:
            raise ArithmeticError(path+': exact metadata differs')
    else:
        near(actual, expected, path)


def near(a, b, path='conservation'):
    if (type(a) not in (int, float) or type(b) not in (int, float)
            or not math.isfinite(a) or not math.isfinite(b)
            or abs(a-b)>1e-9+1e-12*max(abs(a), abs(b))):
        raise ArithmeticError(path+': finite financial value differs')


def exact(actual,expected,path='immutable'):
    """Typed input/source identities have no financial tolerance."""
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():
            raise ArithmeticError(path+': exact immutable fields differ')
        for key,value in expected.items():exact(actual[key],value,path+'/'+key)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):
            raise ArithmeticError(path+': exact immutable sequence differs')
        for i,value in enumerate(expected):exact(actual[i],value,path+'/'+str(i))
    elif type(actual) is not type(expected) or actual!=expected or (
            type(expected) is float and (not math.isfinite(actual) or actual.hex()!=expected.hex())):
        raise ArithmeticError(path+': exact immutable value differs')


def keys(v, names):
    domain.fields(v, names if isinstance(names,str) else ' '.join(names))


def size(v):
    return len(source.canonical_bytes(v))


def parse_raw(text, max_bytes=1024*1024):
    """Strict caller byte-domain counterpart; duplicate fields never collapse."""
    if type(max_bytes) is not int or not 0<max_bytes<=MAX_BYTES or not isinstance(text,str) or len(text.encode())>max_bytes:
        raise ArithmeticError('bounded raw input bytes')
    def pairs(items):
        result={}
        for k,v in items:
            if k in result: raise ArithmeticError('duplicate raw field '+k)
            result[k]=v
        return result
    return json.loads(text,object_pairs_hook=pairs,parse_constant=lambda v:(_ for _ in ()).throw(ArithmeticError('finite raw JSON')))


def output_size_guard(v,check):
    """Reject excessive retained output before source/replay traversal."""
    count=0
    for i,chunk in enumerate(json.JSONEncoder(ensure_ascii=False,allow_nan=False,separators=(',',':')).iterencode(v)):
        if i%256==0:check_cancel(check)
        count+=len(chunk.encode())
        if count>MAX_BYTES:raise ArithmeticError('complete serialized output bound')
    return count


def closed(book):
    for ch in book['slots']+[x['book'] for x in book['claims']]:
        for amount in ch.values():
            near(amount, 0., 'complete unweighted channel')
    for amount in book['borrower_principal']+book['borrower_contract_interest']:
        near(amount, 0., 'complete gross borrower memo')
    for claim in book['claims']:
        near(claim['economic_principal'], 0., 'complete principal claim')
        near(claim['economic_contract_interest'], 0., 'complete interest claim')


def money_gross(values, check=lambda:False):
    """Additional schema domain: no netting, clipping or tolerance."""
    total=Fraction(0)
    for value in values:
        check_cancel(check)
        if type(value) not in (int,float) or not math.isfinite(value) or abs(value)>MAX_GROSS_MONEY:
            raise ArithmeticError('combined10k monetary component domain')
        total+=Fraction.from_float(abs(float(value)))
        if total>10000:
            raise ArithmeticError('combined10k gross monetary domain')
    return float(total)


def money_book(book, check=lambda:False, max_claims=48):
    if type(max_claims) is not int or not 0<=max_claims<=48 or len(book['claims'])>max_claims:
        raise ArithmeticError('admitted origin-claim cardinality')
    owned=[v for channel in book['slots'] for v in channel.values()]
    owned += [v for claim in book['claims'] for v in claim['book'].values()]
    owned += [book['cash'],book['original_eir_income'],book['reviewed_income']]
    money_gross(owned,check)
    money_gross(book['borrower_principal']+book['borrower_contract_interest'],check)
    money_gross([claim[k] for claim in book['claims'] for k in
                 ('economic_principal','economic_contract_interest')],check)


def money_inputs(r,spec,events,check):
    values=[a[k] for a in spec['accounts'] for k in ('cash','equity','cash_floor','capital_deductions')]
    values += [p[k] for p in spec['positions'] for k in
               ('balance','book_adjustment','opening_accrued','allowance','commitment','pledged_face')]
    values += [e['event']['components'][k] for e in events for k in
               ('principal','cash_interest','accrued_interest','book_amortization','fees')]
    values += [s[k] for plan in r['dated']['dated']['accrual_plans'] for s in plan['segments']
               for k in ('accrued_interest','book_amortization')]
    for item in r['sources']:
        source_=item['request']['source']
        values += source_['contract']['opening_investor_principal']+[source_['acquisition']['actual_cost']]
    return money_gross(values,check)


def wire_domains(raw):
    """New wire enums require strings; singleton object aliases are rejected."""
    def string(value,path):
        if not isinstance(value,str):raise ArithmeticError(path+': enum string required')
    for item in raw['sources']:
        source_=item['request']['source']
        for field in ('contract','original_contract'):
            c=source_[field]
            for value,path in ((c['identity']['measure'],'identity.measure'),(c['day_count'],'day_count'),
                (c['rate']['kind'],'rate.kind'),(c['ownership']['kind'],'ownership.kind')):
                string(value,field+'.'+path)
            if c['rate']['kind']=='arm':
                terms=c['rate']['terms'];string(terms['index']['kind'],field+'.rate.terms.index.kind')
                string(terms['payment_recast'],field+'.rate.terms.payment_recast')
            if c['ownership']['kind']=='agency_pass_through':
                string(c['ownership']['default_coupon_policy'],field+'.ownership.default_coupon_policy')
        interest=source_['policy']['interest']
        for field in ('accounting_accrued','economic_remainder'):string(interest[field],'policy.interest.'+field)
    for owner in raw['dated']['dated']['owners']:
        string(owner['direction'],'dated.owner.direction');string(owner['ownership'],'dated.owner.ownership')
    string(raw['dated']['recognition_policy']['fees'],'dated.recognition_policy.fees')
    for source_ in raw['dated']['dated']['sources']:
        for event in source_['schedule']['events']:string(event['measure'],'dated.event.measure')


def admit(raw, check=lambda:False):
    """All source and every binding work are charged before projection."""
    check_cancel(check)
    keys(raw, 'schema dated sources owners reporting_end_ordinal policy')
    keys(raw['policy'], 'identity '+' '.join(POLICY))
    domain.identity(raw['policy']['identity'])
    if raw['schema']!=SCHEMA or any(raw['policy'][k]!=v for k,v in POLICY.items()):
        raise ArithmeticError('explicit immutable seven-slot source policy')
    if size(raw)>1024*1024 or not 1<=len(raw['sources'])<=64 or not 1<=len(raw['owners'])<=64:
        raise ArithmeticError('bounded raw source/owner admission')
    wire_domains(raw)
    r=deepcopy(raw)
    # Match declared f64 serialization, retaining all integer dates/indices and
    # the raw JSON specification Value unchanged. No economics are inferred.
    for run in r['dated']['dated']['runs']:run['severity']=domain.finite(run['severity'])
    for item in r['dated']['dated']['sources']:
        for event in item['schedule']['events']:
            for k,v in event['components'].items():event['components'][k]=domain.finite(v,low=-domain.MAX_AMOUNT)
    for plan in r['dated']['dated']['accrual_plans']:
        for segment in plan['segments']:
            for k in ('accrued_interest','book_amortization'):segment[k]=domain.finite(segment[k],low=-domain.MAX_AMOUNT)
    spec=dated._normalize(r['dated']['specification'])
    start=r['dated']['dated']['as_of_ordinal']; days=spec['horizon_days']; end=start+days
    if not 1<=days<=64 or type(r['reporting_end_ordinal']) is not int or not start<=r['reporting_end_ordinal']<=end:
        raise ArithmeticError('exact full/reporting 64-day calendar')
    for a in spec['accounts']:
        if any(a[k]!=0. for k in 'cet1_floor leverage_floor lcr_floor nsfr_floor tax_rate annual_fees annual_costs annual_dividends'.split()) or a['htm_asset_limit']!=1. or abs(a['cash'])>10000. or abs(a['equity'])>10000.:
            raise ArithmeticError('unsupported account/expense/capital policy')
    dated_ids={o['position_id'] for o in r['dated']['dated']['owners']}
    filtered=deepcopy(spec); filtered['positions']=[p for p in spec['positions'] if p['id'] in dated_ids]
    backowners,runs,events,full=dated._backbook(r['dated'],filtered,check)
    if full!=end:
        raise ArithmeticError('actual dated container full horizon differs')
    for e in events:
        e['owner']=backowners[e['event']['position_id']]
    supplied_gross_money=money_inputs(r,spec,events,check)
    bypos={p['id']:p for p in spec['positions']}; accounts={a['id']:a for a in spec['accounts']}
    for runid in runs:
        ps={p['id']:p['balance'] for p in filtered['positions']}; bs={p['id']:p['book_adjustment'] for p in filtered['positions']}
        for e in events:
            if e['selector'] not in (None,runid): continue
            event=e['event']; pid=event['position_id']; c=event['components']
            period=event['accrual_period']; finish=period['end_ordinal'] if period else event['payment_ordinal']
            if event['payment_ordinal']>end or finish>end: raise ArithmeticError('incomplete reviewed dated tail')
            near(dated._cumulative(e['segments'],end,'accrued_interest')-c['cash_interest'],0.,'dated complete interest right')
            if event['payment_ordinal']>=start: ps[pid]-=abs(c['principal'])
            bs[pid]+=dated._side(bypos[pid])*(dated._cumulative(e['segments'],end,'book_amortization')-dated._cumulative(e['segments'],start,'book_amortization'))
        for v in list(ps.values())+list(bs.values()): near(v,0.,'dated complete principal/basis')
    registry={}; audits={}; source_work=0
    for item in r['sources']:
        check_cancel(check); keys(item,'source_id request'); dated._label(item['source_id'])
        if item['source_id'] in registry: raise ArithmeticError('duplicate immutable source id')
        normalized,a=pending.admit(item['request'],check)
        item['request']=normalized; registry[item['source_id']]=item; audits[item['source_id']]=a
        sr=normalized['source']
        if sr['contract']['monetary_unit']!=r['dated']['monetary_unit'] or sr['acquisition']['purchase_ordinal']<start or a['reserved_end_ordinal']>end:
            raise ArithmeticError('source actual full clock/unit differs')
        source_work+=a['work']
    bindings=[]; used=set(); owners=set(); units=set(); replay=0; binding_bytes=0; run_key_bounds=defaultdict(int)
    expected={(v['run_id'],v['scenario_id'],v['severity']) for v in runs.values()}
    for oi,o in enumerate(r['owners']):
        keys(o,'unit_id position_id account_id unit_face allocated_face source_identity original_yield_anchor runs')
        for k in ('unit_id','position_id','account_id'): dated._label(o[k])
        domain.identity(o['source_identity']); domain.identity(o['original_yield_anchor'])
        if o['unit_id'] in units or o['position_id'] in owners: raise ArithmeticError('duplicate mortgage owner/unit')
        units.add(o['unit_id']); owners.add(o['position_id'])
        p=bypos.get(o['position_id']); account=accounts.get(o['account_id'])
        if p is None or account is None or p['account']!=o['account_id'] or p['id'] in dated_ids: raise ArithmeticError('mortgage missing/double owned position')
        zeros='start_day maturity_day balance book_adjustment opening_accrued allowance rate commitment draw_fraction watch_fraction annual_pd migration_rate monthly_runoff floating_beta rollover eligible_fraction encumbered_fraction pledged_face risk_weight rsf_weight asf_weight hqla_weight lcr_outflow_weight lcr_inflow_weight'.split()
        if p['kind']!='loan' or p['classification']!='ac' or p['opening_market_price']!=1. or p['collateral_pool'] or p['collateral_position'] or any(p[k]!=0. for k in zeros): raise ArithmeticError('exclusive zero mortgage placeholder required')
        face=domain.finite(o['unit_face']); allocated=domain.finite(o['allocated_face'])
        o['unit_face']=face;o['allocated_face']=allocated
        if not 0.<face<=10000. or not 0.<=allocated<=face: raise ArithmeticError('bounded allocated/source face')
        got=set()
        for run in o['runs']:
            keys(run,'ledger_run_id scenario_id severity paths')
            triple=(run['ledger_run_id'],run['scenario_id'],domain.finite(run['severity']))
            run['severity']=triple[2]
            if triple in got: raise ArithmeticError('duplicate mapped run')
            got.add(triple)
        if got!=expected: raise ArithmeticError('complete ordinary run coverage required')
        anchor=None
        for ri,run in enumerate(o['runs']):
            ids=set(); weight=0.
            if not 1<=len(run['paths'])<=32: raise ArithmeticError('bounded per-run path count')
            for pi,path in enumerate(run['paths']):
                check_cancel(check); keys(path,'path_id source_weight source_id')
                dated._label(path['path_id']); dated._label(path['source_id'])
                w=domain.finite(path['source_weight'])
                path['source_weight']=w
                if path['path_id'] in ids or not 0.<=w<=1.: raise ArithmeticError('path id/weight domain')
                ids.add(path['path_id']); weight+=w
                item=registry.get(path['source_id'])
                if item is None: raise ArithmeticError('unbound immutable source reference')
                used.add(path['source_id']); sr=item['request']['source']; a=audits[path['source_id']]
                if sr['contract']['identity']['position_id']!=o['position_id'] or sr['contract']['identity']['currency']!=account['currency'] or sr['path_id']!=path['path_id'] or sr['economic_source']!=o['source_identity'] or sr['acquisition']['original_yield_anchor']!=o['original_yield_anchor']: raise ArithmeticError('exact source owner/original anchor mismatch')
                near(sum(sr['contract']['opening_borrower_principal']),face,'source borrower face'); near(sum(sr['contract']['opening_investor_principal']),face,'source investor face')
                identity=[sr['acquisition'],sr['original_contract'],sr['original_economic_source']]
                if anchor is not None and anchor!=identity: raise ArithmeticError('original immutable acquisition changed across paths/runs')
                anchor=identity
                f=(allocated/face)*w
                bindings.append(dict(owner=o,run=run,path=path,source_id=path['source_id'],factor=f,owner_index=oi,run_index=ri,path_index=pi))
                claims=3*a['interval_count']
                replay+=(days+1)*(1024+128*claims)+a['max_groups']*(2048+128*claims)
                run_key_bounds[run['ledger_run_id']]+=24+3*claims
                header=size([o['unit_id'],o['position_id'],o['account_id'],path,run['ledger_run_id'],run['scenario_id']])
                if header>8192: raise ArithmeticError('binding escaped header admission')
                binding_bytes+=(days+1)*(claims*512+2048+2*header)+a['max_groups']*24*(512+2*header)
            near(weight,1.,'absolute per-run probability mass')
    if used!=registry.keys(): raise ArithmeticError('unreferenced immutable registry entry')
    if owners|dated_ids!=bypos.keys(): raise ArithmeticError('complete every-position ownership required')
    work_dated=dated._dated_work(spec,events,end,len(runs))
    # Store sizing is an ABI-specific capacity charge, independently computed
    # from typed immutable source inventory, never producer admission metadata.
    store_bytes=store_capacity(r['dated']['dated'],events)
    reserved=2*size(r)+sum(a['reserved_bytes'] for a in audits.values())+binding_bytes+store_bytes+(work_dated+(days+1)*len(accounts))*1024
    run_count=len(runs);binding_count=len(bindings)
    max_gl_keys=16*len(accounts)+12*len(backowners)+max(run_key_bounds.values(),default=0)
    max_transactions=len(accounts)*run_count+sum(audits[b['source_id']]['max_groups'] for b in bindings)+2*(days+1)*len(events)*run_count
    segments=sum(len(p['segments']) for p in r['dated']['dated']['accrual_plans'])
    guard_work=sum((a['max_groups']+a['days']+2)*(1024+64*3*a['interval_count']) for a in audits.values())
    guard_work+=max_transactions*(9*max_gl_keys+128+max(48,16*len(spec['positions'])+16*len(accounts)))
    guard_work+=(days+1)*run_count*(64*len(spec['positions'])+48*len(accounts)+256)
    components=6*len(spec['positions'])+5*len(events)+2*segments+4*len(accounts)+4*len(audits)
    guard_work+=16*components+128
    reserved+=128*max_gl_keys+3072*binding_count+288
    work=source_work+replay+work_dated+guard_work
    if work>MAX_WORK or reserved>MAX_BYTES: raise ArithmeticError('joined8Mwork/32MiB admission before generation')
    admission=dict(work_units=work,source_work_units=source_work,replay_work_units=replay,
        dated_work_units=work_dated,monetary_guard_work_units=guard_work,max_gl_keys=max_gl_keys,
        supplied_gross_money=supplied_gross_money,gross_money_limit=MAX_GROSS_MONEY,
        reserved_bytes=reserved,unique_sources=len(registry),bindings=len(bindings),
        joined_work_limit=MAX_WORK,output_byte_limit=MAX_BYTES)
    return r,spec,events,bindings,admission


def store_capacity(raw, events):
    """Windows64 immutable store charge; vector growth is explicit below."""
    def pushed(n):
        if not n: return 0
        cap=4
        while cap<n: cap*=2
        return cap
    strings=set(); provenances=set()
    for run in raw['runs']:
        strings.update(run[k] for k in ('run_id','scenario_id','input_revision'))
    for owner in raw['owners']:
        strings.update(owner[k] for k in ('position_id','account_id','currency','source_id','source_revision'))
    settlements=defaultdict(int); boundaries=defaultdict(int)
    for e in events:
        event=e['event']; provenance=event['provenance']
        strings.add(event['event_id']); strings.update(v for k,v in provenance.items() if k!='kind')
        provenances.add(source.canonical_bytes(provenance))
        settlements[event['day_offset']]+=1
        for s in e['segments']:
            boundaries[s['start_ordinal']]+=1; boundaries[s['end_ordinal']]+=1
    # Vec::extend may reserve a whole explicit plan at once. Account for that
    # growth separately from the one-row push arenas and exact-size collect.
    seglen=segcap=0
    for plan in raw['accrual_plans']:
        n=len(plan['segments'])
        if seglen+n>segcap: segcap=max(4,segcap*2,seglen+n)
        seglen+=n
    return (sum(len(s.encode()) for s in strings)+pushed(len(strings))*16+len(strings)*16
        +pushed(len(provenances))*8+len(provenances)*(56+16)
        +len(raw['runs'])*56+len(raw['owners'])*88+pushed(len(events))*144+segcap*24
        +sum(pushed(n)*8 for n in settlements.values())+sum(pushed(n)*24 for n in boundaries.values())
        +(len(settlements)+len(boundaries))*128)


def line_gl(position,path,line):
    a=line['account']; slot=line['slot']; claim=line['claim_id']
    if a=='cash': return 'cash',''
    if a=='original_eir': return 'pnl:mortgage_original_effective_interest',position
    if a=='reviewed_income': return 'pnl:mortgage_reviewed_realization',position
    suffix={'principal':'principal','recognized_interest':'interest_receivable','basis':'basis'}[a]
    if claim is not None:
        return {'principal':'credit_default_principal','recognized_interest':'credit_default_interest_receivable','basis':'credit_default_book_adjustment'}[a],f'{position}:{path}:claim:{claim}'
    if type(slot) is not int or not 0<=slot<7: raise ArithmeticError('exact seven-slot posting')
    if slot<3: return {'principal':'asset_principal','recognized_interest':'dated_interest_receivable','basis':'book_adjustment'}[a],position
    return 'mortgage_pending_'+('default_' if slot<6 else 'prepay_')+suffix,f'{position}:{path}:pending:{slot}'


def observe(spec,positions,accounts,rights,books,bindings,scenario,day):
    """Research reporting is derived from separately conserved current books."""
    result=[]; ratio=lambda a,b:a/b if b>0. else None
    for a in accounts:
        assets=a['cash']; liabilities=rwa=rsf=inflow=outflow=l2=htm=0.
        hqla=max(a['cash'],0.); asf=max(a['equity'],0.)
        for p in positions:
            if p['account']!=a['id']: continue
            carrying=p['balance']+p['book_adjustment']-p['allowance']
            if dated._side(p)>0.:
                assets+=carrying; rwa+=max(carrying,0.)*p['risk_weight']*(1.+p['watch_fraction']); rsf+=max(carrying,0.)*p['rsf_weight']
                free=p['balance']*max(1.-p['encumbered_fraction'],0.)
                if p['hqla_level']=='level2a': l2+=free*p['hqla_weight']
                else: hqla+=free*p['hqla_weight']
                inflow+=p['balance']*p['lcr_inflow_weight']
            else:
                liabilities+=p['balance']+p['book_adjustment']; asf+=p['balance']*p['asf_weight']; outflow+=p['balance']*p['lcr_outflow_weight']
            assets+=rights[p['id']][0]+rights[p['id']][2]; liabilities-=rights[p['id']][1]+rights[p['id']][3]
            if p['classification']=='htm': htm+=p['balance']+p['book_adjustment']
        for bi,b in enumerate(bindings):
            book=books[bi]
            if b['owner']['account_id']!=a['id'] or book is None: continue
            f=b['factor']
            assets+=pending.ordered_sum(x['recognized_interest'] for x in book['slots'][:3])*f
            assets+=(pending.ordered_sum(pending.carrying(x) for x in book['slots'][3:])+pending.ordered_sum(pending.carrying(x['book']) for x in book['claims']))*f
        hqla+=min(l2,hqla*.4/(1.-.4)); outflow=max(outflow-min(inflow,.75*outflow),0.)
        error=assets-liabilities-a['equity']; cet1=a['equity']-a['capital_deductions']; exposure=max(assets,0.)
        if abs(error)>1e-8*max(1.,abs(assets),abs(liabilities)): raise ArithmeticError('independent pooled financial statement does not balance')
        if a['cash']<a['cash_floor']-1e-9-1e-12*abs(a['cash_floor']): raise ArithmeticError('pooled end-of-day cash floor')
        result.append(dict(scenario=scenario,account=a['id'],entity=a['entity'],currency=a['currency'],day=day,cash=float(a['cash']),restricted_cash=0.,cash_floor=float(a['cash_floor']),assets=float(assets),liabilities=float(liabilities),equity=float(a['equity']),aoci=0.,cet1=float(cet1),rwa=rwa,cet1_ratio=ratio(cet1,rwa),leverage_ratio=ratio(cet1,exposure),usable_collateral=0.,undrawn=0.,hqla_proxy=hqla,lcr_proxy=ratio(hqla,outflow),nsfr_proxy=ratio(asf,rsf),reconciliation_error=error,cash_headroom=float(a['cash']-a['cash_floor']),cet1_headroom=cet1-a['cet1_floor']*rwa,leverage_headroom=float(cet1-a['leverage_floor']*exposure),lcr_headroom=hqla-a['lcr_floor']*outflow,nsfr_headroom=asf-a['nsfr_floor']*rsf,htm_headroom=a['htm_asset_limit']*max(assets,0.)-htm))
    return result


def replay_rows(r,spec,events,bindings,sources,check=lambda:False,verified_posting_lines=None):
    """Merge raw dated events and reconstructed native-owner actions once."""
    out=defaultdict(list); asof=r['dated']['dated']['as_of_ordinal']; horizon=spec['horizon_days']; end=asof+horizon
    scenario_order=['baseline']+[s['name'] for s in spec['scenarios']]
    runs={v['scenario_id']:v for v in r['dated']['dated']['runs']}; totals={}
    for scenario in scenario_order:
        check_cancel(check); run=runs[scenario]; runid=run['run_id']
        selected=[e for e in events if e['selector'] in (None,runid)]
        bb=[b for b in bindings if b['run']['ledger_run_id']==runid]
        books=[None]*len(bb); positions=deepcopy(spec['positions']); bypos={p['id']:p for p in positions}; accounts=deepcopy(spec['accounts']); byaccount={a['id']:a for a in accounts}
        rights={p['id']:[0.,0.,0.,0.] for p in positions}; state=[]; gl={}; sequence=0; aggregates={}; verified_aggregate_presence={}; mincash={a['id']:a['cash'] for a in accounts}; maxerror=0.; breaches={}
        for e in selected:
            earned=dated._cumulative(e['segments'],asof,'accrued_interest'); book=dated._cumulative(e['segments'],asof,'book_amortization')
            state.append(dict(earned=earned,book=book,settled=False))
            pid=e['event']['position_id']
            for i,v in enumerate(dated._rights(earned,0.,False)): rights[pid][i]+=v
        for p in positions: near(sum(rights[p['id']]),dated._side(p)*p['opening_accrued'],'opening source gross rights')

        def post(day,account,event,lines,cash=0.,income=0.,position='',aggregate=True,append=True,topology_mask=None):
            nonlocal sequence
            if append: lines=list(lines)+[('cash','',cash),('pnl:'+event,position,-income)]
            lines=([l for l,include in zip(lines,topology_mask,strict=True) if include] if topology_mask is not None else [l for l in lines if l[2]!=0.])
            if lines:
                if abs(math.fsum(l[2] for l in lines))>1e-9*max(1.,sum(abs(l[2]) for l in lines)): raise ArithmeticError('ordered journal transaction balance')
                staged=gl.copy()
                for code,instrument,value in lines:
                    staged[account,code,instrument]=staged.get((account,code,instrument),0.)+value
                money_gross((staged[key] for key in sorted(staged)),check)
                key_bound=16*len(accounts)+12*len(r['dated']['dated']['owners'])+sum(24+9*sources[b['source_id']]['admission']['interval_count'] for b in bb)
                if len(staged)>key_bound:
                    raise ArithmeticError('bounded GL keys')
                sequence+=1
                for code,instrument,value in lines:
                    gl[account,code,instrument]=gl.get((account,code,instrument),0.)+value
                    out['journal'].append(dict(transaction_number=sequence,scenario=scenario,day=day,account=account,event=event,gl_account=code,instrument_id=instrument,value=float(value)))
            if aggregate:
                byaccount[account]['cash']+=cash; byaccount[account]['equity']+=income
                sums=aggregates.setdefault((account,event),[0.,0.]); sums[0]+=cash; sums[1]+=income
            return sequence if lines else None

        for a in accounts:
            lines=[('cash','',a['cash']),('opening_equity','',-a['equity'])]
            for p in positions:
                if p['account']==a['id']:
                    lines +=[(dated._principal(p),p['id'],dated._side(p)*p['balance']),('book_adjustment',p['id'],dated._side(p)*p['book_adjustment']),('allowance',p['id'],-p['allowance'])]
                    lines +=[(code,p['id'],value) for code,value in zip(dated.RIGHTS,rights[p['id']],strict=True)]
            post(0,a['id'],'opening',lines,aggregate=False,append=False)

        def dated_event(i,day,earned,book,settlement):
            e=selected[i]; event=e['event']; p=bypos[event['position_id']]; value=state[i]; components=event['components']
            before=dated._rights(value['earned'],components['cash_interest'],value['settled'])
            if settlement and value['settled']: raise ArithmeticError('duplicate actual dated settlement')
            value['earned']+=earned; value['book']+=book; value['settled']=value['settled'] or settlement
            after=dated._rights(value['earned'],components['cash_interest'],value['settled']); delta=[a-b for a,b in zip(after,before,strict=True)]
            for j,v in enumerate(delta): rights[p['id']][j]+=v
            principal=components['principal'] if settlement else 0.; cashinterest=components['cash_interest'] if settlement else 0.
            if abs(principal)>p['balance']+1e-9: raise ArithmeticError('source principal exceeds owned backbook')
            p['balance']=max(p['balance']-abs(principal),0.); p['book_adjustment']+=dated._side(p)*book
            lines=[(dated._principal(p),p['id'],-dated._side(p)*abs(principal)),('book_adjustment',p['id'],book)]+[(code,p['id'],v) for code,v in zip(dated.RIGHTS,delta,strict=True)]
            tx=post(day,p['account'],'dated_settlement' if settlement else 'dated_recognition',lines,cash=principal+cashinterest,income=earned+book,position=p['id'])
            owner=e['owner']; provenance=event['provenance']; ordered={'kind':provenance['kind']}; ordered.update({k:v for k,v in provenance.items() if k!='kind'})
            out['event_lineage'].append(dict(scenario=scenario,run_id=runid,input_revision=run['input_revision'],day=day,transaction_number=tx,position_id=p['id'],event_id=event['event_id'],phase='settlement' if settlement else 'recognition',source_id=owner['source_id'],source_revision=owner['source_revision'],provenance_json=json.dumps(ordered,separators=(',',':'),ensure_ascii=False),measure=event['measure'],ownership=owner['ownership'],policy_id=r['dated']['recognition_policy']['policy_id'],policy_revision=r['dated']['recognition_policy']['policy_revision'],principal=principal,cash_interest=cashinterest,accrued_interest=earned,book_amortization=book,fees=0.))

        for day in range(horizon+1):
            check_cancel(check); ordinal=asof+day
            if day:
                for i,e in enumerate(selected):
                    earned=book=0.; active=False
                    for s in e['segments']:
                        if s['start_ordinal']<=ordinal-1<s['end_ordinal']:
                            n=s['end_ordinal']-s['start_ordinal']; f0=(ordinal-1-s['start_ordinal'])/n; f1=(ordinal-s['start_ordinal'])/n
                            earned+=s['accrued_interest']*f1-s['accrued_interest']*f0; book+=s['book_amortization']*f1-s['book_amortization']*f0; active=True
                    if active: dated_event(i,day,earned,book,False)
            # Registry compiled source remains immutable. Each binding owns a
            # private book and acquisition; no cashflow overlay reaches it.
            for bi,b in enumerate(bb):
                native=sources[b['source_id']]; owner=b['owner']; path=b['path']; factor=b['factor']
                for gi,group in enumerate(native['groups']):
                    if group['ordinal']!=ordinal: continue
                    if books[bi] is None and group['kind']!='acquisition': raise ArithmeticError('first atomic group must acquire')
                    if books[bi] is not None and group['kind']=='acquisition': raise ArithmeticError('duplicate actual acquisition')
                    lines=[]; mask=[]; cash=income=0.; wire_cash=wire_income=0.
                    # Native uniformization and SciPy expm can round a retired
                    # basis to -0.0 versus -1e-14. Zero filtering is exact wire
                    # behavior. Only AFTER independent complete source/line
                    # verification may those verified wire amounts determine
                    # zero-filter topology; identities/order/books stay raw
                    # reconstructed, with unchanged financial tolerance.
                    wire=(verified_posting_lines[b['source_id']][gi] if verified_posting_lines is not None else group['lines'])
                    for l,w in zip(group['lines'],wire,strict=True):
                        equal({k:v for k,v in w.items() if k!='amount'},{k:v for k,v in l.items() if k!='amount'},'verified topology line identity')
                        code,instrument=line_gl(owner['position_id'],path['path_id'],l); amount=l['amount']*factor; lines.append((code,instrument,amount))
                        mask.append(w['amount']*factor!=0.)
                        if code=='cash': cash+=amount
                        if code.startswith('pnl:'): income-=amount
                        if code=='cash': wire_cash+=w['amount']*factor
                        if code.startswith('pnl:'): wire_income-=w['amount']*factor
                    event='mortgage:'+group['kind']
                    tx=post(day,owner['account_id'],event,lines,cash,income,aggregate=True,append=False,topology_mask=mask if verified_posting_lines is not None else None)
                    key=(owner['account_id'],event)
                    presence=verified_aggregate_presence.setdefault(key,[0.,0.]); presence[0]+=wire_cash; presence[1]+=wire_income
                    money_book(group['committed'],check,3*native['admission']['interval_count'])
                    books[bi]=deepcopy(group['committed'])
                    out['mortgage_lineage'].append(dict(scenario=scenario,run_id=runid,ordinal=ordinal,position_id=owner['position_id'],path_id=path['path_id'],registry_source_id=b['source_id'],source_weight=path['source_weight'],allocation_factor=owner['allocated_face']/owner['unit_face'],native_group_index=gi,native_source_id=group['source_id'],source_action_indices=group['source_action_indices'],review=group['review'],original_yield_anchor=owner['original_yield_anchor'],transaction_number=tx,source_groups_sole_postings=True))
            for bi,b in enumerate(bb):
                native=sources[b['source_id']]; purchase=native['source']['raw_request']['acquisition']['purchase_ordinal']; acquired=ordinal>=purchase
                if acquired:
                    daily=deepcopy(native['daily'][min(ordinal-purchase,len(native['daily'])-1)])
                    for k in ('slots','cash','original_eir_income','reviewed_income'):
                        equal(books[bi][k],daily[k],'source financial checkpoint/'+k)
                    equal([c['book'] for c in books[bi]['claims']],[c['book'] for c in daily['claims']],'actual financial claim checkpoint')
                    # Scheduled boundary moves contractual borrower rights after
                    # financial groups. This is a verified zero-posting memo
                    # checkpoint, never a monetary plug or implicit disposal.
                    daily['ordinal']=ordinal; money_book(daily,check,3*native['admission']['interval_count']); books[bi]=daily
                elif books[bi] is not None: raise ArithmeticError('pre-acquisition rights')
                o=b['owner']; p=b['path']
                out['mortgage_daily'].append(dict(scenario=scenario,run_id=runid,ordinal=ordinal,position_id=o['position_id'],path_id=p['path_id'],registry_source_id=b['source_id'],acquired=acquired,source_weight=p['source_weight'],allocation_factor=o['allocated_face']/o['unit_face'],unweighted_book=deepcopy(books[bi]),native_daily_index=min(ordinal-purchase,len(native['daily'])-1) if acquired else None,native_daily_ordinal=native['daily'][min(ordinal-purchase,len(native['daily'])-1)]['ordinal'] if acquired else None,memo_checkpoint_source='native_complete_daily_boundary',capital_ftp_eligible=False,closing_capital_eligible=False))
            for o in r['owners']:
                summaries=[0.,0.,0.]
                for bi,b in enumerate(bb):
                    if b['owner']['position_id']!=o['position_id'] or books[bi] is None: continue
                    for ch in books[bi]['slots'][:3]:
                        for j,k in enumerate(('principal','basis','recognized_interest')): summaries[j]+=ch[k]*b['factor']
                bypos[o['position_id']]['balance']=summaries[0]; bypos[o['position_id']]['book_adjustment']=summaries[1]
            for i,e in enumerate(selected):
                if e['event']['payment_ordinal']==ordinal: dated_event(i,day,0.,0.,True)
            money_gross((a[k] for a in accounts for k in ('cash','equity','cash_floor','capital_deductions')),check)
            combined=[]
            backbook_ids={x['position_id'] for x in r['dated']['dated']['owners']}
            for p in positions:
                if p['id'] in backbook_ids:
                    combined += [p['balance'],p['book_adjustment'],p['allowance'],dated._side(p)*sum(rights[p['id']]),p['commitment']]
            for bi,b in enumerate(bb):
                if books[bi] is None: continue
                combined += [x[k]*b['factor'] for x in books[bi]['slots'] for k in ('principal','recognized_interest','basis')]
                combined += [claim['book'][k]*b['factor'] for claim in books[bi]['claims'] for k in ('principal','recognized_interest','basis')]
            money_gross(combined,check)
            rows=observe(spec,positions,accounts,rights,books,bb,scenario,day)
            for row in rows:
                mincash[row['account']]=min(mincash[row['account']],row['cash']); maxerror=max(maxerror,abs(row['reconciliation_error']))
                for label in ('cash','cet1','leverage','lcr','nsfr','htm'):
                    if row[label+'_headroom']< -1e-9 and (row['account'],label) not in breaches:
                        breaches[row['account'],label]=day; out['breaches'].append(dict(scenario=scenario,account=row['account'],day=day,metric=label))
            if day<=30 or day%30==0 or day==horizon or ordinal==r['reporting_end_ordinal']: out['path']+=rows
            if day==0 or day%30==0 or day==horizon or ordinal==r['reporting_end_ordinal']:
                for p in positions:
                    accrued=dated._side(p)*sum(rights[p['id']])
                    for bi,b in enumerate(bb):
                        if b['owner']['position_id']==p['id'] and books[bi] is not None: accrued+=pending.ordered_sum(x['recognized_interest'] for x in books[bi]['slots'][:3])*b['factor']
                    out['exposures'].append(dict(scenario=scenario,day=day,account=p['account'],position=p['id'],kind=p['kind'],balance=float(p['balance']),allowance=float(p['allowance']),watch_fraction=float(p['watch_fraction']),commitment=float(p['commitment']),market_factor=1.,encumbered_fraction=float(p['encumbered_fraction']),accrued_interest=float(accrued)))
            for (account,event),sums in aggregates.items():
                presence=(verified_aggregate_presence.get((account,event),sums) if verified_posting_lines is not None else sums)
                if any(v!=0. for v in presence):
                    out['ledger'].append(dict(scenario=scenario,day=day,account=account,event=event,cash=sums[0],earnings=sums[1],aoci=0.,memo_amount=0.))
                    total=totals.setdefault((scenario,account,event),[0.,0.]); total[0]+=sums[0]; total[1]+=sums[1]
            aggregates.clear()
            verified_aggregate_presence.clear()
            for table, chunk in out.items():
                for row in chunk: yield table,row
            out.clear()
        trial=[dict(scenario=scenario,account=a,gl_account=g,instrument_id=i,balance=v) for (a,g,i),v in sorted(gl.items())]
        out['trial_balance']+=trial
        from portfolio_risk.analytics.journal import statements
        from portfolio_risk.analytics.balance_stress import Account,_decode
        statements_rows,consolidated=statements(trial,{a['id']:_decode(Account,a) for a in accounts},[scenario])
        out['closing_statements']+=statements_rows; out['consolidated']+=consolidated
        for a in accounts:
            row=dict(scenario=scenario,account=a['id'],entity=a['entity'],currency=a['currency'],final_cash=float(a['cash']),final_equity=float(a['equity']),minimum_cash=float(mincash[a['id']]),peak_cash_shortfall=float(max(a['cash_floor']-mincash[a['id']],0.)),max_reconciliation_error=maxerror)
            for field,metric in [('first_cash_breach_day','cash'),('first_capital_breach_day','cet1'),('first_leverage_breach_day','leverage'),('first_lcr_breach_day','lcr'),('first_nsfr_breach_day','nsfr'),('first_htm_breach_day','htm')]: row[field]=breaches.get((a['id'],metric))
            out['summary'].append(row)
        out['dated_rights']+=dated._rights_rows(selected,state,scenario,runid,end,{}, {}, [])
        for bi,b in enumerate(bb):
            closed(books[bi]); o=b['owner']; p=b['path']
            out['mortgage_closing'].append(dict(scenario=scenario,position_id=o['position_id'],path_id=p['path_id'],registry_source_id=b['source_id'],unweighted_book=books[bi],full_tail_complete=True,capital_ftp_eligible=False,closing_capital_eligible=False))
        for table, chunk in out.items():
            for row in chunk: yield table,row
        out.clear()
    pairs=sorted({(account,event) for _,account,event in totals})
    for s in spec['scenarios']:
        for account,event in pairs:
            base=totals.get(('baseline',account,event),[0.,0.]); current=totals.get((s['name'],account,event),[0.,0.])
            out['attribution'].append(dict(scenario=s['name'],account=account,event=event,cash_delta=current[0]-base[0],earnings_delta=current[1]-base[1],aoci_delta=0.))
    for table,chunk in out.items():
        for row in chunk: yield table,row

