"""Independent cost-basis/FV/OCI accounting checks; no pricer implementation mirror."""
from copy import deepcopy
import polars as pl
import pytest
from portfolio_risk.analytics.balance_stress import run_balance_stress, validate
from portfolio_risk.analytics.balance_workflow import from_accounting
from test_balance_stress import small, row


def premium(classification='afs'):
    spec = small()
    spec['accounts'][0].update(equity=45., include_aoci=False)
    spec['positions'][0].update(classification=classification, book_adjustment=10., opening_market_price=1.05)
    return spec


def test_opening_cost_and_fair_value_classified_without_new_income():
    spec = premium()
    result = run_balance_stress(spec)
    opening, final = row(result, 0), row(result, 30)
    assert opening['assets'] == 125. and opening['equity'] == 45.
    assert opening['aoci'] == pytest.approx(-5.)
    assert opening['cet1'] == pytest.approx(50.)
    assert final['equity'] == opening['equity']
    assert result['ledger'].is_empty()
    assert result['closing_statements']['equity'].to_list() == [45., 45.]


@pytest.mark.parametrize('classification,expected_income,expected_oci', [('afs', 4., -4.), ('trading', 5., 0.)])
def test_coupon_and_basis_amortization_reconcile_to_unchanged_clean_fv(classification, expected_income, expected_oci):
    spec = premium(classification)
    spec['cashflows'] = [dict(position='s', day=30, cash_interest=5., accrual_interest=5., book_amortization=-1.)]
    result = run_balance_stress(spec)
    final = row(result, 30)
    assert final['cash'] == 25.
    assert final['assets'] == 130. and final['equity'] == 50.
    assert final['aoci'] == pytest.approx(expected_oci)
    flow = result['ledger'].filter((pl.col('scenario')=='stress') & (pl.col('event')=='contractual_cashflow'))
    assert flow['earnings'].sum() == pytest.approx(expected_income)


def test_partial_sale_reclassifies_only_sold_cost_basis():
    spec = premium()
    spec['policies'] = [dict(id='sale', account='a', kind='sell', position='s', trigger_cash=72.5, limit=52.5)]
    result = run_balance_stress(spec)
    final = row(result, 2)
    assert final['cash'] == 72.5 and final['equity'] == 45.
    assert final['aoci'] == pytest.approx(-2.5)
    sale = result['ledger'].filter((pl.col('scenario')=='stress') & (pl.col('event')=='security_sale'))
    assert sale['earnings'].sum() == pytest.approx(-2.5)
    assert sale['aoci'].sum() == pytest.approx(2.5)


@pytest.mark.parametrize('classification', ['afs', 'trading'])
@pytest.mark.parametrize('scheduled', [False, True])
def test_redemption_removes_cost_and_fair_value_without_double_loss(classification, scheduled):
    spec = premium(classification)
    if scheduled:
        spec['cashflows'] = [dict(position='s', day=3, principal=100., book_amortization=-10.)]
    else:
        spec['positions'][0]['maturity_day'] = 3
    result = run_balance_stress(spec)
    final = row(result, 3)
    assert final['cash'] == pytest.approx(120.)
    assert final['assets'] == pytest.approx(120.)
    assert final['equity'] == pytest.approx(40.)
    assert final['aoci'] == pytest.approx(0.)
    balances = result['trial_balance'].filter(pl.col('instrument_id')=='s')
    remaining = balances.filter(pl.col('gl_account').is_in(['asset_principal','book_adjustment','fair_value_adjustment']))
    assert max(abs(v) for v in remaining['balance']) < 1e-9


def test_stress_is_relative_to_opening_quote_and_sale_does_not_rebook_loss():
    spec = premium()
    spec['scenarios'][0]['rate_shift'] = .1
    spec['policies'] = [dict(id='sale', account='a', kind='sell', position='s', trigger_cash=104., limit=84.)]
    result = run_balance_stress(spec)
    assert row(result, 1)['equity'] == pytest.approx(24.)
    assert row(result, 2)['equity'] == pytest.approx(24.)
    assert row(result, 2)['aoci'] == pytest.approx(0.)


def test_forward_premium_purchase_posts_cash_at_cost_and_marks_to_fv():
    spec = premium()
    spec['positions'][0]['start_day'] = 2
    spec['accounts'][0].update(cash=130., equity=50.)
    result = run_balance_stress(spec)
    assert row(result, 0)['equity'] == 50. and row(result, 0)['aoci'] == 0.
    assert row(result, 2)['cash'] == 20.
    assert row(result, 2)['equity'] == pytest.approx(45.)
    assert row(result, 2)['aoci'] == pytest.approx(-5.)


def test_saved_security_uses_engine_quote_and_cost_as_separate_inputs():
    spec = small()
    spec['positions'] = []
    spec['accounts'][0]['equity'] = 125.
    accounting = dict(instrument_openings=pl.DataFrame([dict(book='mbs', id='x', balance=100., book_adjustment=10., market_price=1.05, side='asset')]),
        instrument_cashflows=pl.DataFrame([dict(book='mbs', id='x', month=1, principal=0., cash_interest=5., accrual_interest=5., book_amortization=-1.)]))
    mapping = {'mbs:x': dict(account='a', kind='security', classification='afs', risk_weight=1.,
        asf_weight=0., rsf_weight=1., lcr_outflow_weight=0., hqla_weight=0.)}
    original = deepcopy(accounting)
    result = run_balance_stress(from_accounting(spec, accounting, mapping, 1.))
    assert row(result, 0)['aoci'] == pytest.approx(-5.)
    assert row(result, 30)['equity'] == pytest.approx(130.)
    assert accounting['instrument_openings'].equals(original['instrument_openings'])
    mapping['mbs:x']['opening_market_price'] = 2.
    with pytest.raises(ValueError, match='cannot override'):
        from_accounting(spec, accounting, mapping, 1.)


@pytest.mark.parametrize('value', [0., -1., float('nan'), 101.])
def test_invalid_opening_quotes_rejected(value):
    spec = premium()
    spec['positions'][0]['opening_market_price'] = value
    with pytest.raises(ValueError):
        validate(spec)


@pytest.mark.parametrize('classification', ['afs', 'trading'])
def test_discount_accretion_and_oci_offset(classification):
    spec = premium(classification)
    spec['accounts'][0]['equity'] = 35.
    spec['positions'][0].update(book_adjustment=-10., opening_market_price=.95)
    spec['cashflows'] = [dict(position='s', day=30, cash_interest=5., accrual_interest=5., book_amortization=1.)]
    result = run_balance_stress(spec)
    assert row(result, 30)['equity'] == pytest.approx(40.)
    assert row(result, 30)['aoci'] == pytest.approx(4. if classification == 'afs' else 0.)


def test_cached_type_metadata_preserves_strict_validation_and_fresh_objects():
    from portfolio_risk.analytics.balance_stress import Position, _decode, _field_types
    _field_types.cache_clear()
    raw = dict(id='x', account='a', kind='loan', balance=1.)
    first = _decode(Position, raw)
    second = _decode(Position, raw)
    assert first == second and first is not second
    assert _field_types.cache_info().misses == 1
    with pytest.raises(ValueError):
        _decode(Position, raw | {'balance': True})
    assert raw['balance'] == 1.


def test_zero_account_is_retained_in_journal_derived_statements():
    spec = small()
    spec['positions'] = []
    spec['accounts'][0].update(cash=0., equity=0.)
    result = run_balance_stress(spec)
    assert result['journal'].is_empty()
    assert result['closing_statements'].height == 2
    assert result['closing_statements']['assets'].to_list() == [0., 0.]
