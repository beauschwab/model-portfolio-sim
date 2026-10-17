"""Independent replay of hedge-treasury-1: closing rows, GL mapping and capital; never production.

The embedded hedge result is checked by the hedge reference; rows are rebuilt from
the independently replayed journal; capital amounts and the thirteen ratios are
recomputed from the rows and supplied policies with the shared bridge rules.
"""
from copy import deepcopy
from datetime import date
import math
from .hedge_accounting_reference import replay as replay_hedge, verify as verify_hedge, _compare, _require, _money, _typed
from .modeled_json_identity import exact_json_identity

SCHEMA = 'hedge-treasury-1'
ZERO_ROW = 1e-9
ASSETS = frozenset('cash restricted_cash asset_principal book_adjustment allowance accrued_interest fair_value_adjustment recovery_receivable derivative_value posted_margin intercompany_receivable dated_interest_receivable dated_prepaid_expense credit_default_principal credit_default_book_adjustment credit_default_interest_receivable live_interest_basis_adjustment default_interest_basis_adjustment hedged_item_basis_adjustment'.split())
LIABILITIES = frozenset('funding_principal secured_funding received_margin intercompany_payable dated_interest_payable dated_deferred_income credit_default_deferred_income hedged_item_basis_liability'.split())
GL = {'cash': 'cash', 'derivative_asset': 'derivative_value', 'derivative_liability': 'derivative_value', 'unpaid_receivable': 'derivative_unpaid_receivable',
      'unpaid_payable': 'derivative_unpaid_payable', 'posted_vm_asset': 'posted_margin', 'posted_im_asset': 'posted_margin',
      'received_vm_liability': 'received_margin', 'opening_equity': 'opening_equity', 'mark_gain_loss': 'pnl:derivative_mark',
      'contractual_income': 'pnl:derivative_contractual_income', 'collateral_interest_income': 'pnl:derivative_collateral_interest',
      'counterparty_loss_expense': 'pnl:derivative_counterparty_loss', 'posted_collateral_return_loss_expense': 'pnl:derivative_collateral_return_loss',
      'cash_flow_hedge_oci': 'cash_flow_hedge_oci', 'hedged_item_basis_gain_loss': 'pnl:hedged_item_basis',
      'hedged_item_basis_amortization': 'pnl:hedged_item_basis_amortization', 'hedged_item_basis_derecognition': 'pnl:hedged_item_basis_derecognition',
      'hedged_forecast_reclassification': 'pnl:hedged_forecast_reclassification'}
METRICS = ('cet1', 'tier1', 'total_capital', 'advanced_cet1', 'advanced_tier1', 'advanced_total_capital', 'tier1_leverage', 'slr', 'tlac_rwa',
           'tlac_leverage', 'ltd_rwa', 'ltd_leverage', 'tce_ta')


def rows(raw, hedge, k=-1):
    """Bridge rows through hedge day k from the independently replayed journal and opening anchors."""
    balances = {}
    report = hedge['daily'][k]['ordinal']
    def add(account, instrument, value):
        balances[(account, instrument)] = _money(balances.get((account, instrument), 0.) + value)
    for intent in hedge['intents']:
        if intent['ledger'] != 'financial' or intent['ordinal'] > report: continue
        for p in intent['postings']: add(p['account'], p['position_id'] or p['bucket'] or 'book', p['debit_amount'])
    o = raw['derivative']['opening']
    for account, value in (('cash', o['cash']), ('posted_vm_asset', o['posted_vm_asset']), ('received_vm_liability', -o['received_vm_liability']),
                           ('posted_im_asset', o['posted_im_asset']), ('opening_equity', -o['equity_credit_balance'])):
        add(account, 'book', value)
    for t in o['trades']:
        add('derivative_asset', t['position_id'], max(t['clean_value'], 0.)); add('derivative_liability', t['position_id'], -max(-t['clean_value'], 0.))
    closing = {b['account']: b['debit_balance'] for b in hedge['daily'][k]['financial_trial_balance']}
    totals = {}
    for (account, _), v in balances.items(): totals[account] = totals.get(account, 0.) + v
    for account, v in closing.items():
        _require(math.isclose(totals.get(account, 0.), v, rel_tol=1e-12, abs_tol=1e-9), 'closing account does not reconcile postings')
    items = {i['item_id']: i['direction'] for i in raw['hedged_items']}
    mapped = {}
    for (account, instrument), v in balances.items():
        if account.startswith('memo_'): continue
        if account == 'hedged_item_basis_adjustment':
            gl = 'hedged_item_basis_adjustment' if items[instrument] == 'asset' else 'hedged_item_basis_liability'
        else: gl = GL[account]
        mapped[(gl, instrument)] = _money(mapped.get((gl, instrument), 0.) + v)
    scenario = raw['derivative']['scenario_id']
    out = [dict(scenario=scenario, account=None, gl_account=gl, instrument_id=instrument, balance=v) for (gl, instrument), v in sorted(mapped.items()) if abs(v) > ZERO_ROW]
    _require(not any(r['gl_account'].startswith('derivative_unpaid') for r in out), 'unpaid rights at closing')
    return out


def _typed_policy(p):
    """Mirror the bridge's f64 policy fields as floats; identities and flags keep their categories."""
    p = deepcopy(p)
    for key in ('equity_exclusions', 'intangible_assets', 'leverage_addon', 'leverage_deductions'): p[key] = _money(p[key])
    for name in ('gl_risk_weights', 'instrument_risk_weights'): p[name] = {k: _money(v) for k, v in p[name].items()}
    _require(type(p.get('aoci_eligibility')) is dict, 'reviewed AOCI deferred-tax/opt-out/phase-in policy required')
    a = p['aoci_eligibility']; a['deferred_tax_rate'] = _money(a['deferred_tax_rate']); a['phase_in_fraction'] = _money(a['phase_in_fraction'])
    _require(0. <= a['deferred_tax_rate'] < 1. and 0. <= a['phase_in_fraction'] <= 1. and type(a['opt_out_elected']) is bool, 'AOCI eligibility policy domain')
    c = p['capital']
    for key, value in list(c.items()):
        if key in ('entity', 'currency', 'scenario', 'period', 'requirements'): continue
        c[key] = None if value is None else _money(value)
    for rule in c['requirements'].values():
        for key in ('minimum', 'buffer', 'management'): rule[key] = _money(rule[key])
    return p


def capital(rows, policies, multiplier):
    """Independent bridge: sums, eligible amounts, contributions and thirteen ratio rows."""
    rules = {(p['scenario'], p['account']): p for p in policies}
    amounts = {}; contributions = []
    for r in rows:
        key = (r['scenario'], r['account']); p = rules[key]; v = r['balance'] * multiplier; gl = r['gl_account']
        a = amounts.setdefault(key, [0.] * 8)
        if gl in ('opening_equity', 'equity_distributions'): a[2] -= v
        elif gl.startswith('pnl:'): a[3] -= v
        elif gl == 'oci': a[4] -= v
        elif gl == 'cash_flow_hedge_oci': a[4] -= v; a[7] -= v
        elif gl in LIABILITIES or gl == 'derivative_value' and v < 0.: a[1] -= v
        elif gl in ASSETS:
            weight = p['instrument_risk_weights'].get(r['instrument_id'], p['gl_risk_weights'].get(gl))
            if weight is None:
                _require(v == 0., 'explicit asset risk weight required'); weight = 0.
            a[0] += v; a[5] += v * weight
            contributions.append(dict(scenario=key[0], account=key[1], gl_account=gl, instrument_id=r['instrument_id'], balance=v, risk_weight=float(weight), rwa_contribution=v * weight))
        else: raise ValueError('unmapped closing GL ' + gl)
        a[6] += v
    audits = []; capitals = []
    for key, a in sorted(amounts.items()):
        p = rules[key]; equity = a[2] + a[3] + a[4]
        _require(abs(a[6]) <= 1e-8 * max(abs(a[0]), abs(a[1]), 1.) and abs(a[0] - a[1] - equity) <= 1e-8 * max(abs(a[0]), abs(a[1]), 1.), 'unbalanced bridge')
        c = deepcopy(p['capital'])
        included = (a[4] - (0. if p.get('cash_flow_hedge_aoci_eligible') else a[7])) if p['include_aoci'] else 0.
        policy = p['aoci_eligibility']
        deferred_tax = -included * policy['deferred_tax_rate']
        eligible = 0. if policy['opt_out_elected'] else included * (1. - policy['deferred_tax_rate']) * policy['phase_in_fraction']
        c.update(scenario=key[0], common_equity=a[2] - p['equity_exclusions'], retained_earnings=a[3], eligible_aoci=eligible, credit_rwa=a[5],
                 total_leverage_exposure=a[0] + p['leverage_addon'] - p['leverage_deductions'],
                 tangible_common_equity=equity - p['equity_exclusions'] - p['intangible_assets'], tangible_assets=a[0] - p['intangible_assets'])
        capitals.append(c)
        audits.append(dict(scenario=key[0], account=key[1], entity=c['entity'], currency=c['currency'], assets=a[0], liabilities=a[1], book_equity=equity,
                           common_equity=c['common_equity'], retained_earnings=a[3], book_aoci=a[4], eligible_aoci=eligible,
                           cash_flow_hedge_aoci=a[7], included_aoci=included, deferred_tax_on_aoci=deferred_tax,
                           aoci_opt_out_elected=policy['opt_out_elected'], aoci_phase_in_fraction=policy['phase_in_fraction'],
                           aoci_eligibility_policy=deepcopy(policy['identity']),
                           equity_exclusions=p['equity_exclusions'], credit_rwa=a[5], reconciliation_error=a[6]))
    metrics = []
    for c in capitals:
        cet1 = c['common_equity'] + c['retained_earnings'] + c['eligible_aoci'] - c['cet1_deductions']; tier1 = cet1 + c['additional_tier1']; total = tier1 + c['tier2']
        rwa = c['credit_rwa'] + c['market_rwa'] + c['operational_rwa']; exposure = c['total_leverage_exposure']
        components = [('cet1', cet1, rwa), ('tier1', tier1, rwa), ('total_capital', total, rwa), ('advanced_cet1', cet1, c['advanced_rwa']),
                      ('advanced_tier1', tier1, c['advanced_rwa']), ('advanced_total_capital', total, c['advanced_rwa']), ('tier1_leverage', tier1, c['average_assets']),
                      ('slr', tier1, exposure), ('tlac_rwa', c['eligible_tlac'], rwa), ('tlac_leverage', c['eligible_tlac'], exposure), ('ltd_rwa', c['eligible_ltd'], rwa),
                      ('ltd_leverage', c['eligible_ltd'], exposure), ('tce_ta', c['tangible_common_equity'], c['tangible_assets'])]
        for label, numerator, denominator in components:
            denominator = denominator if denominator is not None and denominator > 0 else None
            rule = c['requirements'].get(label); ratio = numerator / denominator if denominator is not None else None
            required = rule['minimum'] + rule['buffer'] if rule else None; management = required + rule['management'] if rule else None
            head = numerator - required * denominator if required is not None and denominator is not None else None
            management_head = numerator - management * denominator if management is not None and denominator is not None else None
            status = 'unavailable' if denominator is None else 'unconfigured' if rule is None else 'breach' if head < 0 else 'below_management_target' if management_head < 0 else 'pass'
            metrics.append({k: c[k] for k in ('entity', 'currency', 'scenario', 'period')} | dict(metric=label, numerator=numerator, denominator=denominator, ratio=ratio,
                           minimum=rule['minimum'] if rule else None, buffer=rule['buffer'] if rule else None, required_ratio=required, management_target=management,
                           headroom=head, management_headroom=management_head, headroom_bp=(ratio - required) * 10000 if ratio is not None and required is not None else None, status=status))
    return audits, contributions, capitals, metrics


def verify(raw, result, check=lambda: False):
    check()
    _require(type(raw) is dict and raw.get('schema') == SCHEMA and type(result) is dict and result.get('schema') == SCHEMA, 'hedge treasury schema')
    _require(type(result.get('hedge')) is dict and verify_hedge(raw['hedge'], result['hedge'], check)['verified'], 'embedded hedge result differs')
    hedge = replay_hedge(raw['hedge'], check); check()
    interim = raw.get('interim_report', False); _require(type(interim) is bool, 'explicit interim selection')
    k = len(hedge['daily']) - 1
    if interim:
        dates = [date.fromordinal(d['ordinal']).isoformat() for d in hedge['daily']]
        _require(raw['treasury']['as_of'] in dates[:-1], 'interim report date must be a hedge path node before the complete tail')
        k = dates.index(raw['treasury']['as_of'])
    last = hedge['daily'][k]['ordinal']; closing = date.fromordinal(last).isoformat()
    multiplier = _money(raw['amount_multiplier']); _require(multiplier > 0., 'positive multiplier')
    _require(raw['treasury']['as_of'] == closing and not any(raw['treasury'][k_] for k_ in ('capital', 'curves', 'positions', 'capital_deltas', 'strategy_units')), 'treasury closing selection')
    policies = [_typed_policy(p) for p in raw['policies']]; _require(1 <= len(policies) <= 16, 'policy count')
    scenario = raw['hedge']['derivative']['scenario_id']
    for p in policies:
        p.setdefault('cash_flow_hedge_aoci_eligible', False)
        _require(p['scenario'] == scenario and p['account'] == raw['account'] and p['capital']['period'] == closing, 'policy scope')
    eligible = {p['cash_flow_hedge_aoci_eligible'] for p in policies}; _require(len(eligible) == 1, 'one eligibility decision')
    expected_rows = rows(_typed(raw['hedge']), hedge, k)
    for r in expected_rows: r['account'] = raw['account']
    audits, contributions, capitals, metrics = capital(expected_rows, policies, multiplier)
    expected = dict(schema=SCHEMA, closing_ordinal=last, closing_date=closing, scenario=scenario, account=raw['account'], mapping=deepcopy(raw['mapping']),
                    rows=expected_rows, policies=deepcopy(raw['policies']), amount_multiplier=multiplier, cash_flow_hedge_aoci_eligible=eligible.pop(),
                    closing_capital_selected=not interim, interim_capital=interim, ftp_selected=False, accounting_standard_approval=False,
                    regulatory_credit_rwa_approval=False, empirical_validation=False, persisted_journal_verified=False)
    actual = {k: v for k, v in result.items() if k not in ('hedge', 'capital', 'disclosure')}
    _require(set(actual) == set(expected) and type(result.get('disclosure')) is str and result['disclosure'].strip() != '', 'hedge treasury fields')
    _compare(expected, actual)
    cap = result['capital']; _require(type(cap) is dict, 'capital result')
    _compare(audits, cap['capital_bridge'], 'capital_bridge'); _compare(contributions, cap['rwa_contributions'], 'rwa_contributions')
    _compare(capitals, cap['prepared_specification']['capital'], 'prepared capital'); _compare(metrics, cap['capital_metrics'], 'capital_metrics')
    _require(cap['ftp_positions'] == [] and cap['ftp_reconciliation'] == [] and cap['capital_limits'] == [] and cap['schema'] == 'treasury-1'
             and cap['as_of'] == closing, 'closing capital cannot infer FTP or limits')
    check()
    return dict(verified=True, hedge_journal_replayed=True, closing_rows_recomputed=True, closing_capital_recomputed=True,
                cash_flow_hedge_aoci_eligible=expected['cash_flow_hedge_aoci_eligible'], interim_capital=interim, ftp_selected=False,
                accounting_standard_approval=False, regulatory_credit_rwa_approval=False, empirical_validation=False, persisted_journal_verified=False)
