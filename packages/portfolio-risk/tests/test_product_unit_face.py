"""Actual-face admission is independent of reciprocal quote normalization."""
from copy import deepcopy
import json
from pathlib import Path
import pytest
from portfolio_risk.analytics import product_strategy_reference as reference

ROOT = Path(__file__).resolve().parents[3]


def raw():
    return json.loads((ROOT/'examples/models/product-units-1.json').read_text())


def no_projection(monkeypatch):
    monkeypatch.setattr(reference, '_graph', lambda *a, **k: pytest.fail('admission called a source projector'))


@pytest.mark.parametrize('kind', ['consumer_installment', 'card', 'mortgage', 'deposit'])
@pytest.mark.parametrize('scale', [.5, 2.])
def test_reciprocal_face_quote_edit_is_rejected_before_independent_projection(monkeypatch,kind,scale):
    no_projection(monkeypatch)
    r=raw();u=r['units'][0]
    names=dict(consumer_installment='advanced',card='card',mortgage='mortgage')
    g=(json.loads((ROOT/f'examples/models/{names[kind]}-product-graph-1.json').read_text())
       if kind!='deposit' else u['purchase_graph']['request'])
    if kind=='consumer_installment':principal=g['positions'][0]['product']['contract']['opening_principal']
    elif kind=='deposit':principal=sum(s['opening_balance'] for s in g['positions'][0]['histories'][0]['physical'][0]['request']['segments'])
    else:principal=sum(g['positions'][0]['contract']['opening_principal' if kind=='card' else 'opening_investor_principal'])
    u['purchase_graph']=dict(kind=kind,request=g);u['unit_face']=principal*scale;u['quote_price_per_face_unit']=1/scale
    with pytest.raises(ArithmeticError,match='unit_face must equal'):
        reference.verify_preparation(r,{})


@pytest.mark.parametrize('role',reference.ROLES)
@pytest.mark.parametrize('branch',[False,True])
@pytest.mark.parametrize('later',[False,True])
def test_each_deposit_scenario_role_path_and_age_branch_has_raw_face_guard(monkeypatch,role,branch,later):
    no_projection(monkeypatch);r=raw();u=r['units'][0]
    g=(u['age_branches'][1]['graph'] if branch else u['purchase_graph'])['request']
    histories=g['positions'][0]['histories']
    if later:
        histories.append(deepcopy(histories[0]));paths=histories[1][role]
        paths.append(deepcopy(paths[0]));selected=paths[1]
    else:selected=histories[0][role][0]
    selected['request']['segments'][0]['opening_balance']+=1.
    with pytest.raises(ArithmeticError,match='unit_face must equal'):
        reference.verify_preparation(r,{})


def test_existing_monetary_tolerance_covers_sum_roundoff_and_rejects_larger_difference():
    g=raw()['units'][0]['purchase_graph']['request'];principal=1000.
    budget=reference._Budget(lambda:False)
    reference._admit_face(g,'deposit',principal+1e-10,budget)
    with pytest.raises(ArithmeticError,match='unit_face must equal'):
        reference._admit_face(g,'deposit',principal+1e-7,budget)
    # Decimal components can have a different binary sum from the quoted total.
    card=dict(positions=[dict(contract=dict(opening_principal=[.1,.2]))])
    reference._admit_face(card,'card',.3,budget)


@pytest.mark.parametrize('amount',[-1.,1e15+1.,True])
def test_opening_component_domains_are_not_hidden_by_a_matching_face(amount):
    g=raw()['units'][0]['purchase_graph']['request']
    g['positions'][0]['histories'][0]['physical'][0]['request']['segments'][0]['opening_balance']=amount
    with pytest.raises(ArithmeticError,match='bounded nonnegative'):
        reference._admit_face(g,'deposit',amount,reference._Budget(lambda:False))
