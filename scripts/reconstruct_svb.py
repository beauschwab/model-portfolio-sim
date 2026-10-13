"""Reproducible aggregate SVB case study, not a calibrated bank-run forecast.

Financial state transitions and capital ratios execute in the existing Rust
engines. Python constructs explicit research inputs, verifies results and writes
the comparison report. No production model is added or fitted to the outcome.
Run: uv run --project apps/api python scripts/reconstruct_svb.py
"""
from __future__ import annotations

import argparse
from copy import deepcopy
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import platform
import time

import polars as pl

from portfolio_risk import __version__
from portfolio_risk.analytics.balance_stream import run_streamed_balance_stress, load_streamed_result, source_identity
from portfolio_risk.analytics.treasury import evaluate
from portfolio_risk.core.runtime import RunConfig, run_context

ROOT = Path(__file__).resolve().parents[1]


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n", encoding="utf-8")


def make_opening(reference):
    o, a = reference['observed_opening'], reference['assumptions']
    account = 'svbfg'
    positions = []

    def add(ident, kind, balance, **kwargs):
        positions.append(dict(id=ident, account=account, kind=kind, balance=balance,
                              source_id=f"svb-2022-10k:{ident}", risk_weight=0., **kwargs))

    add('afs', 'security', o['afs_cost'], classification='afs',
        opening_market_price=o['afs_fair_value']/o['afs_cost'],
        duration=a['afs_duration_years'], encumbered_fraction=o['afs_pledged_cost']/o['afs_cost'])
    add('htm', 'security', o['htm_carrying_value_net_allowance'], classification='htm',
        opening_market_price=o['htm_fair_value']/o['htm_carrying_value_net_allowance'],
        duration=o['htm_duration_years'])
    add('net_loans', 'loan', o['net_loans'])
    for key in ('nonmarketable_equity_securities', 'premises_equipment', 'goodwill',
                'other_intangibles', 'lease_right_of_use', 'other_assets'):
        add(key, 'security', o[key], classification='ac')
    for key in ('noninterest_deposits', 'interest_deposits'):
        add(key, 'deposit', o[key], uninsured_fraction=o['uninsured_share_approx'],
            concentration=a['deposit_concentration'], digital_fraction=a['deposit_digital_fraction'],
            operational_fraction=a['deposit_operational_fraction'])
    for key in ('short_term_borrowings', 'lease_liabilities', 'other_liabilities', 'long_term_debt'):
        add(key, 'funding', o[key])
    return dict(version='balance-stress-2', horizon_days=30, positions=positions,
                accounts=[dict(id=account, entity=reference['entity'], currency='USD',
                               cash=o['cash'], equity=o['total_equity'], cet1_floor=0., leverage_floor=0.,
                               lcr_floor=0., nsfr_floor=0.)],
                scenarios=[dict(name='static')], provenance=dict(case_id=reference['case_id'],
                    basis='observed aggregated carrying values and explicit uncalibrated stress assumptions',
                    amount_unit='USD millions', as_of=reference['balance_sheet_date']))


def scenario_specs(reference):
    base = make_opening(reference)
    specs = {'opening_and_rate_sensitivity': deepcopy(base)}
    specs['opening_and_rate_sensitivity']['scenarios'] = [dict(name='rates_up_100bp', rate_shift=.01),
        dict(name='rates_up_200bp', rate_shift=.02)]
    behavioral = deepcopy(base)
    behavioral['scenarios'] = [dict(name=f'flight_{int(x*100):03d}', deposit_flight=x)
                               for x in (.05, .10, .20, .30, .40, .50, 1.)]
    specs['behavioral_sensitivity'] = behavioral
    o, event = reference['observed_opening'], reference['observed_event']
    # These scenarios condition on the observed/requested outflows. They are not
    # behavioral predictions. Proportional allocation across two deposit cohorts
    # is an explicit placeholder because the depositor-level tape is unavailable.
    for label, two_days, sale_limit, capacity, delay in [
        ('observed_day1_cash_only', False, 0., 0., 1),
        ('observed_day1_sale21', False, 21000., 0., 1),
        ('requested_day2_sale21', True, 21000., 0., 1),
        ('observed_day1_all_free_afs', False, 1e9, 0., 1),
        ('observed_day1_sale21_funding5', False, 21000., 5000., 1),
        ('observed_day1_sale21_funding10', False, 21000., 10000., 1),
        ('observed_day1_sale21_funding10_lag3', False, 21000., 10000., 3),
        ('requested_day2_sale21_funding20', True, 21000., 20000., 1),
    ]:
        spec = deepcopy(base)
        spec['scenarios'] = [dict(name='event', start_day=3)]
        spec['cashflows'] = []
        for key in ('noninterest_deposits', 'interest_deposits'):
            share = o[key]/o['total_deposits']
            spec['cashflows'].append(dict(position=key, day=3, principal=0., scenario='baseline'))
            spec['cashflows'].append(dict(position=key, day=3,
                principal=event['march9_withdrawals']*share, scenario='event'))
            if two_days:
                spec['cashflows'].append(dict(position=key, day=4,
                    principal=event['march10_additional_expected_withdrawals_lower_bound']*share,
                    scenario='event'))
        spec['policies'] = []
        if sale_limit:
            spec['policies'].append(dict(id='afs_sale', account='svbfg', kind='sell', position='afs',
                trigger_cash=o['cash']+min(sale_limit, o['afs_fair_value']), limit=sale_limit, delay_days=1))
        if capacity:
            htm = next(p for p in spec['positions'] if p['id'] == 'htm')
            htm.update(collateral_pool='assumed_available_htm', eligible_fraction=1., haircut=.05)
            spec['policies'].append(dict(id='htm_borrowing_counterfactual', account='svbfg',
                kind='secured_funding', position='htm', trigger_cash=o['cash']+21000.+capacity,
                limit=capacity, delay_days=delay, funding_rate=0., funding_tenor_days=365))
        spec['provenance']['basis'] = 'conditional event replay on frozen year-end consolidated balance sheet'
        specs[label] = spec
    return specs


def capital_spec(reference):
    o = reference['observed_opening']
    # Regulatory eligibility is observed, not reconstructed or inferred from
    # the aggregate ledger's research risk weights. Zero exposure denominators
    # make unavailable SLR/leverage/TLAC/LTD measures unavailable, not zero ratios.
    c = dict(entity=reference['entity'], currency='USD', scenario='reported_opening', period='2022-12',
        common_equity=o['cet1']*1e6, retained_earnings=0., eligible_aoci=0., cet1_deductions=0.,
        additional_tier1=(o['tier1']-o['cet1'])*1e6, tier2=(o['total_capital']-o['tier1'])*1e6,
        eligible_tlac=0., eligible_ltd=0., credit_rwa=o['rwa']*1e6, market_rwa=0., operational_rwa=0.,
        advanced_rwa=None, average_assets=0., total_leverage_exposure=0.,
        tangible_common_equity=o['tangible_common_equity']*1e6, tangible_assets=o['tangible_assets']*1e6,
        requirements={k:dict(minimum=v, buffer=.025, management=0.)
                      for k,v in [('cet1',.045), ('tier1',.06), ('total_capital',.08)]})
    return dict(as_of='2022-12-31', policy_id='svb-disclosed-standardized-capital-v1',
                capital=[c], curves=[], positions=[])


def path_row(result, scenario, day):
    return result['path'].filter((pl.col('scenario') == scenario) & (pl.col('day') == day)).row(0, named=True)


def run(reference, directory):
    outputs, results, validations = {}, {}, []
    experiment_source = source_identity()
    start = time.perf_counter()
    for name, spec in scenario_specs(reference).items():
        manifest = run_streamed_balance_stress(spec, directory/name, backend='rust')
        result = load_streamed_result(manifest)
        m = result['manifest']
        assert m['backend'] == 'rust' and m['validation']['journal_replayed']
        assert result['summary']['max_reconciliation_error'].max() < 1e-6
        outputs[name] = dict(manifest=str(manifest.relative_to(ROOT)), timings=m['timings'],
            validation=m['validation'], binary_sha256=m['binary_sha256'], source_sha256=m['source_sha256'],
            input_sha256=hashlib.sha256(json.dumps(spec, sort_keys=True).encode()).hexdigest(),
            summaries=result['summary'].to_dicts())
        results[name] = result
        validations.append(dict(check=f'{name}: independently replayed journal and balanced subledger', passed=True))
        print(f'{name}: journal verified', flush=True)
    assert {v['source_sha256'] for v in outputs.values()} == {experiment_source}
    assert len({v['binary_sha256'] for v in outputs.values()}) == 1
    o, event = reference['observed_opening'], reference['observed_event']
    opening = path_row(results['opening_and_rate_sensitivity'], 'baseline', 0)
    comparisons = []
    for field, target in [('assets','total_assets'), ('liabilities','total_liabilities'),
                          ('equity','total_equity'), ('cash','cash')]:
        actual = opening[field]
        assert math.isclose(actual, o[target], abs_tol=1e-7)
        comparisons.append(dict(metric=target, model=actual, observed=o[target],
            difference=actual-o[target], units='USD millions', status='input reconciliation, not prediction'))
    static = path_row(results['opening_and_rate_sensitivity'], 'baseline', 30)
    assert math.isclose(static['equity'], opening['equity'], abs_tol=1e-7)
    assert math.isclose(static['cash'], opening['cash'], abs_tol=1e-7)
    with run_context(RunConfig(compute_backend='rust')):
        capital = evaluate(capital_spec(reference))
    capital_rows = []
    for row in capital['capital_metrics'].to_dicts():
        target = {'cet1':'cet1_ratio', 'tier1':'tier1_ratio',
                  'total_capital':'total_capital_ratio', 'tce_ta':'tce_ratio'}.get(row['metric'])
        if target:
            assert abs(row['ratio']-o[target]) < .00005
            capital_rows.append(row | dict(observed_ratio=o[target], difference_bp=(row['ratio']-o[target])*1e4))
    behavioral = []
    for name in [s['name'] for s in scenario_specs(reference)['behavioral_sensitivity']['scenarios']]:
        result = results['behavioral_sensitivity']
        day1 = path_row(result, name, 1)
        loss = o['cash']-day1['cash']
        summary = next(r for r in outputs['behavioral_sensitivity']['summaries'] if r['scenario'] == name)
        behavioral.append(dict(scenario=name, first_day_outflow=loss,
            fraction_of_observed_day1=loss/event['march9_withdrawals'],
            day1_cash=day1['cash'], first_cash_breach_day=summary['first_cash_breach_day']))
    replays = []
    for name, result in results.items():
        if name in {'opening_and_rate_sensitivity', 'behavioral_sensitivity'}:
            continue
        d2, d3, d4 = (path_row(result, 'event', d) for d in (2,3,4))
        sale = result['ledger'].filter((pl.col('scenario') == 'event') & (pl.col('event') == 'security_sale'))
        funding = result['ledger'].filter((pl.col('scenario') == 'event') & (pl.col('event') == 'secured_funding'))
        replays.append(dict(case=name, march8_cash=d2['cash'], march9_cash=d3['cash'],
            march10_counterfactual_cash=d4['cash'], sale_proceeds=sale['cash'].sum(),
            realized_sale_pretax_loss=-sale['earnings'].sum(), funding_received=funding['cash'].sum(),
            march9_cash_difference_vs_observed=d3['cash']-event['march9_closing_cash_approx']))
    sale_case = next(r for r in replays if r['case'] == 'observed_day1_sale21')
    assert abs(sale_case['march9_cash']-(o['cash']+21000.-42000.)) < 1e-6
    assert abs(sale_case['realized_sale_pretax_loss']-21000.*(o['afs_cost']/o['afs_fair_value']-1)) < 1e-6
    # Realizing an already-marked AFS loss reclassifies OCI to earnings. It
    # must not deduct the same loss from total book equity a second time.
    assert abs(path_row(results['observed_day1_sale21'],'event',3)['equity']-o['total_equity']) < 1e-6
    day2 = next(r for r in replays if r['case'] == 'requested_day2_sale21')
    assert abs(day2['march10_counterfactual_cash']-(o['cash']+21000.-142000.)) < 1e-6
    fast = next(r for r in replays if r['case'] == 'observed_day1_sale21_funding10')
    slow = next(r for r in replays if r['case'] == 'observed_day1_sale21_funding10_lag3')
    assert abs(fast['march9_cash']-2803.) < 1e-6
    assert abs(slow['march9_cash']+7197.) < 1e-6 and abs(slow['march10_counterfactual_cash']-2803.) < 1e-6
    for name in ('observed_day1_sale21', 'requested_day2_sale21'):
        # Non-anticipation: adding a future request cannot alter prior days.
        frame = results[name]['path'].filter((pl.col('scenario')=='event') & (pl.col('day')<4))
        if name.startswith('observed'):
            first = frame
        else:
            assert first.equals(frame)
    rate_rows = []
    for name in ('baseline', 'rates_up_100bp', 'rates_up_200bp'):
        result = results['opening_and_rate_sensitivity']
        ex = result['exposures'].filter((pl.col('scenario')==name) & (pl.col('day')==30) & (pl.col('position')=='htm'))
        # Inspect native market-factor output; opening loss is supplied data,
        # additional shocks use the existing duration approximation.
        market = ex.row(0,named=True)['market_factor']
        htm_value = o['htm_carrying_value_net_allowance']*market
        book = path_row(result,name,1)
        rate_rows.append(dict(scenario=name, htm_market_value=htm_value,
            htm_unrecognized_loss=o['htm_carrying_value_net_allowance']-htm_value,
            book_equity=book['equity'], parent_equity_less_htm_gap=book['equity']-o['noncontrolling_equity']
                -(o['htm_carrying_value_net_allowance']-htm_value)))
    validations.extend([
        dict(check='opening assets/liabilities/total equity/cash match independently transcribed disclosure', passed=True),
        dict(check='zero-event cash and equity unchanged through day 30', passed=True),
        dict(check='four native capital ratios match disclosed rounding', passed=True),
        dict(check='cash replay independently matches cash + proceeds - withdrawals', passed=True),
        dict(check='AFS realized loss matches independent cost allocation; no double-counted book equity loss', passed=True),
        dict(check='future March 10 requests do not change earlier output', passed=True),
        dict(check='prearranged funding covers only obligations after its modeled settlement date', passed=True),
        dict(check='same engine source and ledger binary across all ten simulations', passed=True),
    ])
    assert source_identity() == experiment_source, 'engine source changed during case study'
    return dict(case_id=reference['case_id'], engine_version=__version__, financial_backend='rust',
        created_utc=datetime.now(timezone.utc).isoformat(), platform=platform.platform(),
        measured_scope='ten small aggregate native ledger runs, persisted local Parquet, independent replay, native capital check; excludes source research, report writing and API/browser',
        elapsed_seconds=time.perf_counter()-start, reference=reference, runs=outputs,
        opening_comparison=comparisons, capital_comparison=capital_rows,
        behavioral_sensitivity=behavioral, conditional_replays=replays, rate_sensitivity=rate_rows,
        after_tax_sale_comparison=[dict(tax_rate=t, proxy_loss=sale_case['realized_sale_pretax_loss']*(1-t),
            observed_loss=1800., difference=sale_case['realized_sale_pretax_loss']*(1-t)-1800.)
            for t in reference['assumptions']['tax_sensitivity']],
        validations=validations,
        limitations=['Aggregate consolidated entity, not March 9 bank legal-entity cash reconstruction',
            'Opening book totals and HTM fair values are inputs, not predicted outcomes',
            'No security/depositor tape, observed bank-specific histories, historical OAS or MBS prepayment calibration',
            'No Jan-March balance-sheet roll-forward, NII forecast, credit/default calibration or hedges',
            'Run size/date exogenous in replay; behavioral sensitivities uncalibrated; no probability estimate',
            'Daily time steps do not model intraday payment, collateral-transfer or discount-window cutoffs',
            'Negative cash measures unpaid funding need; not a completed withdrawal financed by a hidden overdraft',
            'March 10 expected withdrawals are a hypothetical obligation; receivership interrupts actual flows',
            'Gross AFS OCI excludes tax and other AOCI components; reported consolidated AOCI is not replicated',
            'HTM permission is a policy limit, not full accounting taint/reclassification logic',
            'Daily liquidity/capital proxies are not regulatory results; no LCR/NSFR, SLR or TLAC validation',
            'Funding access, sale settlement, tax realization, legal-entity fungibility and intraday run timing remain material unknowns'])


def report(r):
    o = r['reference']['observed_opening']
    lines = ['# SVB reconstruction and historical comparison', '',
        '**Conclusion: the aggregate ledger reproduces disclosed opening totals and demonstrates the liquidity failure mechanism. It does not reproduce the precise March cash position or predict the run.**', '',
        f"Engine {r['engine_version']}; Rust financial runtime. Amounts below are USD billions unless stated. "
        f"Ten aggregate runs plus native capital checks completed in {r['elapsed_seconds']:.2f} seconds, including local Parquet and independent journal verification.", '',
        '## Scope and information boundary', '',
        'Opening: December 31, 2022 consolidated SVB Financial Group, disclosed February 24. '
        'This differs from Silicon Valley Bank alone. Total equity includes $0.291bn noncontrolling interests. '
        'The opening balance sheet is frozen for the event-window proxy; there is no claim to have reconstructed the intervening two months.', '',
        'The uncalibrated behavioral sweep uses no observed withdrawal volume. The separate event replay explicitly supplies '
        'the March 8 sale and March 9 withdrawals. The March 10 $100bn is a lower-bound requested/expected outflow counterfactual, not a settled historical payment.', '',
        '## Opening reconciliation (input checks, not predictive validation)', '',
        '| Measure | Native ledger | Disclosure | Difference |', '|---|---:|---:|---:|']
    for row in r['opening_comparison']:
        lines.append(f"| {row['metric']} | {row['model']/1000:.3f} | {row['observed']/1000:.3f} | {row['difference']/1000:.9f} |")
    lines += ['', 'The 15 position cohorts cover AFS, HTM, net loans, nonmarketable equity, premises, goodwill, other intangibles, '
        'lease assets, other assets, two deposit cohorts, short-term borrowings, lease liabilities, other liabilities and long-term debt. '
        'Non-securities fixed-value assets use explicitly labeled ledger placeholders. Loans enter at net carrying value; the $0.636bn allowance is retained in the reference facts rather than re-estimated.', '',
        f"AFS: cost {o['afs_cost']/1000:.3f}, fair value {o['afs_fair_value']/1000:.3f}. "
        f"HTM: net carrying value {o['htm_carrying_value_net_allowance']/1000:.3f}, disclosed fair value {o['htm_fair_value']/1000:.3f}. "
        f"Deposits: {o['total_deposits']/1000:.3f}; parent equity: {o['parent_equity']/1000:.3f}.", '',
        '## Capital and hidden valuation risk', '',
        '| Native metric | Recomputed | Reported | Difference (bp) |', '|---|---:|---:|---:|']
    for row in r['capital_comparison']:
        lines.append(f"| {row['metric']} | {100*row['ratio']:.4f}% | {100*row['observed_ratio']:.2f}% | {row['difference_bp']:.3f} |")
    lines += ['', 'Eligible capital and aggregate RWA are supplied from disclosures. These checks validate native arithmetic, '
        'not bottom-up capital eligibility, credit-RWA construction or a daily regulatory capital projection. '
        'Other generated treasury metrics are omitted when inputs or applicability are unavailable.', '',
        '| Mark sensitivity | HTM market value | Unrecognized HTM loss | Parent equity less HTM gap, pre-tax |',
        '|---|---:|---:|---:|']
    for row in r['rate_sensitivity']:
        lines.append(f"| {row['scenario']} | {row['htm_market_value']/1000:.3f} | {row['htm_unrecognized_loss']/1000:.3f} | {row['parent_equity_less_htm_gap']/1000:.3f} |")
    lines += ['', 'The $15.152bn opening HTM gap is 94.7% of parent equity and exceeds $11.880bn tangible common equity. '
        'Subtracting it leaves $0.852bn of parent equity or negative $3.272bn tangible common equity, before tax. '
        '**These are securities-only haircut indicators, not full economic equity, regulatory insolvency findings or resolution-loss estimates.** '
        'They omit liability franchise value, other fair-value changes, taxes and resolution recoveries. AFS marks are already in book equity; subtracting them again would double-count.', '',
        'Incremental rate marks use disclosed 6.2-year HTM duration and assumed 3.5-year AFS duration in the existing linear-duration ledger model. '
        'They are sensitivities, not historical yield-curve/MBS repricing; convexity, extension, hedge offsets and term-structure changes are absent.', '',
        '## Can the deposit behavior model predict the speed?', '',
        '| Flight input | First-day model outflow | Share of observed $42bn | First cash breach (day, cash only) |',
        '|---|---:|---:|---:|']
    for row in r['behavioral_sensitivity']:
        lines.append(f"| {row['scenario']} | {row['first_day_outflow']/1000:.3f} | {100*row['fraction_of_observed_day1']:.1f}% | {row['first_cash_breach_day']} |")
    lines += ['', 'Inputs are stress severities, not probabilities. The daily model combines a monthly flight input with uninsured, '
        'concentration, digital and operational multipliers and converts it into a daily hazard. Uninsured share is approximately observed; '
        'the other three behavior parameters are assumptions, with no fitted SVB withdrawal history. '
        'The cap on monthly hazard creates saturation at extreme severities. Selecting a severity after seeing $42bn would be calibration to the answer. '
        '**The model can generate a large one-day outflow when forced hard enough; this experiment supplies no evidence it would anticipate the confidence shock or its date.**', '',
        '## Conditional event replay and funding sensitivity', '',
        '| Case | March 9 cash | Difference from reported -0.958 | March 10 counterfactual cash |',
        '|---|---:|---:|---:|']
    for row in r['conditional_replays']:
        lines.append(f"| {row['case']} | {row['march9_cash']/1000:.3f} | {row['march9_cash_difference_vs_observed']/1000:.3f} | {row['march10_counterfactual_cash']/1000:.3f} |")
    lines += ['', 'Only cases prefixed `requested_day2` add the March 10 $100bn demand. Other March 10 values reflect the existing '
        'March 9 demand and any lagged funding execution, without another $100bn withdrawal. Negative cash is an unmet obligation.', '',
        '**Unadjusted sale replay: 13.803 cash + 21.000 sale proceeds - 42.000 withdrawals = -7.197bn.** '
        'The observed balance was approximately -0.958bn, a $6.239bn difference. This is an unexplained reconciliation residual, '
        'not an inferred actual borrowing. Different dates/entities, intervening runoff, cash movements and funding explain why '
        'a December consolidated snapshot cannot tie a March bank cash figure exactly. No balancing plug was inserted.', '',
        'Even selling all AFS available under the opening $0.530bn cost-based pledge approximation leaves a first-day shortfall. '
        'HTM borrowing cases assume capacity of $5bn, $10bn or $20bn, enough unencumbered eligible HTM, a 5% haircut, '
        'no funding interest and no outage. A higher cash-buffer target schedules borrowing on day 1: '
        'one-day lag settles March 8, three-day lag settles March 10. These are prearranged funding counterfactuals. '
        'They test capacity and settlement lag, not actual SVB facility access. '
        'The three-day delayed $10bn case cannot supply cash before the March 9 breach. '
        'Do not infer that generic HTM collateral value could have been monetized within actual payment-system deadlines.', '',
        '## Sale-loss comparison', '',
        'The sale uses year-end AFS cost/value allocation and treats the rounded $21bn sale size as proceeds. '
        'Native realized pre-tax loss is about $2.040bn. The actual sold subset and March 8 market marks were different.', '',
        '| Assumed tax benefit | Proxy after-tax loss | Announced after-tax loss | Model minus announced |',
        '|---|---:|---:|---:|']
    for row in r['after_tax_sale_comparison']:
        lines.append(f"| {100*row['tax_rate']:.0f}% | {row['proxy_loss']/1000:.3f} | {row['observed_loss']/1000:.3f} | {row['difference']/1000:.3f} |")
    lines += ['', 'The 26-28% range comes from March 8 tax guidance and is only an external sensitivity, not an exact sale tax rate. '
        'No immediate tax refund is added to simulated cash. Native realization reclassifies existing AFS OCI into earnings '
        'without charging total equity twice. The engine does not replicate the complete tax/AOCI bridge: its gross AFS OCI '
        'is -$2.533bn versus reported total AOCI of -$1.911bn.', '',
        '## Regulatory comparison', '',
        'SVB was not subject to LCR/NSFR at failure. The Fed retrospectively estimated reduced LCR of 103.1% at December 30 '
        'and 102.5% at February 28. Those ratios did not establish resilience to an hours-long run. '
        'The reduced/full LCR distinction and legal applicability matter. This reconstruction does not claim to reproduce '
        'LCR or NSFR without the relevant flow, encumbrance and funding-detail data. SVB was not a GSIB; GSIB TLAC/SLR requirements '
        'must not be applied indiscriminately.', '',
        '## Validation and remaining gaps', '',
        f"All {len(r['validations'])} explicit reconstruction checks passed. Each run persisted Parquet, checked its hashes and schema, "
        'independently replayed the journal and reconciled the native closing ledger. '
        'Cash-breach scenarios intentionally have `dynamic_validated=false`; that is a stress result, not failed journal validation. '
        'Native binary and engine source identities, inputs, timings and artifact paths are retained in the JSON companion.', '',
        'The separate [validation record](2026-10-01-svb-validation.json) records the regression checkpoint: '
        '749 passed, two skipped, one source-identity interruption during concurrent engine edits; '
        'the entire affected streamed-ledger suite then passed 55/55. This is not a claim of a clean all-suite run on one frozen revision.', '',
        'Reconciliation is strong; sale-loss magnitude is approximate; bank cash tie is materially incomplete; run probability and timing are unvalidated. '
        'No solver was invoked because this is a historical reconstruction with specified management actions, not an optimized hindsight strategy. '
        'No API database, saved user portfolio or production state was changed.', '',
        'Prioritized next requirements:', '',
        '1. Bank-only December and February/March opening trial balances, actual deposit and security cohorts, and a full intervening cash bridge.',
        '2. Intraday withdrawal requests, settlement queues, payment cutoffs, collateral location/prepositioning and facility-specific execution capacity.',
        '3. Separate ordinary deposit decay from confidence-driven correlated jumps; estimate parameters on pre-event data and test across held-out bank events.',
        '4. Historical security-level repricing with observed curves, spreads, prepayment/extension and hedge positions; preserve fixed-OAS scenario contracts.',
        '5. Deferred-tax and AOCI-to-CET1 bridges, HTM accounting consequences, legal-entity restrictions and actual LCR/NSFR data.', '',
        '## Reproduce', '',
        '```powershell', 'uv run --project apps/api python scripts/reconstruct_svb.py', '```', '',
        'Reference facts and assumptions: `examples/svb-2022/reference.json`. Each run gets fresh immutable artifact directories. '
        'The default script runs offline from the reviewed facts; it does not silently refetch or revise disclosures.', '',
        '## Primary sources', '']
    for key, source in r['reference']['sources'].items():
        lines.append(f"- [{key}]({source['url']}) — {source['locator']}. {source['role']}.")
    return '\n'.join(lines)+'\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, default=ROOT/'examples/svb-2022/reference.json')
    parser.add_argument('--artifacts', type=Path, default=ROOT/'artifacts/svb-reconstruction/runs')
    parser.add_argument('--output', type=Path, default=ROOT/'docs/reviews/2026-10-01-svb-reconstruction.json')
    args = parser.parse_args()
    reference = json.loads(args.reference.read_text(encoding='utf-8'))
    result = run(reference, args.artifacts.resolve())
    result['reference_sha256'] = hashlib.sha256(args.reference.read_bytes()).hexdigest()
    source = ROOT/'artifacts/svb-reconstruction/sources/svb-2022-10k.html'
    if source.is_file():
        result['downloaded_source_sha256'] = hashlib.sha256(source.read_bytes()).hexdigest()
    write_json(args.output, result)
    args.output.with_suffix('.md').write_text(report(result), encoding='utf-8')
    print(json.dumps(dict(report=str(args.output.with_suffix('.md')), elapsed_seconds=result['elapsed_seconds'],
                          checks_passed=len(result['validations'])), indent=2))


if __name__ == '__main__':
    main()
