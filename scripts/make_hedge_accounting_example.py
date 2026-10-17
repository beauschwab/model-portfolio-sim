"""Generate examples/models/hedge-accounting-1.json: a synthetic two-period book.

Two payer swaps on term-1m floating over January-March 2024 with daily
derivative nodes and a 6% collateral curve that drops to 4% on 1 February. The
first period fixed at 6.5%; the second fixes at 4% on 1 February; the third is
projected at 6% until 1 February, then 4.5%, and fixes at 4.5% on 1 March.
`swap-fvh` (fixed 6%) hedges a 7% fixed-rate loan whose benchmark component is
6%. `swap-cfh` is fixed at the 1 February par rate so a matched-terms cash-flow
hedge of forecast floating interest starts there with zero instrument value.
Legs are deliberately not identical: exactly offsetting legs produce floating
noise marks that the exact-zero posting filter treats differently across
implementations. Opening anchors use the same simple discount formula as the
curve; this is fixture construction, not a production calculation.
"""
from copy import deepcopy
import datetime as dt
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
JAN1, FEB1, MAR1, APR1, MAY1 = (dt.date(2024, m, 1).toordinal() for m in (1, 2, 3, 4, 5))
LAST = APR1
PERIODS = [('1', JAN1, FEB1), ('2', FEB1, MAR1), ('3', MAR1, APR1)]
# The treasury example hedges one forecast period beyond the instrument so closing
# cash-flow-hedge AOCI is nonzero and its CET1 eligibility is a visible policy choice.
EXTENDED = PERIODS + [('4', APR1, MAY1)]
OBSERVED = {'1': .065, '2': .04, '3': .045, '4': .045}
SWAPS = ['swap-fvh', 'swap-cfh']


def projected(pid, t):
    """Pricing projection known at node t for an unfixed period."""
    return .06 if t < FEB1 else .045


def df(rate, days):
    return 1. / (1. + rate * days / 360.)


def par_rate_feb1(periods=PERIODS):
    """Fixed rate making the remaining periods at market on 1 February (4% simple curve)."""
    weights = [((e - s) / 360., df(.04, e - FEB1), OBSERVED[pid] if s <= FEB1 else .045) for pid, s, e in periods if s >= FEB1]
    return sum(r * tau * d for tau, d, r in weights) / sum(tau * d for tau, d, _ in weights)


FIXED = {'swap-fvh': .06, 'swap-cfh': par_rate_feb1()}


def opening_value(position):
    """Payer clean value on 1 January: observed 6.5%, projected 6%, 6% simple curve."""
    pv = 0.
    for pid, s, e in PERIODS:
        rate = OBSERVED[pid] if s <= JAN1 else projected(pid, JAN1)
        pv += (rate - FIXED[position]) * 100. * (e - s) / 360. * df(.06, e - JAN1)
    return pv


def ident(i, r='r1'):
    return dict(id=i, revision=r)


def utc(ordinal, hour):
    return (ordinal - 719163) * 86400 + hour * 3600


def curve(t, last=APR1):
    rate = .06 if t < FEB1 else .04
    points = [dict(ordinal=t, discount_factor=1.)]
    points += [dict(ordinal=d, discount_factor=df(rate, d - t)) for d in (FEB1, MAR1, APR1, MAY1) if t < d <= last]
    return dict(source=ident('discount'), collateral_discount_policy=ident('discount-policy'), currency='USD',
                asof_ordinal=t, information_unix_seconds=utc(t, 9), basis='collateralized_clean', points=points)


def observation(t, pid, start, end):
    known = start <= t
    source = (dict(kind='observed', fixing=ident(f'observed-reset-{pid}'), publication_unix_seconds=utc(start, 8)) if known
              else dict(kind='conditional_projection', model=ident('index-model'), path=ident('index-path'),
                        information_unix_seconds=utc(min(t, start), 9), measure='pricing'))
    return dict(interest_start_ordinal=start, interest_end_ordinal=end, observation_start_ordinal=start,
                observation_end_ordinal=end, fixing_ordinal=start, selection_cutoff_unix_seconds=utc(start, 9),
                rate=OBSERVED[pid] if known else projected(pid, t), source=source)


def floating(t, pid, start, end):
    coupon = dict(kind='indexed', index=dict(index_id='term-1m', kind='term', tenor_months=1, average_calendar_days=None),
                  calendar=ident('calendar'), complete_calendar_mapping=True, calculation='term_simple',
                  margin_treatment='simple', margin=0., index_bounds=dict(floor=None, cap=None),
                  all_in_bounds=dict(floor=None, cap=None), reset_cutoff_unix_seconds=utc(start, 9),
                  lookback_business_days=0, observation_shift=False, observations=[observation(t, pid, start, end)])
    return dict(period_id='l' + pid, start_ordinal=start, end_ordinal=end, payment_ordinal=end, day_count='act360',
                coupon=coupon, fee=0.)


def swap(t, position, periods=PERIODS, fixed=None, last=LAST):
    return dict(kind='swap', request=dict(
        contract=ident(position + '-contract'), index_basis=ident('index-basis'), rate_path=ident('index-path'),
        position_id=position, scenario_id='physical', currency='USD', monetary_unit='currency_units',
        asof_ordinal=t, valuation_cutoff_unix_seconds=utc(t, 9), tail_end_ordinal=last,
        payment_timing='after_scheduled_payments', side='payer', notional=100.,
        fixed_rate=FIXED[position] if fixed is None else fixed,
        fixed_periods=[dict(period_id='f' + pid, start_ordinal=s, end_ordinal=e, payment_ordinal=e, day_count='act360')
                       for pid, s, e in periods],
        floating_periods=[floating(t, pid, s, e) for pid, s, e in periods], discount_curve=curve(t, last)))


def node(t):
    return dict(ordinal=t, valuation_cutoff_unix_seconds=utc(t, 9), payment_timing='after_scheduled_payments',
                trades=[swap(t, p) for p in SWAPS], discount_from_asof=1., discount_source=ident('discount'),
                collateral_rate=0., collateral_rate_source=ident('collateral-rate'), funding_spread=0.,
                funding_source=ident('funding'), settlement_available=True)


def derivative():
    policy = dict(identity=ident('csa'), entity='bank', counterparty='cp', currency='USD', monetary_unit='currency_units',
                  receive_threshold=0., post_threshold=0., minimum_transfer=0., settlement_lag_calendar_days=0,
                  opening_vm=0., opening_received_im=0., opening_posted_im=0., opening_source=ident('opening'),
                  received_vm_reusable_for_funding=False, received_im_segregated=True,
                  collateral_interest_day_count='act360', tail_return_ordinal=LAST)
    mapping = [dict(position_id=p, contract=ident(p + '-contract'), entity='bank', counterparty='cp', currency='USD',
                    netting_set_id='ns', legal_review=ident('legal'), enforceable_netting=True) for p in SWAPS]
    path = dict(path_id='physical', source=ident('physical-path'), probability=1., default=None,
                nodes=[node(t) for t in range(JAN1, LAST + 1)])
    raw = dict(schema='derivative-contract-1', model_version='derivative-contracts-1', operation=dict(
        kind='netting', request=dict(asof_ordinal=JAN1, measure='physical', conditional_model=ident('index-model'),
                                     crn=ident('crn'), policy=policy, mapping=mapping, paths=[path])))
    reviews = [dict(position_id=p, designation='undesignated_economic_hedge', designation_review=ident('designation'),
                    mark_recognition='clean_value_changes_in_earnings', coupon_recognition='settle_derivative_carrying',
                    delivery_review=None) for p in SWAPS]
    physical = [dict(position_id=p, period_id='l' + pid, path=ident('physical-path'),
                     observations=[dict(observation(LAST, pid, s, e), rate=OBSERVED[pid])])
                for p in SWAPS for pid, s, e in PERIODS]
    return dict(schema='derivative-accounting-1', model_version='derivative-accounting-1', run=ident('accounting-run'),
                scenario_id='physical', entity='bank', currency='USD', monetary_unit='currency_units', measure='physical',
                basis='modeled_physical_scenario',
                policy=dict(identity=ident('accounting-policy'), trades=reviews, custody='segregated_custodian_memo_only',
                            default='reviewed_modeled_closeout_shortfall_in_earnings', xva='report_only',
                            modeled_funding_cost='report_only'),
                opening=dict(source=ident('opening-ledger'), ordinal=JAN1, cash=100.,
                             trades=[dict(position_id=p, clean_value=opening_value(p)) for p in SWAPS], posted_vm_asset=0.,
                             received_vm_liability=0., posted_im_asset=0., received_im_custody=0.,
                             equity_credit_balance=100. + sum(opening_value(p) for p in SWAPS)),
                derivative=raw, physical_index_paths=physical, pricing_xva_reference=None)


def periods(prefix, source=PERIODS):
    return [dict(period_id=prefix + pid, start_ordinal=s, end_ordinal=e, payment_ordinal=e, day_count='act360')
            for pid, s, e in source]


def request(extended=False):
    policy = dict(identity=ident('hedge-policy'), rule_version='asc815-asu2017-12',
                  adoption=dict(asu_2025_09=dict(adopted=False, adoption_ordinal=None, elections=dict(
                      similar_risk_grouping=False, selectable_rate_debt=False, swap_written_option_exception=False,
                      dual_hedge_basis=False)), asu_2022_01_portfolio_layer=False),
                  assessment=dict(identity=ident('assessment-policy'), method='cumulative_dollar_offset',
                                  lower_bound=.8, upper_bound=1.25),
                  fvh_basis_amortization='straight_line_actual_days_to_principal',
                  cfh_reclassification='hedged_period_hypothetical_settlement')
    items = [dict(item_id='loan-1', source=ident('loan-1-terms'), carrying_source=ident('loan-1-carrying'),
                  kind='fixed_rate_instrument', direction='asset', notional=100., contractual_rate=.07,
                  periods=periods('p'), principal_ordinal=APR1)]
    exposure_periods = EXTENDED if extended else PERIODS
    exposures = [dict(exposure_id='float-1', source=ident('float-1-forecast'), probability_evidence=ident('float-1-probability'),
                      direction='payment', notional=100., index_id='term-1m', periods=periods('e', exposure_periods))]
    hypothetical = None if not extended else dict(evidence=ident('cfh-1-hypothetical'), nodes=[
        dict(ordinal=t, request=swap(t, 'hypothetical', EXTENDED, par_rate_feb1(EXTENDED), MAY1)['request'])
        for t in range(FEB1, LAST + 1)])
    relationships = [
        dict(relationship_id='fvh-1', predecessor_relationship_id=None, designation='fair_value_hedge',
             hedging_instrument_position_id='swap-fvh', designated_proportion=1., hedged_item_id='loan-1',
             hedged_notional=100., forecast_exposure_id=None, hedged_risk='benchmark_interest_rate', benchmark_rate=.06,
             inception_ordinal=JAN1, risk_management_objective=ident('fvh-1-objective'),
             documentation=ident('fvh-1-documentation'), hypothetical=None, matched_critical_terms_evidence=None),
        dict(relationship_id='cfh-1', predecessor_relationship_id=None, designation='cash_flow_hedge',
             hedging_instrument_position_id='swap-cfh', designated_proportion=1., hedged_item_id=None,
             hedged_notional=None, forecast_exposure_id='float-1', hedged_risk='benchmark_interest_rate',
             benchmark_rate=None, inception_ordinal=FEB1, risk_management_objective=ident('cfh-1-objective'),
             documentation=ident('cfh-1-documentation'), hypothetical=hypothetical,
             matched_critical_terms_evidence=None if extended else ident('cfh-1-critical-terms')),
    ]
    events = [dict(event_id=f'assess-{r}-{o}', relationship_id=r, ordinal=o, sequence=i, kind='assess',
                   review=ident('assessment-' + r)) for o, i, r in ((FEB1, 0, 'fvh-1'), (MAR1, 0, 'fvh-1'), (MAR1, 1, 'cfh-1'))]
    return dict(schema='hedge-accounting-1', model_version='hedge-accounting-1', run=ident('hedge-run'), entity='bank',
                currency='USD', monetary_unit='currency_units', policy=policy, derivative=derivative(), hedged_items=items,
                forecast_exposures=exposures, relationships=relationships, events=events)


def requirement(minimum, buffer=0.):
    return dict(minimum=minimum, buffer=buffer, management=.005)


def treasury_request():
    """Closing capital on the extended hedge: one reviewed policy for the Physical scenario."""
    closing = dt.date.fromordinal(LAST).isoformat()
    capital = dict(entity='bank', currency='USD', scenario='physical', period=closing, common_equity=0., retained_earnings=0.,
                   eligible_aoci=0., cet1_deductions=2., additional_tier1=5., tier2=4., eligible_tlac=60., eligible_ltd=20.,
                   credit_rwa=0., market_rwa=10., operational_rwa=20., advanced_rwa=240., average_assets=400.,
                   total_leverage_exposure=0., tangible_common_equity=0., tangible_assets=0.,
                   requirements=dict(cet1=requirement(.045, .025), tier1=requirement(.06, .025), total_capital=requirement(.08, .025),
                                     advanced_cet1=requirement(.045), advanced_tier1=requirement(.06), advanced_total_capital=requirement(.08),
                                     tier1_leverage=requirement(.04), slr=requirement(.03), tlac_rwa=requirement(.18),
                                     tlac_leverage=requirement(.075), ltd_rwa=requirement(.06), ltd_leverage=requirement(.045),
                                     tce_ta=requirement(.05)))
    policy = dict(account='bank', scenario='physical', capital=capital, include_aoci=True, cash_flow_hedge_aoci_eligible=False,
                  aoci_eligibility=dict(identity=ident('aoci-capital-policy'), deferred_tax_rate=.21, opt_out_elected=False, phase_in_fraction=.4),
                  equity_exclusions=0., intangible_assets=0., leverage_addon=0., leverage_deductions=0.,
                  gl_risk_weights={'cash': 0., 'derivative_value': 1., 'posted_margin': 0., 'hedged_item_basis_adjustment': 0.},
                  instrument_risk_weights={})
    return dict(schema='hedge-treasury-1', hedge=request(extended=True), account='bank', mapping=dict(identity=ident('hedge-gl-mapping')),
                treasury=dict(as_of=closing, policy_id='synthetic-example-not-regulatory-advice-v1', capital=[], curves=[], positions=[],
                              strategy_units=[], capital_deltas=[]),
                policies=[policy], amount_multiplier=1.)


def derivative_treasury_request():
    """Closing capital on the standalone derivative journal: no hedge accounting, every mark in earnings."""
    hedge = treasury_request(); policy = deepcopy(hedge['policies'][0])
    policy['gl_risk_weights'] = {'cash': 0., 'derivative_value': 1., 'posted_margin': 0.}
    return dict(schema='derivative-treasury-1', derivative=derivative(), account='bank', mapping=dict(identity=ident('derivative-gl-mapping')),
                treasury=dict(hedge['treasury']), policies=[policy], amount_multiplier=1., interim_report=False)


if __name__ == '__main__':
    for name, body in (('hedge-accounting-1', request()), ('hedge-treasury-1', treasury_request()), ('derivative-treasury-1', derivative_treasury_request())):
        target = ROOT / f'examples/models/{name}.json'
        target.write_text(json.dumps(body, indent=1) + '\n', encoding='utf-8')
        print(target, target.stat().st_size, 'bytes')
