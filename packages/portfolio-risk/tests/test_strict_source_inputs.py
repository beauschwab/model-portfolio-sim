"""Independent raw shape gates run before finance or persisted-row replay."""
from copy import deepcopy
import pytest
from portfolio_risk.analytics import product_strategy_reference as strategy
from portfolio_risk.analytics import credit_verification as credit

SELECTORS = [
    {'kind': 'all_assets'}, {'kind': 'all_liabilities'},
    {'kind': 'unit', 'unit_id': 'unit-1'},
    {'kind': 'template', 'template_id': 'template-1'},
    {'kind': 'accounting_category', 'accounting_category': 'AFS'},
]
ACTIONS = [
    {'kind': 'maturity'}, {'kind': 'draw', 'amount_by_state': [0., 0., 0.]},
    {'kind': 'repayment', 'principal_fraction': [0., 0., 0.],
     'accrued_fraction': [0., 0., 0.], 'cancel_undrawn_fraction': [0., 0., 0.]},
]


@pytest.mark.parametrize('selector', SELECTORS)
def test_documented_selector_shapes_remain_admitted(selector):
    strategy._admit_face_selectors({'face_limits': [{'selector': selector}]}, strategy._Budget(lambda: False))


@pytest.mark.parametrize('selector', SELECTORS)
@pytest.mark.parametrize('mutation', ['extra', 'missing_kind'])
def test_selector_shape_fails_before_even_infeasible_source_reconstruction(selector,mutation,monkeypatch):
    selector=deepcopy(selector)
    if mutation=='extra':selector['unreviewed_narrowing_field']='unit-1'
    else:selector.pop('kind')
    raw={'schema':'strategy-product-1','op':'optimize_product','face_limits':[{'selector':selector}]}
    def forbidden(*args,**kwargs):pytest.fail('invalid selector reached financial reconstruction')
    monkeypatch.setattr(strategy,'_admit_product',forbidden)
    with pytest.raises(ArithmeticError,match='selector fields'):
        strategy.verify(raw,{'feasible':False})


@pytest.mark.parametrize('action', ACTIONS)
def test_documented_loan_action_shapes_remain_admitted(action):
    credit._admit_loan_action(action)


@pytest.mark.parametrize('action', ACTIONS)
@pytest.mark.parametrize('mutation', ['extra', 'missing_kind'])
def test_all_boundary_shapes_are_checked_even_beyond_reporting_horizon(action,mutation,monkeypatch):
    action=deepcopy(action)
    if mutation=='extra':action['unreviewed_payment_amount']=0.
    else:action.pop('kind')
    raw={'runs':[{'positions':[{'boundaries':[{'recognition_ordinal':999999,'action':action}]}]}]}
    class ForbiddenRows:
        def frames(self,*args):pytest.fail('invalid action reached persisted replay')
    def forbidden(*args,**kwargs):pytest.fail('invalid action reached credit finance')
    monkeypatch.setattr(credit,'coefficients',forbidden)
    with pytest.raises(ArithmeticError,match='action fields'):
        credit.verify_credit_sources(raw,{},ForbiddenRows(),lambda:False)


def test_boundary_preflight_observes_cancellation():
    raw={'runs':[{'positions':[{'boundaries':[{'action':{'kind':'maturity'}}]}]}]}
    with pytest.raises(InterruptedError,match='cancelled'):
        credit._admit_all_loan_actions(raw,lambda:True)
