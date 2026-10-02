"""Independent accounting, timing, non-anticipation and failure-boundary gates."""
from copy import deepcopy
import pytest
import polars as pl
from portfolio_risk.analytics.balance_stress import (
    MODEL_VERSION, run_balance_stress, example_specification, validate,
)


def small():
    return dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id="a", entity="bank", currency="USD", cash=20., equity=40.)],
                positions=[dict(id="s", account="a", kind="security", balance=100., classification="afs", duration=2., collateral_pool="pool", eligible_fraction=1.),
                           dict(id="d", account="a", kind="funding", balance=80.)],
                scenarios=[dict(name="stress")])


def row(result, day, scenario="stress", account="a"):
    return result["path"].filter((pl.col("day") == day) & (pl.col("scenario") == scenario) & (pl.col("account") == account)).to_dicts()[0]


def test_zero_shock_and_no_input_mutation():
    spec = small()
    original = deepcopy(spec)
    result = run_balance_stress(spec)
    assert spec == original
    base = result["path"].filter(pl.col("scenario") == "baseline").drop("scenario")
    stress = result["path"].filter(pl.col("scenario") == "stress").drop("scenario")
    assert base.equals(stress)
    assert row(result, 30)["equity"] == 40
    assert result["breaches"].height == 0


def test_cash_accrual_settlement_and_equity_bridge():
    spec = small()
    spec["positions"][0].update(rate=.0365, payment_interval_days=30)
    result = run_balance_stress(spec)
    assert row(result, 1)["cash"] == 20
    assert row(result, 1)["assets"] == pytest.approx(120.01)
    assert row(result, 30)["cash"] == pytest.approx(20.3)
    assert row(result, 30)["equity"] == pytest.approx(40.3)


@pytest.mark.parametrize("classification,expected_oci", [("afs", -20.), ("trading", 0.), ("htm", 0.)])
def test_marks_vs_book_classification(classification, expected_oci):
    spec = small()
    spec["positions"][0]["classification"] = classification
    spec["scenarios"][0]["rate_shift"] = .1
    result = run_balance_stress(spec)
    day = row(result, 1)
    assert day["aoci"] == pytest.approx(expected_oci)
    assert day["equity"] == pytest.approx(40 if classification == "htm" else 20)
    assert day["cash"] == 20


def test_afs_sale_reclassification_does_not_double_count_mark():
    spec = small()
    spec["scenarios"][0]["rate_shift"] = .1
    spec["policies"] = [dict(id="sell", account="a", kind="sell", position="s", trigger_cash=60., limit=40.)]
    result = run_balance_stress(spec)
    assert row(result, 2)["cash"] == pytest.approx(60)
    assert row(result, 2)["equity"] == pytest.approx(20)
    assert row(result, 2)["aoci"] == pytest.approx(-10)
    sales = result["ledger"].filter((pl.col("scenario") == "stress") & (pl.col("event") == "security_sale"))
    assert sales["earnings"].sum() == pytest.approx(-10)
    assert sales["aoci"].sum() == pytest.approx(10)


@pytest.mark.parametrize("allow,limit,cash", [(False, 100., 20.), (True, 10., 28.)])
def test_htm_sale_requires_explicit_permission_and_face_limit(allow, limit, cash):
    spec = small()
    spec["positions"][0]["classification"] = "htm"
    spec["scenarios"][0]["rate_shift"] = .1
    spec["policies"] = [dict(id="sell", account="a", kind="sell", position="s", trigger_cash=60., limit=40., allow_htm_sale=allow, htm_sale_limit=limit)]
    result = run_balance_stress(spec)
    assert row(result, 30)["cash"] == pytest.approx(cash)
    assert row(result, 30)["equity"] == pytest.approx(40-(2 if allow else 0))


def test_provision_chargeoff_recovery_independent_identity():
    spec = small()
    spec["positions"][0] = dict(id="loan", account="a", kind="loan", balance=100., annual_pd=.1, lgd=.4, recovery_days=2)
    result = run_balance_stress(spec)
    default = 100*(1-.9**(1/365))
    allowance = (100-default)*.1*.4
    loss = default*.4
    assert row(result, 1)["equity"] == pytest.approx(40-loss-allowance)
    assert row(result, 2)["cash"] == 20
    assert row(result, 3)["cash"] == pytest.approx(20+default*.6)
    led = result["ledger"].filter(pl.col("scenario") == "stress")
    assert 40+led["earnings"].sum()+led["aoci"].sum() == pytest.approx(row(result, 30)["equity"])


def test_default_and_provision_remove_nonaccrual_principal():
    spec = small()
    spec["positions"][0] = dict(id="loan", account="a", kind="loan", balance=100., annual_pd=.9, lgd=1., rate=.1)
    result = run_balance_stress(spec)
    remaining = 100*.1**(1/365)
    led = result["ledger"].filter((pl.col("scenario") == "stress") & (pl.col("day") == 1) & (pl.col("event") == "interest_income"))
    assert led["earnings"].sum() == pytest.approx(remaining*.1/365)


def test_draws_capped_and_funding_rollover_on_exact_day():
    spec = small()
    spec["positions"].append(dict(id="loan", account="a", kind="loan", balance=0., commitment=30., draw_fraction=1.))
    spec["positions"][1].update(kind="funding", maturity_day=5, rollover=.5)
    spec["scenarios"][0]["draw_multiplier"] = 2.
    result = run_balance_stress(spec)
    assert row(result, 4)["cash"] == pytest.approx(12)
    assert row(result, 5)["cash"] == pytest.approx(-30)
    assert row(result, 30)["cash"] == pytest.approx(-50)
    assert row(result, 30)["equity"] == pytest.approx(40)


def test_collateral_cannot_be_repledged_or_sold_after_pledging():
    spec = small()
    spec["positions"][0].update(eligible_fraction=.5, encumbered_fraction=.2, haircut=.1)
    spec["policies"] = [dict(id="fund", account="a", kind="secured_funding", position="s", trigger_cash=200., limit=200., funding_rate=0.),
                        dict(id="sale", account="a", kind="sell", position="s", trigger_cash=200., limit=200.)]
    result = run_balance_stress(spec)
    led = result["ledger"].filter(pl.col("scenario") == "stress")
    assert led.filter(pl.col("event") == "secured_funding")["cash"].sum() == pytest.approx(27)
    assert led.filter(pl.col("event") == "security_sale")["cash"].sum() == pytest.approx(50)
    assert row(result, 30)["cash"] == pytest.approx(97)


def test_entity_shortfall_is_not_covered_without_transfer_and_donor_keeps_floor():
    spec = small()
    spec["accounts"].append(dict(id="b", entity="dealer", currency="USD", cash=5., equity=5., cash_floor=10.))
    spec["accounts"][0]["cash_floor"] = 18.
    spec["policies"] = [dict(id="support", account="a", destination="b", kind="transfer", trigger_cash=10., limit=10., delay_days=2)]
    result = run_balance_stress(spec)
    assert row(result, 1, account="b")["cash"] == 5
    assert row(result, 3, account="b")["cash"] == 7
    assert row(result, 30)["cash"] == 18
    assert row(result, 30)["equity"] == 40
    assert row(result, 30, account="b")["equity"] == 5


def test_derivative_mark_margin_lag_and_netting_isolation():
    spec = small()
    spec["netting_sets"] = [dict(id="n", account="a", counterparty="c", stress_loss=10., margin_delay=2),
                            dict(id="n2", account="a", counterparty="c", fair_value=10.)]
    spec["accounts"][0]["equity"] = 50.
    spec["scenarios"][0]["market_shock"] = 1.
    result = run_balance_stress(spec)
    assert row(result, 1)["equity"] == 40
    assert row(result, 2)["cash"] == 20
    assert row(result, 3)["cash"] == 10  # positive second set must not offset negative first
    assert row(result, 3)["equity"] == 40


def test_future_shocks_do_not_change_earlier_policy_actions_and_zero_severity():
    spec = example_specification()
    spec["horizon_days"] = 30
    spec["scenarios"] = [dict(name="late", start_day=20, deposit_flight=.8, outage_days=5)]
    spec["reverse_severities"] = [0.]
    result = run_balance_stress(spec)
    before = result["path"].filter(pl.col("day") < 20)
    assert before.filter(pl.col("scenario") == "baseline").drop("scenario").equals(before.filter(pl.col("scenario") == "late").drop("scenario"))
    base = result["summary"].filter(pl.col("scenario") == "baseline")["final_cash"].to_list()
    assert result["reverse_grid"]["final_cash"].to_list() == pytest.approx(base)


def test_full_example_reconciles_all_days_and_bridge():
    spec = example_specification()
    spec["horizon_days"] = 60
    spec["reverse_severities"] = [0., 1.]
    result = run_balance_stress(spec)
    for summary in result["summary"].to_dicts():
        initial = next(a for a in spec["accounts"] if a["id"] == summary["account"])
        led = result["ledger"].filter((pl.col("scenario") == summary["scenario"]) & (pl.col("account") == summary["account"]))
        assert initial["cash"]+led["cash"].sum() == pytest.approx(summary["final_cash"])
        assert initial["equity"]+led["earnings"].sum()+led["aoci"].sum() == pytest.approx(summary["final_equity"])
        assert summary["max_reconciliation_error"] < 1e-8
        if summary['scenario'] != 'baseline':
            baseline = result['summary'].filter((pl.col('scenario') == 'baseline') & (pl.col('account') == summary['account'])).to_dicts()[0]
            attribution = result['attribution'].filter((pl.col('scenario') == summary['scenario']) & (pl.col('account') == summary['account']))
            assert attribution['cash_delta'].sum() == pytest.approx(summary['final_cash']-baseline['final_cash'])
            assert attribution['earnings_delta'].sum()+attribution['aoci_delta'].sum() == pytest.approx(summary['final_equity']-baseline['final_equity'])


def test_cross_currency_transfer_rejected_and_work_budget_bounded():
    spec = small()
    spec['accounts'].append(dict(id='euro', entity='bank', currency='EUR', cash=0., equity=0.))
    spec['policies'] = [dict(id='fx', account='a', destination='euro', kind='transfer', trigger_cash=10., limit=10.)]
    with pytest.raises(ValueError, match='no implicit FX'):
        validate(spec)
    spec = small()
    spec['horizon_days'] = 1080
    spec['positions'] = [dict(id=f'loan{i}', account='a', kind='loan', balance=0.) for i in range(200)]
    spec['scenarios'] = [dict(name=f's{i}') for i in range(8)]
    spec['reverse_severities'] = [0., 1., 2.]
    with pytest.raises(ValueError, match='work budget'):
        validate(spec)


def test_maturity_reverses_afs_mark_and_settles_accrued_interest():
    spec = small()
    spec['positions'][0].update(maturity_day=10, rate=.0365, payment_interval_days=30)
    spec['scenarios'][0]['rate_shift'] = .1
    result = run_balance_stress(spec)
    assert row(result, 9)['equity'] == pytest.approx(20.09)
    assert row(result, 10)['equity'] == pytest.approx(40.1)
    assert row(result, 10)['aoci'] == pytest.approx(0)
    assert row(result, 10)['cash'] == pytest.approx(120.1)


def test_aoci_capital_filter_and_outage_delayed_execution():
    spec = small()
    spec['accounts'][0]['include_aoci'] = False
    spec['policies'] = [dict(id='sale', account='a', kind='sell', position='s', trigger_cash=60., limit=40.)]
    spec['scenarios'][0].update(rate_shift=.1, outage_days=5)
    result = run_balance_stress(spec)
    assert row(result, 1)['equity'] == pytest.approx(20)
    assert row(result, 1)['cet1'] == pytest.approx(40)
    assert row(result, 5)['cash'] == 20
    assert row(result, 6)['cash'] == 60
    assert row(result, 6)['cet1'] == pytest.approx(30)


@pytest.mark.parametrize("mutate", [
    lambda x: x["accounts"][0].update(equity=999.),
    lambda x: x["positions"][0].update(balance=float("nan")),
    lambda x: x["positions"][0].update(ignored_field=1),
    lambda x: x["positions"][0].update(account="missing"),
    lambda x: x.update(reverse_severities=[1., .5]),
    lambda x: x.update(horizon_days=True),
    lambda x: x["accounts"].append(dict(id="b", entity="bank", currency="USD", cash=0., equity=0.)),
    lambda x: x["scenarios"][0].update(name="baseline"),
])
def test_invalid_inputs_fail_closed(mutate):
    spec = small()
    mutate(spec)
    with pytest.raises(ValueError):
        validate(spec)
