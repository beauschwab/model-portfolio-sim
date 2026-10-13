"""Independent source/constraint publication gates with retained native fixtures."""
from copy import deepcopy
import json
import math
from pathlib import Path
import pytest

from portfolio_risk.analytics.product_strategy_reference import verify
from portfolio_risk.core.product_strategy_native import run

ROOT = Path(__file__).resolve().parents[3]


def example():
    return json.loads((ROOT/'examples/models/strategy-product-1.json').read_text())


@pytest.fixture(scope='module')
def valid():
    raw = example()
    return raw, run(raw, threads=1)[0]


def test_complete_source_allocation_cash_and_original_eir_hand_calculation(valid, monkeypatch):
    raw, result = valid
    from portfolio_risk.core import quant_native, decision_native
    def forbidden(*args, **kwargs):
        pytest.fail('verification called native finance')
    monkeypatch.setattr(quant_native, 'term_call', forbidden)
    monkeypatch.setattr(decision_native.NativeDecision, 'call', forbidden)
    receipt = verify(raw, result)
    assert receipt['verified'] and receipt['product_source_recomputed']
    assert not receipt['global_optimality_proven'] and not receipt['empirical_validation']
    assert receipt == verify(json.loads(json.dumps(raw)), json.loads(json.dumps(result)))
    assert [r['face'] for r in result['allocation']] == pytest.approx([20, 80])
    purchase = 20*raw['product']['units'][0]['quote_price_per_face_unit'] + 80*raw['product']['units'][1]['quote_price_per_face_unit']
    cash = result['source_replay']['scenarios'][0]['cash']
    assert cash[-1]['cumulative_bank_cash'] == pytest.approx(104.45-purchase, abs=1e-10)
    for i, y in enumerate((.02,.03)):
        quote = 529*math.exp(-math.log1p(y)*29/365) + 515.5*math.exp(-math.log1p(y)*60/365)
        assert raw['product']['units'][i]['quote_price_per_face_unit'] == pytest.approx(quote/1000)
        expected = quote*math.expm1(math.log1p(y)*29/365)/1000
        assert result['product_preparation']['preparation']['values'][i*20] == pytest.approx(expected, abs=1e-12)


@pytest.mark.parametrize('path', [
    ('product_preparation','units',0,'capital_policy_revision'),
    ('product_preparation','preparation','raw_source','identity','crn_id'),
    ('product_preparation','preparation','values',0),
    ('product_preparation','preparation','values',4),
    ('product_preparation','preparation','funding','demand',0),
    ('product_preparation','preparation','funding','cash_events',0,'source_event_id'),
    ('product_preparation','preparation','units',0,'income',0,'source_weight'),
    ('product_preparation','preparation','units',0,'income',0,'opening_states',1,'gross_carrying_value'),
    ('product_preparation','preparation','units',0,'income',0,'native_income','native_income_result','rows',0,'effective_interest'),
    ('product_preparation','preparation','units',0,'income',0,'native_income','raw_income_request','policy','revision'),
    ('product_preparation','preparation','units',0,'complete_purchase_result','native_sources',0,'projection','cashflows',0,'principal'),
    ('product_preparation','preparation','units',0,'complete_purchase_result','metrics',0,'pv'),
    ('product_preparation','preparation','units',0,'age_marks',1,'dv01_per_face'),
    ('allocation',0,'face'),
    ('allocation',0,'unit','purchase_ordinal'),
    ('native_allocated_face',0),
    ('source_replay','scenarios',0,'cash',1,'principal_cash'),
    ('source_replay','scenarios',0,'values',0),
    ('source_replay','total_asset_purchase_cost'),
    ('source_replay','template_face','loan-template-1'),
    ('replay',0,'kpi_path','lcr_pct',0),
    ('product_face_replay',0,'required_face'),
    ('worst_case_nii_$',),
])
def test_any_retained_financial_or_source_tampering_fails(valid,path):
    raw,result=valid;bad=deepcopy(result);obj=bad
    for key in path[:-1]:obj=obj[key]
    key=path[-1];obj[key]=obj[key]+'-tampered' if isinstance(obj[key],str) else obj[key]+1
    with pytest.raises(ArithmeticError):verify(raw,bad)


def test_original_source_numeric_identity_and_full_tail_omission_fail(valid):
    raw,result=valid
    bad=deepcopy(result);bad['product_preparation']['preparation']['raw_source']['reporting_ordinals'][0]=float(raw['product']['reporting_ordinals'][0])
    with pytest.raises(ArithmeticError,match='identity'):verify(raw,bad)
    bad=deepcopy(result);bad['product_preparation']['preparation']['funding']['cash_events'].pop()
    with pytest.raises(ArithmeticError):verify(raw,bad)
    bad=deepcopy(result);bad['allocation'].reverse()
    with pytest.raises(ArithmeticError):verify(raw,bad)


def test_selected_coefficient_evaluation_does_not_require_solver_vector(valid):
    raw,result=valid;selected=deepcopy(result);selected.pop('native_allocated_face')
    assert verify(raw,selected)['allocation_verified']


def test_nonfeasible_only_source_is_verified_and_no_infeasibility_claim(valid):
    raw,result=valid
    bad=dict(feasible=False,allocation=[],validated=False,dynamic_validated=False,product_preparation=deepcopy(result['product_preparation']))
    receipt=verify(raw,bad)
    assert receipt['verified'] and not receipt['allocation_verified'] and not receipt['solver_infeasibility_proven']
    bad['allocation']=result['allocation']
    with pytest.raises(ArithmeticError):verify(raw,bad)


def test_cancellation_work_and_unsupported_source_fail_before_generation(valid,monkeypatch):
    raw,result=valid
    with pytest.raises(InterruptedError):verify(raw,result,check=lambda:True)
    bad=deepcopy(raw);bad['product']['units'][0]['purchase_graph']['kind']='unknown'
    with pytest.raises(ArithmeticError,match='coverage'):verify(bad,result)
    from portfolio_risk.analytics import consumer_schedule_reference
    def forbidden(*a,**k):pytest.fail('work admission failed to precede source calculation')
    monkeypatch.setattr(consumer_schedule_reference,'project',forbidden)
    bad=deepcopy(raw);g=bad['product']['units'][0]['purchase_graph']['request'];g['positions'][0]['product']['contract']['payment_dates']=list(range(4000))
    with pytest.raises(ArithmeticError,match='work admission'):verify(bad,result)


def test_deposit_expense_eir_tail_and_saved_kpi_base():
    product=json.loads((ROOT/'examples/models/product-units-1.json').read_text())
    # Positive premium deposit proves signed bank expense; separate signed coupon
    # source fixture is covered by the reusable declared deposit oracle.
    raw=example();raw['product']=product;raw['constraints']['cash_budget']=10.;raw['face_limits']=[];raw['capital_dependency']=None;raw['max_purchase_cost_assets']=None
    raw['bases'][0]['scenario_id']='baseline'
    normalized=raw['bases'][0]['base']
    raw['bases'][0]['base']={'nii_total_$':normalized['nii'],'eve':{'dv01_net_$':normalized['dv01'],'eve_$':normalized['eve'],'mv_assets_$':normalized['mv_assets']},'lcr':{'hqla_$':normalized['l1'],'hqla_l2a_$':normalized['l2'],'net_outflows_$':normalized['nco']},'nsfr':{'asf_$':normalized['asf'],'rsf_$':normalized['rsf']},'capital':{'cet1_path':[{'cet1_$':normalized['cet1']}],'rwa_total_$':normalized['rwa']}}
    result=run(raw,threads=1)[0]
    assert verify(raw,result)['verified']
    assert result['product_preparation']['preparation']['values'][0]<0


def test_explicit_capital_limits_replay_and_stale_dependency(valid):
    raw,_=valid;raw=deepcopy(raw);units=raw['product']['units']
    raw['constraints']['capital_limits']=[dict(label='reviewed',policy_id='reviewed-v1',metric='cet1',scenario=0,month=1,units=[dict(template=u['unit_id'],h=0,side=1.) for u in units],numerator=30.,denominator=100.,required_ratio=.1,numerator_per_unit=[0.,0.],denominator_per_unit=[1.,1.])]
    raw['capital_dependency']=deepcopy(raw['product'])
    result=run(raw,threads=1)[0]
    assert verify(raw,result)['capital_constraints_replayed']
    bad=deepcopy(result);bad['capital_replay'][0]['numerator']+=1
    with pytest.raises(ArithmeticError):verify(raw,bad)
    bad=deepcopy(raw);bad['capital_dependency']['identity']['input_revision']='stale'
    with pytest.raises(ArithmeticError):verify(bad,result)


def test_signed_deposit_coupon_and_eir_are_independent_bank_income():
    product=json.loads((ROOT/'examples/models/product-units-1.json').read_text())
    u=product['units'][0];quote=999*math.exp(-math.log1p(-.01)*2/365)
    u['quote_price_per_face_unit']=quote/1000.;u['income_policy']['original_annual_effective_rate']=-.01
    for tagged in [u['purchase_graph']]+[b['graph'] for b in u['age_branches']]:
        g=tagged['request'];g['positions'][0]['oas_target']['base_oas']=math.log1p(-.01)-.05
        for history in g['positions'][0]['histories']:
            for role in ('physical','pricing','pricing_down','pricing_up'):
                for path in history[role]:
                    for segment in path['request']['segments']:
                        segment['opening_paid_rate']=-.1825
                        segment['repricing']['floor']=-.2
                        segment['repricing']['target']['intercept']=-.1825
    raw=example();raw['product']=product;raw['bases'][0]['scenario_id']='baseline';raw['constraints']['cash_budget']=10.;raw['max_purchase_cost_assets']=None;raw['capital_dependency']=None
    raw['face_limits']=[dict(label='liability-cap',policy_id='synthetic-v1',selector=dict(kind='all_liabilities'),sense='<=',amount=100.)]
    result=run(raw,threads=1)[0]
    assert verify(raw,result)['verified']
    p=result['product_preparation']['preparation'];assert p['values'][0]>0.
    assert p['units'][0]['income'][0]['native_income']['native_income_result']['rows'][0]['contractual_interest_earned']==pytest.approx(-.5)
    assert p['values'][0]==pytest.approx(-quote*math.expm1(math.log1p(-.01)/365)/1000.,abs=1e-12)


def test_physical_path_weights_applied_once_and_purchase_once(valid):
    raw,result=valid;raw=deepcopy(raw)
    for u in raw['product']['units']:
        original=u['purchase_graph']['request']['positions'][0]['product']['histories'][0]['physical'][0]
        physical=[dict(deepcopy(original),path_id='p1',weight=.25),dict(deepcopy(original),path_id='p2',weight=.75)]
        u['purchase_graph']['request']['positions'][0]['product']['histories'][0]['physical']=physical
        branches=[]
        for b in u['age_branches']:
            for path in physical:
                copy=deepcopy(b);copy['physical_path_id']=path['path_id'];copy['graph']['request']['positions'][0]['product']['histories'][0]['physical']=[dict(deepcopy(path),weight=1.)];branches.append(copy)
        u['age_branches']=branches
    weighted=run(raw,threads=1)[0]
    assert verify(raw,weighted)['verified']
    assert weighted['product_preparation']['preparation']['values']==pytest.approx(result['product_preparation']['preparation']['values'],abs=1e-12)
    cash=weighted['product_preparation']['preparation']['funding']['cash_events']
    assert len([e for e in cash if e['path_id']=='purchase'])==2


def test_constraint_changes_and_forged_binding_rows_fail(valid):
    raw,result=valid
    bad=deepcopy(raw);bad['constraints']['cash_budget']=0.
    with pytest.raises(ArithmeticError,match='cash budget'):verify(bad,result)
    bad=deepcopy(raw);bad['face_limits'][1]['amount']=10.
    with pytest.raises(ArithmeticError,match='face limit'):verify(bad,result)
    bad=deepcopy(result);bad['binding_constraints']=[dict(constraint='not-a-row',shadow_price=1.)]
    with pytest.raises(ArithmeticError,match='binding'):verify(raw,bad)


def product_for(kind):
    """Actual purchase quoted by hand, with supplied full original age histories."""
    g=json.loads((ROOT/f'examples/models/{kind}-product-graph-1.json').read_text())
    y=.02;start=g['as_of_ordinal']
    if kind=='card':
        c=g['positions'][0]['contract'];c['initial_purchase_grace']=False;c['regain_grace_after_full_statement_payment']=False;c['horizon_days']=4
        g['reporting_ordinals']=[start+i for i in range(5)]
        c['cycles']=[dict(statement_ordinal=start+1,due_ordinal=start+2),dict(statement_ordinal=start+3,due_ordinal=start+4)]
        for history in g['positions'][0]['histories']:
            for role in ('physical','pricing','pricing_down','pricing_up'):
                for h in history[role]:h['steps']=[dict(ordinal=start+2,action=dict(kind='payment',amount=1001.)),dict(ordinal=start+4,action=dict(kind='payment',amount=1.)),dict(ordinal=start+4,action=dict(kind='set_limit',amount=0.))]
        face=1000.;quote=1001*math.exp(-math.log1p(y)*2/365)+math.exp(-math.log1p(y)*4/365)
    else:
        face=100.;quote=34*math.exp(-math.log1p(y)*12/365)+34*math.exp(-math.log1p(y)*22/365)+32.1982661*math.exp(-math.log1p(y)*32/365)
    curve=g['scenarios'][0]['collateral_curve'];rate=-math.log(curve['discount_factors'][-1])/curve['times_years'][-1]
    g['positions'][0]['oas_target']['base_oas']=math.log1p(y)-rate
    product=json.loads((ROOT/'examples/models/product-units-1.json').read_text());u=product['units'][0]
    u['unit_id']=g['positions'][0]['position_id'];u['unit_face']=face;u['purchase_ordinal']=start;u['quote_price_per_face_unit']=quote/face;u['income_policy']['original_annual_effective_rate']=y;u['purchase_graph']=dict(kind=kind,request=g)
    path=g['positions'][0]['histories'][0]['physical'][0]['path_id']
    u['age_branches']=[dict(ordinal=day,scenario_id=g['base_scenario_id'],physical_path_id=path,graph=deepcopy(u['purchase_graph'])) for day in g['reporting_ordinals'][:-1]]
    product['reporting_ordinals']=g['reporting_ordinals'];product['base_scenario_id']=g['base_scenario_id']
    raw=example();raw['product']=product;raw['bases'][0]['scenario_id']=g['base_scenario_id'];raw['constraints']['cash_budget']=1000.;raw['max_purchase_cost_assets']=None;raw['capital_dependency']=None
    raw['face_limits']=[dict(label='asset-cap',policy_id='hand-v1',selector=dict(kind='all_assets'),sense='<=',amount=100.)]
    return raw,quote


@pytest.mark.parametrize('kind',['card','mortgage'])
def test_card_and_mortgage_actual_tail_source_preparation_and_strategy(kind,monkeypatch):
    from portfolio_risk.analytics.product_strategy_reference import verify_preparation
    raw,quote=product_for(kind);result=run(raw,threads=1)[0];p=result['product_preparation']['preparation'];u=p['units'][0]
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*args,**kwargs):pytest.fail('independent source replay invoked native finance')
    monkeypatch.setattr(quant_native,'term_call',forbidden);monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    assert verify(raw,result)['verified']
    receipt=verify_preparation(raw['product'],p)
    assert receipt['coefficient_preparation_verified'] and not receipt['allocation_verified']
    assert receipt==verify_preparation(json.loads(json.dumps(raw['product'])),json.loads(json.dumps(p)))
    if kind=='card':
        source=u['income'][0]['native_income']['complete_source_projection']
        assert source['cashflows'][0]['principal']==1000 and source['cashflows'][1]['interest']==1
        assert p['values'][2]>0. and u['income'][0]['opening_states'][2]['principal']==0.
        assert p['values'][0]==pytest.approx(quote*math.expm1(math.log1p(.02)/365)/1000,abs=1e-12)
    else:
        assert p['funding']['ordinals'][-1]==raw['product']['units'][0]['purchase_ordinal']+32
        assert u['income'][0]['opening_states'][1]['principal']==100.
        assert p['values'][0]==pytest.approx(quote*math.expm1(math.log1p(.02)*10/365)/100,abs=1e-12)
        assert u['income'][0]['native_income']['complete_source_projection']['entitlements'][0]['remittance_ordinal']==raw['product']['units'][0]['purchase_ordinal']+12
    for path in [('values',0),('funding','cash_events',-1,'interest_cash'),('units',0,'complete_purchase_result','native_sources',0,'projection','points',0,'principal' if kind=='card' else 'borrower_principal',0),('units',0,'complete_purchase_result','native_sources',0,'projection','cashflows',-1,'interest')]:
        bad=deepcopy(p);obj=bad
        for key in path[:-1]:obj=obj[key]
        obj[path[-1]]+=1
        with pytest.raises(ArithmeticError):verify_preparation(raw['product'],bad)


def test_preparation_only_retains_exact_original_identity_and_cancellation(valid):
    from portfolio_risk.analytics.product_strategy_reference import verify_preparation
    raw,result=valid;p=result['product_preparation']['preparation']
    assert verify_preparation(raw['product'],p)['verified']
    bad=deepcopy(p);bad['raw_source']['identity']['crn_id']='wrong'
    with pytest.raises(ArithmeticError):verify_preparation(raw['product'],bad)
    with pytest.raises(InterruptedError):verify_preparation(raw['product'],p,check=lambda:True)


def reanchor(raw,quote,face):
    u=raw['product']['units'][0];u['quote_price_per_face_unit']=quote/face;u['unit_face']=face
    g=u['purchase_graph']['request'];path=g['positions'][0]['histories'][0]['physical'][0]['path_id']
    u['age_branches']=[dict(ordinal=day,scenario_id=g['base_scenario_id'],physical_path_id=path,graph=deepcopy(u['purchase_graph'])) for day in g['reporting_ordinals'][:-1]]


def test_three_card_classes_minimum_order_excess_apr_promotion_and_compounding():
    raw,_=product_for('card');g=raw['product']['units'][0]['purchase_graph']['request'];c=g['positions'][0]['contract'];start=g['as_of_ordinal']
    c['opening_principal']=[100.,200.,300.];c['compound_billed_interest']=True
    c['rates'].update(purchase=0.,cash_advance=0.,promotional=.365,post_promotion=0.,promotional_expiry_ordinal=start+3)
    c['minimum'].update(floor=50.,principal_fraction=0.,include_billed_interest=False,include_fees=False,allocation_order=['purchase','cash_advance','promotional'])
    # Day one bills .3. Minimum pays purchase 50, excess pays promotional
    # billed .3 then principal49.7. Prior-day compound interest .3003 is
    # still unbilled; day three earns .2503 before promotion expires.
    final=500.3+.3003+.2503
    for histories in g['positions'][0]['histories']:
        for role in ('physical','pricing','pricing_down','pricing_up'):
            for h in histories[role]:h['steps'][0]['action']['amount']=100.;h['steps'][1]['action']['amount']=final+1e-10
    quote=100.*math.exp(-math.log1p(.02)*2/365)+final*math.exp(-math.log1p(.02)*4/365)
    reanchor(raw,quote,600.)
    result=run(raw,threads=1)[0]
    assert verify(raw,result)['verified']
    source=result['product_preparation']['preparation']['units'][0]['income'][0]['native_income']['complete_source_projection']
    assert source['points'][2]['principal']==pytest.approx([50.,200.,250.3],abs=1e-12)
    assert source['points'][2]['unbilled_interest'][2]==pytest.approx(.3003,abs=1e-12)
    assert source['statements'][1]['billed_interest'][2]==pytest.approx(.5506,abs=1e-12)
    assert source['cashflows'][0]['interest']==pytest.approx(.3,abs=1e-12)
    assert source['cashflows'][1]['principal']==pytest.approx(500.3,abs=1e-12)


def test_agency_fixed_mortgage_investor_coupon_is_not_borrower_coupon():
    raw,_=product_for('mortgage');g=raw['product']['units'][0]['purchase_graph']['request'];c=g['positions'][0]['contract']
    c['ownership']=dict(kind='agency',issuer='synthetic-guarantor',guarantee=dict(id='reviewed-guarantee',revision='v1'),remittance_lag_days=2,default_buyout_lag_days=5,servicing_and_guarantee_spread=.01,default_coupon_policy='frozen_at_default')
    c['horizon_days']=35
    principals=[33.9,33.9339,32.1661];interest=[100*.0265*10/365,66.1*.0265*10/365,32.1661*.0265*10/365]
    quote=sum((p+i)*math.exp(-math.log1p(.02)*d/365) for p,i,d in zip(principals,interest,[12,22,32]))
    reanchor(raw,quote,100.)
    result=run(raw,threads=1)[0]
    assert verify(raw,result)['verified']
    source=result['product_preparation']['preparation']['units'][0]['income'][0]['native_income']['complete_source_projection']
    assert source['cashflows'][0]['interest']==pytest.approx(interest[0],abs=1e-12)
    assert source['borrower_cashflows'][0]['interest']==pytest.approx(.1,abs=1e-12)
    assert source['guarantee_principal_advance']==0 and source['guarantee_interest_advance']==0
    assert source['cashflows'][-1]['ordinal']>raw['product']['reporting_ordinals'][-1]


@pytest.mark.parametrize('kind,mutate',[
    ('card',lambda c,h:c.update(initial_purchase_grace=True)),
    ('card',lambda c,h:h['steps'].insert(0,dict(ordinal=c['as_of_ordinal']+1,action=dict(kind='draw',balance_class='purchase',amount=1.)))),
    ('mortgage',lambda c,h:c.update(rate=dict(kind='arm'))),
    ('mortgage',lambda c,h:h['path'][0]['credit_hazards_per_year'][0].__setitem__(3,.1)),
])
def test_new_source_oracles_reject_unowned_histories(kind,mutate):
    from portfolio_risk.analytics.product_strategy_reference import _card,_mortgage
    raw,_=product_for(kind);g=raw['product']['units'][0]['purchase_graph']['request'];c=g['positions'][0]['contract'];h=g['positions'][0]['histories'][0]['physical'][0]
    mutate(c,h)
    with pytest.raises(ArithmeticError,match='unsupported'):
        (_card if kind=='card' else _mortgage)(c,h,c['identity'],lambda:False)
