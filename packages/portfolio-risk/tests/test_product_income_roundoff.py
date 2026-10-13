"""Independent positive-EIR mortgage arithmetic retains accrued roundoff rights."""
from copy import deepcopy
import math
import pytest
from portfolio_risk.analytics import product_strategy_reference as reference
from test_product_strategy_reference import product_for


def source():
    request,quote=product_for('mortgage');u=request['product']['units'][0]
    graph=u['purchase_graph']['request'];contract=graph['positions'][0]['contract']
    history=graph['positions'][0]['histories'][0]['physical'][0]
    p=reference._mortgage(contract,history,dict(currency='USD'),lambda:False)
    start=u['purchase_ordinal']
    return u,p,list(range(start,start+33)),quote


def test_daily_positive_eir_mortgage_preserves_residual_accrual_and_basis(monkeypatch):
    from portfolio_risk.core import quant_native,decision_native
    def forbidden(*a,**k):pytest.fail('independent income oracle called native finance')
    monkeypatch.setattr(quant_native,'term_call',forbidden)
    monkeypatch.setattr(decision_native.NativeDecision,'call',forbidden)
    u,p,reports,quote=source()
    native,states=reference._income(u,p,'mortgage',reports,reference._Budget(lambda:False))
    rows=native['native_income_result']['rows'];last=rows[-1]
    bound=64.*math.ulp(1.)*(abs(last['opening_gross_carrying_value'])+abs(last['cash_received'])+abs(last['effective_interest']))
    closing=states[-1]
    assert closing['principal']==closing['gross_carrying_value']==0.
    assert closing['accrued_interest']==-4.163336342344337e-17
    assert abs(closing['accrued_interest'])<=bound
    assert closing['book_adjustment']==-closing['accrued_interest']
    assert quote==100.07972856995012
    assert all(row['modification_gain_loss']==0. for row in rows)
    # Reconstructed settlement amounts retain the original full remittance tail.
    assert sum(row['cash_received'] for row in rows)==sum(c['principal']+c['interest'] for c in p['cashflows'])


@pytest.mark.parametrize('residual',[-1e-6,1e-6])
def test_true_accrued_right_cannot_enable_carrying_roundoff_clamp(residual):
    u,p,reports,_=source();p=deepcopy(p)
    p['recognition'][0]['interest']+=residual
    with pytest.raises(ArithmeticError,match='carry becomes negative'):
        reference._income(u,p,'mortgage',reports,reference._Budget(lambda:False))


def test_materially_negative_carry_remains_rejected():
    u,p,reports,_=source();u=deepcopy(u)
    # Within the preexisting quote/PV reconciliation tolerance, but many orders
    # larger than the independent terminal arithmetic roundoff bound.
    u['quote_price_per_face_unit']-=9e-9/u['unit_face']
    with pytest.raises(ArithmeticError,match='carry becomes negative'):
        reference._income(u,p,'mortgage',reports,reference._Budget(lambda:False))
