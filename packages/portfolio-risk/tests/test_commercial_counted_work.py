"""Independent counted source census and measured native operation ledgers."""
import json
import os
from pathlib import Path
import pytest
from portfolio_risk.analytics.commercial_factor_work_reference import summary,work
ROOT=Path(__file__).resolve().parents[3]

def test_fixed_census_charges_each_distinct_query_and_each_byte_scan():
    q=json.loads((ROOT/'examples/models/commercial-generated-candidate-journal-1.json').read_text())['commercial']['units'][0]['graph']['positions'][0]['generated_policy']['pricing_training']
    contract=q['source']['source']['current'];value=summary(contract,14);u=value['upper']
    assert value['use_bound']==dict(earned=14,canonical=14,pending=0,terminal=0)
    assert u['periods']==u['segments']==1 and u['observations']==0
    assert u['earned_steps']==14*4 and u['canonical_steps']==14*3
    assert u['calendar']==1+(2*14+14)*2
    assert work(u)<250000
    changed=json.loads(json.dumps(contract));changed['periods'][0]['coupon']['annual_rate']=.123456789
    assert summary(changed,14)==value

def test_observable_census_charges_binding_scan_and_terminal_tail_product():
    from test_commercial_observable_coupons import source_request
    raw=source_request();value=summary(raw['source']['current'],14,True,True);u=value['upper']
    assert u['observations']==u['segments']==3
    assert u['binding']==2+2*3+3**2
    assert value['use_bound']['canonical']==3*14+3+1
    assert value['use_bound']['terminal']==1 and u['terminal_steps']==3*3+1*3
    assert value['use_bound']['pending']==2*14+68+66*3

def test_measured_native_source_ledgers_match_independent_census():
    path=os.environ.get('PORTFOLIO_COUNTED_WORK_RECEIPT')
    if not path:pytest.skip('run native counted controls with explicit receipt path first')
    values=[json.loads(line)for line in Path(path).read_text().splitlines()if line]
    assert len(values)>=8
    seen=set()
    for item in values:
        actual=item['summary'];raw=item['raw'];uses=actual['use_bound'];observable=uses['pending']>0;terminal=uses['terminal']>0
        # Unused original observable sources have no pending calls; both policies
        # have identical zero-query construction and conservative binding may vary.
        candidates=[summary(raw,uses['earned'],observable,terminal)]
        if not uses['earned']:candidates.append(summary(raw,0,True,False))
        assert actual in candidates
        measured=item['measured'];assert all(measured[k]<=actual['upper'][k]for k in measured)
        assert work(measured)<=work(actual['upper'])
        if measured['observations']:seen.add('indexed')
        else:seen.add('fixed')
        if measured['terminal_calls']:seen.add('terminal')
        if raw['periods'][0]['day_count']=='act_act_isda':seen.add('calendar')
    assert seen=={'fixed','indexed','terminal','calendar'}


@pytest.mark.parametrize('schema,key',[
    ('commercial-credit-ledger-1','spec'),
    ('commercial-candidate-journal-1','specification'),
    ('commercial-generated-candidate-journal-1','specification'),
])
def test_counted_transport_keeps_opaque_raw_ledger_integer_identity(schema,key):
    from portfolio_risk.analytics.commercial_dto import typed,graph_typed
    raw=dict(schema=schema,amount=1,**{key:dict(amount=2,weight=1,nested=[dict(rate=3)])})
    for normalize in (typed,graph_typed):
        result=normalize(raw)
        assert type(result['amount'])is float
        assert result[key]==raw[key]and type(result[key]['amount'])is int
        assert type(result[key]['nested'][0]['rate'])is int


@pytest.mark.parametrize('name',['discount_curve','collateral_curve'])
def test_counted_transport_normalizes_declared_native_curve_vectors_only(name):
    from portfolio_risk.analytics.commercial_dto import typed
    curve=dict(curve_id='c',source_id='s',source_revision='1',currency='USD',role='collateral',times_years=[0,1],discount_factors=[1,.99],right_tail='flat_zero')
    result=typed({name:curve})[name]
    assert all(type(x)is float for k in ('times_years','discount_factors')for x in result[k])
    opaque=typed(dict(schema='commercial-generated-candidate-journal-1',specification={name:curve}))
    assert type(opaque['specification'][name]['times_years'][0])is int
    curve['times_years']=[False,1]
    with pytest.raises(ArithmeticError,match='finite native supplied curve'):
        typed({name:curve})
