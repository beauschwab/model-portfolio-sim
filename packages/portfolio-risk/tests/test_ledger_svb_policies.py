"""Liquidity-rule applicability (#33), trapped cash and transfer restrictions (#32),
HTM sale accounting (#31) and deferred tax, AOCI and CET1 (#30) in the daily ledger.

Every case runs through the Rust state engine and the independent Python
reference and must agree on all published tables. Expected values are computed
here from the stated inputs, not read back from either engine.
"""
from copy import deepcopy

import polars as pl
from polars.testing import assert_frame_equal
import pytest

from portfolio_risk.analytics.balance_stress import MODEL_VERSION, validate
from portfolio_risk.analytics.balance_stream import (SCHEMAS, binary_path, load_streamed_result,
                                                     run_streamed_balance_stress)


@pytest.fixture(scope='module', autouse=True)
def native_available():
    if not binary_path().is_file():
        pytest.skip('native state engine is not built')


def run(spec, tmp_path):
    """Run both engines; assert every table agrees; return the Rust result."""
    results = {}
    for backend in ('python', 'rust'):
        out = tmp_path/backend
        out.mkdir(parents=True)
        results[backend] = load_streamed_result(run_streamed_balance_stress(deepcopy(spec), out, backend=backend))
    for table in SCHEMAS:
        reference, actual = results['python'][table], results['rust'][table]
        assert actual.height == reference.height, table
        if reference.height:
            assert_frame_equal(actual.select(reference.columns), reference, check_dtypes=False,
                               rel_tol=1e-12, abs_tol=1e-8)
    return results['rust']


def path(result, day, account='a', scenario='stress'):
    rows = result['path'].filter((pl.col('day') == day) & (pl.col('account') == account)
                                 & (pl.col('scenario') == scenario)).to_dicts()
    return rows[0]


def summary(result, account='a', scenario='stress'):
    return result['summary'].filter((pl.col('account') == account) & (pl.col('scenario') == scenario)).to_dicts()[0]


# ---- #33 liquidity applicability ---------------------------------------------

def liquidity_book(**account):
    return dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id='a', entity='bank', currency='USD', cash=60., equity=160., **account)],
                positions=[dict(id='l2', account='a', kind='security', balance=100., classification='afs',
                                hqla_level='level2a', hqla_weight=1.),
                           dict(id='l1', account='a', kind='security', balance=100., classification='afs',
                                hqla_weight=1.),
                           dict(id='ln', account='a', kind='loan', balance=100., lcr_inflow_weight=.9),
                           dict(id='dep', account='a', kind='deposit', balance=200., lcr_outflow_weight=.5,
                                asf_weight=.9, rsf_weight=0.)],
                scenarios=[dict(name='stress')])


def test_liquidity_proxies_follow_hqla_and_inflow_caps(tmp_path):
    result = run(liquidity_book(lcr_floor=1., nsfr_floor=1.), tmp_path)
    level1 = 60. + 100.                                  # cash and the level 1 security
    level2 = min(100., level1*.4/.6)                     # level 2A capped at 40% of HQLA
    outflow = 200.*.5
    net = outflow - min(100.*.9, .75*outflow)            # inflows capped at 75% of outflows
    asf = 160. + 200.*.9                                 # equity plus stable deposits
    rsf = 100. + 100. + 100.                             # every asset at its 100% default
    row = path(result, 0)
    assert row['hqla_proxy'] == pytest.approx(level1 + level2)
    assert row['lcr_proxy'] == pytest.approx((level1 + level2)/net)
    assert row['nsfr_proxy'] == pytest.approx(asf/rsf)
    assert row['lcr_status'] == 'within_limit' and row['nsfr_status'] == 'within_limit'
    assert row['lcr_headroom'] == pytest.approx(level1 + level2 - net)
    s = summary(result)
    assert (s['lcr_applicability'], s['lcr_status']) == ('internal_limit', 'within_limit')


def test_unconfigured_and_not_applicable_rules_never_report_a_pass(tmp_path):
    result = run(liquidity_book(nsfr_applicability='not_applicable'), tmp_path)
    row = path(result, 0)
    assert row['lcr_status'] == 'unconfigured' and row['nsfr_status'] == 'not_applicable'
    assert row['lcr_headroom'] is None and row['nsfr_headroom'] is None
    assert row['lcr_proxy'] is not None                  # the research proxy is still reported
    s = summary(result)
    assert (s['lcr_applicability'], s['lcr_status']) == ('unconfigured', 'unconfigured')
    assert (s['nsfr_applicability'], s['nsfr_status']) == ('not_applicable', 'not_applicable')
    assert result['breaches'].is_empty()


def test_internal_limit_breach_and_missing_inputs_are_explicit(tmp_path):
    spec = liquidity_book(lcr_floor=10.)
    spec['positions'][3]['lcr_outflow_weight'] = 1.        # 267 HQLA < 10 x 110 net outflow
    breach = run(spec, tmp_path/'breach')
    assert path(breach, 0)['lcr_status'] == 'breach'
    assert summary(breach)['lcr_status'] == 'breach'
    assert 'lcr' in set(breach['breaches']['metric'])
    spec = liquidity_book(lcr_floor=1.)
    spec['positions'][3]['lcr_outflow_weight'] = 0.        # no outflows: the ratio is undefined
    empty = run(spec, tmp_path/'empty')
    assert path(empty, 0)['lcr_status'] == 'unavailable'
    assert summary(empty)['lcr_status'] == 'unavailable'


@pytest.mark.parametrize('account, message', [
    (dict(lcr_applicability='regulatory', lcr_floor=1.), 'source-backed rule set'),
    (dict(lcr_applicability='not_applicable', lcr_floor=1.), 'internal_limit applicability'),
    (dict(nsfr_applicability='internal_limit'), 'positive floor'),
    (dict(lcr_applicability='certified'), 'unknown LCR applicability'),
])
def test_liquidity_applicability_is_validated(account, message):
    with pytest.raises(ValueError, match=message):
        validate(liquidity_book(**account))


# ---- #32 trapped cash and transfer restrictions -------------------------------

def group(**transfer):
    """A solvent group whose cash sits at the holding company; the bank repays
    maturing funding on day 5 and runs short."""
    spec = dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id='hold', entity='holdco', currency='USD', cash=300., equity=300.),
                          dict(id='bank', entity='bank', currency='USD', cash=30., equity=50., cash_floor=20.)],
                positions=[dict(id='ln', account='bank', kind='loan', balance=220.),
                           dict(id='wf', account='bank', kind='funding', balance=50., maturity_day=5),
                           dict(id='dep', account='bank', kind='deposit', balance=150.)],
                scenarios=[dict(name='stress')])
    if transfer:
        spec['policies'] = [dict(id='support', account='hold', destination='bank', kind='transfer',
                                 trigger_cash=25., limit=100., **transfer)]
    return spec


def test_solvent_group_still_fails_at_the_bank_when_cash_is_trapped(tmp_path):
    result = run(group(), tmp_path)
    bank, hold = summary(result, 'bank'), summary(result, 'hold')
    assert bank['first_cash_breach_day'] == 5
    assert hold['first_cash_breach_day'] is None
    day5 = path(result, 5, 'bank')['cash'] + path(result, 5, 'hold')['cash']
    assert day5 == pytest.approx(280.)                   # consolidated cash stays positive
    assert path(result, 5, 'bank')['cash'] == pytest.approx(-20.)


def test_transfer_waits_for_approval_and_posts_eliminating_journals(tmp_path):
    prompt = run(group(delay_days=1), tmp_path/'prompt')
    approved = run(group(delay_days=1, approval_day=12), tmp_path/'approved')
    executed = lambda r: r['actions'].filter((pl.col('status') == 'executed') & (pl.col('scenario') == 'stress'))
    assert executed(prompt)['day'].to_list() == [6]
    assert executed(approved)['day'].to_list() == [12]
    delayed = approved['actions'].filter(pl.col('status') == 'delayed')
    assert set(delayed['reason']) == {'awaiting transfer approval'}
    assert summary(approved, 'bank')['first_cash_breach_day'] == 5
    assert path(approved, 12, 'bank')['cash'] == pytest.approx(25.)
    # receivable and payable eliminate across the group
    trial = approved['trial_balance'].filter(pl.col('scenario') == 'stress')
    intercompany = trial.filter(pl.col('gl_account').is_in(['intercompany_receivable', 'intercompany_payable']))
    assert intercompany.height == 2
    assert intercompany['balance'].sum() == pytest.approx(0., abs=1e-9)


def test_blocked_transfer_posts_nothing(tmp_path):
    spec = group(delay_days=1)
    spec['accounts'][0]['cash_floor'] = 300.             # the donor keeps every dollar
    result = run(spec, tmp_path)
    actions = result['actions'].filter(pl.col('scenario') == 'stress')
    assert 'executed' not in set(actions['status'])
    assert 'blocked' in set(actions['status'])
    journal = result['journal']
    assert journal.filter(pl.col('event').str.starts_with('intercompany')).is_empty()


def test_approval_day_applies_only_to_transfers():
    spec = liquidity_book()
    spec['policies'] = [dict(id='sale', account='a', kind='sell', position='l1', trigger_cash=1., limit=1.,
                             approval_day=3)]
    with pytest.raises(ValueError, match='approval_day applies to transfers'):
        validate(spec)


# ---- #31 HTM sale accounting ---------------------------------------------------

def htm_book(reason='liquidity', materiality=0., policy='asc320-htm-v1', maturity=0, horizon=30):
    """Two HTM securities; a rate rise marks them at 0.9 and a cash need forces a sale."""
    account = dict(id='a', entity='bank', currency='USD', cash=10., equity=60.)
    if policy:
        account.update(htm_accounting=policy, htm_taint_materiality=materiality)
    sale = dict(id='sale', account='a', kind='sell', position='h1', trigger_cash=50., limit=30.,
                allow_htm_sale=True, htm_sale_limit=100.)
    if reason:
        sale['sale_reason'] = reason
    return dict(version=MODEL_VERSION, horizon_days=horizon, accounts=[account],
                positions=[dict(id='h1', account='a', kind='security', balance=100., classification='htm',
                                duration=5., maturity_day=maturity),
                           dict(id='h2', account='a', kind='security', balance=100., classification='htm',
                                duration=5.),
                           dict(id='wf', account='a', kind='funding', balance=150.)],
                policies=[sale], scenarios=[dict(name='stress', rate_shift=.02)])


def events(result, scenario='stress'):
    return result['accounting_events'].filter(pl.col('scenario') == scenario).sort('position')


def test_non_exception_sale_taints_and_reclassifies_the_portfolio_to_afs(tmp_path):
    result = run(htm_book(), tmp_path)
    face = 30./.9                                        # proceeds of 30 at a 0.9 price
    rows = events(result).to_dicts()
    sale = [r for r in rows if r['qualification'] == 'non_exception']
    moved = {r['position']: r['aoci'] for r in rows if r['qualification'] == 'reclassified_to_afs'}
    assert len(sale) == 1 and sale[0]['amortized_cost'] == pytest.approx(face)
    assert moved == pytest.approx({'h1': (100. - face)*(.9 - 1.), 'h2': 100.*(.9 - 1.)})
    assert summary(result)['htm_reclassification_day'] == sale[0]['day']
    after = path(result, 30)
    assert after['aoci'] == pytest.approx(sum(moved.values()))
    assert after['htm_headroom'] == pytest.approx(after['assets'])   # nothing is held to maturity now
    journal = result['journal'].filter((pl.col('scenario') == 'stress') & (pl.col('event') == 'htm_reclassification'))
    assert journal.height and (journal['debit'].sum() - journal['credit'].sum()) == pytest.approx(0., abs=1e-9)


def test_sale_under_materiality_or_permitted_exception_keeps_htm(tmp_path):
    small = run(htm_book(materiality=.5), tmp_path/'small')
    assert set(events(small)['qualification']) == {'non_exception'}
    assert summary(small)['htm_reclassification_day'] is None
    exception = run(htm_book(reason='credit_deterioration'), tmp_path/'credit')
    assert set(events(exception)['qualification']) == {'permitted_exception'}
    assert path(exception, 30)['aoci'] == pytest.approx(0.)


def test_near_maturity_needs_the_three_month_window(tmp_path):
    inside = run(htm_book(reason='near_maturity', maturity=80, horizon=120), tmp_path/'inside')
    assert set(events(inside)['qualification']) == {'permitted_exception'}
    outside = run(htm_book(reason='near_maturity', maturity=0), tmp_path/'outside')
    qualifications = events(outside)['qualification'].to_list()
    assert 'unsupported_judgment' in qualifications       # explicit, and conservatively counted
    assert 'reclassified_to_afs' in qualifications


def test_without_a_selected_policy_sales_are_not_assessed(tmp_path):
    result = run(htm_book(reason='', policy=''), tmp_path)
    assert set(events(result)['qualification']) == {'not_assessed'}
    assert set(events(result)['accounting_policy']) == {'none'}
    assert summary(result)['htm_reclassification_day'] is None


@pytest.mark.parametrize('mutate, message', [
    (lambda s: s['policies'][0].pop('sale_reason'), 'needs a sale_reason'),
    (lambda s: s['accounts'][0].update(htm_accounting='ifrs9-hold'), 'unsupported HTM accounting policy'),
    (lambda s: s['accounts'][0].update(htm_accounting='', htm_taint_materiality=.1), 'needs a selected HTM'),
    (lambda s: s['policies'][0].update(sale_reason='rebalancing'), 'known reason'),
])
def test_htm_accounting_inputs_are_validated(mutate, message):
    spec = htm_book()
    mutate(spec)
    with pytest.raises(ValueError, match=message):
        validate(spec)


def test_large_transfers_eliminate_whatever_the_summation_order(tmp_path):
    # Rust sums intercompany balances in hash order; at this scale a currency-wide
    # sum used to leave an ulp-sized residue and fail the elimination check.
    k = 1e8
    spec = dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id='hold', entity='holdco', currency='USD', cash=300.1234567*k, equity=300.1234567*k),
                          dict(id='bank', entity='bank', currency='USD', cash=30.987654321*k,
                               equity=50.987654321*k, cash_floor=20.*k)],
                positions=[dict(id='ln', account='bank', kind='loan', balance=220.*k),
                           dict(id='wf', account='bank', kind='funding', balance=50.3333333*k, maturity_day=5),
                           dict(id='wf2', account='bank', kind='funding', balance=7.77777777*k, maturity_day=9),
                           dict(id='wf3', account='bank', kind='funding', balance=3.1415926*k, maturity_day=13),
                           dict(id='dep', account='bank', kind='deposit',
                                balance=(30.987654321+220.-50.987654321-50.3333333-7.77777777-3.1415926)*k)],
                policies=[dict(id=f'support{i}', account='hold', destination='bank', kind='transfer',
                               trigger_cash=25.11111*k, limit=1000.*k) for i in range(2)],
                scenarios=[dict(name='stress')])
    for attempt in range(5):
        result = run(spec, tmp_path/str(attempt))
        assert (result['actions']['status'] == 'executed').sum() > 2


# ---- #30 deferred tax, AOCI and CET1 ------------------------------------------

def tax_book(shift=.02, price=.9, sale=True, **account):
    """One AFS security marked at `price`; a rate move re-marks it by duration x shift and a
    cash need sells part of it. Opening equity is the reported, after-tax figure."""
    policy = dict(tax_accounting='deferred-tax-aoci-1', securities_tax_rate=.25) | account
    gross = 100.*(price-1)
    tax = -gross*policy['securities_tax_rate']*(1-policy.get('tax_benefit_allowance', 0.) if gross < 0 else 1.)
    equity = 10. + 100.*price + tax - 62.5
    spec = dict(version=MODEL_VERSION, horizon_days=30,
                accounts=[dict(id='a', entity='bank', currency='USD', cash=10., equity=equity, **policy)],
                positions=[dict(id='afs', account='a', kind='security', balance=100., classification='afs',
                                opening_market_price=price, duration=5.),
                           dict(id='wf', account='a', kind='funding', balance=62.5)],
                scenarios=[dict(name='stress', rate_shift=shift)])
    if sale:
        spec['policies'] = [dict(id='sale', account='a', kind='sell', position='afs', trigger_cash=50., limit=40.)]
    return spec


def balance(result, account, scenario='stress'):
    rows = result['trial_balance'].filter((pl.col('scenario') == scenario) & (pl.col('gl_account') == account))
    return rows['balance'].sum()


def test_oci_is_carried_net_of_tax_and_a_sale_does_not_charge_equity_twice(tmp_path):
    result = run(tax_book(other_aoci=-1.), tmp_path)
    # Opening: gross mark -10, deferred tax asset 2.5, supplied other AOCI -1.
    assert path(result, 0)['aoci'] == pytest.approx(-10. + 2.5 - 1.)
    assert path(result, 0)['assets'] == pytest.approx(10. + 90. + 2.5)
    # Day 1: the price falls from 0.9 to 0.9 x (1 - 5 x 0.02) = 0.81; gross -9, net of 25% tax -6.75.
    day1 = path(result, 1)
    assert day1['aoci'] - path(result, 0)['aoci'] == pytest.approx(-6.75)
    assert day1['equity'] == pytest.approx(path(result, 0)['equity'] - 6.75)
    # Day 2: sell for 40 of proceeds at 0.81. Face 40/0.81; realized loss face x 0.19.
    face = 40./.81
    loss = face*.19
    sale = result['ledger'].filter((pl.col('scenario') == 'stress') & (pl.col('day') == 2))
    by_event = {r['event']: r for r in sale.to_dicts()}
    assert by_event['security_sale']['earnings'] == pytest.approx(-loss)
    assert by_event['security_sale']['aoci'] == pytest.approx(loss*.75)          # release, net of its tax
    assert by_event['securities_tax']['earnings'] == pytest.approx(loss*.25)
    assert by_event['securities_tax']['cash'] == 0.
    day2 = path(result, 2)
    assert day2['equity'] == pytest.approx(day1['equity'])                       # already marked: no second loss
    assert day2['cash'] == pytest.approx(50.)                                    # proceeds only, no tax cash
    assert balance(result, 'tax_receivable') == pytest.approx(loss*.25)
    assert balance(result, 'deferred_tax') == pytest.approx((100.-face)*.19*.25)
    assert balance(result, 'oci_other') == pytest.approx(1.)                     # credit balance of -(-1)
    events = result['accounting_events'].filter(pl.col('scenario') == 'stress').to_dicts()
    assert [(e['qualification'], e['accounting_policy']) for e in events] == [('realized_tax_benefit', 'deferred-tax-aoci-1')]
    assert events[0]['amortized_cost'] == pytest.approx(-loss) and events[0]['aoci'] == pytest.approx(loss*.25)


def test_tax_benefit_becomes_cash_only_on_a_dated_refund(tmp_path):
    never = run(tax_book(), tmp_path/'never')
    dated = run(tax_book(tax_refund_day=20), tmp_path/'dated')
    benefit = 40./.81*.19*.25
    assert path(never, 30)['cash'] == pytest.approx(50.)
    assert path(dated, 19)['cash'] == pytest.approx(50.)
    assert path(dated, 20)['cash'] == pytest.approx(50. + benefit)
    assert path(dated, 20)['equity'] == pytest.approx(path(dated, 19)['equity'])   # settlement, not income
    assert balance(dated, 'tax_receivable') == pytest.approx(0.)
    assert 'tax_refund' in set(dated['accounting_events']['qualification'])


def test_unrealizable_benefits_are_not_recognized_and_gains_are_taxed_in_full(tmp_path):
    # 40% of the benefit on losses is not recognized: deferred tax asset 10 x 0.25 x 0.6.
    loss = run(tax_book(tax_benefit_allowance=.4, sale=False), tmp_path/'loss')
    assert path(loss, 0)['aoci'] == pytest.approx(-10. + 1.5)
    assert path(loss, 1)['aoci'] == pytest.approx(-19. + 19.*.25*.6)
    # A gain is a liability in full, whatever the allowance: price 1.0 -> 1.1, gross +10, tax -2.5.
    gain = run(tax_book(shift=-.02, price=1., tax_benefit_allowance=.4, sale=False), tmp_path/'gain')
    after = path(gain, 1)
    assert after['aoci'] == pytest.approx(7.5)
    assert balance(gain, 'deferred_tax') == pytest.approx(-2.5)
    assert after['liabilities'] == pytest.approx(62.5 + 2.5)


def test_cet1_follows_the_dated_opt_out_and_the_tax_asset_deduction(tmp_path):
    base = dict(sale=False, other_aoci=-1.)
    opted = run(tax_book(aoci_cet1_treatment='opt_out', aoci_cet1_effective_day=5, **base), tmp_path/'opted')
    kept = run(tax_book(aoci_cet1_treatment='opt_out', other_aoci_in_cet1=True, **base), tmp_path/'kept')
    included = run(tax_book(aoci_cet1_treatment='include', dta_cet1_deduction=1., **base), tmp_path/'included')
    equity, net_marks = path(opted, 10)['equity'], -19.*.75
    assert path(opted, 4)['cet1'] == pytest.approx(path(opted, 4)['equity'])        # before the election applies
    assert path(opted, 10)['cet1'] == pytest.approx(equity - (net_marks - 1.))      # all AOCI excluded
    assert path(kept, 10)['cet1'] == pytest.approx(equity - net_marks)              # other components stay in
    assert path(included, 10)['cet1'] == pytest.approx(equity - 19.*.25)            # deferred tax asset deducted
    assert path(opted, 10)['equity'] == pytest.approx(path(included, 10)['equity']) # treatment never moves equity


@pytest.mark.parametrize('account, message', [
    (dict(tax_accounting='ifrs-ias12'), 'unsupported tax accounting policy'),
    (dict(tax_accounting='', securities_tax_rate=.25), 'need a selected tax accounting policy'),
    (dict(tax_accounting='', other_aoci=-1., securities_tax_rate=0.), 'need a selected tax accounting policy'),
    (dict(aoci_cet1_treatment='exclude'), 'include or opt_out'),
    (dict(aoci_cet1_effective_day=3), 'applies only to an opt_out'),
    (dict(tax_refund_day=31), 'within the horizon'),
    (dict(securities_tax_rate=1.), 'securities_tax_rate'),
])
def test_tax_accounting_inputs_are_validated(account, message):
    spec = tax_book()
    spec['accounts'][0].update(account)
    with pytest.raises(ValueError, match=message):
        validate(spec)


def test_without_a_tax_policy_oci_stays_gross_and_sales_carry_no_tax(tmp_path):
    spec = tax_book()
    for key in ('tax_accounting', 'securities_tax_rate'):
        spec['accounts'][0].pop(key)
    spec['accounts'][0]['equity'] = 10. + 90. - 62.5
    result = run(spec, tmp_path)
    assert path(result, 1)['aoci'] == pytest.approx(-19.)
    assert 'securities_tax' not in set(result['ledger']['event'])
    assert balance(result, 'deferred_tax') == 0. and result['accounting_events'].is_empty()
