"""Balanced, replayable simulation journals. Signed balances are debit-positive.

This module knows no pricing or risk models. Entries balance independently by
entity/currency bucket; consumers must not insert an unexplained balancing plug.
"""
from collections import defaultdict
import math
import polars as pl

SCHEMA = {"transaction_id": pl.String, "scenario": pl.String, "day": pl.Int64,
          "account": pl.String, "event": pl.String, "gl_account": pl.String,
          "instrument_id": pl.String, "debit": pl.Float64, "credit": pl.Float64}


def replay(rows):
    """Validate raw persisted entry lines and reconstruct every subledger balance."""
    balances, totals = defaultdict(float), defaultdict(list)
    for r in rows:
        debit, credit = r["debit"], r["credit"]
        if not all(math.isfinite(v) and v >= 0 for v in (debit, credit)) or (debit and credit):
            raise ValueError("invalid journal debit/credit")
        key = (r["scenario"], r["account"], r["gl_account"], r["instrument_id"])
        value = debit-credit
        balances[key] += value
        totals[r["transaction_id"], r["scenario"], r["account"]].append(value)
    for key, amounts in totals.items():
        if abs(math.fsum(amounts)) > 1e-9 * max(1., sum(abs(v) for v in amounts)):
            raise ArithmeticError(f"unbalanced journal transaction: {key}")
    return dict(balances)


class Journal:
    def __init__(self, scenario):
        self.scenario = scenario
        self.rows = []
        self.balances = defaultdict(float)
        self.sequence = 0

    def post(self, day, account, event, changes):
        changes = {key: float(v) for key, v in changes.items() if v}
        if not changes:
            return
        values = list(changes.values())
        if not all(math.isfinite(v) for v in values):
            raise ValueError("nonfinite journal entry")
        if abs(math.fsum(values)) > 1e-9 * max(1., sum(abs(v) for v in values)):
            raise ArithmeticError(f"unbalanced event {event}: {changes}")
        self.sequence += 1
        tx = f"{self.scenario}:{self.sequence}"
        for (gl, instrument), value in changes.items():
            self.balances[account, gl, instrument] += value
            self.rows.append(dict(transaction_id=tx, scenario=self.scenario, day=day,
                                 account=account, event=event, gl_account=gl, instrument_id=instrument,
                                 debit=max(value, 0.), credit=max(-value, 0.)))

    def verify(self, expected):
        for key in set(expected) | set(self.balances):
            got, want = self.balances.get(key, 0.), expected.get(key, 0.)
            if not math.isclose(got, want, rel_tol=1e-9, abs_tol=1e-8):
                raise ArithmeticError(f"subledger mismatch {key}: {got} != {want}")

    def close(self):
        reconstructed = replay(self.rows)
        for (account, gl, instrument), value in self.balances.items():
            if not math.isclose(reconstructed.get((self.scenario, account, gl, instrument), 0.), value, rel_tol=1e-12, abs_tol=1e-9):
                raise ArithmeticError("journal replay failed")
        balances = [dict(scenario=self.scenario, account=a, gl_account=g, instrument_id=i, balance=v)
                    for (a, g, i), v in sorted(self.balances.items())]
        return self.rows, balances


def statements(trial, accounts, scenarios=()):
    """Derive closing statements solely from independently replayed GL balances."""
    funding = {(r['scenario'], r['account'], r['instrument_id']) for r in trial if r['gl_account'] == 'funding_principal'}
    totals = defaultdict(lambda: dict(assets=0., liabilities=0., equity=0., cash=0., restricted_cash=0., intercompany_assets=0., intercompany_liabilities=0.))
    for scenario in scenarios:
        for account in accounts:
            totals[scenario, account]  # retain zero-balance accounts without fabricating entries
    for r in trial:
        key = (r['scenario'], r['account'])
        t, gl, value = totals[key], r['gl_account'], r['balance']
        if gl in {'cash', 'restricted_cash'}:
            t[gl] += value
        if gl.startswith('pnl:') or gl in {'opening_equity', 'oci', 'equity_distributions'}:
            t['equity'] -= value
        elif gl in {'funding_principal', 'secured_funding', 'received_margin', 'intercompany_payable'}:
            t['liabilities'] -= value
        elif gl in {'book_adjustment', 'accrued_interest'} and (*key, r['instrument_id']) in funding:
            t['liabilities'] -= value
        elif gl == 'derivative_value' and value < 0:
            t['liabilities'] -= value
        else:
            t['assets'] += value
        if gl == 'intercompany_receivable':
            t['intercompany_assets'] += value
        elif gl == 'intercompany_payable':
            t['intercompany_liabilities'] -= value
    rows, consolidated = [], defaultdict(lambda: dict(assets=0., liabilities=0., equity=0., cash=0., restricted_cash=0.))
    for (scenario, account), t in totals.items():
        currency = accounts[account].currency
        error = t['assets']-t['liabilities']-t['equity']
        if abs(error) > 1e-8*max(1., abs(t['assets']), abs(t['liabilities'])):
            raise ArithmeticError('journal-derived financial statements do not balance')
        rows.append(dict(scenario=scenario, account=account, currency=currency, **t, reconciliation_error=error))
        group = consolidated[scenario, currency]
        for field in group:
            group[field] += t[field]
        group['assets'] -= t['intercompany_assets']
        group['liabilities'] -= t['intercompany_liabilities']
    return rows, [dict(scenario=s, currency=c, **r) for (s, c), r in consolidated.items()]
