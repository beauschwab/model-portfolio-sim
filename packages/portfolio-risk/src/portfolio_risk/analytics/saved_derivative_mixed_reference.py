"""Independent raw saved-derivative/dated source and partitioned single-GL verifier.
Never a production financial backend. Retains source control and current GL only;
persisted journal/path partitions are reread as bounded streams.
"""
from collections import defaultdict
from copy import deepcopy
from dataclasses import dataclass
import json,math,hashlib
from .saved_derivative_mapping_reference import exact,require,verify_with_check as verify_saved
from .saved_derivative_financial_reference import checked_derive
from .saved_derivative_work_bounds_reference import standalone_upper
from .saved_derivative_dated_reference import verify_dated_sources,DATED_SCHEMAS
from .saved_derivative_identity import encoded,loads,probe,exact_identity
from portfolio_risk.analytics.balance_stress import Account,Position,Scenario
from portfolio_risk.analytics.balance_stream import replay_partitions
_SEAL=object()
GL = {
    'cash': 'cash', 'opening_equity': 'opening_equity',
    'derivative_asset': 'derivative_asset', 'derivative_liability': 'derivative_liability',
    'unpaid_receivable': 'derivative_unpaid_receivable', 'unpaid_payable': 'derivative_unpaid_payable',
    'posted_vm_asset': 'derivative_posted_vm_asset', 'received_vm_liability': 'derivative_received_vm_liability',
    'posted_im_asset': 'derivative_posted_im_asset', 'mark_gain_loss': 'pnl:derivative_mark',
    'contractual_income': 'pnl:derivative_contractual_income',
    'collateral_interest_income': 'pnl:derivative_collateral_interest',
    'counterparty_loss_expense': 'pnl:derivative_counterparty_loss',
    'posted_collateral_return_loss_expense': 'pnl:derivative_collateral_return_loss',
    'memo_custodian_asset': 'memo:derivative_custodian_asset',
    'memo_custodian_obligation': 'memo:derivative_custodian_obligation',
}
LIABILITIES = {'derivative_liability', 'derivative_unpaid_payable',
                'derivative_received_vm_liability', 'funding_principal',
                'dated_interest_payable', 'dated_deferred_income'}


def near(actual, expected, label):
    require(type(actual) in (int, float) and math.isfinite(actual) and
            math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-9), label)


def book_key(book):
    return json.dumps(book['identity'], separators=(',', ':'), ensure_ascii=False)


def instrument(key, position=None, bucket=None):
    return '@derivative:' + json.dumps([key, position, bucket], separators=(',', ':'), ensure_ascii=False)


def posting(key, code, value, position=None, bucket=None):
    gl = GL[code]
    return (gl, '' if code in {'cash', 'opening_equity'} else instrument(key, position, bucket), value)


def opening_lines(saved, source):
    key = book_key(saved['book'])
    csa = saved['physical']['derivative']['operation']['request']
    buckets = {m['position_id']: m['netting_set_id'] if m['enforceable_netting']
               else 'trade:' + m['position_id'] for m in csa['mapping']}
    first = sorted(set(buckets.values()))[0]
    opening = source['opening']
    lines = [posting(key, 'cash', opening['cash']),
             posting(key, 'opening_equity', -opening['equity_credit_balance'])]
    for code, value in [('posted_vm_asset', opening['posted_vm_asset']),
                        ('received_vm_liability', -opening['received_vm_liability']),
                        ('posted_im_asset', opening['posted_im_asset']),
                        ('memo_custodian_asset', opening['received_im_custody']),
                        ('memo_custodian_obligation', -opening['received_im_custody'])]:
        lines.append(posting(key, code, value, bucket=first))
    for anchor in opening['trades']:
        trade = next(t for t in saved['book']['trades'] if
                     (t.get('request') or t['terminal'])['position_id'] == anchor['position_id'])
        position = (trade.get('delivered_underlying') or {}).get('position_id', anchor['position_id'])
        for code, value in [('derivative_asset', max(anchor['clean_value'], 0.)),
                            ('derivative_liability', -max(-anchor['clean_value'], 0.))]:
            lines.append(posting(key, code, value, position, buckets[anchor['position_id']]))
    return lines


def match_rows(actual, expected, label):
    require(len(actual) == len(expected), label + ' row count')
    for got, want in zip(actual, expected):
        require(set(got) == set(want), label + ' fields')
        for field, value in want.items():
            if type(value) is float:
                near(got[field], value, label + ':' + field)
            else:
                exact(got[field], value)


def classified(balances, scenario, account, funding_positions):
    totals = dict(assets=0., liabilities=0., equity=0., cash=0., restricted_cash=0.)
    for (s, a, gl, _), value in balances.items():
        if (s, a) != (scenario, account):
            continue
        require(not gl.startswith('memo:'), 'custody entered financial GL')
        if gl in ('cash', 'restricted_cash'):
            totals[gl] += value
        if gl.startswith('pnl:') or gl in {'opening_equity', 'oci'}:
            totals['equity'] -= value
        elif gl in LIABILITIES or (gl == 'book_adjustment' and
                (s, a, _) in funding_positions):
            totals['liabilities'] -= value
        else:
            totals['assets'] += value
    return totals



def opening_dated_rights(dated, scenario, position_id):
    """Derive gross pre-settlement rights from original raw calendar segments.

    Day-zero cash is a subsequent transaction, never a substitute opening anchor.
    Summation follows original source and segment order without netting events
    with opposite coupon direction into one obligation.
    """
    runs = [r for r in dated['dated']['runs'] if r['scenario_id'] == scenario and r['severity'] == 1.]
    require(len(runs) == 1, 'one exact independent dated source run')
    run_id = runs[0]['run_id']
    asof = dated['dated']['as_of_ordinal']
    plans = {(p['position_id'], p['event_id'], json.dumps(p['run'], sort_keys=True)):
             p['segments'] for p in dated['dated']['accrual_plans']}
    require(len(plans) == len(dated['dated']['accrual_plans']), 'duplicate opening recognition plan')
    rights = dict(dated_interest_receivable=0., dated_interest_payable=0.,
                  dated_prepaid_expense=0., dated_deferred_income=0.)
    event_keys = set()
    for source in dated['dated']['sources']:
        for event in source['schedule']['events']:
            if event['position_id'] != position_id:
                continue
            selector = event['scenario']
            if selector['kind'] != 'all' and selector['scenario_id'] != run_id:
                continue
            key = event['event_id']
            require(key not in event_keys, 'duplicate opening source event')
            event_keys.add(key)
            segments = plans.get((position_id, key, json.dumps(selector, sort_keys=True)), [])
            earned = 0.
            for segment in segments:
                duration = segment['end_ordinal'] - segment['start_ordinal']
                require(duration > 0, 'invalid opening recognition duration')
                elapsed = max(0, min(asof, segment['end_ordinal']) - segment['start_ordinal'])
                earned += segment['accrued_interest'] * elapsed / duration
            if earned >= 0.:
                rights['dated_interest_receivable'] += earned
            else:
                rights['dated_interest_payable'] += earned
    return rights


@dataclass(frozen=True)
class CheckedControl:
    _raw:bytes
    _header:bytes
    _data:dict
    _digest:str
    _seal:object


def dated_work(raw):
    dated=raw['dated']['dated'];spec=raw['dated']['specification'];asof=dated['as_of_ordinal'];end=asof+spec['horizon_days']
    require(type(spec['horizon_days'])is int and 0<spec['horizon_days']<=1080,'dated calendar bound')
    require(not spec.get('reverse_severities'),'reverse scenarios require a separate source domain')
    iterations=1+len(spec['scenarios']);work=len(spec['positions'])*spec['horizon_days']*iterations
    plans={(p['position_id'],p['event_id'],None if p['run']['kind']=='all'else p['run']['scenario_id']):p['segments']for p in dated['accrual_plans']}
    runs={r['run_id']:r for r in dated['runs']}
    for source in dated['sources']:
        for event in source['schedule']['events']:
            selector=None if event['scenario']['kind']=='all'else event['scenario']['scenario_id']
            repetitions=iterations if selector is None else int(runs[selector]['scenario_id']=='baseline'or runs[selector]['severity']==1.)
            segments=plans.get((event['position_id'],event['event_id'],selector),[])
            work+=(2+sum(max(0,min(s['end_ordinal'],end)-max(s['start_ordinal'],asof))for s in segments))*repetitions
    return work

def verify_preparation(raw,prepared,receipt,check=lambda:False):
    probe(check)
    require(type(raw)is dict and set(raw)=={'schema','dated','owners'} and raw['schema']=='saved-derivative-mixed-1','explicit raw mixed schema')
    require(len(encoded(raw))<=32*1024*1024,'raw byte cap')
    exact(prepared,raw)
    envelope=receipt
    require(set(receipt)=={'schema','original_request','native_sources','joined_work_units','owner_capacity_bytes','empirical_validation','capital_ftp_eligible'},'source receipt fields')
    exact(receipt['original_request'],raw)
    require(receipt['schema']==raw['schema']and receipt['empirical_validation']is False and receipt['capital_ftp_eligible']is False,'source permission/type')
    require(type(receipt['joined_work_units'])is int and 0<receipt['joined_work_units']<=8000000 and type(receipt['owner_capacity_bytes'])is int and 0<receipt['owner_capacity_bytes']<=128*1024*1024,'native bounded admission')
    require(type(raw['owners'])is list and 0<len(raw['owners'])<=64 and len(receipt['native_sources'])<=64,'source cardinality')
    dated, spec = raw['dated'], deepcopy(raw['dated']['specification'])
    spec['accounts'] = [Account(**a) for a in spec['accounts']]
    spec['positions'] = [Position(**p) for p in spec['positions']]
    spec['scenarios'] = [Scenario(**s) for s in spec['scenarios']]
    require(not spec.get('policies') and not spec.get('netting_sets') and not spec.get('cashflows') and
            {p.id for p in spec['positions']} == {o['position_id'] for o in dated['dated']['owners']},
            'independent mixed evidence requires a fully dated backbook')
    require(all(a.annual_fees == a.annual_costs == a.tax_rate == a.annual_dividends == 0.
                for a in spec['accounts']), 'account cost/tax evidence requires separate verifier')
    accounts = {a.id: a for a in spec['accounts']}
    funding_positions = {(s, p.account, p.id) for s in ['baseline'] +
        [s.name for s in spec['scenarios']] for p in spec['positions']
        if p.kind not in {'loan', 'security', 'reverse_repo'}}
    scenarios = ['baseline'] + [s.name for s in spec['scenarios']]
    asof = dated['dated']['as_of_ordinal']
    source_count=sum(len(o['runs'])for o in raw['owners'])
    work=dated_work(raw)
    preflight_work=standalone_upper(raw['owners'],spec['horizon_days']+1,work,check)
    expected_financial, expected_custody, expected_opening = [], [], []
    opening_stocks = defaultdict(list)
    book_seen, netting_seen = set(), set()
    flat = [(o, r) for o in raw['owners'] for r in o['runs']]
    require(len(flat) == len(envelope['native_sources']), 'source ownership cardinality')
    for owner in raw['owners']:
        first = owner['runs'][0]['saved']
        key = book_key(first['book'])
        require(key not in book_seen, 'duplicate saved book')
        book_seen.add(key)
        require({r['ledger_scenario'] for r in owner['runs']} == set(scenarios) and
                len(owner['runs']) == len(scenarios), 'exact derivative scenario coverage')
        mapping = first['physical']['derivative']['operation']['request']['mapping']
        for bucket in {(owner['account_id'], m['counterparty'], m['netting_set_id'])
                       for m in mapping if m['enforceable_netting']}:
            require(bucket not in netting_seen, 'split legal netting bucket')
            netting_seen.add(bucket)
        for run in owner['runs']:
            exact(run['saved']['book'], first['book'])
            exact(run['saved']['physical']['opening'], first['physical']['opening'])
    for (owner, run), native in zip(flat, envelope['native_sources']):
        saved, scenario, account = run['saved'], run['ledger_scenario'], owner['account_id']
        probe(check)
        verify_saved(saved, {'original_request': saved, 'result': native},check)
        # Reconstruct from the raw financial contracts again for expected mixed postings.
        expected = checked_derive(saved['physical'],check)
        for field in ('run','measure','basis','entity','currency','monetary_unit','policy','opening'):
            exact(native['source'][field],expected[field])
        for field in ('policy','conditional_model','crn','legal_mapping'):
            exact(native['source']['native'][field],expected['native'][field])
        for actual_path,expected_path in zip(native['source']['native']['paths'],expected['native']['paths']):
            for field in ('path_id','probability','source','closing_legal_review','mpor_policy','replacement_value_policy'):
                exact(actual_path[field],expected_path[field])
        source_opening=len(opening_lines(saved,expected));source_lines=sum(len(i['postings'])for i in expected['intents'])
        daily_lines=sum(len(d['financial_trial_balance'])+len(d['custody_trial_balance'])for d in expected['daily'])
        work+=250000+source_opening+source_lines+daily_lines+6*(source_opening+source_lines)*(spec['horizon_days']+1)
        require(work<=8000000,'complete source replay work')
        key = book_key(saved['book'])
        a = accounts[account]
        require(a.entity == expected['entity'] and a.currency == expected['currency'] and
                asof == expected['opening']['ordinal'], 'native owner headers/date differ')
        exact(float(a.cash), float(expected['opening']['cash']))
        for gl, identifier, value in opening_lines(saved, expected):
            stock = gl not in {'cash', 'opening_equity'} and not gl.startswith('memo:')
            expected_opening.append(dict(scenario=scenario, account=account, book_key=key,
                ordinal=asof, gl_account=gl, instrument_id=identifier, debit_amount=float(value),
                posted_in_shared_opening=stock, custody_memo=gl.startswith('memo:')))
            if stock and value:
                opening_stocks[scenario, account].append((gl, identifier, float(value)))
        for index, intent in enumerate(expected['intents']):
            rows = expected_custody if intent['ledger'] == 'custody_memo' else expected_financial
            for posting_index, p in enumerate(intent['postings']):
                gl, identifier, value = posting(key, p['account'], p['debit_amount'],
                    p['position_id'], p['bucket'])
                rows.append(dict(scenario=scenario, account=account, book_key=key,
                    source_event_id=intent['event_id'], source_intent_index=index,
                    source_posting_index=posting_index, ordinal=intent['ordinal'], day=intent['ordinal']-asof,
                    gl_account=gl, instrument_id=identifier, debit_amount=float(value)))
    # Native Sim traverses scenarios, then original owner order.
    order = {s: i for i, s in enumerate(scenarios)}
    for rows in (expected_financial, expected_custody):
        rows.sort(key=lambda r: (order[r['scenario']], r['day']))
    expected_opening.sort(key=lambda r: order[r['scenario']])
    probe(check)
    data=dict(spec=spec,accounts=accounts,funding_positions=funding_positions,scenarios=scenarios,
        expected_financial=expected_financial,expected_custody=expected_custody,expected_opening=expected_opening,opening_stocks=dict(opening_stocks),asof=asof)
    require(work==receipt['joined_work_units'],'independent source/daily work count differs')
    require(work<=preflight_work,'observed full source/daily work exceeds raw structural reservation')
    # Data is private verified expected economics; callers receive no mutable authority.
    return CheckedControl(encoded(raw),encoded({'prepared_request':prepared,'source_receipt':receipt}),data,hashlib.sha256(repr(data).encode()).hexdigest(),_SEAL)


def rows(facade,table,check):
    for frame in facade.frames(table):
        probe(check)
        for i,row in enumerate(frame.iter_rows(named=True)):
            if i%128==0:probe(check)
            yield row
    probe(check)

def compare_rows(actual,expected,label):
    iterator=iter(actual)
    for want in expected:
        got=next(iterator,None);require(got is not None,label+' missing row');match_rows([got],[want],label)
    require(next(iterator,None)is None,label+' extra row')


def verify_stream(raw,control,facade,check=lambda:False):
    probe(check)
    require(type(control)is CheckedControl and control._seal is _SEAL,'private verified source control required')
    exact(loads(control._raw),raw)
    require(hashlib.sha256(repr(control._data).encode()).hexdigest()==control._digest,'private source expected control rebased')
    require(exact_identity(facade.completion,{'complete':True,'protocol':3}),'actual strict producer completion required')
    data=control._data;spec=data['spec'];accounts=data['accounts'];scenarios=data['scenarios'];funding=data['funding_positions']
    dated=raw['dated'];order={s:i for i,s in enumerate(scenarios)}
    for table,key in [('derivative_journal_lineage','expected_financial'),('derivative_custody_journal','expected_custody'),('derivative_opening_lineage','expected_opening')]:
        compare_rows(rows(facade,table,check),data[key],table)
    require(all(not r['gl_account'].startswith('memo:')for r in rows(facade,'journal',check)),'custody cannot post in financial GL')
    # Every nonzero native financial intent must be posted once in its original line order.
    expected=iter(r for r in data['expected_financial']if r['debit_amount']!=0.)
    source_transactions={};native_transactions={};previous={};opening=defaultdict(list);count=0
    phase={'opening':0,'dated_recognition':1,'dated_settlement':2,'native_derivative_intent':3}
    for got in rows(facade,'journal',check):
        count+=1;scenario=got['scenario'];require(scenario in order and got['event']in phase,'unknown journal owner')
        require(type(got['day'])is int and 0<=got['day']<=spec['horizon_days'],'journal date type/domain')
        number=int(got['transaction_id'].rsplit(':',1)[1]);exact(got['transaction_id'],f'{scenario}:{number}')
        previous_key=previous.get(scenario,(0,-1,-1,None));pn,pd,pp,meta=previous_key
        current=(got['day'],got['account'],got['event'])
        require(number in(pn,pn+1)and got['day']>=pd and (number!=pn or current==meta),'ordered contiguous journal transaction')
        require(got['day']!=pd or phase[got['event']]>=pp,'mixed source phase reordered')
        previous[scenario]=(number,got['day'],phase[got['event']],current)
        if got['event']=='opening':opening[scenario,got['account']].append(got)
        elif got['event']=='native_derivative_intent':
            want=next(expected,None);require(want is not None,'extra derivative posting')
            for field in ('scenario','account','day','gl_account','instrument_id'):exact(got[field],want[field])
            near(got['debit']-got['credit'],want['debit_amount'],'independent derivative posting')
            source=(scenario,want['book_key'],want['source_intent_index']);native=(scenario,got['transaction_id'])
            require(source_transactions.setdefault(source,native)==native and native_transactions.setdefault(native,source)==source,'source intent split/merged')
    require(next(expected,None)is None,'missing derivative posting')
    for scenario in scenarios:
        for account,a in accounts.items():
            got=opening.pop((scenario,account),[])
            require(len({r['transaction_id']for r in got})==1 and all(r['day']==0 for r in got),'single opening transaction')
            want=[('cash','',float(a.cash)),('opening_equity','',-float(a.equity))]
            for p in spec['positions']:
                if p.account!=account:continue
                sign=1. if p.kind in {'loan','security','reverse_repo'}else -1.
                require(p.allowance==0. and p.start_day==0,'unsupported opening credit/acquisition')
                want.extend([('asset_principal'if sign==1. else 'funding_principal',p.id,sign*p.balance),('book_adjustment',p.id,sign*p.book_adjustment)])
                right=opening_dated_rights(dated,scenario,p.id);near(sum(right.values()),sign*p.opening_accrued,'gross opening rights')
                want.extend((gl,p.id,v)for gl,v in right.items())
            want+=data['opening_stocks'].get((scenario,account),[]);want=[r for r in want if r[2]!=0.]
            require(len(got)==len(want),'opening stock count')
            for r,(gl,identifier,value)in zip(got,want):
                exact(r['gl_account'],gl);exact(r['instrument_id'],identifier);near(r['debit']-r['credit'],value,'original opening anchor')
    require(not opening,'unknown opening account/scenario')
    dated_receipt=verify_dated_sources(dated,spec,facade,lambda:probe(check))
    balances,journal_count=replay_partitions(facade.frames('journal'));require(journal_count==count,'journal reread count changed')
    seen=set()
    for r in rows(facade,'trial_balance',check):
        key=tuple(r[k]for k in('scenario','account','gl_account','instrument_id'));require(key not in seen and key in balances,'trial ownership')
        seen.add(key);near(r['balance'],balances[key],'ordered journal trial')
    require(seen==set(balances),'missing closing trial')
    # Stream exact day coverage, applying journal chronological postings without day arrays.
    journal=iter(rows(facade,'journal',check));r=next(journal,None);state=defaultdict(float);path_count=0
    for scenario in scenarios:
        for day in range(spec['horizon_days']+1):
            while r is not None and (r['scenario'],r['day'])==(scenario,day):
                state[r['scenario'],r['account'],r['gl_account'],r['instrument_id']]+=r['debit']-r['credit'];r=next(journal,None)
            for account in accounts:
                if day>30 and day%30!=0 and day!=spec['horizon_days']:continue
                yield_key=(scenario,day,account)
                # Path iterator is advanced below, preserving native account order.
                if path_count==0:path_iter=iter(rows(facade,'path',check))
                point=next(path_iter,None);require(point is not None,'missing daily path');exact(tuple(point[k]for k in('scenario','day','account')),yield_key)
                want=classified(state,scenario,account,funding)
                for field,value in want.items():near(point[field],value,'independent daily gross '+field)
                near(point['reconciliation_error'],want['assets']-want['liabilities']-want['equity'],'daily reconciliation')
                path_count+=1
                verify_path_metrics(point,state,data,scenario,day,account)
    require(r is None and next(path_iter,None)is None,'source/path extra dates')
    verify_secondary_reports(facade,data,check)
    require(hashlib.sha256(repr(control._data).encode()).hexdigest()==control._digest,'source control mutated during stream')
    require(exact_identity(facade.completion,{'complete':True,'protocol':3}),'completion changed during replay')
    probe(check)
    return dict(schema='saved-derivative-mixed-verification-1',verified=True,native_derivative_source_recomputed=True,
        ordered_mixed_journal_recomputed=True,single_daily_gl_verified=True,complete_gross_rights_verified=True,
        native_source_count=len(raw['owners'][0]['runs'])if len(raw['owners'])==1 else sum(len(o['runs'])for o in raw['owners']),
        shared_journal_lines=count,daily_account_rows=path_count,dated=dated_receipt,
        empirical_validation=False,accounting_standard_approval=False,capital_ftp_eligible=False,saved_pricing_graph_adoption=False)


def metrics(state,data,scenario,day,account):
    a=data['accounts'][account];stock=classified(state,scenario,account,data['funding_positions'])
    rwa=rsf=outflow=collateral=undrawn=l2=inflow=htm=0.;hqla=max(stock['cash'],0.);asf=max(stock['equity'],0.)
    case=next((s for s in data['spec']['scenarios']if s.name==scenario),Scenario('baseline'))
    for p in data['spec']['positions']:
        if p.account!=account:continue
        asset=p.kind in {'loan','security','reverse_repo'};sign=1. if asset else -1.
        principal=sign*state.get((scenario,account,'asset_principal'if asset else 'funding_principal',p.id),0.)
        book=sign*state.get((scenario,account,'book_adjustment',p.id),0.);carrying=principal+book
        if asset:
            rwa+=max(carrying,0.)*p.risk_weight*(1.+p.watch_fraction)+p.commitment*p.risk_weight
            rsf+=max(carrying,0.)*p.rsf_weight;free=principal*max(1.-p.encumbered_fraction,0.)
            if p.hqla_level=='level2a':l2+=free*p.hqla_weight
            else:hqla+=free*p.hqla_weight
            inflow+=principal*p.lcr_inflow_weight
            collateral+=principal*max(p.eligible_fraction-p.encumbered_fraction,0.)*(1.-min(.99,p.haircut+case.haircut_add*(day>=case.start_day)))
            undrawn+=p.commitment
        else:asf+=principal*p.asf_weight;outflow+=principal*p.lcr_outflow_weight
        if p.classification=='htm':htm+=carrying
    hqla+=min(l2,hqla*.4/(1.-.4));outflow=max(outflow-min(inflow,.75*outflow),0.)
    cet1=stock['equity']-a.capital_deductions;exposure=max(stock['assets'],0.)+undrawn
    ratio=lambda x,d:x/d if d>0. else None
    values=dict(aoci=0.,cash_floor=a.cash_floor,cet1=cet1,rwa=rwa,cet1_ratio=ratio(cet1,rwa),leverage_ratio=ratio(cet1,exposure),
        usable_collateral=collateral,undrawn=undrawn,hqla_proxy=hqla,lcr_proxy=ratio(hqla,outflow),nsfr_proxy=ratio(asf,rsf))
    values.update(cash_headroom=stock['cash']-a.cash_floor,cet1_headroom=cet1-a.cet1_floor*rwa,leverage_headroom=cet1-a.leverage_floor*exposure,
        lcr_headroom=hqla-a.lcr_floor*outflow,nsfr_headroom=asf-a.nsfr_floor*rsf,htm_headroom=a.htm_asset_limit*max(stock['assets'],0.)-htm)
    return stock,values

def verify_path_metrics(row,state,data,scenario,day,account):
    a=data['accounts'][account];exact(row['entity'],a.entity);exact(row['currency'],a.currency)
    _,values=metrics(state,data,scenario,day,account)
    for field,v in values.items():
        if v is None:exact(row[field],None)
        else:near(row[field],v,'journal-derived research path '+field)

def verify_secondary_reports(facade,data,check):
    # No unverified side table may bypass ordered source/GL ownership. Replay only
    # current state, aggregate account-event totals and bounded report row lists.
    state=defaultdict(float);ledger=[];exposures=[];summaries=[];breaches=[];totals=defaultdict(lambda:[0.,0.,0.]);position=data['spec']['positions']
    journal=iter(rows(facade,'journal',check));r=next(journal,None)
    for scenario in data['scenarios']:
        minimum={a:float(x.cash) for a,x in data['accounts'].items()};first={};max_error=0.
        for day in range(data['spec']['horizon_days']+1):
            events={}
            while r is not None and (r['scenario'],r['day'])==(scenario,day):
                state[scenario,r['account'],r['gl_account'],r['instrument_id']]+=r['debit']-r['credit']
                if r['event']!='opening':
                    key=r['account'],r['event'];values=events.setdefault(key,[0.,0.,0.]);money=r['debit']-r['credit']
                    if r['gl_account']=='cash':values[0]+=money
                    elif r['gl_account'].startswith('pnl:'):values[1]-=money
                    elif r['gl_account']=='oci':values[2]-=money
                r=next(journal,None)
            for (account,event),v in events.items():
                if any(x!=0. for x in v):ledger.append(dict(scenario=scenario,day=day,account=account,event=event,cash=v[0],earnings=v[1],aoci=v[2],memo_amount=0.))
                t=totals[scenario,account,event]
                for i in range(3):t[i]+=v[i]
            if day in (0,data['spec']['horizon_days'])or day%30==0:
                for p in position:
                    asset=p.kind in {'loan','security','reverse_repo'};sign=1. if asset else -1.;pgl='asset_principal'if asset else 'funding_principal'
                    accrued=sign*sum(state.get((scenario,p.account,gl,p.id),0.)for gl in ('dated_interest_receivable','dated_interest_payable','dated_prepaid_expense','dated_deferred_income'))
                    exposures.append(dict(scenario=scenario,day=day,account=p.account,position=p.id,kind=p.kind,balance=sign*state.get((scenario,p.account,pgl,p.id),0.),allowance=0.,watch_fraction=p.watch_fraction,commitment=p.commitment,market_factor=1.,encumbered_fraction=p.encumbered_fraction,accrued_interest=accrued))
            for account,a in data['accounts'].items():
                stock,m=metrics(state,data,scenario,day,account);minimum[account]=min(minimum[account],stock['cash'])
                error=stock['assets']-stock['liabilities']-stock['equity'];max_error=max(max_error,abs(error))
                for metric in ('cash','cet1','leverage','lcr','nsfr','htm'):
                    if m[metric+'_headroom']< -1e-9 and (account,metric)not in first:
                        first[account,metric]=day;breaches.append(dict(scenario=scenario,account=account,day=day,metric=metric))
        for account,a in data['accounts'].items():
            stock=classified(state,scenario,account,data['funding_positions']);summary=dict(scenario=scenario,account=account,entity=a.entity,currency=a.currency,final_cash=stock['cash'],final_equity=stock['equity'],minimum_cash=minimum[account],peak_cash_shortfall=max(0.,a.cash_floor-minimum[account]),max_reconciliation_error=max_error)
            summary.update({f'first_{metric}_breach_day':first.get((account,metric))for metric in ('cash','capital','leverage','lcr','nsfr','htm')})
            summary['first_capital_breach_day']=first.get((account,'cet1'));summaries.append(summary)
    compare_rows(rows(facade,'ledger',check),ledger,'journal-derived phase ledger')
    compare_rows(rows(facade,'exposures',check),exposures,'journal-derived source exposure')
    compare_rows(rows(facade,'summary',check),summaries,'journal-derived source summary')
    compare_rows(rows(facade,'breaches',check),breaches,'journal-derived research breaches')
    attribution=[]
    keys=sorted({(a,e)for _,a,e in totals})
    for scenario in data['scenarios'][1:]:
        for account,event in keys:
            b,s=totals['baseline',account,event],totals[scenario,account,event]
            attribution.append(dict(scenario=scenario,account=account,event=event,cash_delta=s[0]-b[0],earnings_delta=s[1]-b[1],aoci_delta=s[2]-b[2]))
    compare_rows(rows(facade,'attribution',check),attribution,'journal-derived attribution')
    for table in ('actions','funding_claims','reverse_grid'):
        require(next(iter(rows(facade,table,check)),None)is None,'unverified discretionary/legacy owner output')
