"""Independent replay of derivative-treasury-1: reporting-date rows, GL mapping and capital; never production.

The embedded derivative result is checked by the existing raw derivative reference; rows are rebuilt
from the independently derived journal through the selected reporting node; capital amounts and the
thirteen ratios are recomputed from the rows and supplied policies with the shared bridge rules.
"""
from copy import deepcopy
from datetime import date
import math
from .derivative_verification import _derive, verify_derivative_accounting
from .hedge_accounting_reference import _compare, _money, _require
from .hedge_treasury_reference import GL, ZERO_ROW, _typed_policy, capital

SCHEMA = 'derivative-treasury-1'


def rows(raw, derivative, k=-1):
    """Bridge rows through derivative day k from the independently derived journal and opening anchors."""
    balances = {}
    report = derivative['daily'][k]['ordinal']
    def add(account, instrument, value):
        balances[(account, instrument)] = _money(balances.get((account, instrument), 0.) + value)
    for intent in derivative['intents']:
        if intent['ledger'] != 'financial' or intent['ordinal'] > report: continue
        for p in intent['postings']: add(p['account'], p['position_id'] or p['bucket'] or 'book', p['debit_amount'])
    o = raw['opening']
    for account, value in (('cash', o['cash']), ('posted_vm_asset', o['posted_vm_asset']), ('received_vm_liability', -o['received_vm_liability']),
                           ('posted_im_asset', o['posted_im_asset']), ('opening_equity', -o['equity_credit_balance'])):
        add(account, 'book', value)
    for t in o['trades']:
        add('derivative_asset', t['position_id'], max(t['clean_value'], 0.)); add('derivative_liability', t['position_id'], -max(-t['clean_value'], 0.))
    closing = {b['account']: b['debit_balance'] for b in derivative['daily'][k]['financial_trial_balance']}
    totals = {}
    for (account, _), v in balances.items(): totals[account] = totals.get(account, 0.) + v
    for account, v in closing.items():
        _require(math.isclose(totals.get(account, 0.), v, rel_tol=1e-12, abs_tol=1e-9), 'closing account does not reconcile postings')
    mapped = {}
    for (account, instrument), v in balances.items():
        if account.startswith('memo_'): continue
        _require(account in GL and not account.startswith('hedged') and account != 'cash_flow_hedge_oci', 'derivative journal account')
        gl = GL[account]; mapped[(gl, instrument)] = _money(mapped.get((gl, instrument), 0.) + v)
    scenario = raw['scenario_id']
    out = [dict(scenario=scenario, account=None, gl_account=gl, instrument_id=instrument, balance=v) for (gl, instrument), v in sorted(mapped.items()) if abs(v) > ZERO_ROW]
    _require(not any(r['gl_account'].startswith('derivative_unpaid') for r in out), 'unpaid rights at the reporting date')
    return out


def verify(raw, result, check=lambda: False):
    check()
    _require(type(raw) is dict and raw.get('schema') == SCHEMA and type(result) is dict and result.get('schema') == SCHEMA, 'derivative treasury schema')
    _require(type(result.get('derivative')) is dict and verify_derivative_accounting(raw['derivative'], result['derivative'])['verified'] is True, 'embedded derivative result differs')
    derivative = _derive(raw['derivative']); check()
    interim = raw.get('interim_report', False); _require(type(interim) is bool, 'explicit interim selection')
    k = len(derivative['daily']) - 1
    if interim:
        dates = [date.fromordinal(d['ordinal']).isoformat() for d in derivative['daily']]
        _require(raw['treasury']['as_of'] in dates[:-1], 'interim report date must be a derivative path node before the complete tail')
        k = dates.index(raw['treasury']['as_of'])
    last = derivative['daily'][k]['ordinal']; closing = date.fromordinal(last).isoformat()
    multiplier = _money(raw['amount_multiplier']); _require(multiplier > 0., 'positive multiplier')
    _require(raw['treasury']['as_of'] == closing and not any(raw['treasury'][k_] for k_ in ('capital', 'curves', 'positions', 'capital_deltas', 'strategy_units')), 'treasury closing selection')
    policies = [_typed_policy(p, require_aoci=False) for p in raw['policies']]; _require(1 <= len(policies) <= 16, 'policy count')
    scenario = raw['derivative']['scenario_id']
    for p in policies:
        p.setdefault('cash_flow_hedge_aoci_eligible', False)
        _require(p['scenario'] == scenario and p['account'] == raw['account'] and p['capital']['period'] == closing, 'policy scope')
    expected_rows = rows(raw['derivative'], derivative, k)
    for r in expected_rows: r['account'] = raw['account']
    audits, contributions, capitals, metrics = capital(expected_rows, policies, multiplier)
    expected = dict(schema=SCHEMA, closing_ordinal=last, closing_date=closing, scenario=scenario, account=raw['account'], mapping=deepcopy(raw['mapping']),
                    rows=expected_rows, policies=deepcopy(raw['policies']), amount_multiplier=multiplier, closing_capital_selected=not interim,
                    interim_capital=interim, ftp_selected=False, hedge_accounting_applied=False, accounting_standard_approval=False,
                    regulatory_credit_rwa_approval=False, empirical_validation=False, persisted_journal_verified=False)
    actual = {k_: v for k_, v in result.items() if k_ not in ('derivative', 'capital', 'disclosure')}
    _require(set(actual) == set(expected) and type(result.get('disclosure')) is str and result['disclosure'].strip() != '', 'derivative treasury fields')
    _compare(expected, actual)
    cap = result['capital']; _require(type(cap) is dict, 'capital result')
    _compare(audits, cap['capital_bridge'], 'capital_bridge'); _compare(contributions, cap['rwa_contributions'], 'rwa_contributions')
    _compare(capitals, cap['prepared_specification']['capital'], 'prepared capital'); _compare(metrics, cap['capital_metrics'], 'capital_metrics')
    _require(cap['ftp_positions'] == [] and cap['ftp_reconciliation'] == [] and cap['capital_limits'] == [] and cap['schema'] == 'treasury-1'
             and cap['as_of'] == closing, 'closing capital cannot infer FTP or limits')
    check()
    return dict(verified=True, derivative_journal_replayed=True, derivative_closing_capital_recomputed=True, closing_rows_recomputed=True,
                hedge_accounting_applied=False, interim_capital=interim, ftp_selected=False, accounting_standard_approval=False,
                regulatory_credit_rwa_approval=False, empirical_validation=False, persisted_journal_verified=False)
