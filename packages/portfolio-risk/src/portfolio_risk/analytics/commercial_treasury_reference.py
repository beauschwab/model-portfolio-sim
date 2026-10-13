"""Explicit independent complete commercial journal/capital/daily FTP oracle.

Producer checkpoints never feed financial reference calculations. The raw
source, CTMC, coupon ownership and ordered journal first pass independent
reconstruction; the reference then creates its own channel states and policies.
"""
from collections import defaultdict
from copy import deepcopy
from datetime import date
import json
import math
import polars as pl
from .commercial_source_reference import compare
from .commercial_ledger_reference import verify_preparation,verify_stream
from .commercial_ledger_transport import SCHEMAS
from .modeled_product_funding_reference import LANES,prepare
from .modeled_json_identity import exact_json_identity


class Frames:
    def __init__(self,messages,check):
        self.tables=defaultdict(list);self.completion=None;dictionary=[]
        for message in messages:
            check()
            if self.completion is not None:raise ArithmeticError('commercial treasury rows after completion')
            if 'complete' in message:self.completion=deepcopy(message);continue
            if set(message)=={'workflow'}:continue
            table=message['table']
            if table=='journal':
                dictionary.extend(message['dictionary']);columns=message['columns'];rows=[]
                for i,value in enumerate(columns['value']):
                    if type(value)is not float or not math.isfinite(value):raise ArithmeticError('strict commercial treasury journal money')
                    row={k:dictionary[columns[k][i]] for k in ('scenario','account','event','gl_account','instrument_id')}
                    row.update(day=columns['day'][i],transaction_id=f"{row['scenario']}:{columns['transaction_number'][i]}",debit=max(value,0.),credit=max(-value,0.));rows.append(row)
            else:
                rows=deepcopy(message['rows'])
                if table.startswith('commercial_'):
                    field='native_atomic_group' if table=='commercial_source_lineage' else 'unweighted_book'
                    for row in rows:row[field+'_json']=json.dumps(row.pop(field),allow_nan=False,separators=(',',':'))
            self.tables[table].append(pl.DataFrame(rows,schema=SCHEMAS[table]))
    def frames(self,table):return iter(self.tables[table])


def _state(snapshot,day,undrawn):
    channels=[dict(channel=c,amount=0.) for c in LANES];origins=[];default=snapshot['in_default']
    def add(lane,identity,event,amount):
        channels[lane]['amount']+=amount;origins.append(dict(origin_id=identity,source_event_id=event,channel=LANES[lane],amount=amount))
    add(3 if default else 0,'principal','principal-owner',snapshot['principal'])
    add(10 if default else 8,'principal-basis','original-yield-owner',snapshot['book_adjustment'])
    add(16,'undrawn','commitment-owner',undrawn)
    if snapshot.get('reviewed_credit_state') is not None:
        reviewed=snapshot['reviewed_credit_state']
        add(12,'reviewed-legal-principal','legal-claim-owner',reviewed['legal_principal'])
        add(17,'reviewed-allowance','reviewed-contra-owner',reviewed['allowance'])
    for lot in snapshot['lots']:
        amount=lot['recognized_remaining'];lane=(6 if amount>=0. else 7) if lot['default_claim'] else (4 if amount>=0. else 5)
        add(lane,f"{lot['coupon_id']}:{lot['earning_origin_index']}",lot['source_event_id'],abs(amount))
    if 'canonical_legal_coupons' in snapshot:
        for coupon in snapshot['canonical_legal_coupons']:
            amount=coupon['legal_remaining'];add(13,f"canonical-legal:{coupon['native_audit_index']}:{'receivable' if amount>=0. else 'payable'}",coupon['coupon_id'],abs(amount))
    else:
        for lot in snapshot['lots']:
            amount=lot['legal_remaining'];add(13,f"origin-legal:{json.dumps(lot['coupon_id'])}:{lot['earning_origin_index']}:{'receivable' if amount>=0. else 'payable'}",lot['source_event_id'],abs(amount))
    return dict(ordinal=day,source_checkpoint_id=f'commercial:daily:{day}',channels=channels,origins=origins)


def _day(year):
    days=year*365.;rounded=round(days)
    day=rounded if year==rounded/365. else math.ceil(days)
    return day+1 if year>day/365. else day


def _funding(raw,control,review,check):
    participation=next(p for p in raw['participations'] if (p['scenario'],p['position_id'],p['path_key'])==(review['scenario'],review['position_id'],review['path_key']))
    source=next(s for s in control['sources'] if (s['raw']['source']['current']['scenario_id'],s['raw']['source']['current']['position_id'])==(review['scenario'],review['position_id']))
    current=source['raw']['source']['current'];acquisition=source['raw']['source']['acquisition'];start=current['asof_ordinal'];end=start+control['horizon']
    groups=[(i,g) for i,g in enumerate(source['groups']) if g['path_key']==review['path_key']]
    points=[p for p in source['replay']['carry_points'] if p['path_key']==review['path_key']]
    states=[];undrawns=[];at=pi=0;snapshot=None;undrawn=source['raw']['credit'].get('opening_undrawn_commitment',0.)
    for day in range(control['horizon']+1):
        check()
        while at<len(groups) and _day(groups[at][1]['year'])<=day:snapshot=groups[at][1]['next'];at+=1
        while pi<len(points) and _day(points[pi]['year'])<=day:undrawn=points[pi]['undrawn_commitment'];pi+=1
        states.append(_state(snapshot,start+day,undrawn));undrawns.append(undrawn)
    cash=[];earned=[];opening_cash=0.
    for index,group in groups:
        day=start+_day(group['year']);identity=f'commercial-group:{index}'
        if day==start:
            if any(p['posting']['account']in('interest_income','basis_recognition_expense','credit_loss_expense','recovery_income')and p['posting']['amount']!=0. for p in group['postings']):raise ArithmeticError('opening commercial P&L requires separate FTP opening-earnings ownership')
            opening_cash+=group['cash'];continue
        cash.append(dict(event_id=identity,ordinal=day,kind='source_atomic_cash',bank_cash=group['cash'],funding_change_sequence=None,exclusion_review=deepcopy(review['cash_exclusion_review'])))
        for channel,account in [('original_eir','interest_income'),('reviewed_basis','basis_recognition_expense'),('incurred_credit','credit_loss_expense'),('recovery_income','recovery_income')]:
            amount=-sum((p['posting']['amount']for p in group['postings']if p['posting']['account']==account),0.)
            if amount!=0.:earned.append(dict(event_id=f'{identity}:{channel}',ordinal=day,channel=channel,bank_earnings=amount,review=deepcopy(review['income_review'])))
    def balance(state):return sum(a['amount']*p['coefficient'] for a,p in zip(state['channels'],review['policy']['channels'],strict=True))
    changes=[dict(sequence=i+1,ordinal=after['ordinal'],source_event_id=after['source_checkpoint_id'],kind='reviewed_other_reallocation',amount=balance(after)-balance(before),cash_event_id=None,review=deepcopy(review['channel_change_review'])) for i,(before,after) in enumerate(zip(states,states[1:]))]
    face=next(a['allocated_face'] for a in raw['allocations'] if a['position_id']==review['position_id'])
    path=next(p for p in source['raw']['credit']['paths'] if p['path_key']==review['path_key'])
    request=dict(schema='modeled-product-ftp-source-channels-1',measure='physical',source=deepcopy(current['contract']),original_yield_anchor=deepcopy(acquisition['original_yield_anchor']),path_id=path['path_id'],entity=review['entity'],currency_unit=review['currency'],scenario_id=review['scenario'],side='asset',start_ordinal=start,full_end_ordinal=end,source_weight=participation['source_weight'],unit_face=current['opening_principal'],allocated_face=face,opening_funding_balance=balance(states[0]),policy=deepcopy(review['policy']),states=states,cash=cash,changes=changes,earned=earned,resets=deepcopy(review['resets']))
    return prepare(request,check),undrawns,opening_cash


def _ftp(request):
    curves={c['id']:c for c in request['curves']};rows=[];totals=defaultdict(lambda:[0.]*4)
    def interpolate(x,xs,ys):
        if x<=xs[0]:return float(ys[0])
        if x>=xs[-1]:return float(ys[-1])
        i=next(i for i in range(1,len(xs)) if x<=xs[i]);return ys[i-1]+(ys[i]-ys[i-1])*(x-xs[i-1])/(xs[i]-xs[i-1])
    for p in request['positions']:
        c=curves[p['curve_id']];reference=interpolate(p['repricing_years'],c['tenors'],c['reference_rates']);liquidity=interpolate(p['funding_years'],c['tenors'],c['liquidity_spreads']);basis=p['average_balance']*p['accrual_fraction'];reference_charge=basis*reference;liquidity_charge=basis*liquidity;option_charge=basis*p['option_spread'];charge=reference_charge+liquidity_charge+option_charge+p['contingent_liquidity_charge'];external=p['external_interest']+p['fees']-p['operating_cost']-p['expected_loss'];profit=external-charge;capital=p['allocated_capital']*p['cost_of_capital']*p['accrual_fraction']
        row={k:p[k] for k in ('id','loan_id','cohort_id','entity','currency','scenario','period','curve_id')};row.update(reference_rate=reference,liquidity_spread=liquidity,reference_charge=reference_charge,liquidity_charge=liquidity_charge,option_charge=option_charge,contingent_liquidity_charge=p['contingent_liquidity_charge'],ftp_charge=charge,external_profit=external,business_profit=profit,capital_cost=capital,economic_profit=profit-capital,raroc=profit/p['allocated_capital']/p['accrual_fraction'] if p['allocated_capital']>0. else None,flat_tail_used=min(p['repricing_years'],p['funding_years'])<c['tenors'][0] or max(p['repricing_years'],p['funding_years'])>c['tenors'][-1]);rows.append(row)
        key=tuple(p[k] for k in ('entity','currency','scenario','period'));v=totals[key];v[0]+=external;v[1]+=profit;v[2]+=charge;v[3]+=capital
    reconciliation=[dict(zip(('entity','currency','scenario','period'),key))|dict(external_profit=v[0],business_profit=v[1],treasury_profit=v[2],consolidated_profit=v[1]+v[2],capital_cost=v[3],elimination_error=v[1]+v[2]-v[0]) for key,v in sorted(totals.items())]
    return rows,reconciliation


def verify(request,actual,check=lambda:False):
    check()
    fields='schema raw_request source_receipt ledger_messages capital ftp funding_paths full_end_ordinal native_ordered_journal_replayed journal_rows bridge_work_units bridge_work_bound bridge_work_limit stream_decode_bytes closing_capital_selected daily_ftp_selected economic_loss_deducted_again independent_persisted_verification empirical_validation regulatory_credit_rwa_approval accounting_standard_approval production_adoption scope'
    if type(actual)is not dict or set(actual)!=set(fields.split()):raise ArithmeticError('strict commercial treasury result fields required')
    if not exact_json_identity(request,actual['raw_request']) or request['schema']!='commercial-treasury-1' or actual['schema']!=request['schema']:raise ArithmeticError('commercial treasury raw schema identity differs')
    raw=request['raw_commercial'];control=verify_preparation(raw,raw,actual['source_receipt'],check);frames=Frames(actual['ledger_messages'],check);source_receipt=verify_stream(raw,control,frames,check)
    start=raw['commercial'][0]['source']['current']['asof_ordinal'];end=start+control['horizon']
    work=sum(len(m['columns']['value'])*16 if m.get('table')=='journal' else len(m['rows'])*32 if m.get('table') in ('trial_balance','closing_statements','commercial_daily') else 0 for m in actual['ledger_messages'])
    bound=sum(s['admission']['carry_points_upper']*(16+8*s['admission']['lots_per_path_upper'])*48 for s in actual['source_receipt']['sources'])+32*(len(raw['participations'])*(control['horizon']+1)+len(control['spec']['accounts'])*(len(control['spec']['scenarios'])+1))
    if type(actual['bridge_work_bound'])is not int or actual['bridge_work_bound']!=bound or not work<=bound<=2_000_000:raise ArithmeticError('commercial treasury admitted bridge upper differs')
    if type(actual['bridge_work_units'])is not int or actual['bridge_work_units']!=work or not 0<=work<=2_000_000 or type(actual['bridge_work_limit'])is not int or actual['bridge_work_limit']!=2_000_000 or type(actual['stream_decode_bytes'])is not int or not 0<actual['stream_decode_bytes']<=32<<20:raise ArithmeticError('commercial treasury bounded integer receipt differs')
    for name,value in [('full_end_ordinal',end),('journal_rows',source_receipt['journal_rows']),('native_ordered_journal_replayed',True),('closing_capital_selected',request.get('capital') is not None),('daily_ftp_selected',request.get('ftp') is not None),('economic_loss_deducted_again',False)]:
        if not exact_json_identity(value,actual[name]):raise ArithmeticError('commercial treasury authority differs/'+name)
    for name in ('independent_persisted_verification','empirical_validation','regulatory_credit_rwa_approval','accounting_standard_approval','production_adoption'):
        if actual[name] is not False:raise ArithmeticError('commercial treasury eligibility differs/'+name)
    if request.get('capital') is not None and any(p['capital']['period']!=date.fromordinal(end).isoformat() for p in request['capital']['policies']):raise ArithmeticError('commercial exact closing capital period differs')
    if request.get('capital') is not None:_capital(request['capital'],actual['capital'],frames,check)
    elif actual['capital'] is not None:raise ArithmeticError('unselected commercial capital')
    ftp=request.get('ftp');paths=[];positions=[]
    if ftp is not None:
        reviews={(p['scenario'],p['position_id'],p['path_key']):p for p in ftp['reviews']}
        if len(reviews)!=len(raw['participations']):raise ArithmeticError('complete commercial FTP review coverage required')
        for participation in raw['participations']:
            key=tuple(participation[k] for k in ('scenario','position_id','path_key'));review=reviews[key];funding,undrawn,opening=_funding(raw,control,review,check);paths.append(dict(scenario=key[0],position_id=key[1],path_key=key[2],preparation=funding,opening_cash=opening))
            for i,day in enumerate(funding['days']):
                avg_u=undrawn[i] if review['policy']['averaging']=='opening_daily' else (undrawn[i]+undrawn[i+1])*.5
                positions.append(dict(id=f'{key[0]}:{key[1]}:{key[2]}:{day["ordinal"]}',loan_id=key[1],cohort_id=None,entity=review['entity'],currency=review['currency'],scenario=key[0],period=date.fromordinal(day['ordinal']).isoformat(),curve_id=review['curve_id'],side='asset',average_balance=day['scaled_average_funding_balance'],accrual_fraction=day['accrual_fraction'],repricing_years=day['reference_years'],funding_years=day['liquidity_years'],external_interest=day['original_eir'],fees=sum(a['amount'] for a in day['earnings_by_channel'] if a['channel']!='original_eir'),operating_cost=review['daily_operating_cost']*funding['scale'],expected_loss=0.,allocated_capital=review['allocated_capital']*funding['scale'],cost_of_capital=float(review['annual_cost_of_capital']),option_spread=float(review['option_spread']),contingent_liquidity_charge=avg_u*funding['scale']*review['contingent_annual_rate']*day['accrual_fraction']))
        spec=dict(as_of=ftp['as_of'],policy_id=ftp['policy_review']['id'],capital=[],curves=deepcopy(ftp['curves']),positions=positions,strategy_units=[],capital_deltas=[])
        for c in spec['curves']:
            for name in ('tenors','reference_rates','liquidity_spreads'):c[name]=[float(v) for v in c[name]]
        compare(spec,actual['ftp']['prepared_specification'],check=check);rows,reconciliation=_ftp(spec);compare(rows,actual['ftp']['ftp_positions'],check=check);compare(reconciliation,actual['ftp']['ftp_reconciliation'],check=check)
        for key,value in [('schema','treasury-1'),('as_of',ftp['as_of']),('policy_id',ftp['policy_review']['id'])]:
            if not exact_json_identity(actual['ftp'][key],value):raise ArithmeticError('commercial FTP result identity differs')
        if actual['ftp']['capital_metrics'] or actual['ftp']['capital_limits']:raise ArithmeticError('commercial FTP cannot create capital requirements')
    elif actual['ftp'] is not None:raise ArithmeticError('unselected commercial FTP')
    compare(paths,actual['funding_paths'],check=check)
    return dict(verified=True,raw_source_recomputed=True,ordered_journal_replayed=True,complete_tail_verified=True,closing_capital_verified=request.get('capital') is not None,daily_ftp_verified=ftp is not None,economic_loss_deducted_again=False,empirical_validation=False,regulatory_credit_rwa_approval=False)


def _capital(request,actual,frames,check):
    from . import _mortgage_mixed_financial_reference as financial
    from .mortgage_closing_capital_reference import _financial_policy,exposure_policy,ASSETS,LIABILITIES,PENDING_GLS
    if not exact_json_identity(actual['commercial_closing_review'],request['review']):raise ArithmeticError('commercial closing review identity differs')
    policies={(p['scenario'],p['account']):_financial_policy(p) for p in request['policies']}; amounts={}; contributions=[];groups={}
    treatment=exposure_policy(request['exposure_policy'])
    rows=[r for f in frames.frames('trial_balance') for r in f.iter_rows(named=True) if not r['gl_account'].startswith('memo:')]  # bounded closing GL only, no journal hydration
    funding={(r['scenario'],r['account'],r['instrument_id']) for r in rows if r['gl_account']=='funding_principal'}
    multiplier=request['amount_multiplier']
    if type(multiplier) not in (float,int) or not math.isfinite(multiplier) or multiplier<=0: raise ArithmeticError('positive monetary multiplier required')
    for row in rows:
        check(); key=(row['scenario'],row['account']); p=policies[key]
        v=row['balance']*multiplier; gl=row['gl_account']; ident=row['instrument_id']
        a=amounts.setdefault(key,[0.]*7)
        if gl in PENDING_GLS: financial.near(row['balance'],0.,'closed pending key')
        if gl in ('opening_equity','equity_distributions'): a[2]-=v
        elif gl.startswith('pnl:'): a[3]-=v
        elif gl=='oci': a[4]-=v
        elif gl in LIABILITIES or gl in ('book_adjustment','accrued_interest') and (*key,ident) in funding or gl=='derivative_value' and v<0: a[1]-=v
        elif gl in ASSETS:
            weight=p['instrument_risk_weights'].get(ident,p['gl_risk_weights'].get(gl))
            if weight is None:
                if v==0: weight=0.
                else: raise ArithmeticError('explicit asset risk weight required')
            if type(weight) not in (float,int) or not math.isfinite(weight) or weight<0: raise ArithmeticError('risk weight domain')
            a[0]+=v; a[5]+=v*weight
            group=groups.setdefault((*key,ident),[0.,weight])
            if group[1]!=weight:raise ArithmeticError('one reviewed risk weight required per exposure group')
            group[0]+=v
            contributions.append(dict(scenario=key[0],account=key[1],gl_account=gl,instrument_id=ident,balance=v,risk_weight=weight,rwa_contribution=v*weight))
        else: raise ArithmeticError('unreviewed closing GL mapping '+gl)
        a[6]+=v
    group_audit=[];research_scopes=set()
    for a in amounts.values():a[5]=0.
    for (scenario,account,instrument),(net,weight) in sorted(groups.items()):
        if net<0. and treatment=='reject_negative_exposure_groups':raise ArithmeticError('negative net carrying exposure group')
        exposure=max(net,0.) if treatment=='reviewed_nonnegative_group_exposure' else net
        if net<0. and treatment=='research_signed_exposure_groups':research_scopes.add((scenario,account))
        contribution=exposure*weight;amounts[scenario,account][5]+=contribution
        group_audit.append(dict(scenario=scenario,account=account,instrument_id=instrument,accounting_net_carrying=net,policy_exposure=exposure,risk_weight=weight,policy_rwa_contribution=contribution,negative_accounting_exposure=net<0.))
    financial.equal(actual['exposure_groups'],group_audit,'independent grouped carrying/exposure policy')
    # Computed tolerances do not make a negative-exposure boolean interchangeable
    # with a positive/zero group. If independent arithmetic differs at zero, fail.
    for a,e in zip(actual['exposure_groups'],group_audit,strict=True):
        financial.exact(a['negative_accounting_exposure'],e['negative_accounting_exposure'],'negative exposure identity')
        financial.exact(a['negative_accounting_exposure'],a['accounting_net_carrying']<0.,'reported group sign consistency')
        financial.exact(a['risk_weight'],float(e['risk_weight']),'supplied group risk weight')
        if treatment=='reviewed_nonnegative_group_exposure' and a['accounting_net_carrying']<0.:
            financial.exact(a['policy_exposure'],0.,'explicit reviewed zero exposure')
    financial.exact(actual['exposure_policy'],request['exposure_policy'],'reviewed exposure policy')
    financial.exact(actual['signed_exposure_research_only'],bool(research_scopes),'research signed exposure disclosure')
    financial.exact(actual['regulatory_credit_rwa_approval'],False,'no regulatory approval')
    audits=[]; capitals=[]
    for key,a in sorted(amounts.items()):
        p=policies[key]; equity=a[2]+a[3]+a[4]; capital=deepcopy(p['capital'])
        capital.update(scenario=key[0],common_equity=a[2]-p['equity_exclusions'],retained_earnings=a[3],eligible_aoci=a[4] if p['include_aoci'] else 0.,credit_rwa=a[5],total_leverage_exposure=a[0]+p['leverage_addon']-p['leverage_deductions'],tangible_common_equity=equity-p['equity_exclusions']-p['intangible_assets'],tangible_assets=a[0]-p['intangible_assets'])
        capitals.append(capital)
        audits.append(dict(scenario=key[0],account=key[1],entity=capital['entity'],currency=capital['currency'],assets=a[0],liabilities=a[1],book_equity=equity,common_equity=capital['common_equity'],retained_earnings=a[3],book_aoci=a[4],eligible_aoci=capital['eligible_aoci'],equity_exclusions=p['equity_exclusions'],credit_rwa=a[5],reconciliation_error=a[6]))
    financial.equal(actual['capital_bridge'],audits,'native closing capital independent GL')
    financial.equal(actual['rwa_contributions'],contributions,'signed carrying/RWA and explicit weights')
    financial.equal(actual['prepared_specification']['capital'],capitals,'prepared eligible amounts')
    metrics=[]
    for capital in capitals:
        cet1=capital['common_equity']+capital['retained_earnings']+capital['eligible_aoci']-capital['cet1_deductions']
        tier1=cet1+capital['additional_tier1']; total=tier1+capital['tier2']
        rwa=capital['credit_rwa']+capital['market_rwa']+capital['operational_rwa']
        exposure=capital['total_leverage_exposure']
        components=[('cet1',cet1,rwa),('tier1',tier1,rwa),('total_capital',total,rwa),
            ('advanced_cet1',cet1,capital['advanced_rwa']),('advanced_tier1',tier1,capital['advanced_rwa']),('advanced_total_capital',total,capital['advanced_rwa']),
            ('tier1_leverage',tier1,capital['average_assets']),('slr',tier1,exposure),('tlac_rwa',capital['eligible_tlac'],rwa),('tlac_leverage',capital['eligible_tlac'],exposure),
            ('ltd_rwa',capital['eligible_ltd'],rwa),('ltd_leverage',capital['eligible_ltd'],exposure),('tce_ta',capital['tangible_common_equity'],capital['tangible_assets'])]
        for label,numerator,denominator in components:
            denominator=denominator if denominator is not None and denominator>0 else None
            rule=capital['requirements'].get(label)
            ratio=numerator/denominator if denominator is not None else None
            required=rule['minimum']+rule['buffer'] if rule else None
            management=required+rule['management'] if rule else None
            head=numerator-required*denominator if required is not None and denominator is not None else None
            management_head=numerator-management*denominator if management is not None and denominator is not None else None
            status='unavailable' if denominator is None else 'unconfigured' if rule is None else 'breach' if head<0 else 'below_management_target' if management_head<0 else 'pass'
            if label in ('cet1','tier1','total_capital','tlac_rwa','ltd_rwa') and any(policies[key]['capital']['entity']==capital['entity'] and policies[key]['capital']['currency']==capital['currency'] and policies[key]['capital']['period']==capital['period'] and key[0]==capital['scenario'] for key in research_scopes):status='research_only_signed_exposure'
            metrics.append({k:capital[k] for k in ('entity','currency','scenario','period')} | dict(metric=label,numerator=numerator,denominator=denominator,ratio=ratio,minimum=rule['minimum'] if rule else None,buffer=rule['buffer'] if rule else None,required_ratio=required,management_target=management,headroom=head,management_headroom=management_head,headroom_bp=(ratio-required)*10000 if ratio is not None and required is not None else None,status=status))
    financial.equal(actual['capital_metrics'],metrics,'thirteen independent native ratios/statuses')
    if actual['ftp_positions'] or actual['ftp_reconciliation']:
        raise ArithmeticError('closing source may not infer modeled FTP')
    for closing in (r for f in frames.frames('closing_statements') for r in f.iter_rows(named=True)):
        audit=next(a for a in audits if (a['scenario'],a['account'])==(closing['scenario'],closing['account']))
        for x,y in [('assets','assets'),('liabilities','liabilities'),('book_equity','equity')]: financial.near(audit[x],closing[y]*multiplier,'verified native closing statement')
