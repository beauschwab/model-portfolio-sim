from copy import deepcopy
import numpy as np
import polars as pl
import pytest
from portfolio_risk.analytics.balance_stress import run_balance_stress, validate
from portfolio_risk.analytics.balance_workflow import add_candidate, from_accounting, backtest, check_mapping
from portfolio_risk.analytics.journal import replay, Journal
from test_balance_stress import small, row


def test_tax_includes_realized_policy_gain():
    spec = small()
    spec['accounts'][0]['tax_rate'] = .2
    spec['scenarios'][0]['rate_shift'] = -.1
    spec['policies'] = [dict(id='sell', account='a', kind='sell', position='s', trigger_cash=80., limit=60.)]
    result = run_balance_stress(spec)
    assert row(result, 30)['cash'] == pytest.approx(78.)
    assert row(result, 30)['equity'] == pytest.approx(58.)


def funded(maturity=3):
    spec = small()
    spec['positions'][0].update(maturity_day=maturity, haircut=.1)
    spec['policies'] = [dict(id='borrow', account='a', kind='secured_funding', position='s',
                             trigger_cash=110., limit=90., funding_rate=0.)]
    return spec


def test_collateral_maturity_repays_claim_instead_of_creating_free_cash():
    result = run_balance_stress(funded())
    assert row(result, 2)['cash'] == pytest.approx(110.)
    assert row(result, 3)['cash'] == pytest.approx(120.)
    assert row(result, 3)['liabilities'] == pytest.approx(80.)
    assert result['funding_claims']['balance'].sum() == 0.


def test_opening_unknown_pledge_is_restricted_and_known_pledge_repaid():
    spec = small()
    spec['positions'][0].update(maturity_day=3, encumbered_fraction=.8)
    result = run_balance_stress(spec)
    assert row(result, 3)['cash'] == pytest.approx(40.)
    assert row(result, 3)['restricted_cash'] == pytest.approx(80.)
    spec['positions'][1].update(collateral_position='s', pledged_face=80.)
    result = run_balance_stress(spec)
    assert row(result, 3)['cash'] == pytest.approx(40.)
    assert row(result, 3)['restricted_cash'] == 0.
    assert row(result, 3)['liabilities'] == 0.


def test_funding_matures_and_releases_collateral():
    spec = funded(0)
    spec['policies'][0]['funding_tenor_days'] = 2
    result = run_balance_stress(spec)
    assert row(result, 4)['cash'] == pytest.approx(20.)
    assert row(result, 4)['usable_collateral'] == pytest.approx(90.)


def test_linked_maturity_interest_independent_of_position_order():
    spec = small()
    spec['positions'][0].update(maturity_day=3, encumbered_fraction=.8)
    spec['positions'][1].update(collateral_position='s', pledged_face=80., rate=.0365)
    a = run_balance_stress(spec)
    spec['positions'].reverse()
    b = run_balance_stress(spec)
    assert row(a, 3)['cash'] == pytest.approx(40.-.024)
    for key in ('cash', 'assets', 'liabilities', 'equity'):
        assert row(a, 3)[key] == pytest.approx(row(b, 3)[key])


def test_independent_persisted_journal_replay_and_tamper_detection():
    result = run_balance_stress(funded())
    entries = result['journal'].to_dicts()
    balances = replay(entries)
    for r in result['trial_balance'].to_dicts():
        assert balances[r['scenario'], r['account'], r['gl_account'], r['instrument_id']] == pytest.approx(r['balance'])
    entries[0]['debit'] += 1
    with pytest.raises(ArithmeticError, match='unbalanced'):
        replay(entries)
    journal = Journal('a')
    with pytest.raises(ArithmeticError):
        journal.post(0, 'a', 'bad', {('cash', ''): 1.})
    assert not journal.rows and not journal.balances


@pytest.mark.parametrize('metric', ['lcr', 'nsfr', 'htm'])
def test_all_rule_breaches_invalidate_replay_and_reverse_grid(metric):
    spec = small()
    spec['reverse_severities'] = [0.]
    if metric == 'lcr':
        spec['accounts'][0]['lcr_floor'] = 1.
        spec['positions'][1]['lcr_outflow_weight'] = 1.
    elif metric == 'nsfr':
        spec['accounts'][0]['nsfr_floor'] = 1.
    else:
        spec['positions'][0]['classification'] = 'htm'
        spec['accounts'][0]['htm_asset_limit'] = .5
    result = run_balance_stress(spec)
    assert metric in result['breaches']['metric'].to_list()
    assert result['reverse_grid']['breached'].all()
    assert not result['validation']['dynamic_validated']


def test_scheduled_principal_releases_mark_and_pays_linked_funding():
    spec = funded(0)
    spec['scenarios'][0]['rate_shift'] = .1
    spec['cashflows'] = [dict(position='s', day=3, principal=100.)]
    result = run_balance_stress(spec)
    assert row(result, 3)['cash'] == pytest.approx(120.)
    assert row(result, 3)['equity'] == pytest.approx(40.)
    assert row(result, 3)['aoci'] == pytest.approx(0.)


def test_scheduled_credit_survival_and_sale_do_not_overpay_principal():
    spec = small()
    spec['positions'][0] = dict(id='s', account='a', kind='loan', balance=100., annual_pd=.1)
    spec['cashflows'] = [dict(position='s', day=30, principal=100., accrual_interest=1., cash_interest=1.)]
    result = run_balance_stress(spec)
    assert row(result, 30)['cash'] < 121.
    assert abs(row(result, 30)['reconciliation_error']) < 1e-8
    spec = small()
    spec['cashflows'] = [dict(position='s', day=30, principal=100.)]
    spec['policies'] = [dict(id='sell', account='a', kind='sell', position='s', trigger_cash=70., limit=50.)]
    assert row(run_balance_stress(spec), 30)['cash'] == pytest.approx(120.)


def test_schedule_overlap_and_missing_scenario_rejected():
    spec = small()
    spec['cashflows'] = [dict(position='s', day=30, principal=100., scenario='stress')]
    with pytest.raises(ValueError, match='baseline'):
        validate(spec)
    spec['cashflows'].append(dict(position='s', day=30, principal=100.))
    with pytest.raises(ValueError, match='overlap'):
        validate(spec)


def meta(kind='security'):
    return dict(account='a', kind=kind, classification='ac', risk_weight=1.,
                asf_weight=0., rsf_weight=1., lcr_outflow_weight=0., hqla_weight=0.)


def test_saved_book_adapter_reconciles_premium_and_cash_accrual_timing():
    spec = small()
    spec['positions'] = []
    spec['accounts'][0]['equity'] = 121.
    accounting = dict(instrument_openings=pl.DataFrame([dict(book='loans', id='x', balance=100., book_adjustment=1., side='asset')]),
        instrument_cashflows=pl.DataFrame([dict(book='loans', id='x', month=1, principal=0., cash_interest=0., accrual_interest=1., book_amortization=-.1)]))
    mapped = from_accounting(spec, accounting, {'loans:x': meta('loan')}, 1.)
    result = run_balance_stress(mapped)
    assert row(result, 30)['cash'] == 20.
    assert row(result, 30)['equity'] == pytest.approx(121.9)
    with pytest.raises(ValueError, match='mapping'):
        check_mapping({'loans': pl.DataFrame({'id': ['x']})}, {})


def test_forward_candidate_origination_and_cash_timing():
    spec = small()
    library = dict(units=[dict(template='loan', h=0, side=1.)], horizon=1,
                   runoff=np.array([[.5]]), cash_interest=np.array([[.02]]), nii=np.array([[.02]]))
    original = deepcopy(spec)
    candidate = add_candidate(spec, [dict(template='loan', purchase_m=0, notional=10.)], library, {'loan': meta('loan')})
    result = run_balance_stress(candidate)
    assert spec == original
    assert row(result, 0)['cash'] == 20.
    assert row(result, 1)['cash'] == 10.
    assert row(result, 30)['cash'] == pytest.approx(15.2)
    assert row(result, 30)['equity'] == pytest.approx(40.2)


def test_heldout_backtest_requires_provenance_and_reports_error():
    result = run_balance_stress(small())
    obs = dict(scenario='baseline', account='a', day=30, metric='cash', actual=19., source='independent-test-ledger')
    assert backtest(result['path'], [obs])['error'][0] == 1.
    with pytest.raises(ValueError):
        backtest(result['path'], [obs, obs])


def test_real_six_book_cashflows_match_nii_and_reconcile():
    from portfolio_risk.analytics.accounting import run_balance_sheet_nii
    from portfolio_risk.demo import demo_market, model_balance_sheet, demo_deposit_history
    from portfolio_risk.core.runtime import run_context, RunConfig
    bs = model_balance_sheet(scale=.001, basis='amortized_cost', include_markets_bs=True)
    sr, vp = demo_market()
    with run_context(RunConfig(n_paths=32, n_paths_base=32, compute_backend='python')):
        accounting = run_balance_sheet_nii(bs, sr, vp, demo_deposit_history(), horizon=3, capture_cashflows=True)
    openings = accounting['instrument_openings'].to_dicts()
    assert {r['book'] for r in openings} == {'mbs', 'loans', 'debt', 'deposits', 'cds', 'mm'}
    scale = 1e-6
    equity = sum((r['balance']+r['book_adjustment'])*(1 if r['side']=='asset' else -1) for r in openings)*scale
    spec = small()
    spec.update(horizon_days=90, positions=[])
    spec['accounts'][0].update(cash=0., equity=equity)
    mapping = {f"{r['book']}:{r['id']}": meta('loan' if r['side']=='asset' else 'funding') for r in openings}
    result = run_balance_stress(from_accounting(spec, accounting, mapping, scale))
    assert row(result, 90)['equity']-equity == pytest.approx(accounting['monthly']['nii'].sum()*scale, abs=1e-8)
    assert result['summary']['max_reconciliation_error'].max() < 1e-8
    from portfolio_risk.analytics.balance_workflow import run_saved_book_stress
    request = dict(specification=spec, position_mapping=mapping, amount_scale=scale,
        allocation=[dict(template='cml_fixed_5y', purchase_m=0, notional=1e6)],
        template_mapping={'cml_fixed_5y': meta('loan')})
    with run_context(RunConfig(n_paths=32, n_paths_base=32, compute_backend='python')):
        saved = run_saved_book_stress(bs, sr, vp, demo_deposit_history(), request)
    assert saved['source']['allocation'] == request['allocation']
    assert len(saved['source']['specification_sha256']) == 64
    assert saved['journal'].filter(pl.col('event') == 'origination').height > 0
    assert saved['summary']['max_reconciliation_error'].max() < 1e-8
    assert not saved['validation']['dynamic_validated']  # zero cash cannot fund new assets


def test_calibration_holdout_cannot_leak_into_training():
    import datetime as dt
    from portfolio_risk.analytics.balance_calibration import fit_joint_drivers, empirical_joint_scenarios, DRIVERS
    rng = np.random.default_rng(7)
    dates = [dt.date(2025, 1, 1)+dt.timedelta(days=i) for i in range(45)]
    data = {'date': dates, **{k: np.abs(rng.normal(0., .01, 45)) for k in DRIVERS}}
    history = pl.DataFrame(data)
    changed = history.with_columns(pl.when(pl.col('date') > dates[29]).then(.2).otherwise(pl.col('rate_shift')).alias('rate_shift'))
    a, b = [fit_joint_drivers(frame, dates[29], 'synthetic test') for frame in (history, changed)]
    assert a['mean'] == b['mean'] and a['covariance'] == b['covariance']
    assert a['holdout_rmse'] != b['holdout_rmse']
    assert empirical_joint_scenarios(history, dates[29]) == empirical_joint_scenarios(changed, dates[29])
    assert not a['production_validated']
