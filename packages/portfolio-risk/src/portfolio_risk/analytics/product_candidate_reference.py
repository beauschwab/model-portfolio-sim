"""Independent acquisition-aware candidate journal publication verification.

This is an oracle only. It cannot execute a production book or infer reviewed
accounting policies, observed behavior, mixed-book acceptance or optimality.
"""
from copy import deepcopy
from dataclasses import asdict
import json
import math
import sys

from . import product_strategy_reference as product
from .balance_stress import Account, Position, Scenario, _decode

SCHEMA = 'product-candidate-journal-1'
MAX_DAILY_ROWS = 250_000
MAX_WORK = 8_000_000
MAX_CAPACITY_BYTES = 128 * 1024 * 1024


def _contractual_interest_sign(path, check=lambda: False):
    """Economic coupon direction from full earned/cash source, never EIR."""
    sign = 0.
    for i, entry in enumerate((*path['recognition'], *path['cash'])):
        if i % 256 == 0 and check():
            raise InterruptedError('candidate contractual direction cancelled')
        value = product._finite(entry['interest'])
        if value:
            next_sign = 1. if value > 0. else -1.
            if sign and sign != next_sign:
                raise ArithmeticError('candidate mixed-sign contractual interest requires gross direction ownership')
            sign = next_sign
    return sign or 1.


def _right(side, coupon_sign, accrued):
    natural = coupon_sign * accrued
    if side * coupon_sign > 0.:
        return 'dated_interest_receivable' if natural >= 0. else 'dated_deferred_income'
    return 'dated_interest_payable' if natural >= 0. else 'dated_prepaid_expense'


def _keys(value, names, path):
    if not isinstance(value, dict) or set(value) != set(names.split()):
        raise ArithmeticError(path + ': exact contract fields required')


def _size(value):
    return len(json.dumps(value, allow_nan=False, separators=(',', ':'), ensure_ascii=False).encode())


def _journal_admission(raw, preparation, ends, source_rows, source_work):
    # No daily source arrays or postings exist before this aggregate admission.
    full = max([raw['reporting_end_ordinal']] + ends)
    states = sum(end - raw['as_of_ordinal'] + 1 for end in ends)
    accounts = (full - raw['as_of_ordinal'] + 1) * len(raw['specification']['accounts']) * len(preparation['scenario_ids'])
    lines = source_rows * 12 + len(ends) * 4
    work = source_work + states * 8 + accounts * 4 + lines
    wrapper = dict(schema='product-strategy-preparation-1', units=_unit_identities(raw['product'], preparation), preparation=preparation)
    # Escape-aware lineage sizing: every repeated source/day record is charged
    # for its complete metadata, including the entire possible origin list.
    # Prepared values are already independently raw-source verified here; this
    # use is strictly cardinality/label sizing, never financial source truth.
    path_capacity = 0
    index = 0
    for ui, unit in enumerate(preparation['units']):
        kind = raw['product']['units'][ui]['purchase_graph']['kind']
        for income in unit['income']:
            end = ends[index]; index += 1
            scene = income['scenario_id']; path = income['path_id']
            source = next(s for s in unit['complete_purchase_result']['native_sources'] if s['role'] == 'physical' and s['scenario_id'] == scene and s['path_id'] == path)
            projection = income['native_income']['complete_source_projection']
            recognition = projection['interest' if kind == 'deposit' else 'recognition']
            cash = projection['cashflows']
            origins = [f"deposit:{r['segment_id']}:{r['start_ordinal']}:{r['end_ordinal']}" if kind == 'deposit' else r['origin_id'] for r in recognition]
            cash_ids = [c['event_id'] for c in cash]
            request = income['native_income']['raw_income_request']
            mapping = next(m for m in raw['run_mapping'] if m['source_scenario_id'] == scene)
            metadata = [raw['owners'][ui], raw['policy'], mapping, unit['unit_id'], scene, path, source['run_id'], raw['product']['currency'], request['contract'], request['original_yield_anchor'], request['policy'], origins, cash_ids, f"{unit['unit_id']}:{scene}:{path}:income:{end}"]
            width = 4 * _size(metadata) + 4096
            records = (end - unit['purchase_ordinal']) * 8 + (end - raw['as_of_ordinal'] + 1) + 12 + len(cash) + len(recognition)
            path_capacity += records * width
    if index != len(ends):
        raise ArithmeticError('candidate admitted path cardinality differs')
    account_capacity = 0
    for scene in preparation['scenario_ids']:
        for account in raw['specification']['accounts']:
            width = 4 * _size([account, scene]) + 2048
            account_capacity += (full - raw['as_of_ordinal'] + 1 + 5) * width
    capacity = path_capacity + account_capacity + (_size(raw) + _size(wrapper)) * 4
    if states > MAX_DAILY_ROWS or accounts > MAX_DAILY_ROWS or lines > 2_000_000 or work > MAX_WORK or capacity > MAX_CAPACITY_BYTES:
        raise ArithmeticError('candidate aggregate journal/state/output work admission exceeded before daily generation')
    return full, work


def _unit_identities(raw, preparation):
    reports = raw['reporting_ordinals']
    return [dict(unit_id=u['unit_id'], template_id=u['template_id'], purchase_ordinal=u['purchase_ordinal'], purchase_period=reports[:-1].index(u['purchase_ordinal']), side=u['side'], accounting_category=u['accounting_category'], capital_policy_revision=u['capital_policy_revision'], quote_price_per_face_unit=u['quote_price_per_face_unit']) for u in preparation['units']]


def _equal(actual, expected, path):
    # The shared financial comparator preserves typed dates/identities and the
    # final 1e-12 relative + 1e-9 absolute financial source tolerances.
    if isinstance(expected,dict):
        if not isinstance(actual,dict) or actual.keys()!=expected.keys():raise ArithmeticError(path+': fields differ')
        for k,v in expected.items():_equal(actual[k],v,path+'/'+k)
    elif isinstance(expected,list):
        if not isinstance(actual,list) or len(actual)!=len(expected):raise ArithmeticError(path+': length differs')
        for i,v in enumerate(expected):_equal(actual[i],v,path+'/'+str(i))
    elif isinstance(expected,int) and not isinstance(expected,bool) and any(k in path.split('/') for k in ('transaction_id','unit_index','native_source_index','full_end_ordinal','admitted_work','admitted_daily_rows')):
        if type(actual) is not int or actual!=expected:raise ArithmeticError(path+': exact integer differs')
    else:product._equal(actual,expected,path)


def _spec(raw):
    allowed={'version','provenance','horizon_days','accounts','positions','scenarios','reverse_severities','ruleset','netting_sets','policies','cashflows'}
    if set(raw)-allowed or raw.get('version')!='balance-stress-2':
        raise ArithmeticError('candidate raw specification version/fields differ')
    if any(raw.get(k,[]) for k in ('netting_sets','policies','cashflows')):
        raise ArithmeticError('candidate mixed/legacy owners unsupported')
    normalized=dict(horizon_days=raw.get('horizon_days',360),accounts=[asdict(_decode(Account,x)) for x in raw.get('accounts',[])],positions=[asdict(_decode(Position,x)) for x in raw.get('positions',[])],scenarios=[asdict(_decode(Scenario,x)) for x in raw.get('scenarios',[])],netting_sets=[],policies=[],cashflows=[],reverse_severities=deepcopy(raw.get('reverse_severities',[])),ruleset=raw.get('ruleset','research-weights-v2'))
    if not 30<=normalized['horizon_days']<=1080 or type(normalized['horizon_days']) is not int or normalized['ruleset']!='research-weights-v2':
        raise ArithmeticError('candidate specification calendar/ruleset unsupported')
    for key,identity in [('accounts','id'),('positions','id'),('scenarios','name')]:
        ids=[x[identity] for x in normalized[key]]
        if len(ids)!=len(set(ids)) or any(not x.strip() for x in ids):raise ArithmeticError('candidate unique source/account identity required')
    if not 1<=len(normalized['accounts'])<=50 or len(normalized['positions'])>256 or not normalized['scenarios']:
        raise ArithmeticError('candidate specification count admission exceeded')
    for s in normalized['scenarios']:
        if s['name']=='baseline' or s['start_day']<1:
            raise ArithmeticError('candidate scenario identity/start day domain differs')
        for field in ('rate_shift','spread_shift'):
            if not -.5<=s[field]<=.5:raise ArithmeticError('candidate scenario rate domain differs')
        for field in ('deposit_flight','lgd_add','rollover_loss','haircut_add'):
            if not 0.<=s[field]<=1.:raise ArithmeticError('candidate scenario fraction domain differs')
        for field in ('pd_multiplier','migration_multiplier','draw_multiplier','market_shock'):
            if not 0.<=s[field]<=20.:raise ArithmeticError('candidate scenario multiplier domain differs')
    reverse=normalized['reverse_severities']
    if len(reverse)>12 or any(isinstance(v,bool) or not isinstance(v,(int,float)) or not math.isfinite(v) or not 0.<=v<=5. for v in reverse) or any(a>=b for a,b in zip(reverse,reverse[1:])):
        raise ArithmeticError('candidate reverse severity domain differs')
    return normalized


def _daily_income(u, scenario, path, cash, recognition, end, currency, check):
    start=u['purchase_ordinal'];y=u['income_policy']['original_annual_effective_rate'];source=dict(id=u['unit_id'],revision=u['income_policy']['review']['revision'])
    principal=u['unit_face'];carrying=principal*u['quote_price_per_face_unit'];accrued=0.;events=[];rows=[];previous=start
    for day in range(start+1,end+1):
        if check():raise InterruptedError('candidate daily source verification cancelled')
        earned=0.;pc=ic=0.
        for r in recognition:
            if r['start_ordinal']<day<=r['end_ordinal']:earned+=r['interest']/(r['end_ordinal']-r['start_ordinal'])
        for c in cash:
            if c['ordinal']==day:pc+=c['principal'];ic+=c['interest']
        event=dict(event_id=f"{u['unit_id']}:{scenario}:{path}:income:{day}",source=source,payment_ordinal=day,contractual_interest_earned=earned,principal_cash=pc,interest_cash=ic,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None)
        opening=carrying;effective=opening*math.expm1(math.log1p(y)*(day-previous)/365.)
        carrying+=effective-pc-ic;principal-=pc;accrued+=earned-ic
        if principal<0.:raise ArithmeticError('candidate repayment exceeds original principal')
        roundoff=64*sys.float_info.epsilon*(abs(opening)+abs(pc+ic)+abs(effective))
        if carrying<0. and principal==0. and abs(accrued)<=roundoff and abs(carrying)<=roundoff:carrying=0.
        if carrying<0.:raise ArithmeticError('negative candidate gross carry requires reviewed retirement owner')
        rows.append(dict(event_id=event['event_id'],source=source,payment_ordinal=day,opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=effective,contractual_interest_earned=earned,basis_amortization=effective-earned,modification_gain_loss=0.,recognized_nonintegral_fee_income=0.,cash_received=pc+ic,closing_principal=principal,closing_accrued_interest=accrued,closing_gross_carrying_value=carrying,closing_book_adjustment=carrying-principal-accrued,yield_review=None));events.append(event);previous=day
    if principal!=0. or abs(accrued)>1e-9 or abs(carrying)>1e-9:
        raise ArithmeticError('candidate complete daily principal/accrual/carry tail does not close')
    common=dict(schema='dated-income-1',contract=source,original_yield_anchor=u['income_policy']['original_yield_anchor'],policy=u['income_policy']['review'],currency=currency,monetary_unit='currency_units',measure='physical')
    request=dict(common,as_of_ordinal=start,full_contract_end_ordinal=end,opening_principal=u['unit_face'],opening_accrued_interest=0.,opening_gross_carrying_value=u['unit_face']*u['quote_price_per_face_unit'],original_annual_effective_rate=y,opening_annual_effective_rate=y,events=events)
    result=dict(common,rows=rows,remaining_principal=principal,remaining_accrued_interest=accrued,remaining_gross_carrying_value=carrying)
    return request,result


def _capture(raw, preparation, check):
    units=raw['product']['units'];allocation=raw['allocation'];paths=[];admitted_work=daily_rows=0;full=raw['reporting_end_ordinal'];inputs=[]
    # Preparation has already been independently source-verified. Its row
    # cardinalities are now safe admission metadata, never cash/EIR truth.
    pre_work=pre_rows=0;pre_ends=[]
    for ui,receipt in enumerate(preparation['units']):
        for income in receipt['income']:
            day=income['native_income']['native_income_result']['rows'][-1]['payment_ordinal']
            days=day-receipt['purchase_ordinal'];p=income['native_income']['complete_source_projection'];kind=units[ui]['purchase_graph']['kind']
            pre_ends.append(day)
            pre_rows+=days;pre_work+=days*(len(p['interest' if kind=='deposit' else 'recognition'])+len(p['cashflows'])+24)
            if not 1<=days<=4095 or pre_rows>MAX_DAILY_ROWS or pre_work>MAX_WORK:raise ArithmeticError('candidate aggregate source/work admission exceeded before daily generation')
    _journal_admission(raw, preparation, pre_ends, pre_rows, pre_work)
    for ui,u in enumerate(units):
        graph=u['purchase_graph']['request'];kind=u['purchase_graph']['kind'];contract=product._contract(graph,kind)
        for si,(scene,histories) in enumerate(zip(graph['scenarios'],product._histories(graph,kind),strict=True)):
            for history in histories['physical']:
                if check():raise InterruptedError('candidate capture verification cancelled')
                count=history['request']['day_count'] if kind=='deposit' else contract['day_count']
                if count not in {'act360','act365_fixed'}:raise ArithmeticError('candidate daily recognition supports ACT360/ACT365 only')
                opening=(sum(x['opening_balance'] for x in history['request']['segments']) if kind=='deposit' else contract['opening_principal'] if kind=='consumer_installment' else sum(contract['opening_principal' if kind=='card' else 'opening_investor_principal']))
                if opening!=u['unit_face']:raise ArithmeticError('candidate actual face must equal original principal')
                budget=product._Budget(check);projection=product._project(graph,kind,history,scene,'physical',budget)
                cash=product._cash(projection,kind);recognition=[]
                for r in projection['interest' if kind=='deposit' else 'recognition']:
                    a=r['start_ordinal'];b=r['end_ordinal']
                    if b<=a or a<u['purchase_ordinal']:raise ArithmeticError('candidate recognition interval outside complete tail')
                    recognition.append(dict(origin_id=f"deposit:{r['segment_id']}:{a}:{b}" if kind=='deposit' else r['origin_id'],start_ordinal=a,end_ordinal=b,interest=r['amount' if kind=='deposit' else 'interest']))
                end=max([raw['reporting_end_ordinal']]+[c['ordinal'] for c in cash]+[r['end_ordinal'] for r in recognition]);days=end-u['purchase_ordinal']
                if any(c['ordinal']<=u['purchase_ordinal'] or c['ordinal']>end or c['principal']<0. for c in cash):
                    raise ArithmeticError('candidate same-day acquisition cash, draws or out-of-tail settlement unsupported')
                if not 1<=days<=4095:raise ArithmeticError('candidate complete daily tail admission exceeded')
                daily_rows+=days;admitted_work+=days*(len(recognition)+len(cash)+24);full=max(full,end)
                # Source indices include every earlier physical/Q/down/up history.
                native_index=sum(len(h[role]) for h in product._histories(graph,kind)[:si] for role in product.ROLES)+histories['physical'].index(history)
                inputs.append((ui,u,kind,scene,history,native_index,cash,recognition,end))
    if daily_rows>MAX_DAILY_ROWS or admitted_work>MAX_WORK:raise ArithmeticError('candidate aggregate daily source/work admission exceeded')
    _journal_admission(raw, preparation, [x[-1] for x in inputs], daily_rows, admitted_work)
    for ui,u,kind,scene,h,index,cash,recognition,end in inputs:
        request,income=_daily_income(u,scene['scenario_id'],h['path_id'],cash,recognition,end,raw['product']['currency'],check)
        factor=allocation[ui]['face']/u['unit_face']
        direction=_contractual_interest_sign(dict(cash=cash,recognition=recognition),check)
        paths.append(dict(unit_index=ui,native_source_index=index,unit_id=u['unit_id'],product=kind,scenario_id=scene['scenario_id'],run_id=scene['run_id'],path_id=h['path_id'],source_weight=h['weight'],allocated_face=allocation[ui]['face'],unit_face=u['unit_face'],allocation_factor=factor,side=-1. if kind=='deposit' else 1.,interest_direction=direction,purchase_ordinal=u['purchase_ordinal'],full_end_ordinal=end,cash=cash,recognition=recognition,daily_income_request=request,daily_income=income))
    return dict(schema='product-candidate-source-1',paths=paths,admitted_work=admitted_work,admitted_daily_rows=daily_rows,scope='immutable supplied physical performing purchase/cash/recognition with reviewed original-EIR daily source; pricing histories retained separately; no observed cash, accounting-standard or mixed ledger verification claim'),full


def _admit(raw, result, check):
    if check():raise InterruptedError('candidate verification cancelled')
    _keys(raw, 'schema product allocation as_of_ordinal reporting_end_ordinal owners policy run_mapping specification verified_opening_identity', 'candidate/raw')
    _keys(result, 'schema raw_request normalized_specification native_preparation source_capture journals daily_positions daily_accounts reported_positions complete_closing_positions reported_accounts complete_closing_accounts trial_balance full_end_ordinal admitted_work independent_persisted_verification dynamic_mixed_acceptance scope', 'candidate/result')
    if _size(raw)>32*1024*1024 or _size(result)>32*1024*1024:
        raise ArithmeticError('candidate input/result byte admission exceeded')
    _equal(result['raw_request'],raw,'candidate/raw_source')
    if raw.get('schema')!=SCHEMA or raw['product']['monetary_unit']!='currency_units':raise ArithmeticError('candidate explicit version and currency units required')
    units=raw['product']['units'];owners=raw['owners'];allocation=raw['allocation'];policy=raw['policy'];maps=raw['run_mapping']
    _keys(policy, 'identity book_basis acquisition_phase fees defaults marks tail', 'candidate/policy')
    for o in owners:_keys(o, 'unit_id position_id account_id source ownership accounting_category ledger_classification pre_purchase_principal pre_purchase_accrued pre_purchase_basis', 'candidate/owner')
    for a in allocation:_keys(a, 'unit_id face', 'candidate/allocation')
    for m in maps:_keys(m, 'source_scenario_id ledger_scenario_id severity', 'candidate/run_mapping')
    expected_policy=dict(book_basis='reviewed_original_eir-1',acquisition_phase='after_verified_opening_before_recognition-1',fees='reject',defaults='reject',marks='reject',tail='retain_complete-1')
    product._fields(policy,expected_policy,'candidate/reviewed_policy')
    if len(owners)!=len(units) or len(allocation)!=len(units) or not 1<=len(units)<=256 or not 1<=len(maps)<=13:raise ArithmeticError('candidate complete bounded ordered ownership required')
    for identity in [policy['identity'],raw['verified_opening_identity']]+[o['source'] for o in owners]:
        if set(identity)!={'id','revision'} or any(not isinstance(v,str) or not v.strip() or len(v.encode())>120 or any(ord(c)<32 or ord(c)==127 for c in v) for v in identity.values()):raise ArithmeticError('candidate reviewed/source identity required')
    for field in ('unit_id','position_id'):
        if len({o[field] for o in owners})!=len(owners):raise ArithmeticError('candidate unique owner mapping required')
    if len({(o['source']['id'],o['source']['revision']) for o in owners})!=len(owners):raise ArithmeticError('candidate unique source mapping required')
    spec=_spec(raw['specification']);_equal(result['normalized_specification'],spec,'candidate/normalized_spec')
    if spec['horizon_days']!=raw['reporting_end_ordinal']-raw['as_of_ordinal'] or len(spec['positions'])!=len(owners):raise ArithmeticError('candidate reporting/spec placeholder coverage differs')
    reports=raw['product']['reporting_ordinals']
    if reports[0]!=raw['as_of_ordinal'] or reports[-1]!=raw['reporting_end_ordinal']:raise ArithmeticError('candidate immutable report grid differs')
    positions={p['id']:p for p in spec['positions']};accounts={a['id']:a for a in spec['accounts']}
    for u,o,a in zip(units,owners,allocation,strict=True):
        if (o['unit_id']!=u['unit_id'] or a['unit_id']!=u['unit_id'] or o['accounting_category']!=u['accounting_category'] or o['ownership']!='native_product_candidate-1'
                or o['ledger_classification'] not in {'ac','htm'} or any(o[k]!=0. for k in ('pre_purchase_principal','pre_purchase_accrued','pre_purchase_basis'))):raise ArithmeticError('candidate zero pre-purchase source/category mapping required')
        face=product._finite(a['face'])
        if face<0.:raise ArithmeticError('candidate allocated face must be nonnegative')
        p=positions[o['position_id']];account=accounts[o['account_id']];deposit=u['purchase_graph']['kind']=='deposit'
        zeros='balance opening_accrued book_adjustment allowance start_day rate annual_pd watch_fraction migration_rate commitment draw_fraction monthly_runoff floating_beta rollover eligible_fraction encumbered_fraction pledged_face maturity_day'.split()
        if (p['account']!=o['account_id'] or account['currency']!=raw['product']['currency'] or p['classification']!=o['ledger_classification']
                or p['kind'] not in ({'deposit'} if deposit else {'loan','security'}) or any(p[k]!=0. for k in zeros) or p['collateral_position'] or p['collateral_pool'] or p['opening_market_price']!=1.):raise ArithmeticError('candidate generic/legacy financial ownership overlap')
        graph=u['purchase_graph']['request'];kind=u['purchase_graph']['kind'];contract=product._contract(graph,kind)
        # Every supplied purchase role describes the same face being acquired.
        # Quote ratios cannot substitute for checking original gross principal.
        for histories in product._histories(graph,kind):
            for role in product.ROLES:
                for history in histories[role]:
                    count=history['request']['day_count'] if deposit else contract['day_count']
                    opening=(sum(s['opening_balance'] for s in history['request']['segments']) if deposit else contract['opening_principal'] if kind=='consumer_installment' else sum(contract['opening_principal' if kind=='card' else 'opening_investor_principal']))
                    if count not in {'act360','act365_fixed'} or opening!=u['unit_face']:
                        raise ArithmeticError('candidate every P/Q opening face and daily recognition calendar must match actual purchase')
    for account in accounts.values():
        if account['currency']!=raw['product']['currency'] or any(account[k]!=0. for k in ('annual_fees','annual_costs','tax_rate','annual_dividends','capital_deductions')) or account['cash']<0. or abs(account['cash']-account['equity'])>1e-8*max(1.,account['cash']):raise ArithmeticError('candidate account opening cash/equity or reviewed policy differs')
    actual={(m['ledger_scenario_id'],m['severity']) for m in maps};expected={('baseline',1.)}|{(s['name'],severity) for s in spec['scenarios'] for severity in [1.]+spec['reverse_severities']}
    sourceids=[m['source_scenario_id'] for m in maps]
    if len(actual)!=len(maps) or len(sourceids)!=len(set(sourceids)) or actual!=expected:raise ArithmeticError('candidate exact scenario/reverse-run coverage required')
    for m in maps:
        if (m['source_scenario_id']==raw['product']['base_scenario_id'])!=(m['ledger_scenario_id']=='baseline'):raise ArithmeticError('candidate baseline mapping differs')
    preparation=result['native_preparation'];_equal(preparation['schema'],'product-strategy-preparation-1','candidate/preparation/schema')
    product.verify_preparation(raw['product'],preparation['preparation'],check)
    _keys(preparation, 'schema units preparation', 'candidate/preparation')
    expected_ids=_unit_identities(raw['product'],preparation['preparation'])
    _equal(preparation['units'],expected_ids,'candidate/ordered_unit_identity')
    if set(sourceids)!=set(preparation['preparation']['scenario_ids']):raise ArithmeticError('candidate native source scenario coverage differs')
    return spec,preparation['preparation']


def _replay(raw, spec, capture, full, scenarios, check):
    journals=[];daily_positions=[];daily_accounts=[];reported_positions=[];reported_accounts=[];closing_positions=[];closing_accounts=[];trial=[]
    def principal(side):return 'asset_principal' if side>0. else 'funding_principal'
    def basis(side):return 'asset_book_adjustment' if side>0. else 'funding_book_adjustment'
    def posting(code,value):return dict(gl_account=code,value=value)
    def rights(side,coupon_sign,old,new):return [posting(_right(side,coupon_sign,old),-side*old),posting(_right(side,coupon_sign,new),side*new)]
    signs={id(p):_contractual_interest_sign(p,check) for p in capture['paths']}
    for scenario in scenarios:
        if check():raise InterruptedError('candidate scenario verification cancelled')
        mapping=next(m for m in raw['run_mapping'] if m['source_scenario_id']==scenario)
        gl={}
        for a in spec['accounts']:gl[a['id'],'','','cash']=a['cash'];gl[a['id'],'','','equity']=-a['equity']
        paths=[p for p in capture['paths'] if p['scenario_id']==scenario]
        states=[dict(source_scenario_id=scenario,ordinal=raw['as_of_ordinal'],unit_id=p['unit_id'],position_id=raw['owners'][p['unit_index']]['position_id'],account_id=raw['owners'][p['unit_index']]['account_id'],path_id=p['path_id'],source_weight=p['source_weight'],allocated_principal=0.,accrued_interest=0.,book_adjustment=0.,gross_carrying_value=0.,effective_interest_to_date=0.,acquired=False,full_end_ordinal=p['full_end_ordinal']) for p in paths]
        def append(p,owner,day,phase,ids,postings):
            net=sum(x['value'] for x in postings);magnitude=sum(abs(x['value']) for x in postings)
            if abs(net)>1e-9+1e-12*magnitude:raise ArithmeticError('independent candidate journal unbalanced')
            for line in postings:
                product._finite(line['value']);pooled=line['gl_account'] in {'cash','equity','pnl:effective_interest'}
                key=(owner['account_id'],'' if pooled else owner['position_id'],'' if pooled else p['path_id'],line['gl_account']);gl[key]=product._finite(gl.get(key,0.)+line['value'])
            journals.append(dict(transaction_id=len(journals)+1,source_scenario_id=scenario,ledger_scenario_id=mapping['ledger_scenario_id'],severity=mapping['severity'],ordinal=day,phase=phase,unit_id=p['unit_id'],position_id=owner['position_id'],account_id=owner['account_id'],source=owner['source'],path_id=p['path_id'],source_weight=p['source_weight'],allocation_factor=p['allocation_factor'],native_source_index=p['native_source_index'],native_event_ids=ids,policy=raw['policy']['identity'],postings=postings))
        for day in range(raw['as_of_ordinal'],full+1):
            if check():raise InterruptedError('candidate daily posting verification cancelled')
            for p,state in zip(paths,states,strict=True):
                owner=raw['owners'][p['unit_index']];factor=p['allocation_factor']*p['source_weight'];side=p['side']
                if day==p['purchase_ordinal']:
                    face=p['unit_face']*factor;cost=p['daily_income_request']['opening_gross_carrying_value']*factor;adj=cost-face
                    append(p,owner,day,'acquisition',['purchase:'+p['unit_id']],[posting(principal(side),side*face),posting(basis(side),side*adj),posting('cash',-side*cost)])
                    state.update(allocated_principal=face,book_adjustment=adj,gross_carrying_value=cost,acquired=True)
                elif p['purchase_ordinal']<day<=p['full_end_ordinal']:
                    row=p['daily_income']['rows'][day-p['purchase_ordinal']-1];earned=row['contractual_interest_earned']*factor;effective=row['effective_interest']*factor;amort=row['basis_amortization']*factor
                    new=state['accrued_interest']+earned
                    ids=[r['origin_id'] for r in p['recognition'] if r['start_ordinal']<day<=r['end_ordinal']]+[row['event_id']]
                    append(p,owner,day,'recognition',ids,rights(side,signs[id(p)],state['accrued_interest'],new)+[posting(basis(side),side*amort),posting('pnl:effective_interest',-side*effective)])
                    state['accrued_interest']=new;state['book_adjustment']+=amort;state['effective_interest_to_date']+=effective
                    cash=[c for c in p['cash'] if c['ordinal']==day]
                    if cash:
                        pc=ic=0.
                        for c in cash:pc+=c['principal']*factor;ic+=c['interest']*factor
                        new=state['accrued_interest']-ic
                        append(p,owner,day,'settlement',[c['event_id'] for c in cash],[posting('cash',side*(pc+ic)),posting(principal(side),-side*pc)]+rights(side,signs[id(p)],state['accrued_interest'],new))
                        state['allocated_principal']-=pc;state['accrued_interest']=new
                    state['gross_carrying_value']=state['allocated_principal']+state['accrued_interest']+state['book_adjustment']
                    for statekey,rowkey in [('allocated_principal','closing_principal'),('accrued_interest','closing_accrued_interest'),('book_adjustment','closing_book_adjustment'),('gross_carrying_value','closing_gross_carrying_value')]:_equal(state[statekey],row[rowkey]*factor,'candidate/daily_money_conservation')
                if day<=p['full_end_ordinal']:
                    state['ordinal']=day
                    for code,want in [(principal(side),side*state['allocated_principal']),(basis(side),side*state['book_adjustment'])]:_equal(gl.get((owner['account_id'],owner['position_id'],p['path_id'],code),0.),want,'candidate/position_gl')
                    total=sum(gl.get((owner['account_id'],owner['position_id'],p['path_id'],code),0.) for code in ('dated_interest_receivable','dated_interest_payable','dated_prepaid_expense','dated_deferred_income'))
                    _equal(total,side*state['accrued_interest'],'candidate/gross_rights_gl')
                    daily_positions.append(deepcopy(state))
                    if day==raw['reporting_end_ordinal']:reported_positions.append(deepcopy(state))
            for a in spec['accounts']:
                cash=gl.get((a['id'],'','','cash'),0.);income=-gl.get((a['id'],'','','pnl:effective_interest'),0.)
                if cash<a['cash_floor']-1e-9:raise ArithmeticError('candidate end-of-day cash floor fails')
                _equal(sum(v for k,v in sorted(gl.items()) if k[0]==a['id']),0.,'candidate/account_gl_balance')
                state=dict(source_scenario_id=scenario,ordinal=day,account_id=a['id'],cash=cash,cumulative_net_effective_income=income,equity=a['equity']+income);daily_accounts.append(state)
                if day==raw['reporting_end_ordinal']:reported_accounts.append(state)
                if day==full:closing_accounts.append(state)
        for state in states:
            state['ordinal']=full
            for field in ('allocated_principal','accrued_interest','gross_carrying_value'):_equal(state[field],0.,'candidate/complete_tail_closure')
            closing_positions.append(state)
        for (account,position,path,code),value in sorted(gl.items()):trial.append(dict(source_scenario_id=scenario,account_id=account,position_id=position,path_id=path,gl_account=code,balance=value))
    return dict(journals=journals,daily_positions=daily_positions,daily_accounts=daily_accounts,reported_positions=reported_positions,complete_closing_positions=closing_positions,reported_accounts=reported_accounts,complete_closing_accounts=closing_accounts,trial_balance=trial)


def verify(raw_request, result, check=lambda:False):
    """Independently reconstruct original product sources and every ordered posting."""
    spec,prepared=_admit(raw_request,result,check)
    capture,full=_capture(raw_request,prepared,check)
    _equal(result['source_capture'],capture,'candidate/source_capture')
    _,work=_journal_admission(raw_request,prepared,[p['full_end_ordinal'] for p in capture['paths']],capture['admitted_daily_rows'],capture['admitted_work'])
    expected=_replay(raw_request,spec,capture,full,prepared['scenario_ids'],check)
    for key,value in expected.items():_equal(result[key],value,'candidate/'+key)
    _equal(result['schema'],SCHEMA,'candidate/schema');_equal(result['full_end_ordinal'],full,'candidate/full_end_ordinal');_equal(result['admitted_work'],work,'candidate/admitted_work')
    if result['independent_persisted_verification'] is not False or result['dynamic_mixed_acceptance'] is not False:raise ArithmeticError('candidate output cannot claim mixed/persisted verification')
    _equal(result['scope'],'standalone acquisition-aware modeled physical expected candidate journal; native raw opening reconciliation and complete original-yield/cash tails; supplied opening identity is provenance only; no independent persisted verification, observed settlement, mixed existing-source composition, regulatory or empirical acceptance','candidate/scope')
    if check():raise InterruptedError('candidate final verification cancelled')
    return dict(verified=True,product_source_recomputed=True,acquisition_postings_recomputed=True,ordered_candidate_journal_recomputed=True,complete_cash_rights_basis_tail_verified=True,standalone_candidate_only=True,dynamic_mixed_acceptance=False,global_optimality_proven=False,empirical_validation=False)
