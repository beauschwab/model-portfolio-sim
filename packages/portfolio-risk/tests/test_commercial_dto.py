"""Browser JSON scalars retain native raw-Value vs typed-owner boundaries."""
from copy import deepcopy
import json
import math
from pathlib import Path
import subprocess
import pytest

from portfolio_risk.analytics.commercial_dto import typed,graph_typed
from portfolio_risk.analytics.modeled_json_identity import exact_json_identity as exact

ROOT=Path(__file__).resolve().parents[3]


def browser(raw):
    # Run the actual JS operation used by fetch, rather than imitating its number
    # formatting with a Python transformation (notably integral f64 and -0).
    body=subprocess.check_output(['node','-e','process.stdout.write(JSON.stringify(JSON.parse(require("fs").readFileSync(0,"utf8"))))'],
        input=json.dumps(raw,allow_nan=False,separators=(',',':')).encode())
    return json.loads(body)


def test_schema_owned_raw_ledger_values_preserve_every_scalar_category():
    spec=dict(horizon_days=30,accounts=[dict(id='bank',cash=1000,equity=1000.,cash_floor=-0.,cet1_floor=0,leverage_floor=0.,capital_deductions=0,
        include_aoci=True,annual_fees=0,annual_costs=0.,tax_rate=0,annual_dividends=0,lcr_floor=0,nsfr_floor=0,htm_asset_limit=1,htm_taint_materiality=0.)],
        positions=[dict(id='loan',account='bank',kind='loan',balance=0,rate=0,floating_beta=0.,risk_weight=0,asf_weight=0,rsf_weight=0,lcr_outflow_weight=0,hqla_weight=0,
        duration=0.,allowance=0,annual_pd=0,lgd=0,watch_fraction=0,watch_pd_multiplier=1,migration_rate=0,commitment=0,draw_fraction=0,monthly_runoff=0,
        uninsured_fraction=0,concentration=0,operational_fraction=0,digital_fraction=0,rollover=0,eligible_fraction=0,encumbered_fraction=0,haircut=0,
        book_adjustment=-0.,opening_accrued=0,pledged_face=0,lcr_inflow_weight=0,opening_market_price=1,maturity_day=30,payment_interval_days=30)],
        netting_sets=[dict(id='n',fair_value=-0.,posted_margin=0,received_margin=0,stress_loss=0,margin_threshold=0,annual_pd=0,lgd=0,wrong_way_multiplier=1,risk_weight=1,margin_delay=0)],
        policies=[dict(id='p',trigger_cash=0,limit=100,execution_cost=0,funding_rate=0,htm_sale_limit=0,delay_days=1,approval_day=0,allow_htm_sale=False)],
        scenarios=[dict(name='stress',rate_shift=0,spread_shift=0,deposit_flight=0,pd_multiplier=1,migration_multiplier=1,lgd_add=0,draw_multiplier=0,
        rollover_loss=0,haircut_add=0,market_shock=0,start_day=1,outage_days=0)],reverse_severities=[0,1.,-0.],
        cashflows=[dict(position='loan',day=1,principal=0,cash_interest=0,accrual_interest=0,book_amortization=-0.)])
    for schema,key in [('commercial-credit-ledger-1','spec'),('commercial-candidate-journal-1','specification'),('commercial-generated-candidate-journal-1','specification')]:
        raw=dict(schema=schema,**{key:spec});before=deepcopy(raw)
        for normalize in(typed,graph_typed):
            actual=normalize(raw);assert exact(actual,raw)and exact(raw,before)
            assert math.copysign(1.,actual[key]['accounts'][0]['cash_floor'])==-1.
            actual[key]['accounts'][0]['cash']=7
            assert raw[key]['accounts'][0]['cash']==1000
    # Omission remains omission; the raw Value rule does not invent defaults.
    assert typed(dict(schema='commercial-credit-ledger-1',spec={}))['spec']=={}
    # The schema boundary is precise, not a blanket skip of arbitrary spec keys.
    assert type(typed(dict(schema='other',spec=dict(rate=0)))['spec']['rate'])is float


@pytest.mark.parametrize('normalize',[typed,graph_typed])
def test_typed_owner_money_remains_f64_and_integer_boolean_identity_stays_strict(normalize):
    raw=dict(schema='commercial-credit-ledger-1',spec=dict(positions=[dict(rate=0)]),
        allocations=[dict(allocated_face=50)],participations=[dict(source_weight=1,path_key=17)],
        source=dict(amount=-0.,asof_ordinal=738886,available=True))
    actual=normalize(raw)
    assert type(actual['allocations'][0]['allocated_face'])is float
    assert type(actual['participations'][0]['source_weight'])is float
    assert type(actual['participations'][0]['path_key'])is int
    assert type(actual['source']['asof_ordinal'])is int and actual['source']['available']is True
    assert math.copysign(1.,actual['source']['amount'])==-1.
    with pytest.raises(ArithmeticError):normalize(dict(amount=True))
    with pytest.raises(ArithmeticError):normalize(dict(rate=float('nan')))
    assert not exact(actual,{**actual,'source':dict(actual['source'],amount=0.)})


@pytest.mark.parametrize('bad',[True,float('inf'),None])
@pytest.mark.parametrize('curve_key',['collateral_curve','discount_curve'])
@pytest.mark.parametrize('normalize',[typed,graph_typed])
def test_supplied_curve_has_declared_f64_vectors_and_rejects_coercions(bad,curve_key,normalize):
    curve=dict(curve_id='c',source_id='s',source_revision='1',currency='USD',role=dict(kind='collateral',collateral_index='SOFR'),
        times_years=[0,1],discount_factors=[1,1],right_tail='reject')
    actual=normalize({curve_key:curve})[curve_key]
    assert all(type(x)is float for key in('times_years','discount_factors')for x in actual[key])
    assert curve['times_years']==[0,1]and type(curve['times_years'][0])is int
    wrong=deepcopy(curve);wrong['discount_factors'][1]=bad
    with pytest.raises(ArithmeticError):normalize({curve_key:wrong})
    assert graph_typed(dict(arbitrary=dict(discount_factors=[1,1])))==dict(arbitrary=dict(discount_factors=[1,1]))


@pytest.fixture(scope='module')
def browser_fee():
    from portfolio_risk.core.commercial_fee_native import run
    raw=browser(json.loads((ROOT/'examples/models/commercial-fee-journal-1.json').read_text()))
    result,_=run(raw,threads=2)
    return raw,result


def test_actual_js_fee_request_matches_native_raw_value_and_typed_owner_receipt(browser_fee):
    from portfolio_risk.analytics.commercial_fee_reference import verify
    raw,result=browser_fee
    assert type(raw['loan']['spec']['positions'][0]['rate'])is int
    assert type(raw['loan']['allocations'][0]['allocated_face'])is int
    assert exact(raw,result['raw_source'])
    assert type(result['loan_receipt']['original_request']['spec']['positions'][0]['rate'])is int
    assert type(result['loan_receipt']['original_request']['allocations'][0]['allocated_face'])is float
    assert verify(raw,result)['single_sim_ordered_journal_replayed']is True


@pytest.mark.parametrize('target',['raw_money','receipt_int_to_float','receipt_bool','source_money','date_type','signed_zero'])
def test_actual_js_fee_receipt_still_refuses_retyped_or_mutated_money(browser_fee,target):
    from portfolio_risk.analytics.commercial_fee_reference import verify
    raw,result=browser_fee;result=deepcopy(result)
    if target=='raw_money':result['raw_source']['loan']['spec']['accounts'][0]['cash']+=1
    elif target=='receipt_int_to_float':result['loan_receipt']['original_request']['spec']['positions'][0]['rate']=0.
    elif target=='receipt_bool':result['loan_receipt']['original_request']['spec']['positions'][0]['rate']=False
    elif target=='source_money':result['loan_receipt']['original_request']['allocations'][0]['allocated_face']+=1.
    elif target=='date_type':result['loan_receipt']['original_request']['spec']['horizon_days']=float(result['loan_receipt']['original_request']['spec']['horizon_days'])
    else:result['loan_receipt']['original_request']['spec']['positions'][0]['rate']=-0.
    with pytest.raises((ArithmeticError,ValueError)):verify(raw,result)


@pytest.mark.parametrize('target',['spec_money_bool','owner_money_bool','integer_clock_bool'])
def test_actual_native_still_refuses_boolean_money_and_clocks(browser_fee,target):
    from portfolio_risk.core.commercial_fee_native import run
    raw=deepcopy(browser_fee[0])
    if target=='spec_money_bool':raw['loan']['spec']['accounts'][0]['cash']=True
    elif target=='owner_money_bool':raw['loan']['allocations'][0]['allocated_face']=True
    else:raw['loan']['commercial'][0]['source']['current']['asof_ordinal']=True
    with pytest.raises((ValueError,RuntimeError)):run(raw,threads=2)


def test_actual_js_ordinary_commercial_stream_preserves_raw_source_and_replays(tmp_path):
    from portfolio_risk.analytics.model_ledger import run_model_ledger
    raw=browser(json.loads((ROOT/'examples/models/commercial-credit-ledger-1.json').read_text()))
    path=run_model_ledger(raw,tmp_path,partition_rows=31)
    manifest=json.loads(path.read_text());header=json.loads((path.parent/'prepared-source.json').read_text())
    assert exact(raw,header['prepared_request'])and exact(raw,header['source_receipt']['original_request'])
    assert manifest['validation']['ordered_same_sim_journal_replayed']is True
    assert manifest['validation']['persisted_partitions_replayed']is True
