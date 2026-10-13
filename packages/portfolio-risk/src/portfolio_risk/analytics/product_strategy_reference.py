"""Independent product-strategy publication oracle; never an execution backend.

Only independently reconstructed performing repayment/runoff sources are admitted.
Native/HiGHS success is not empirical validation or proof of global optimality.
"""
from bisect import bisect_left
from copy import deepcopy
import json
import math
import sys

from . import consumer_schedule_reference as consumer
from . import deposit_schedule_reference as deposit

METRICS = ['nii', 'balance', 'dv01', 'hqla_l2a', 'outflow30', 'asf',
           'rsf', 'rwa', 'assets', 'liabilities']
ROLES = ('physical', 'pricing', 'pricing_down', 'pricing_up')
PRODUCTS = ('consumer_installment', 'deposit', 'card', 'mortgage')
MAX_WORK = 32_000_000
GRAPH_POLICIES = {
    'consumer_installment': {
        'income_policy': 'gross physical contractual note-interest recognition on actual (left,right] intervals; not frozen effective-yield income, provision-adjusted NII or net income',
        'risk_policy': 'pricing-measure supplied paired down/up histories plus native collateral zero-rate bumps at unchanged base OAS; no fitted stochastic behavioral policy',
        'source_status': 'native generation from supplied original contracts, known opening state and explicit P/Q histories; no random draws consumed, automatic legacy switch or ledger publication',
    },
    'deposit': {
        'income_policy': 'weighted physical signed deposit expense recognition and separate bank cash; migration retained as internal state movement, no effective-yield NII or journal inference',
        'risk_policy': 'explicit supplied Q liability outflows minus inflows, original base OAS frozen; paired supplied Q histories plus native curve bumps, no fitted policy/effective DV01 claim',
        'source_status': 'supplied assumption histories; no empirical calibration or P-to-Q inference',
    },
    'card': {
        'income_policy': 'weighted physical supplied-history gross economic recognition, principal/interest/fees/default/recovery cash separately; no effective-yield NII, allowance or journal inference',
        'risk_policy': 'full future Q rights discounted on supplied collateral curve plus fixed original base OAS; central native curve bump with identically paired supplied Q down/up histories; supplied APR paths are independent and no index mapping or stochastic spending/default law is inferred',
        'source_status': 'native generated state/cash rights with raw supplied policy/source lineage; complete zero-rights/zero-limit tail required; analysis source, not saved-book, ledger, empirical calibration or treasury adoption',
    },
    'mortgage': {
        'income_policy': 'weighted physical native investor recognized note/cash-basis interest, actual borrower versus investor cash, entitlement/default/loss/advance evidence; uniform allocation of explicit recognition intervals; no effective-yield NII, allowance, provisioning or ledger inference',
        'risk_policy': 'complete generated investor Q remittances on supplied collateral curve plus original frozen base OAS; central curve bump with explicitly paired supplied Q path/reset histories; no stochastic draw consumption or P-to-Q conversion',
        'source_status': 'native supplied mortgage state graph, retained full borrower/investor/ARM/default/guarantee sources; persist complete raw Request alongside result; supplied interval-end behavior and fixed recovery/agency assumptions, not fitted HPI/geographic credit, automatic legacy, cohort or ledger adoption',
    },
}


def _equal(actual, expected, path='source'):
    """Exact keys/order/identities; tolerance applies only to financial floats."""
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise ArithmeticError(f'{path}: fields differ')
        for k, v in expected.items():
            _equal(actual[k], v, path + '/' + k)
    elif isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise ArithmeticError(f'{path}: length differs')
        for i, v in enumerate(expected):
            _equal(actual[i], v, f'{path}/{i}')
    elif isinstance(expected, (int, float)) and not isinstance(expected, bool) and not (
        isinstance(expected, int) and any(
            token in path.split('/') for token in (
                'ordinal', 'purchase_ordinal', 'purchase_period', 'as_of_ordinal',
                'start_ordinal', 'end_ordinal', 'payment_ordinal', 'full_contract_end_ordinal',
                'full_cash_terminal_ordinal', 'horizon_days', 'reporting_ordinals',
                'payment_dates', 'ordinals', 'shape', 'unit_index', 'month', 'scenario',
                'maturity_ordinal', 'interest_payment_ordinals', 'observation_lag_days',
                'seasoning_days', 'work_units', 'statement_ordinal', 'due_ordinal',
                'entitlement_ordinal', 'remittance_ordinal', 'origination_ordinal',
                'promotional_expiry_ordinal', 'opening_due_ordinal',
            )
        )
    ):
        if (isinstance(actual, bool) or not isinstance(actual, (int, float))
                or not math.isfinite(actual) or not math.isfinite(expected)
                or abs(actual - expected) > 1e-9 + 1e-12 * abs(expected)):
            raise ArithmeticError(f'{path}: financial amount differs')
    elif type(actual) is not type(expected) or actual != expected:
        raise ArithmeticError(f'{path}: exact identity differs')


def _fields(actual, expected, path):
    for k, v in expected.items():
        if k not in actual:
            raise ArithmeticError(f'{path}/{k}: missing')
        _equal(actual[k], v, path + '/' + k)


def _admit_face_selectors(raw, budget):
    fields = {
        'unit': {'kind', 'unit_id'},
        'template': {'kind', 'template_id'},
        'accounting_category': {'kind', 'accounting_category'},
        'all_assets': {'kind'},
        'all_liabilities': {'kind'},
    }
    for limit in raw.get('face_limits', []):
        budget.charge()
        selector = limit.get('selector') if isinstance(limit, dict) else None
        kind = selector.get('kind') if isinstance(selector, dict) else None
        expected = fields.get(kind) if isinstance(kind, str) else None
        if expected is None or selector.keys() != expected:
            raise ArithmeticError('product face selector fields differ from its explicit kind')


class _Budget:
    def __init__(self, check):
        self.check = check
        self.work = 0

    def charge(self, work=1):
        self.work += work
        if self.work > MAX_WORK:
            raise ArithmeticError('independent product verification work admission exceeded')
        if self.check():
            raise InterruptedError('product strategy verification cancelled')


def _finite(x):
    if isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) or abs(x) > 1e15:
        raise ArithmeticError('bounded finite financial amount required')
    return x


def _df(curve, t):
    times, dfs = curve['times_years'], curve['discount_factors']
    if not 0 <= t <= 100 or len(times) != len(dfs) or len(times) < 2:
        raise ArithmeticError('curve domain differs')
    i = bisect_left(times, t)
    if i < len(times) and times[i] == t:
        return dfs[i]
    if i == 0:
        raise ArithmeticError('curve requires origin')
    if i == len(times):
        if curve['right_tail'] == 'reject':
            raise ArithmeticError('curve undeclared tail')
        if curve['right_tail'] == 'flat_zero':
            return math.exp(math.log(dfs[-1]) * t / times[-1])
        if curve['right_tail'] != 'flat_forward':
            raise ArithmeticError('curve unknown tail')
        a, b = len(times)-2, len(times)-1
    else:
        a, b = i-1, i
    return math.exp(math.log(dfs[a]) + (math.log(dfs[b])-math.log(dfs[a])) * (t-times[a])/(times[b]-times[a]))


def _histories(graph, kind):
    p = graph['positions'][0]
    return p['product']['histories'] if kind == 'consumer_installment' else p['histories']


def _contract(graph, kind):
    p = graph['positions'][0]
    return p['product']['contract'] if kind == 'consumer_installment' else p.get('contract')


def _card(c, history, identity, check):
    """Three independent balance classes, actual daily accrual and billed rights."""
    if (c['initial_state'] != 'performing' or c['initial_purchase_grace']
            or c['regain_grace_after_full_statement_payment']
            or c['opening_contingent_purchase_interest'] != 0.
            or any(any(c[k]) for k in ('opening_billed_interest','opening_unbilled_interest','opening_unrecognized_billed_interest','opening_fees'))):
        raise ArithmeticError('unsupported card opening/grace/recognition policy')
    p=list(c['opening_principal']);billed=[0.]*3;unbilled=[0.]*3
    remaining=list(c['opening_statement_remaining']);minimum=c['opening_minimum_remaining'];limit=c['initial_limit'];collected=0.
    points=[];cash=[];recognition=[];statements=[];classes=['purchase','cash_advance','promotional'];rates=c['rates']
    def apr(i,day):
        name='post_promotion' if i==2 and day>=rates['promotional_expiry_ordinal'] else classes[i]
        return rates[name]
    actions={}
    for step in history['steps']:
        a=step['action']
        if a['kind'] not in {'payment','set_limit'} or a['kind']=='set_limit' and a['amount']!=0. or step['ordinal']<=c['as_of_ordinal']:
            raise ArithmeticError('unsupported card transition or same-asof cash')
        actions.setdefault(step['ordinal'],[]).append(a)
    for day in range(c['as_of_ordinal'],c['as_of_ordinal']+c['horizon_days']+1):
        if check():raise InterruptedError('card reference cancelled')
        if day>c['as_of_ordinal']:
            tau=consumer.year_fraction(day-1,day,c['day_count'])
            for i in range(3):
                earned=(p[i]+(billed[i] if c['compound_billed_interest'] else 0.))*apr(i,day-1)*tau
                unbilled[i]+=earned
                if earned:recognition.append(dict(origin_id=f'lot-{i}',start_ordinal=day-1,end_ordinal=day,interest=earned))
        for action in actions.get(day,[]):
            if action['kind']=='set_limit':limit=0.;continue
            amount=action['amount']
            if amount>sum(p)+sum(billed)+1e-9:raise ArithmeticError('card payment exceeds billed rights')
            part=min(amount,minimum);minimum-=part;paid=[0.,0.]
            orders=[([classes.index(x) for x in c['minimum']['allocation_order']],part), (sorted(range(3),key=lambda i:(-apr(i,day),i)),amount-part)]
            for order,available in orders:
                for i in order:
                    if available<=0.:break
                    interest=min(available,billed[i]);billed[i]-=interest;available-=interest;paid[1]+=interest
                    principal=min(available,p[i]);p[i]-=principal;available-=principal;paid[0]+=principal
                    remaining[i]=max(remaining[i]-interest-principal,0.)
            collected+=paid[0];cash.append(dict(event_id='payment-'+str(len(cash)),ordinal=day,kind='payment',principal=paid[0],interest=paid[1],fees=0.))
        if day==c['opening_due_ordinal'] or any(x['due_ordinal']==day for x in c['cycles']):remaining=[0.]*3
        cycle=next((x for x in c['cycles'] if x['statement_ordinal']==day),None)
        if cycle:
            for i in range(3):billed[i]+=unbilled[i];unbilled[i]=0.
            total=sum(p)+sum(billed);rule=c['minimum'];minimum=min(max(rule['floor'],sum(p)*rule['principal_fraction']+(sum(billed) if rule['include_billed_interest'] else 0.)),total)
            remaining=[p[i]+billed[i] for i in range(3)]
            statements.append(dict(ordinal=day,due_ordinal=cycle['due_ordinal'],principal=p.copy(),billed_interest=billed.copy(),fees=[0.]*3,minimum_payment=minimum,statement_total=total))
        debt=sum(p)+sum(billed)
        points.append(dict(ordinal=day,state='performing',principal=p.copy(),billed_interest=billed.copy(),unbilled_interest=unbilled.copy(),fees=[0.]*3,limit=limit,utilization=debt/limit if limit>0 else None,undrawn=max(limit-debt,0.),purchase_grace=False))
    if any(p+billed+unbilled) or limit!=0.:raise ArithmeticError('card complete rights/limit tail required')
    return dict(schema='consumer-card-1',identity=identity,statements=statements,cashflows=cash,recognition=recognition,points=points,default=None,opening_principal=sum(c['opening_principal']),drawn_principal=0.,collected_principal=collected,closing_principal=sum(p),closing_interest=sum(billed),closing_unbilled_interest=sum(unbilled),closing_unrecognized_billed_interest=0.,closing_fees=0.,closing_grace_contingent_interest=0.)


def _mortgage(c, history, identity, check):
    """Zero-transition fixed mortgage; borrower collections and investor rights differ."""
    ownership=c['ownership'];agency=ownership['kind']=='agency';refi=c['refinance']
    if (c['rate']['kind']!='fixed' or history['arm_resets'] or refi['incentive_slope']!=0.
            or c['opening_contractual_interest']!=0. or any(c['opening_borrower_interest']+c['opening_investor_interest'])
            or any(c['opening_probability'][1:]+c['opening_borrower_principal'][1:]+c['opening_investor_principal'][1:])
            or c['borrower_collection_fraction'][0]!=1.
            or not agency and ownership['recognized_accrual_fraction'][0]!=1.):
        raise ArithmeticError('unsupported fixed performing mortgage opening/policy')
    for step in history['path']:
        if step['turnover_hazard_per_year']!=0. or any(any(row) for row in step['credit_hazards_per_year']):raise ArithmeticError('unsupported mortgage credit/prepayment transition')
    start=c['as_of_ordinal'];end=start+c['horizon_days'];days=sorted(d for d in set(c['payment_dates'])|{p['start_ordinal'] for p in history['path']}|{end} if start<d<=end)
    contractual=c['opening_contractual_principal'];borrower=list(c['opening_borrower_principal']);investor=list(c['opening_investor_principal']);bi=[0.]*3;ii=[0.]*3;prob=list(c['opening_probability'])+[0.]*4
    note=c['current_note_rate'];coupon=note-(ownership['servicing_and_guarantee_spread'] if agency else 0.)
    if ownership['kind'] not in {'agency','whole_loan'} or coupon<0.:raise ArithmeticError('unsupported mortgage investor coupon/ownership')
    payment=c['current_payment'];burnout=c['opening_burnout_incentive_years'];ci=matured=0.;previous=start;cash=[];borrowercash=[];entitlements=[];recognition=[];points=[];bp=ip=gp=gi=0.
    for day in days:
        if check():raise InterruptedError('mortgage reference cancelled')
        path=max((p for p in history['path'] if p['start_ordinal']<=previous),key=lambda p:p['start_ordinal'])
        tau=consumer.year_fraction(previous,day,c['day_count']);ci+=contractual*note*tau
        for i in range(3):bi[i]+=borrower[i]*note*tau;ii[i]+=investor[i]*coupon*tau
        earned=sum(investor)*coupon*tau
        if earned:recognition.append(dict(origin_id='mortgage-earned-'+str(len(recognition)),start_ordinal=previous,end_ordinal=day,interest=earned))
        burnout+=max(note-path['refinancing_market_rate']-refi['incentive_threshold'],0.)*(day-previous)/365.
        if day in c['payment_dates']:
            if payment+1e-9<ci:raise ArithmeticError('unsupported negative mortgage amortization')
            final=day==c['payment_dates'][-1];scheduled=contractual if final else min(payment-ci,contractual);contractual-=scheduled
            for i in range(3):
                available=borrower[i]+bi[i] if final else payment*prob[i]
                actual_i=min(available,bi[i]);actual_p=min(available-actual_i,borrower[i]);borrower[i]-=actual_p;bi[i]-=actual_i
                if actual_p or actual_i:borrowercash.append(dict(event_id='scheduled_borrower_collection-'+str(len(borrowercash)),ordinal=day,kind='scheduled_borrower_collection',principal=actual_p,interest=actual_i,fees=0.))
                owner_p=(investor[i] if final else min(scheduled*prob[i],investor[i])) if agency else actual_p
                owner_i=ii[i] if agency else actual_i
                if agency:gp+=max(owner_p-actual_p,0.);gi+=max(ii[i]-actual_i,0.)
                investor[i]-=owner_p;ii[i]-=min(owner_i,ii[i]);bp+=actual_p;ip+=owner_p
                if owner_p or owner_i:entitlements.append(dict(event_id='scheduled_investor_entitlement-'+str(len(entitlements)),entitlement_ordinal=day,remittance_ordinal=day+ownership['remittance_lag_days'],kind='scheduled_investor_entitlement',principal=owner_p,interest=owner_i))
                if final and borrower[i]==bi[i]==investor[i]==ii[i]==0.:matured+=prob[i];prob[i]=0.
            ci=0.
        points.append(dict(ordinal=day,seasoning_days=day-c['origination_ordinal'],burnout_incentive_years=burnout,note_rate=note,current_payment=payment,contractual_principal=contractual,probability=prob.copy(),matured_probability=matured,borrower_principal=borrower.copy(),borrower_interest=bi.copy(),investor_principal=investor.copy(),investor_interest=ii.copy(),turnover_hazard_per_year=0.,refinance_hazard_per_year=0.,borrower_default_principal=0.,investor_default_principal=0.));previous=day
    borrowercash.sort(key=lambda x:(x['ordinal'],x['event_id']));entitlements.sort(key=lambda x:(x['entitlement_ordinal'],x['event_id']))
    for e in entitlements:cash.append(dict(event_id=e['kind']+'-'+str(len(cash)),ordinal=e['remittance_ordinal'],kind=e['kind'],principal=e['principal'],interest=e['interest'],fees=0.))
    cash.sort(key=lambda x:(x['ordinal'],x['event_id']))
    if contractual or any(borrower+bi+investor+ii):raise ArithmeticError('mortgage complete principal/interest tail required')
    return dict(schema='mortgage-state-1',identity=identity,ownership=ownership,monetary_unit=c['monetary_unit'],original_principal=c['original_principal'],original_payment=c['original_payment'],origination_ordinal=c['origination_ordinal'],borrower_cashflows=borrowercash,cashflows=cash,entitlements=entitlements,recognition=recognition,points=points,defaults=[],resets=[],borrower_scheduled_principal=bp,borrower_turnover_principal=0.,borrower_refinance_principal=0.,investor_scheduled_principal=ip,investor_turnover_principal=0.,investor_refinance_principal=0.,guarantee_principal_advance=gp,guarantee_interest_advance=gi,borrower_principal_conservation_error=sum(c['opening_borrower_principal'])-bp-sum(borrower),investor_principal_conservation_error=sum(c['opening_investor_principal'])-ip-sum(investor),calendar_mapping_verified=False,work_units=2*len(days))


def _cash(projection, kind):
    side = -1. if kind == 'deposit' else 1.
    return [dict(event_id=e['event_id'], ordinal=e['ordinal'],
                 principal=side * e['principal'],
                 interest=side * e['cash_interest' if kind == 'deposit' else 'interest'])
            for e in projection['cashflows']]


def _earned(p, kind, left, right):
    rows = p['interest' if kind == 'deposit' else 'recognition']
    name = 'amount' if kind == 'deposit' else 'interest'
    value = 0.
    for r in rows:
        overlap = min(right, r['end_ordinal']) - max(left, r['start_ordinal'])
        if overlap > 0:
            value += r[name] * overlap / (r['end_ordinal'] - r['start_ordinal'])
    return value


def _project(graph, kind, history, scenario, role, budget):
    budget.charge()
    if kind == 'consumer_installment':
        c = _contract(graph, kind)
        if any(s['action']['kind'] not in {'payment', 'payoff'} for s in history['steps']):
            raise ArithmeticError('unsupported product consumer transition')
        p = consumer.project(c, history, scenario, budget.check)
        p['identity']['measure'] = 'physical' if role == 'physical' else 'pricing'
        return p
    if kind == 'deposit':
        return deposit.project_declared(history['request'], measure='physical' if role == 'physical' else 'pricing', check=budget.check)
    if kind in {'card','mortgage'}:
        c=_contract(graph,kind);identity=dict(c['identity'],measure='physical' if role=='physical' else 'pricing',assumption_id=history['assumption_id'],run_id=scenario['run_id'])
        return (_card if kind=='card' else _mortgage)(c,history,identity,budget.check)
    raise ArithmeticError('unsupported independent product source')


def _price(p, kind, purchase, age, curve, spread):
    anchor = _df(curve, (age-purchase)/365.)
    side = -1. if kind == 'deposit' else 1.
    value = 0.
    for c in _cash(p, kind):
        if c['ordinal'] > age:
            value += side * (c['principal']+c['interest']) * _df(curve, (c['ordinal']-purchase)/365.) / anchor * math.exp(-spread*(c['ordinal']-age)/365.)
    return value


def _graph(raw, kind, actual, budget):
    if kind not in PRODUCTS or len(raw['positions']) != 1:
        raise ArithmeticError('unsupported independent product source coverage')
    _equal(actual['identity'], raw['identity'], 'graph/identity')
    _equal(actual['schema'], raw['schema'], 'graph/schema')
    _fields(actual, GRAPH_POLICIES[kind], 'graph/disclosures')
    _equal(actual['base_anchor']['schema'], ('advanced' if kind=='consumer_installment' else kind)+'-base-anchor-1', 'graph/anchor/schema')
    position = raw['positions'][0]
    locked = position['oas_target']
    if locked['kind'] != 'locked':
        raise ArithmeticError('product original locked OAS required')
    expected = []
    for scenario, histories in zip(raw['scenarios'], _histories(raw, kind), strict=True):
        if scenario['scenario_id'] != histories['scenario_id']:
            raise ArithmeticError('graph chronological scenario identity differs')
        for role in ROLES:
            if not histories[role] or abs(sum(h['weight'] for h in histories[role])-1.) > 1e-12:
                raise ArithmeticError('complete graph probability weights required')
            for history in histories[role]:
                p = _project(raw, kind, history, scenario, role, budget)
                source=dict(position_id=position['position_id'], scenario_id=scenario['scenario_id'], run_id=scenario['run_id'], path_id=history['path_id'], weight=history['weight'], role=role, projection=p)
                if kind in {'card','mortgage'}:source['assumption_id']=history['assumption_id']
                expected.append(source)
    _equal(actual['native_sources'], expected, 'graph/native_sources')
    oas = [dict(position_id=position['position_id'], base_oas=locked['base_oas'], calibration_kind='supplied_locked_oas')]
    _equal(actual['base_anchor']['oas'], oas, 'graph/locked_anchor')
    # The source content behind the graph anchor is independently identified.
    base = next(i for i,s in enumerate(raw['scenarios']) if s['scenario_id'] == raw['base_scenario_id'])
    common = [raw['schema'], raw['identity']['model_revision'], raw['identity']['paired_history_source'], raw['identity']['paired_history_revision'], raw['identity']['randomness_domain'], raw['as_of_ordinal'], raw['currency'], raw['base_scenario_id'], raw['scenarios'][base]]
    if kind == 'consumer_installment':
        common.append([[position['position_id'], locked, _contract(raw,kind), _histories(raw,kind)[base]['pricing']]])
    elif kind=='deposit':
        common[8:8] = [raw['tail_policy'], raw['cash_policy']]
        common.append([[position['position_id'], locked, _histories(raw,kind)[base]['pricing']]])
    else:
        common[8:8]=[raw['terminal_policy']]+([raw['cash_timing']] if kind=='card' else [])
        common.append([[position['position_id'],locked,_contract(raw,kind),_histories(raw,kind)[base]['pricing']]])
    _equal(actual['base_anchor']['calibration_input'], common, 'graph/calibration_input')
    _graph_metrics(raw, kind, actual, expected, budget)
    return expected


def _graph_metrics(raw, kind, actual, sources, budget):
    metrics=[]; reports=raw['reporting_ordinals']; position=raw['positions'][0]['position_id']
    for scenario in raw['scenarios']:
        budget.charge()
        paths=[s for s in sources if s['scenario_id']==scenario['scenario_id']]
        prices={role:0. for role in ROLES[1:]}
        for s in paths:
            if s['role']=='physical':continue
            bump=(-raw['bump_bp'] if s['role']=='pricing_down' else raw['bump_bp'] if s['role']=='pricing_up' else 0.)/10000.
            prices[s['role']]+=s['weight']*_price(s['projection'],kind,raw['as_of_ordinal'],raw['as_of_ordinal'],scenario['collateral_curve'],raw['positions'][0]['oas_target']['base_oas']+bump)
        pv=prices['pricing'];dv=(prices['pricing_down']-prices['pricing_up'])/(2.*raw['bump_bp'])
        physical=[s for s in paths if s['role']=='physical']
        if kind=='consumer_installment':
            row=dict(position_id=position,scenario_id=scenario['scenario_id'],pv=pv,supplied_history_dv01=dv,
                     gross_physical_contract_interest=[],physical_principal_cash=[],physical_interest_cash=[],physical_fee_cash=[],physical_default_principal=[],physical_principal_loss=[])
            for left,right in zip(reports,reports[1:]):
                row['gross_physical_contract_interest'].append(sum(s['weight']*_earned(s['projection'],kind,left,right) for s in physical))
                for key,component in [('physical_principal_cash','principal'),('physical_interest_cash','interest'),('physical_fee_cash','fees')]:
                    row[key].append(sum(s['weight']*sum(e[component] for e in s['projection']['cashflows'] if left<e['ordinal']<=right) for s in physical))
                row['physical_default_principal'].append(0.);row['physical_principal_loss'].append(0.)
        elif kind=='deposit':
            def amounts(left,right,opening=False):
                out=dict(interest_expense=0.,bank_principal_cash=0.,bank_interest_cash=0.,external_inflow=0.,withdrawal=0.,closure=0.,contractual_maturity=0.,assumed_terminal_payout=0.,migrated_principal=0.,migrated_accrued_interest=0.)
                for s in physical:
                    w=s['weight'];p=s['projection']
                    if not opening:out['interest_expense']+=w*_earned(p,kind,left,right)
                    for e in p['cashflows']:
                        if (e['ordinal']==left if opening else left<e['ordinal']<=right):
                            out['bank_principal_cash']+=w*e['principal'];out['bank_interest_cash']+=w*e['cash_interest']
                            if e['kind'] in {'withdrawal','closure','contractual_maturity','assumed_terminal_payout'}:out[e['kind']]-=w*e['principal']
                return out
            end=physical[0]['projection']['points'][-1]['ordinal']
            row=dict(position_id=position,scenario_id=scenario['scenario_id'],liability_pv=-pv,bank_cash_pv=pv,supplied_history_liability_dv01=-dv,physical_periods=[amounts(a,b) for a,b in zip(reports,reports[1:])],physical_tail=amounts(reports[-1],end),opening_action_cash=amounts(reports[0],reports[0],True))
        else:
            card_fields='gross_contract_interest principal_cash draw_cash principal_payment_cash interest_cash fee_cash recovery_principal_cash uncovered_recovery_cost_cash default_principal default_accrued_interest default_recognized_accrued_interest default_unrecognized_contract_interest default_fees principal_loss recovery_gross recovery_cost recovery_net'.split()
            mortgage_fields='gross_recognized_investor_interest borrower_principal_cash borrower_interest_cash investor_principal_cash investor_interest_cash principal_entitlement interest_entitlement borrower_default_principal investor_default_principal borrower_default_contract_interest investor_default_accrued_interest borrower_principal_loss borrower_recovery_principal_cash whole_loan_recovery_principal_cash agency_buyout_principal agency_buyout_interest agency_default_guaranteed_coupon_interest'.split()
            def amounts(left,right,opening=False):
                out=dict.fromkeys(card_fields if kind=='card' else mortgage_fields,0.)
                for source in physical:
                    p=source['projection'];w=source['weight']
                    if not opening:out['gross_contract_interest' if kind=='card' else 'gross_recognized_investor_interest']+=w*_earned(p,kind,left,right)
                    for event in p['cashflows']:
                        if (event['ordinal']==left if opening else left<event['ordinal']<=right):
                            if kind=='card':
                                out['principal_cash']+=w*event['principal'];out['interest_cash']+=w*event['interest'];out['principal_payment_cash']+=w*event['principal']
                            else:out['investor_principal_cash']+=w*event['principal'];out['investor_interest_cash']+=w*event['interest']
                    if kind=='mortgage':
                        for event in p['borrower_cashflows']:
                            if left<event['ordinal']<=right:out['borrower_principal_cash']+=w*event['principal'];out['borrower_interest_cash']+=w*event['interest']
                        for event in p['entitlements']:
                            if left<event['entitlement_ordinal']<=right:out['principal_entitlement']+=w*event['principal'];out['interest_entitlement']+=w*event['interest']
                return out
            end=max(max(s['projection']['points'][-1]['ordinal'],max([e['ordinal'] for e in s['projection']['cashflows']],default=reports[-1])) for s in physical)
            row=dict(position_id=position,scenario_id=scenario['scenario_id'],pv=pv,supplied_history_dv01=dv,physical_periods=[amounts(a,b) for a,b in zip(reports,reports[1:])],physical_tail=amounts(reports[-1],end))
            if kind=='card':row['opening_action_cash']=amounts(reports[0],reports[0],True)
            else:
                fields='borrower_scheduled_principal borrower_turnover_principal borrower_refinance_principal investor_scheduled_principal investor_turnover_principal investor_refinance_principal guarantee_principal_advance guarantee_interest_advance'.split()
                row['physical_totals']={k:sum(s['weight']*s['projection'][k] for s in physical) for k in fields}
        metrics.append(row)
    _equal(actual['metrics'],metrics,'graph/metrics')


def _income(u, p, kind, reports, budget):
    cash=_cash(p,kind);start=u['purchase_ordinal'];y=u['income_policy']['original_annual_effective_rate']
    if any(c['ordinal']<=start for c in cash):raise ArithmeticError('same-asof product cash unsupported')
    graph=u['purchase_graph']['request'];contract=_contract(graph,kind)
    if kind=='deposit':principal=sum(s['opening_balance'] for s in graph['positions'][0]['histories'][0]['physical'][0]['request']['segments'])
    elif kind=='consumer_installment':principal=contract['opening_principal']
    else:principal=sum(contract['opening_principal' if kind=='card' else 'opening_investor_principal'])
    carrying=u['quote_price_per_face_unit']*u['unit_face'];quote=carrying;accrued=cumulative=0.
    pv=sum((c['principal']+c['interest'])*math.exp(-math.log1p(y)*(c['ordinal']-start)/365.) for c in cash)
    if abs(pv-quote)>1e-10*max(abs(quote),1.):raise ArithmeticError('original effective yield/quote does not reconcile physical cash')
    origins=p['interest' if kind=='deposit' else 'recognition']
    full=max([start]+[c['ordinal'] for c in cash]+[e[k] for e in origins for k in ('start_ordinal','end_ordinal')])
    dates=sorted(({d for d in reports if d>=start}|{c['ordinal'] for c in cash}|{e[k] for e in origins for k in ('start_ordinal','end_ordinal') if e[k]>=start})-{start})
    budget.charge(len(dates)*(len(cash)+len(origins)+1))
    identity=dict(id=u['unit_id'],revision=u['income_policy']['review']['revision'])
    events=[];rows=[];states={start:dict(ordinal=start,principal=principal,accrued_interest=0.,gross_carrying_value=carrying,book_adjustment=carrying-principal,effective_interest_to_date=0.)};previous=start
    for day in dates:
        budget.charge();pc=ic=0.
        for c in cash:
            if c['ordinal']==day:pc+=c['principal'];ic+=c['interest']
        earned=_earned(p,kind,previous,day);effective=carrying*math.expm1(math.log1p(y)*(day-previous)/365.);opening=carrying
        event=dict(event_id=f"{u['unit_id']}:income:{day}",source=identity,payment_ordinal=day,contractual_interest_earned=earned,principal_cash=pc,interest_cash=ic,integral_fee_cash=0.,nonintegral_fee_cash=0.,yield_change=None)
        events.append(event);carrying+=effective-pc-ic;principal-=pc;accrued+=earned-ic;cumulative+=effective
        roundoff=64.*sys.float_info.epsilon*(abs(opening)+abs(pc+ic)+abs(effective))
        if carrying<0. and principal==0. and abs(accrued)<=roundoff and abs(carrying)<=roundoff:carrying=0.
        if carrying<0.:raise ArithmeticError('reviewed product carry becomes negative')
        basis=carrying-principal-accrued
        rows.append(dict(event_id=event['event_id'],source=identity,payment_ordinal=day,opening_gross_carrying_value=opening,annual_effective_rate=y,effective_interest=effective,contractual_interest_earned=earned,basis_amortization=effective-earned,modification_gain_loss=0.,recognized_nonintegral_fee_income=0.,cash_received=pc+ic,closing_principal=principal,closing_accrued_interest=accrued,closing_gross_carrying_value=carrying,closing_book_adjustment=basis,yield_review=None))
        states[day]=dict(ordinal=day,principal=principal,accrued_interest=accrued,gross_carrying_value=carrying,book_adjustment=basis,effective_interest_to_date=cumulative);previous=day
    terminal=states[full];scale=max(sum(abs(c['principal'])+abs(c['interest']) for c in cash),1.)
    if any(abs(terminal[k])>1e-10*scale for k in ('principal','accrued_interest','gross_carrying_value')):raise ArithmeticError('full physical cash/accrual/carry tail does not close')
    raw=dict(schema='dated-income-1',contract=identity,original_yield_anchor=u['income_policy']['original_yield_anchor'],policy=u['income_policy']['review'],currency=p['identity']['currency'],monetary_unit='currency_units',measure='physical',as_of_ordinal=start,full_contract_end_ordinal=max(full,max(dates)),opening_principal=states[start]['principal'],opening_accrued_interest=0.,opening_gross_carrying_value=quote,original_annual_effective_rate=y,opening_annual_effective_rate=y,events=events)
    result=dict(schema='dated-income-1',contract=identity,policy=u['income_policy']['review'],original_yield_anchor=u['income_policy']['original_yield_anchor'],currency=p['identity']['currency'],monetary_unit='currency_units',measure='physical',rows=rows,remaining_principal=principal,remaining_accrued_interest=accrued,remaining_gross_carrying_value=carrying)
    reportstates=[states[d] if d>=start else dict(ordinal=d,principal=0.,accrued_interest=0.,gross_carrying_value=0.,book_adjustment=0.,effective_interest_to_date=0.) for d in reports]
    return dict(raw_income_request=raw,native_income_result=result,complete_source_projection=p,full_cash_terminal_ordinal=full),reportstates


def _prefix(history, kind, day):
    if kind in {'consumer_installment','card'}:return [s for s in history['steps'] if s['ordinal']<=day]
    if kind=='mortgage':return dict(path=[p for p in history['path'] if p['start_ordinal']<day],arm_resets=[p for p in history['arm_resets'] if p['ordinal']<=day])
    out=deepcopy(history['request']);out['drivers']=[d for d in out['drivers'] if d['ordinal']<day]
    out['identity'].update(measure='prefix',assumption_id='prefix',assumption_rationale='prefix')
    return out


def _rights(sources,kind,weight=1.):
    out={}
    for s in sources:
        if s['role']=='physical':continue
        for c in _cash(s['projection'],kind):
            for name in ('principal','interest'):
                if c[name]:
                    key=(s['scenario_id'],s['role'],c['ordinal'],name)
                    out[key]=out.get(key,0.)+c[name]*s['weight']*weight
    return out


def _preparation(raw, p, budget):
    _equal(p['raw_source'],raw,'product/raw_source')
    reports=raw['reporting_ordinals'];h=len(reports)-1;n=len(raw['units'])
    scenarios=[s['scenario_id'] for s in raw['units'][0]['purchase_graph']['request']['scenarios']];s=len(scenarios)
    _fields(p,dict(schema='product-units-1',identity=raw['identity'],currency=raw['currency'],monetary_unit=raw['monetary_unit'],reporting_ordinals=reports,base_scenario_id=raw['base_scenario_id'],scenario_ids=scenarios,shape=[s,n,10,h],metrics=METRICS),'product')
    _fields(p,dict(income_policy='reviewed frozen original annual effective yield on native physical repayment/runoff cash; gross contractual recognition remains separate; no default/draw/nonaccrual/migration/new funding/modification/fee transitions',scope='actual supplied purchase and exact physical-prefix conditioned supplied pricing histories; coefficient preparation only, no solver, daily candidate acceptance, empirical calibration or 60000-position acceptance'),'product/disclosures')
    if type(p['admitted_work']) is not int or not 0 < p['admitted_work'] <= MAX_WORK:
        raise ArithmeticError('native admitted work receipt outside disclosed domain')
    if len(p['units'])!=n:raise ArithmeticError('product unit source order differs')
    values=[0.]*(s*n*10*h);funding=[];identities=[]
    for ui,(u,receipt) in enumerate(zip(raw['units'],p['units'],strict=True)):
        kind=u['purchase_graph']['kind'];graph=u['purchase_graph']['request'];side=-1. if kind=='deposit' else 1.;face=u['unit_face'];purchase=u['purchase_ordinal'];locked=graph['positions'][0]['oas_target']['base_oas']
        _fields(receipt,{k:u[k] for k in ('unit_id','template_id','purchase_ordinal','unit_face','quote_price_per_face_unit','accounting_category','capital_policy_revision')},'unit')
        _fields(receipt,dict(side=side,locked_purchase_oas=locked),'unit')
        identities.append(dict(unit_id=u['unit_id'],template_id=u['template_id'],purchase_ordinal=purchase,purchase_period=reports[:-1].index(purchase),side=side,accounting_category=u['accounting_category'],capital_policy_revision=u['capital_policy_revision'],quote_price_per_face_unit=u['quote_price_per_face_unit']))
        sources=_graph(graph,kind,receipt['complete_purchase_result'],budget)
        base=next(m for m in receipt['complete_purchase_result']['metrics'] if m['scenario_id']==raw['base_scenario_id'])
        quote=u['quote_price_per_face_unit']*face;basepv=base['liability_pv' if kind=='deposit' else 'pv']
        if abs(basepv-quote)>1e-10*max(abs(quote),1.):raise ArithmeticError('purchase OAS/quote mismatch')
        incomes=[];marks=[];original=_rights(sources,kind);purchase_rights={}
        for si,id_ in enumerate(scenarios):
            funding.append(dict(scenario_id=id_,unit_index=ui,path_id='purchase',source_event_id='purchase:'+u['unit_id'],ordinal=purchase,source_weight=1.,purchase_cash=-side*u['quote_price_per_face_unit'],principal_cash=0.,interest_cash=0.,fee_cash=0.,bank_cash=-side*u['quote_price_per_face_unit']))
            for source in (x for x in sources if x['role']=='physical' and x['scenario_id']==id_):
                path=source['path_id'];weight=source['weight'];projection=source['projection'];native,states=_income(u,projection,kind,reports,budget)
                incomes.append(dict(scenario_id=id_,path_id=path,source_weight=weight,opening_states=states,native_income=native))
                for period,(a,b) in enumerate(zip(states,states[1:])):
                    if reports[period]<purchase:continue
                    principal=a['principal']/face;effective=b['effective_interest_to_date']-a['effective_interest_to_date']
                    metrics=[side*effective/face,principal,0.]+[u['weights'][name]*principal for name in METRICS[3:8]]+[principal if side>0 else 0.,principal if side<0 else 0.]
                    for k,x in enumerate(metrics):values[((si*n+ui)*10+k)*h+period]+=weight*x
                for c in _cash(projection,kind):
                    pc=side*c['principal']*weight/face;ic=side*c['interest']*weight/face
                    funding.append(dict(scenario_id=id_,unit_index=ui,path_id=path,source_event_id=c['event_id'],ordinal=c['ordinal'],source_weight=weight,purchase_cash=0.,principal_cash=pc,interest_cash=ic,fee_cash=0.,bank_cash=pc+ic))
                actual=next(x for x in _histories(graph,kind)[si]['physical'] if x['path_id']==path)
                for period,day in enumerate(reports[:-1]):
                    if day<purchase:continue
                    matches=[b for b in u['age_branches'] if (b['ordinal'],b['scenario_id'],b['physical_path_id'])==(day,id_,path)]
                    if len(matches)!=1:raise ArithmeticError('complete unique conditioned age branch required')
                    branch=matches[0];braw=branch['graph']['request']
                    mark=receipt['age_marks'][len(marks)];bsources=_graph(braw,kind,mark['complete_native_result'],budget)
                    selected=_histories(braw,kind)[0]['physical'];want=dict(actual,weight=1.)
                    _equal(selected,[want],'branch/actual_path')
                    for role in ROLES:
                        for history in _histories(braw,kind)[0][role]:
                            _equal(_prefix(history,kind,day),_prefix(actual,kind,day),'branch/physical_prefix')
                    # Contract, market, run and original OAS remain exactly anchored.
                    if kind!='deposit':_equal(_contract(braw,kind),_contract(graph,kind),'branch/contract')
                    _equal(braw['identity'],graph['identity'],'branch/identity');_equal(braw['positions'][0]['oas_target'],graph['positions'][0]['oas_target'],'branch/oas')
                    _equal(braw['scenarios'],[graph['scenarios'][si]],'branch/market');_equal(braw['bump_bp'],graph['bump_bp'],'branch/bump')
                    _equal(braw['as_of_ordinal'],purchase,'branch/asof')
                    if day==purchase:
                        for key,x in _rights(bsources,kind,weight).items():purchase_rights[key]=purchase_rights.get(key,0.)+x
                    prices={role:0. for role in ROLES[1:]}
                    for bs in bsources:
                        if bs['role']=='physical':continue
                        shift=(-graph['bump_bp'] if bs['role']=='pricing_down' else graph['bump_bp'] if bs['role']=='pricing_up' else 0.)/10000.
                        prices[bs['role']]+=bs['weight']*_price(bs['projection'],kind,purchase,day,braw['scenarios'][0]['collateral_curve'],locked+shift)
                    dv=(prices['pricing_down']-prices['pricing_up'])/(2.*graph['bump_bp'])
                    marks.append(dict(ordinal=day,scenario_id=id_,physical_path_id=path,physical_weight=weight,pv_per_face=prices['pricing']/face,dv01_per_face=dv/face,locked_purchase_oas=locked,complete_native_result=mark['complete_native_result']))
                    values[((si*n+ui)*10+2)*h+period]+=weight*dv/face
        _equal(receipt['income'],incomes,'unit/income');_equal(receipt['age_marks'],marks,'unit/age_marks')
        for key in original.keys()|purchase_rights.keys():
            _equal(purchase_rights.get(key,0.),original.get(key,0.),'unit/purchase_q_rights')
    _equal(p['values'],values,'product/coefficients')
    funding.sort(key=lambda e:e['ordinal']);dates=sorted(set(reports)|{e['ordinal'] for e in funding});f=len(dates);demand=[0.]*(s*n*f);balances=[0.]*(s*n);cursor=0
    for di,day in enumerate(dates):
        budget.charge()
        while cursor<len(funding) and funding[cursor]['ordinal']<=day:
            e=funding[cursor];balances[scenarios.index(e['scenario_id'])*n+e['unit_index']]+=e['bank_cash'];cursor+=1
        for i,x in enumerate(balances):demand[i*f+di]=-x
    _fields(p['funding'],dict(scenario_ids=scenarios,ordinals=dates,shape=[s,n,f],demand=demand,cash_events=funding),'product/full_tail_funding')
    _equal(p['funding']['policy'],'scenario-specific native physical cash including full purchase/repayment/runoff tails; gross pretax end-of-day netting; no taxes, distributions or discretionary reinvestment','product/funding/policy')
    return identities,values,funding,demand,dates


def _base(b):
    if isinstance(b['eve'],(int,float)):return b
    return dict(nii=b.get('nii_total_$',0.),dv01=b['eve']['dv01_net_$'],eve=b['eve']['eve_$'],mv_assets=b['eve']['mv_assets_$'],l1=b['lcr']['hqla_l1_$'] if 'hqla_l1_$' in b['lcr'] else b['lcr']['hqla_$'],l2=b['lcr'].get('hqla_l2a_uncapped_$',b['lcr'].get('hqla_l2a_$',0.)),nco=b['lcr']['net_outflows_$'],asf=b['nsfr']['asf_$'],rsf=b['nsfr']['rsf_$'],cet1=b['capital']['cet1_path'][-1]['cet1_$'],rwa=b['capital']['rwa_total_$'],ni=.43*(1.-.45))


def _allocation(raw,result,p,identities,values,funding,demand,dates,budget):
    x=[_finite(a['face']) for a in result['allocation']]
    if len(x)!=len(identities) or any(a<0. for a in x):raise ArithmeticError('complete nonnegative ordered face allocation required')
    _equal([a['unit'] for a in result['allocation']],identities,'allocation/unit')
    if 'native_allocated_face' in result:_equal(result['native_allocated_face'],x,'allocation/native')
    n=len(x);h=len(raw['product']['reporting_ordinals'])-1;s=len(p['scenario_ids']);f=len(dates);c=raw['constraints']
    defaults=dict(lcr_min=1.10,nsfr_min=1.05,cet1_min=.10,eve_limit=.15,cash_budget=0.,max_total_assets=1e8,commercial=[],capital_limits=[]);c=dict(defaults,**c)
    if c['commercial']:raise ArithmeticError('legacy product commercial selectors unsupported')
    assets=sum(a for a,u in zip(x,identities) if u['side']>0.);liabilities=sum(a for a,u in zip(x,identities) if u['side']<0.);cost=sum(a*u['quote_price_per_face_unit'] for a,u in zip(x,identities) if u['side']>0.)
    templates={};categories={}
    for a,u in zip(x,identities):templates[u['template_id']]=templates.get(u['template_id'],0.)+a;categories[u['accounting_category']]=categories.get(u['accounting_category'],0.)+a
    for value,cap,label in [(assets,c['max_total_assets'],'asset face'),(cost,raw.get('max_purchase_cost_assets'),'purchase cost')]:
        if cap is not None and value>cap+max(.01,abs(cap)*1e-8):raise ArithmeticError(label+' cap fails')
    base_rows=raw['bases'];bases={b['scenario_id']:_base(b['base']) for b in base_rows}
    if len(bases)!=len(base_rows) or set(bases)!=set(p['scenario_ids']):raise ArithmeticError('complete unique scenario bases required')
    scenarios=[];replays=[];worst=math.inf;worstincremental=math.inf;slacks={}
    def row(label,lhs,rhs):
        slacks[label]=rhs-lhs
    for si,id_ in enumerate(p['scenario_ids']):
        budget.charge(n*10*h+n*f);a=[0.]*(10*h)
        for ui,face in enumerate(x):
            for k in range(10*h):a[k]+=face*values[(si*n+ui)*10*h+k]
        nii=sum(a[:h]);b=bases[id_];worst=min(worst,nii+b['nii']);worstincremental=min(worstincremental,nii)
        row(f's{si}:worst_case_nii',result['worst_case_nii_$']-nii,b['nii'])
        for m in range(h):
            row(f's{si}:m{m}:lcr_assets',c['lcr_min']*a[4*h+m]-a[3*h+m],b['l1']+b['l2']-c['lcr_min']*b['nco'])
            row(f's{si}:m{m}:lcr_cap',c['lcr_min']*a[4*h+m],b['l1']/.6-c['lcr_min']*b['nco'])
            row(f's{si}:m{m}:nsfr',c['nsfr_min']*a[6*h+m]-a[5*h+m],b['asf']-c['nsfr_min']*b['rsf'])
            row(f's{si}:m{m}:eve_lo',a[2*h+m],c['eve_limit']*b['eve']/200.-b['dv01'])
            row(f's{si}:m{m}:eve_hi',-a[2*h+m],c['eve_limit']*b['eve']/200.+b['dv01'])
        row(f's{si}:cet1_horizon',c['cet1_min']*a[8*h-1]-b['ni']*nii,b['cet1']-c['cet1_min']*b['rwa'])
        bank=0.;events=[e for e in funding if e['scenario_id']==id_];cursor=0;cash=[];gaps=[]
        for di,day in enumerate(dates):
            pc=principal=interest=fees=0.
            while cursor<len(events) and events[cursor]['ordinal']<=day:
                e=events[cursor];face=x[e['unit_index']];pc+=face*e['purchase_cash'];principal+=face*e['principal_cash'];interest+=face*e['interest_cash'];fees+=face*e['fee_cash'];bank+=face*e['bank_cash'];cursor+=1
            gap=sum(face*demand[(si*n+ui)*f+di] for ui,face in enumerate(x));gaps.append(gap)
            _equal(gap,-bank,'allocation/source_cash_conservation')
            if gap>c['cash_budget']+max(.01,c['cash_budget']*1e-7):raise ArithmeticError('full-tail scenario cash budget fails')
            row(f's{si}:date{day}:cash_resource',gap,c['cash_budget'])
            cash.append(dict(ordinal=day,purchase_cash=pc,principal_cash=principal,interest_cash=interest,fee_cash=fees,cumulative_bank_cash=bank,funding_demand=gap))
        scenarios.append(dict(scenario_id=id_,shape=[10,h],values=a,incremental_external_nii=nii,cash=cash))
        lcr=[(b['l1']+min(b['l2']+a[3*h+m],b['l1']*.4/.6))/max(b['nco']+a[4*h+m],1e-9)*100. for m in range(h)]
        nsfr=[(b['asf']+a[5*h+m])/max(b['rsf']+a[6*h+m],1e-9)*100. for m in range(h)]
        eve=[-(b['dv01']+a[2*h+m])*200./b['eve']*100. for m in range(h)];cet1=(b['cet1']+nii*b['ni'])/(b['rwa']+a[8*h-1])*100.
        if any(v<c['lcr_min']*100.-1e-5 for v in lcr) or any(v<c['nsfr_min']*100.-1e-5 for v in nsfr) or any(abs(v)>c['eve_limit']*100.+1e-5 for v in eve) or cet1<c['cet1_min']*100.-1e-5:raise ArithmeticError('independent KPI constraint fails')
        replays.append({'nii_total_$':nii,'nii_incremental':a[:h],'funding_gap':gaps,'funding_ordinals':dates,'principal_exposure_gap':[a[8*h+m]-a[9*h+m] for m in range(h)],'kpi_path':{'lcr_pct':lcr,'nsfr_pct':nsfr,'d_eve_pct_eve_+200':eve},'kpis':{'cet1_horizon_pct':cet1}})
    _fields(result['source_replay'],dict(schema='product-strategy-replay-1',units=identities,allocated_face=x,total_asset_face=assets,total_liability_face=liabilities,total_asset_purchase_cost=cost,template_face=templates,category_face=categories,scenarios=scenarios,worst_incremental_external_nii=worstincremental),'allocation/source_replay')
    _equal(result['source_replay']['scope'],'retained actual-purchase coefficients and scenario-specific full physical cash tails; reviewed performing effective yield only; no daily journal, empirical or joint credit acceptance','allocation/source_replay/scope')
    _equal(result['replay'],replays,'allocation/solver_replay')
    if abs(result['worst_case_nii_$']-worst)>max(.01,abs(result['worst_case_nii_$'])*1e-7):raise ArithmeticError('objective differs from independently replayed worst external NII')
    _equal(result['total_purchase_cost_assets_$'],cost,'allocation/purchase_cost')
    if 'total_new_assets_$' in result:_equal(result['total_new_assets_$'],assets,'allocation/asset_face')
    if c['max_total_assets'] is not None:row('cap:total_assets',assets,c['max_total_assets'])
    if raw.get('max_purchase_cost_assets') is not None:row('cap:purchase_cost_assets',cost,raw['max_purchase_cost_assets'])
    capital=[];solverunits=[dict(template=u['unit_id'],h=u['purchase_period'],side=u['side']) for u in identities]
    if c['capital_limits']:_equal(raw.get('capital_dependency'),raw['product'],'capital/full_dependency')
    elif raw.get('capital_dependency') is not None:raise ArithmeticError('capital dependency without capital limits')
    for r in c['capital_limits']:
        _equal(r['units'],solverunits,'capital/unit_grid');num=r['numerator'];den=r['denominator']
        if len(r['numerator_per_unit'])!=n or len(r['denominator_per_unit'])!=n:raise ArithmeticError('capital coefficient coverage differs')
        for ui,face in enumerate(x):num+=r['numerator_per_unit'][ui]*face;den+=r['denominator_per_unit'][ui]*face
        required=r['required_ratio']*den
        if den<=0. or num<required-max(.01,abs(required)*1e-8):raise ArithmeticError('explicit reviewed capital requirement fails')
        capital.append(dict(label=r['label'],policy_id=r['policy_id'],metric=r['metric'],scenario=r['scenario'],month=r['month'],numerator=num,denominator=den,ratio=num/den,required_ratio=r['required_ratio'],headroom=num-required))
        row('capital:'+r['label'],required-num,0.)
    if 'capital_replay' in result:_equal(result['capital_replay'],capital,'allocation/capital')
    elif capital:raise ArithmeticError('capital replay absent')
    face_replay=[]
    for r in raw.get('face_limits',[]):
        selector=r['selector'];kind=selector['kind'];amount=0.
        for u,face in zip(identities,x):
            selected=(u['unit_id']==selector['unit_id'] if kind=='unit' else u['template_id']==selector['template_id'] if kind=='template' else u['accounting_category']==selector['accounting_category'] if kind=='accounting_category' else u['side']>0. if kind=='all_assets' else u['side']<0. if kind=='all_liabilities' else None)
            if selected is None:raise ArithmeticError('unknown face selector')
            if selected:amount+=face
        tol=max(.01,abs(r['amount'])*1e-8)
        if r['sense']=='<=' and amount>r['amount']+tol or r['sense']=='>=' and amount<r['amount']-tol or r['sense'] not in {'<=','>='}:raise ArithmeticError('explicit product face limit fails')
        face_replay.append(dict(label=r['label'],policy_id=r['policy_id'],actual_face=amount,sense=r['sense'],required_face=r['amount']))
        sign=-1. if r['sense']=='>=' else 1.
        row('product_face:'+r['label'],sign*amount,sign*r['amount'])
    if face_replay:_equal(result['product_face_replay'],face_replay,'allocation/face_limits')
    violation=max([0.]+[-v for v in slacks.values()])
    if violation>.1:raise ArithmeticError('LP acceptance violation exceeds unchanged threshold')
    if 'native_max_violation_$' in result:_equal(result['native_max_violation_$'],violation,'allocation/lp_violation')
    for binding in result.get('binding_constraints',[]):
        if binding['constraint'] not in slacks or slacks[binding['constraint']]>=.01 or not math.isfinite(binding['shadow_price']):raise ArithmeticError('claimed binding row differs from independent residual')


def _admit_face(graph, kind, face, budget):
    def money(value):
        if isinstance(value,bool) or not isinstance(value,(int,float)) or not math.isfinite(value) or not 0<=value<=1e15:
            raise ArithmeticError('finite bounded nonnegative unit opening principal required')
        return value
    face=money(face)
    def reconcile(components):
        principal=0.
        for value in components:
            budget.charge();principal=money(principal+money(value))
        # Existing independent monetary tolerance; only summation roundoff is admitted.
        if face<=0. or abs(face-principal)>1e-9+1e-12*abs(principal):
            raise ArithmeticError('unit_face must equal contractual opening principal in every history')
    if kind=='deposit':
        for histories in _histories(graph,kind):
            for role in ROLES:
                for history in histories[role]:
                    reconcile(s['opening_balance'] for s in history['request']['segments'])
    else:
        c=_contract(graph,kind)
        reconcile([c['opening_principal']] if kind=='consumer_installment' else c['opening_principal' if kind=='card' else 'opening_investor_principal'])


def _admit_product(raw, result, budget):
    rawbytes=len(json.dumps(raw,allow_nan=False,separators=(',',':')).encode())
    outputbytes=len(json.dumps(result,allow_nan=False,separators=(',',':')).encode())
    if rawbytes>32*1024*1024 or outputbytes>128*1024*1024:raise ArithmeticError('product verifier byte admission exceeded')
    budget.charge(rawbytes*4)
    if raw.get('schema')!='product-units-1' or raw.get('monetary_unit')!='currency_units':raise ArithmeticError('explicit actual-purchase product source required')
    units=raw['units'];reports=raw['reporting_ordinals']
    if not 1<=len(units)<=256 or not 2<=len(reports)<=121 or reports!=sorted(set(reports)):raise ArithmeticError('product source count/date admission exceeded')
    # Bound source and quadratic income replay BEFORE projecting any history.
    estimate=0
    for u in units:
        for tagged in [u['purchase_graph']]+[b['graph'] for b in u['age_branches']]:
            graph=tagged['request'];kind=tagged['kind']
            if kind not in PRODUCTS:raise ArithmeticError('unsupported independent product source coverage')
            _admit_face(graph,kind,u['unit_face'],budget)
            for histories in _histories(graph,kind):
                for role in ROLES:
                    for history in histories[role]:
                        if kind=='consumer_installment':
                            c=_contract(graph,kind);k=len(c['payment_dates'])+len(history['steps'])+1
                            estimate+=k*100+(2*k+len(reports))*(2*k+1)*4
                        elif kind=='deposit':
                            r=history['request'];k=r['horizon_days'];n=len(r['segments']);estimate+=k*n*100+(k+len(reports))*(4*k*n+1)*4
                        elif kind=='card':
                            c=_contract(graph,kind);k=c['horizon_days']+len(history['steps'])+1
                            estimate+=k*(len(c['cycles'])+len(history['steps'])+16)*4+k*300+(k+len(reports))*(4*k+1)*4
                        else:
                            c=_contract(graph,kind);k=len(c['payment_dates'])+len(history['path'])+1;estimate+=k*300+(2*k+len(reports))*(4*k+1)*4
    budget.charge(estimate)


def verify_preparation(raw_product, product_units_result, check=lambda:False):
    """Publication-only raw product economic replay without allocation/solver claims."""
    budget=_Budget(check);budget.charge()
    _admit_product(raw_product,product_units_result,budget)
    _preparation(raw_product,product_units_result,budget)
    budget.charge()
    return dict(verified=True,product_source_recomputed=True,independent_source_products=list(PRODUCTS),full_tail_cash_verified=True,coefficient_preparation_verified=True,allocation_verified=False,daily_candidate_journal_verified=False,empirical_validation=False)


def verify(raw_request, result, check=lambda:False):
    """Reconstruct source economics, then replay allocation without native calls.

    Infeasible returns are source-verified only; no infeasibility/optimality proof.
    Unowned default/draw/grace/migration/ARM sources fail closed.
    """
    budget=_Budget(check);budget.charge()
    if raw_request.get('schema')!='strategy-product-1' or raw_request.get('op')!='optimize_product':raise ArithmeticError('explicit product strategy source required')
    _admit_face_selectors(raw_request,budget)
    raw=raw_request['product'];_admit_product(raw,result,budget)
    wrapper=result['product_preparation'];_equal(wrapper['schema'],'product-strategy-preparation-1','preparation/schema')
    p=wrapper['preparation'];identities,values,funding,demand,dates=_preparation(raw,p,budget)
    _equal(wrapper['units'],identities,'preparation/unit_identities')
    feasible=result.get('feasible')
    if feasible is False:
        if result.get('allocation')!=[] or result.get('validated') is not False or result.get('dynamic_validated') is not False or any(k in result for k in ('source_replay','native_allocated_face')):raise ArithmeticError('nonfeasible result cannot retain accepted allocation')
        return dict(verified=True,product_source_recomputed=True,independent_source_products=list(PRODUCTS),allocation_verified=False,solver_infeasibility_proven=False,empirical_validation=False)
    if result.get('validated') is not True or result.get('dynamic_validated') is not False:raise ArithmeticError('explicit coefficient-only financial validation required')
    _allocation(raw_request,result,p,identities,values,funding,demand,dates,budget)
    budget.charge()
    return dict(verified=True,product_source_recomputed=True,independent_source_products=list(PRODUCTS),allocation_verified=True,full_tail_cash_verified=True,capital_constraints_replayed=True,global_optimality_proven=False,daily_candidate_journal_verified=False,empirical_validation=False)
