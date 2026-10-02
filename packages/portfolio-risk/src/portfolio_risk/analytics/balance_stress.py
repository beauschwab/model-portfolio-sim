"""Explicit, deterministic balance-sheet stress research model.

Amounts are in one user-chosen unit per currency (examples: USD millions).
No prices, risk-neutral paths, calibrated probabilities or regulatory rules are
inferred from the pricing books. Daily transitions precede policy decisions;
policies see only current state and execute after their stated delay.
"""
from __future__ import annotations

from collections import defaultdict
from dataclasses import asdict, dataclass, fields, MISSING
from copy import deepcopy
import math
from typing import get_type_hints
from functools import lru_cache
from .journal import Journal, statements, SCHEMA as JOURNAL_SCHEMA
from .balance_rules import headroom, validation_status, liquidity_components, RULESET

import polars as pl

MODEL_VERSION = "balance-stress-2"


@dataclass
class Account:
    id: str
    entity: str
    currency: str
    cash: float
    equity: float
    cash_floor: float = 0.0
    cet1_floor: float = 0.07
    leverage_floor: float = 0.04
    capital_deductions: float = 0.0
    include_aoci: bool = True
    annual_fees: float = 0.0
    annual_costs: float = 0.0
    tax_rate: float = 0.0
    annual_dividends: float = 0.0
    lcr_floor: float = 0.0
    nsfr_floor: float = 0.0
    htm_asset_limit: float = 1.0


@dataclass
class Position:
    id: str
    account: str
    kind: str  # loan, security, deposit, funding, repo, reverse_repo
    balance: float
    rate: float = 0.0
    payment_interval_days: int = 30
    floating_beta: float = 0.0
    maturity_day: int = 0  # zero = beyond horizon; ACT/365, no calendar adjustment
    classification: str = "ac"  # ac, afs, htm, trading
    risk_weight: float = 1.0
    asf_weight: float = 0.0
    rsf_weight: float = 1.0
    lcr_outflow_weight: float = 0.0
    hqla_weight: float = 0.0
    duration: float = 0.0
    allowance: float = 0.0
    annual_pd: float = 0.0
    lgd: float = 0.45
    watch_fraction: float = 0.0
    watch_pd_multiplier: float = 3.0
    migration_rate: float = 0.0
    recovery_days: int = 90
    commitment: float = 0.0  # undrawn, not total facility
    draw_fraction: float = 0.0  # fraction of original undrawn per stress unit, over 30 days
    monthly_runoff: float = 0.0
    uninsured_fraction: float = 0.0
    concentration: float = 0.0
    operational_fraction: float = 0.0
    digital_fraction: float = 0.0
    rollover: float = 0.0
    collateral_pool: str = ""
    eligible_fraction: float = 0.0
    encumbered_fraction: float = 0.0
    haircut: float = 0.0
    source_id: str = ""
    book_adjustment: float = 0.0
    opening_accrued: float = 0.0
    collateral_position: str = ""
    pledged_face: float = 0.0
    start_day: int = 0  # forward origination exchanges cash for carrying value
    hqla_level: str = 'level1'
    lcr_inflow_weight: float = 0.0
    opening_market_price: float = 1.0  # clean value per unit principal, separate from cost basis


@dataclass
class NettingSet:
    id: str
    account: str
    counterparty: str
    fair_value: float = 0.0  # signed legal-netting-set value
    posted_margin: float = 0.0
    received_margin: float = 0.0
    stress_loss: float = 0.0  # signed-value decrement per market shock unit
    margin_threshold: float = 0.0
    margin_delay: int = 1
    annual_pd: float = 0.0
    lgd: float = 0.6
    wrong_way_multiplier: float = 1.0
    risk_weight: float = 1.0


@dataclass
class Policy:
    id: str
    account: str
    kind: str  # sell, secured_funding, transfer, cut_dividend
    trigger_cash: float
    limit: float  # cumulative cash proceeds/transfer; dividends use 1
    delay_days: int = 1
    position: str = ""
    destination: str = ""
    execution_cost: float = 0.0
    funding_rate: float = 0.05
    allow_htm_sale: bool = False
    htm_sale_limit: float = 0.0  # cumulative face; explicit internal policy, not accounting rule
    funding_tenor_days: int = 30


@dataclass
class Scenario:
    name: str
    start_day: int = 1
    rate_shift: float = 0.0
    spread_shift: float = 0.0
    deposit_flight: float = 0.0  # additional monthly hazard input
    pd_multiplier: float = 1.0
    migration_multiplier: float = 1.0
    lgd_add: float = 0.0
    draw_multiplier: float = 0.0
    rollover_loss: float = 0.0
    haircut_add: float = 0.0
    market_shock: float = 0.0
    outage_days: int = 0  # delays policy execution, not contractual outflows


@dataclass
class Cashflow:
    position: str
    day: int
    principal: float = 0.0
    cash_interest: float = 0.0
    accrual_interest: float = 0.0
    book_amortization: float = 0.0
    scenario: str = "all"


@lru_cache(maxsize=16)
def _field_types(cls):
    """Dataclass schemas are process-static; resolve postponed annotations once."""
    return get_type_hints(cls)


def _decode(cls, raw):
    if not isinstance(raw, dict):
        raise ValueError(f"{cls.__name__} must be an object")
    types = _field_types(cls)
    if set(raw) - set(types):
        raise ValueError(f"unknown {cls.__name__} fields: {sorted(set(raw)-set(types))}")
    try:
        obj = cls(**raw)
    except TypeError as exc:
        raise ValueError(str(exc)) from exc
    for name, typ in types.items():
        v = getattr(obj, name)
        if typ is float:
            if isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) or abs(v) > 1e15:
                raise ValueError(f"{cls.__name__}.{name} must be a finite number of magnitude <= 1e15")
        elif typ is int:
            if type(v) is not int or not 0 <= v <= 36500:
                raise ValueError(f"{name} must be an integer in [0, 36500]")
        elif typ is bool and type(v) is not bool:
            raise ValueError(f"{name} must be boolean")
        elif typ is str and (not isinstance(v, str) or len(v) > 120):
            raise ValueError(f"{name} must be a short string")
    return obj


def _range(obj, names, lo=0., hi=1.):
    for name in names.split():
        if not lo <= getattr(obj, name) <= hi:
            raise ValueError(f"{type(obj).__name__}.{name} must be in [{lo}, {hi}]")


def validate(raw, *, max_positions=2000, max_work=3_000_000):
    """Validate at submission and again at execution; returns detached typed inputs."""
    from ..core.quant_native import enabled
    if enabled():
        from .ledger_native import validate_spec
        return validate_spec(raw,max_positions=max_positions,max_work=max_work)
    if not isinstance(raw, dict) or set(raw) - {"version", "horizon_days", "accounts", "positions", "netting_sets", "policies", "scenarios", "reverse_severities", "cashflows", "provenance", "ruleset"}:
        raise ValueError("unknown stress specification fields")
    if raw.get("version") != MODEL_VERSION:
        raise ValueError(f"version must be {MODEL_VERSION}")
    days = raw.get("horizon_days", 360)
    if type(days) is not int or not 30 <= days <= 1080:
        raise ValueError("horizon_days must be in [30, 1080]")
    out = {"horizon_days": days}
    for key, cls, cap in [("accounts", Account, 50), ("positions", Position, max_positions), ("netting_sets", NettingSet, 500), ("policies", Policy, 100), ("scenarios", Scenario, 8)]:
        items = raw.get(key, [])
        if not isinstance(items, list) or len(items) > cap:
            raise ValueError(f"{key} must be a list of at most {cap} items")
        out[key] = [_decode(cls, x) for x in items]
        ids = [getattr(x, "id", getattr(x, "name", "")) for x in out[key]]
        if len(ids) != len(set(ids)) or any(not x.strip() for x in ids):
            raise ValueError(f"{key} IDs must be nonempty and unique")
    accounts = {x.id: x for x in out["accounts"]}
    positions = {x.id: x for x in out["positions"]}
    if not accounts or not out["scenarios"]:
        raise ValueError("accounts and scenarios are required")
    if len({(a.entity, a.currency) for a in accounts.values()}) != len(accounts):
        raise ValueError("one account per legal entity and currency is required")
    for a in accounts.values():
        if not a.entity.strip() or not a.currency.strip():
            raise ValueError("entity and currency are required")
        _range(a, "cash cash_floor capital_deductions annual_fees annual_costs annual_dividends", 0, 1e15)
        _range(a, "cet1_floor leverage_floor tax_rate")
        _range(a, "lcr_floor nsfr_floor", 0, 10)
        _range(a, "htm_asset_limit")
    for p in out["positions"]:
        if p.account not in accounts or p.kind not in {"loan", "security", "deposit", "funding", "repo", "reverse_repo"}:
            raise ValueError("invalid position account or kind")
        if p.start_day > days or (p.start_day and (p.allowance or p.opening_accrued or p.encumbered_fraction or p.collateral_position)):
            raise ValueError('forward originations cannot carry opening allowance, accrual or collateral')
        if p.classification not in {"ac", "afs", "htm", "trading"} or (p.kind != "security" and p.classification != "ac"):
            raise ValueError("classification applies only to securities")
        _range(p, "balance commitment allowance", 0, 1e15)
        _range(p, "floating_beta asf_weight rsf_weight lcr_outflow_weight hqla_weight annual_pd lgd watch_fraction migration_rate draw_fraction monthly_runoff uninsured_fraction concentration operational_fraction digital_fraction rollover eligible_fraction encumbered_fraction haircut")
        _range(p, "risk_weight watch_pd_multiplier", 0, 100)
        _range(p, "duration", 0, 100)
        _range(p, "rate", -1, 2)
        _range(p, 'lcr_inflow_weight')
        if p.hqla_level not in {'level1', 'level2a'}:
            raise ValueError('unsupported HQLA level')
        if p.payment_interval_days < 1:
            raise ValueError("payment_interval_days must be positive")
        if p.balance+p.book_adjustment < p.allowance or p.opening_accrued < 0:
            raise ValueError("invalid opening carrying value or accrued interest")
        _range(p, 'opening_market_price', .000001, 100.)
        if p.kind != 'security' and p.opening_market_price != 1.:
            raise ValueError('opening market price applies only to securities')
        if not p.balance and p.book_adjustment:
            raise ValueError('book adjustment requires positive principal')
        if p.allowance > p.balance or (p.kind != "loan" and (p.allowance or p.commitment or p.annual_pd or p.migration_rate)):
            raise ValueError("credit inputs apply only to loans; allowance cannot exceed balance")
        if p.eligible_fraction and (p.kind != "security" or not p.collateral_pool):
            raise ValueError("funding collateral requires a security and explicit pool ID")
        if p.collateral_position or p.pledged_face:
            collateral = positions.get(p.collateral_position)
            if (p.kind not in {'repo', 'funding'} or collateral is None or collateral.kind != 'security'
                    or collateral.account != p.account or p.pledged_face <= 0):
                raise ValueError('opening secured funding requires owned security collateral and positive pledged face')
    allocated_pledges = defaultdict(float)
    for debt in positions.values():
        if debt.collateral_position:
            allocated_pledges[debt.collateral_position] += debt.pledged_face
    for p in positions.values():
        allocated = allocated_pledges[p.id]
        if allocated > p.balance*p.encumbered_fraction+1e-8:
            raise ValueError('opening funding links exceed encumbered collateral')
    for n in out["netting_sets"]:
        if n.account not in accounts or not n.counterparty.strip():
            raise ValueError("netting set needs an account and counterparty")
        _range(n, "posted_margin received_margin stress_loss margin_threshold", 0, 1e15)
        _range(n, "annual_pd lgd")
        _range(n, "wrong_way_multiplier risk_weight", 0, 100)
    for p in out["policies"]:
        if p.account not in accounts or p.kind not in {"sell", "secured_funding", "transfer", "cut_dividend"}:
            raise ValueError("invalid policy account or kind")
        _range(p, "limit htm_sale_limit", 0, 1e15)
        _range(p, "execution_cost", 0, .99)
        _range(p, "funding_rate", 0, 2)
        if p.funding_tenor_days < 1:
            raise ValueError("funding tenor must be positive")
        if p.kind in {"sell", "secured_funding"}:
            pos = positions.get(p.position)
            if pos is None or pos.account != p.account or pos.kind != "security":
                raise ValueError("sale/funding policy must reference an owned security")
        if p.kind == "transfer":
            target = accounts.get(p.destination)
            if target is None or target.id == p.account or target.currency != accounts[p.account].currency:
                raise ValueError("transfer needs a distinct account in the same currency; no implicit FX")
    for s in out["scenarios"]:
        if s.start_day < 1:
            raise ValueError("scenario start_day must be positive")
        _range(s, "rate_shift spread_shift", -.5, .5)
        _range(s, "deposit_flight lgd_add rollover_loss haircut_add")
        _range(s, "pd_multiplier migration_multiplier draw_multiplier market_shock", 0, 20)
        if s.name == "baseline":
            raise ValueError("baseline is a reserved scenario name")
    severities = raw.get("reverse_severities", [])
    if not isinstance(severities, list) or len(severities) > 12 or any(type(x) not in (float, int) or not math.isfinite(x) or not 0 <= x <= 5 for x in severities):
        raise ValueError("reverse_severities must contain at most 12 finite values in [0, 5]")
    if severities != sorted(set(severities)):
        raise ValueError("reverse_severities must be unique and increasing")
    out["reverse_severities"] = severities
    flow_raw = raw.get("cashflows", [])
    if not isinstance(flow_raw, list) or len(flow_raw) > 250000:
        raise ValueError("cashflows must be a bounded list")
    flows = [_decode(Cashflow, x) for x in flow_raw]
    for f in flows:
        if f.position not in positions or not 1 <= f.day <= days or f.scenario not in {"all", "baseline", *[s.name for s in out['scenarios']]}:
            raise ValueError("invalid cashflow position, date or scenario")
        if f.principal < 0:
            raise ValueError("principal repayment cannot be negative")
        if f.day < positions[f.position].start_day:
            raise ValueError('cashflow precedes origination')
    if len({(f.position, f.day, f.scenario) for f in flows}) != len(flows):
        raise ValueError("duplicate dated cashflow")
    grouped_flows = defaultdict(list)
    for f in flows:
        grouped_flows[f.position].append(f)
    for pid, position_flows in grouped_flows.items():
        labels = {f.scenario for f in position_flows}
        if 'all' in labels and len(labels) > 1:
            raise ValueError('all-scenario cashflows cannot overlap scenario-specific schedules')
        if 'all' not in labels and labels != {'baseline', *[s.name for s in out['scenarios']]}:
            raise ValueError('scheduled positions require baseline and every scenario')
        for label in labels:
            if sum(f.principal for f in position_flows if f.scenario == label) > positions[pid].balance+1e-7:
                raise ValueError('scheduled principal exceeds opening principal')
        if positions[pid].commitment:
            raise ValueError('scheduled drawn facilities require a separate position for future draws')
    out['cashflows'] = flows
    out['ruleset'] = raw.get('ruleset', RULESET)
    if out['ruleset'] != RULESET:
        raise ValueError("unsupported ruleset; no inferred regulatory certification")
    runs = 1 + len(out["scenarios"]) * (1 + len(severities))
    if days * (len(positions) + len(out["netting_sets"]) + len(accounts) + len(out["policies"])) * runs > max_work:
        raise ValueError("stress work budget exceeded; reduce cohorts, horizon or scenario grid")
    # Opening equity is supplied, never an implicit plug.
    for a in accounts.values():
        assets, liabilities = a.cash, 0.
        for p in out["positions"]:
            if p.account == a.id and not p.start_day:
                if p.kind in {"loan", "security", "reverse_repo"}:
                    carrying = p.balance + p.book_adjustment
                    if p.kind == 'security' and p.classification in {'afs', 'trading'}:
                        carrying = p.balance*p.opening_market_price
                    assets += carrying - p.allowance + p.opening_accrued
                else:
                    liabilities += p.balance + p.book_adjustment + p.opening_accrued
        for n in out["netting_sets"]:
            if n.account == a.id:
                assets += max(n.fair_value, 0) + n.posted_margin
                liabilities += max(-n.fair_value, 0) + n.received_margin
        if abs(assets - liabilities - a.equity) > 1e-8 * max(1, assets, liabilities):
            raise ValueError(f"opening balance sheet does not reconcile for {a.id}")
    return out


def contract():
    """Machine-readable field defaults for the explicit JSON editor."""
    return {cls.__name__: {f.name: {"type": str(f.type), "required": f.default is MISSING,
                                  "default": None if f.default is MISSING else f.default}
                          for f in fields(cls)}
            for cls in (Account, Position, NettingSet, Policy, Scenario, Cashflow)}


def _simulate(spec, scenario, severity=1., detail=True, journal_factory=Journal):
    accounts = {x.id: deepcopy(x) for x in spec["accounts"]}
    positions = deepcopy(spec["positions"])
    netting = deepcopy(spec["netting_sets"])
    pos_by_id = {p.id: p for p in positions}
    commitments = {p.id: p.commitment for p in positions}
    market = {p.id: p.opening_market_price for p in positions}
    marks = defaultdict(float)
    aoci = defaultdict(float)
    extra_assets, extra_debt = defaultdict(float), defaultdict(float)
    funding_interest = defaultdict(float)
    accrued = defaultdict(float)
    journal = journal_factory(scenario.name)
    claims = {}
    claim_maturities, claims_by_position, opening_links = defaultdict(list), defaultdict(list), defaultdict(list)
    for p in positions:
        if p.collateral_position:
            opening_links[p.collateral_position].append(p)
    recovery_claims = defaultdict(float)
    intercompany = defaultdict(float)
    restricted = defaultdict(float)
    flow_by_day = defaultdict(list)
    scheduled_ids = {f.position for f in spec['cashflows']}
    schedule_balance = {p.id: p.balance for p in positions}
    forward = {p.id: (p.balance, p.book_adjustment) for p in positions if p.start_day}
    for p in positions:
        if p.start_day:
            p.balance = p.book_adjustment = 0.
    for f in spec['cashflows']:
        if f.scenario in {'all', scenario.name}:
            flow_by_day[f.day].append(f)
    for p in positions:
        accrued[p.id] = p.opening_accrued
    margin_targets = defaultdict(list)
    recoveries, pending = defaultdict(list), defaultdict(list)
    used, htm_sold = defaultdict(float), defaultdict(float)
    scheduled = set()
    ledger, path, actions, breaches, exposures = [], [], [], [], []
    first = {}
    minimum_cash = {a: accounts[a].cash for a in accounts}
    max_error = 0.
    aggregate = defaultdict(lambda: [0., 0., 0.])
    earnings_by = defaultdict(float)
    opening_equity = {a.id: a.equity for a in accounts.values()}

    for p in positions:
        if p.kind == 'security' and p.classification == 'afs':
            marks[p.id] = p.balance*(market[p.id]-1)-p.book_adjustment
            aoci[p.account] += marks[p.id]
    # Opening supplied equity already includes opening AOCI. Classify it instead
    # of recognizing an opening unrealized mark as current-period income.
    for a in accounts.values():
        opening_equity[a.id] -= aoci[a.id]

    def post(day, account, event, cash=0., earnings=0., oci=0., changes=None, instrument=""):
        entries = defaultdict(float, changes or {})
        entries['cash', ''] += cash
        earnings_gl = 'equity_distributions' if event == 'dividend' else 'pnl:'+event
        entries[earnings_gl, instrument] -= earnings
        entries['oci', ''] -= oci
        journal.post(day, account, event, entries)
        earnings_by[account, earnings_gl, instrument] -= earnings
        a = accounts[account]
        a.cash += cash
        a.equity += earnings + oci
        aoci[account] += oci
        key = (day, account, event)
        row = aggregate[key]
        row[0] += cash
        row[1] += earnings
        row[2] += oci

    def principal_key(p):
        return ('asset_principal' if p.kind in {'loan', 'security', 'reverse_repo'} else 'funding_principal', p.id)

    for a in accounts.values():
        opening = {('cash', ''): a.cash, ('opening_equity', ''): -opening_equity[a.id], ('oci', ''): -aoci[a.id]}
        for p in positions:
            if p.account != a.id:
                continue
            sign = 1 if p.kind in {'loan', 'security', 'reverse_repo'} else -1
            opening[principal_key(p)] = sign*p.balance
            opening['book_adjustment', p.id] = sign*p.book_adjustment
            opening['allowance', p.id] = -p.allowance
            opening['accrued_interest', p.id] = sign*accrued[p.id]
            if p.kind == 'security' and p.classification in {'afs', 'trading'}:
                opening['fair_value_adjustment', p.id] = p.balance*(market[p.id]-1)-p.book_adjustment
        for n in netting:
            if n.account == a.id:
                opening['derivative_value', n.id] = n.fair_value
                opening['posted_margin', n.id] = n.posted_margin
                opening['received_margin', n.id] = -n.received_margin
        journal.post(0, a.id, 'opening', opening)

    def log_action(day, p, status, amount=0., reason=""):
        if detail:
            actions.append(dict(scenario=scenario.name, day=day, policy=p.id, account=p.account,
                                destination=p.destination, kind=p.kind, status=status, amount=amount, reason=reason))

    def observe(day):
        nonlocal max_error
        expected = dict(earnings_by)
        for a in accounts.values():
            expected[a.id, 'cash', ''] = a.cash
            expected[a.id, 'restricted_cash', ''] = restricted[a.id]
            expected[a.id, 'opening_equity', ''] = -opening_equity[a.id]
            expected[a.id, 'oci', ''] = -aoci[a.id]
        for p in positions:
            sign = 1 if p.kind in {'loan', 'security', 'reverse_repo'} else -1
            expected[p.account, *principal_key(p)] = sign*p.balance
            expected[p.account, 'book_adjustment', p.id] = sign*p.book_adjustment
            expected[p.account, 'allowance', p.id] = -p.allowance
            expected[p.account, 'accrued_interest', p.id] = sign*accrued[p.id]
            if p.kind == 'security' and p.classification in {'afs', 'trading'}:
                expected[p.account, 'fair_value_adjustment', p.id] = p.balance*(market[p.id]-1)-p.book_adjustment
        for n in netting:
            expected[n.account, 'derivative_value', n.id] = n.fair_value
            expected[n.account, 'posted_margin', n.id] = n.posted_margin
            expected[n.account, 'received_margin', n.id] = -n.received_margin
        for (account, cid), value in recovery_claims.items():
            expected[account, 'recovery_receivable', cid] = value
        expected.update(intercompany)
        for cid, claim in claims.items():
            expected[claim['account'], 'secured_funding', cid] = -claim['balance']
        journal.verify(expected)
        if detail and (day == 0 or day % 30 == 0 or day == spec["horizon_days"]):
            for p in positions:
                exposures.append(dict(scenario=scenario.name, day=day, account=p.account, position=p.id,
                                      kind=p.kind, balance=p.balance, allowance=p.allowance,
                                      watch_fraction=p.watch_fraction, commitment=p.commitment,
                                      market_factor=market[p.id], encumbered_fraction=p.encumbered_fraction,
                                      accrued_interest=accrued[p.id]))
        for a in accounts.values():
            asset, liability = a.cash + restricted[a.id] + extra_assets[a.id], extra_debt[a.id]
            rwa, hqla, outflow, asf, rsf, collateral, undrawn = 0., max(a.cash, 0.), 0., max(a.equity, 0.), 0., 0., 0.
            level2a, inflow = 0., 0.
            for p in positions:
                if p.account != a.id:
                    continue
                if p.kind in {"loan", "security", "reverse_repo"}:
                    carrying = p.balance + p.book_adjustment - p.allowance
                    if p.kind == "security" and p.classification in {"afs", "trading"}:
                        carrying = p.balance*market[p.id]
                    asset += carrying
                    asset += accrued[p.id]
                    rwa += max(carrying, 0.) * p.risk_weight * (1 + p.watch_fraction)
                    rsf += max(carrying, 0.) * p.rsf_weight
                    free = p.balance * max(0., 1-p.encumbered_fraction) * market[p.id]
                    if p.hqla_level == 'level2a':
                        level2a += free*p.hqla_weight
                    else:
                        hqla += free*p.hqla_weight
                    inflow += p.balance*p.lcr_inflow_weight
                    collateral += p.balance * max(0., p.eligible_fraction-p.encumbered_fraction) * market[p.id] * (1-min(.99, p.haircut + scenario.haircut_add * severity * (day >= scenario.start_day)))
                    undrawn += p.commitment
                    # Explicit conservative CCF=100% research proxy, disclosed.
                    rwa += p.commitment * p.risk_weight
                else:
                    liability += p.balance + p.book_adjustment
                    liability += accrued[p.id]
                    asf += p.balance * p.asf_weight
                    outflow += p.balance * p.lcr_outflow_weight
            for n in netting:
                if n.account == a.id:
                    asset += max(n.fair_value, 0.) + n.posted_margin
                    liability += max(-n.fair_value, 0.) + n.received_margin
                    rwa += max(n.fair_value + n.posted_margin - n.received_margin, 0.) * n.risk_weight
                    rsf += n.posted_margin
            # Intercompany and recovery receivables have 100% proxy RWA/RSF.
            rwa += extra_assets[a.id]
            rsf += extra_assets[a.id]
            hqla, capped_inflow, outflow = liquidity_components(hqla, level2a, outflow, inflow)
            error = asset - liability - a.equity
            max_error = max(max_error, abs(error))
            if abs(error) > 1e-8 * max(1., abs(asset), liability):
                raise ArithmeticError(f"balance sheet failed to reconcile: {a.id}, day {day}, {error}")
            cet1 = a.equity - a.capital_deductions - (0. if a.include_aoci else aoci[a.id])
            leverage_exposure = max(asset, 0.) + undrawn
            ratio = cet1 / rwa if rwa > 0 else None
            leverage = cet1 / leverage_exposure if leverage_exposure > 0 else None
            minimum_cash[a.id] = min(minimum_cash[a.id], a.cash)
            margins = headroom(a, cash=a.cash, cet1=cet1, rwa=rwa, exposure=leverage_exposure,
                hqla=hqla, outflow=outflow, asf=asf, rsf=rsf, assets=asset,
                htm=sum(p.balance+p.book_adjustment for p in positions if p.account == a.id and p.classification == 'htm'))
            checks = {metric: value < -1e-9 for metric, value in margins.items()}
            for metric, failed in checks.items():
                if failed and (a.id, metric) not in first:
                    first[a.id, metric] = day
                    breaches.append(dict(scenario=scenario.name, account=a.id, day=day, metric=metric))
            if detail and (day <= 30 or day % 30 == 0 or day == spec["horizon_days"]):
                path.append(dict(scenario=scenario.name, account=a.id, entity=a.entity, currency=a.currency,
                                 day=day, cash=a.cash, restricted_cash=restricted[a.id], cash_floor=a.cash_floor, assets=asset, liabilities=liability,
                                 equity=a.equity, aoci=aoci[a.id], cet1=cet1, rwa=rwa, cet1_ratio=ratio,
                                 leverage_ratio=leverage, usable_collateral=collateral, undrawn=undrawn,
                                 hqla_proxy=hqla, lcr_proxy=hqla/outflow if outflow > 0 else None,
                                 nsfr_proxy=asf/rsf if rsf > 0 else None, reconciliation_error=error))
                path[-1].update({metric+'_headroom': value for metric, value in margins.items()})

    observe(0)
    for day in range(1, spec["horizon_days"] + 1):
        active = day >= scenario.start_day
        stress = severity if active else 0.
        rate_shift = scenario.rate_shift * stress
        credit_mult = max(0., 1 + (scenario.pd_multiplier-1) * stress)
        migration_mult = max(0., 1 + (scenario.migration_multiplier-1) * stress)
        for account, cid, amount in recoveries.pop(day, []):
            extra_assets[account] -= amount
            recovery_claims[account, cid] -= amount
            post(day, account, "credit_recovery", cash=amount, changes={('recovery_receivable', cid): -amount}, instrument=cid)
        for cid in claim_maturities.pop(day, []):
            claim = claims[cid]
            if not claim['balance']:
                continue
            amount = claim['balance']
            p = pos_by_id[claim['position']]
            p.encumbered_fraction = max(0., p.encumbered_fraction-claim['face']/p.balance) if p.balance else 0.
            extra_debt[claim['account']] -= amount
            funding_interest[claim['account']] -= amount*claim['rate']
            claim['balance'] = claim['face'] = 0.
            post(day, claim['account'], 'secured_repayment', cash=-amount,
                 changes={('secured_funding', cid): amount}, instrument=cid)
        processed_positions = set()
        for p in positions:
            a = p.account
            if p.start_day > day:
                continue
            processed_positions.add(p.id)
            if p.start_day == day:
                p.balance, p.book_adjustment = forward[p.id]
                sign = 1 if p.kind in {'loan', 'security', 'reverse_repo'} else -1
                post(day, a, 'origination', cash=-sign*(p.balance+p.book_adjustment),
                     changes={principal_key(p): sign*p.balance, ('book_adjustment', p.id): sign*p.book_adjustment}, instrument=p.id)
                if p.kind == 'security' and p.balance:
                    market[p.id] = (p.balance+p.book_adjustment)/p.balance
            if p.kind == "security":
                target = p.opening_market_price*max(.01, 1-p.duration*(rate_shift+scenario.spread_shift*stress))
                change = p.balance * (target-market[p.id])
                market[p.id] = target
                if change and p.classification in {"afs", "trading"}:
                    if p.classification == "afs":
                        marks[p.id] += change
                        post(day, a, "security_mark", oci=change, changes={('fair_value_adjustment', p.id): change}, instrument=p.id)
                    else:
                        post(day, a, "security_mark", earnings=change, changes={('fair_value_adjustment', p.id): change}, instrument=p.id)
            if p.kind == "loan":
                draw = min(p.commitment, commitments[p.id]*p.draw_fraction*scenario.draw_multiplier*stress/30) if day < scenario.start_day+30 else 0.
                p.balance += draw
                p.commitment -= draw
                if draw:
                    post(day, a, "commitment_draw", cash=-draw, changes={principal_key(p): draw}, instrument=p.id)
                migrate = (1-p.watch_fraction) * (1-(1-min(.999999, p.migration_rate*migration_mult))**(1/365))
                p.watch_fraction += migrate
                pd = min(.999999, p.annual_pd*credit_mult*(1+p.watch_fraction*(p.watch_pd_multiplier-1)))
                default = p.balance*(1-(1-pd)**(1/365))
                lost_accrual = accrued[p.id]*default/p.balance if p.balance else 0.
                if lost_accrual:
                    accrued[p.id] -= lost_accrual
                    post(day, a, 'default_interest_writeoff', earnings=-lost_accrual,
                         changes={('accrued_interest', p.id): -lost_accrual}, instrument=p.id)
                default_adjustment = p.book_adjustment*default/p.balance if p.balance else 0.
                p.book_adjustment -= default_adjustment
                if default_adjustment:
                    post(day, a, 'default_book_writeoff', earnings=-default_adjustment,
                         changes={('book_adjustment', p.id): -default_adjustment}, instrument=p.id)
                lgd = min(1., max(0., p.lgd + scenario.lgd_add*stress))
                loss, recovery = default*lgd, default*(1-lgd)
                p.balance -= default
                p.allowance -= loss
                if recovery:
                    extra_assets[a] += recovery
                    cid = f'{p.id}:default:{day}'
                    recovery_claims[a, cid] += recovery
                    recoveries[day+max(1, p.recovery_days)].append((a, cid, recovery))
                if default:
                    changes = {principal_key(p): -default, ('allowance', p.id): loss}
                    if recovery:
                        changes['recovery_receivable', cid] = recovery
                    post(day, a, 'credit_chargeoff', changes=changes, instrument=p.id)
                target_allowance = p.balance*pd*lgd
                provision = target_allowance-p.allowance
                p.allowance = target_allowance
                if provision:
                    post(day, a, "credit_provision", earnings=-provision, changes={('allowance', p.id): -provision}, instrument=p.id)
                if default:
                    # Noncash information row; losses hit earnings through provision only.
                    if detail:
                        ledger.append(dict(scenario=scenario.name, day=day, account=a, event="default_gross",
                                           cash=0., earnings=0., aoci=0., memo_amount=default))
            rate = p.rate + p.floating_beta*rate_shift
            interest = 0. if p.id in scheduled_ids else p.balance*rate/365
            sign = 1 if p.kind in {"loan", "security", "reverse_repo"} else -1
            if interest:
                accrued[p.id] += interest
                post(day, a, "interest_income" if sign == 1 else "interest_expense", earnings=sign*interest,
                     changes={('accrued_interest', p.id): sign*interest}, instrument=p.id)
            if p.id not in scheduled_ids and (day % p.payment_interval_days == 0 or p.maturity_day == day):
                post(day, a, "interest_settlement", cash=sign*accrued[p.id],
                     changes={('accrued_interest', p.id): -sign*accrued[p.id]}, instrument=p.id)
                accrued[p.id] = 0.
            if p.kind == "deposit":
                sensitivity = (0.25 + .75*p.uninsured_fraction)*(1+p.concentration)*(1+.5*p.digital_fraction)*(1-.5*p.operational_fraction)
                flight = scenario.deposit_flight*stress*sensitivity
                competition = max(rate_shift*(1-p.floating_beta), 0.)
                runoff = p.balance*(1-(1-min(.999999, (0. if p.id in scheduled_ids else p.monthly_runoff)+flight+competition))**(1/30))
                p.balance -= runoff
                if runoff:
                    post(day, a, "deposit_withdrawal", cash=-runoff, changes={principal_key(p): runoff}, instrument=p.id)
            if p.maturity_day == day and p.id not in scheduled_ids:
                if sign == 1:
                    balance = p.balance
                    mark = marks[p.id]
                    # Maturity repays face; prior marks reverse through their original channel.
                    gain = balance*(1-market[p.id]) if p.kind == "security" and p.classification in {"afs", "trading"} else 0.
                    held = balance*p.encumbered_fraction
                    restricted[a] += held
                    adjustment = p.book_adjustment
                    changes = {principal_key(p): -balance, ('allowance', p.id): p.allowance,
                               ('book_adjustment', p.id): -adjustment, ('restricted_cash', ''): held}
                    if p.classification in {'afs', 'trading'}:
                        changes['fair_value_adjustment', p.id] = gain+adjustment
                    post(day, a, "asset_maturity", cash=balance-held, earnings=p.allowance+(gain if p.classification == "trading" else -adjustment), oci=-mark, changes=changes, instrument=p.id)
                    # Known secured claims consume their linked collateral proceeds.
                    # Unidentified opening pledges remain restricted, never free cash.
                    for cid in claims_by_position[p.id]:
                        claim = claims[cid]
                        if not claim['balance']:
                            continue
                        amount, proceeds = claim['balance'], claim['face']
                        restricted[a] -= proceeds
                        extra_debt[a] -= amount
                        funding_interest[a] -= amount*claim['rate']
                        post(day, a, 'collateral_maturity_repayment', cash=proceeds-amount,
                             changes={('restricted_cash', ''): -proceeds, ('secured_funding', cid): amount}, instrument=cid)
                        claim['balance'] = claim['face'] = 0.
                    for debt in opening_links[p.id]:
                        if not debt.pledged_face:
                            continue
                        proceeds, amount = debt.pledged_face, debt.balance
                        if debt.id not in processed_positions and debt.id not in scheduled_ids:
                            interest = debt.balance*(debt.rate+debt.floating_beta*rate_shift)/365
                            accrued[debt.id] += interest
                            post(day, a, 'interest_expense', earnings=-interest,
                                 changes={('accrued_interest', debt.id): -interest}, instrument=debt.id)
                        restricted[a] -= proceeds
                        post(day, a, 'opening_collateral_repayment', cash=proceeds-amount, earnings=debt.book_adjustment,
                             changes={('restricted_cash', ''): -proceeds, principal_key(debt): amount,
                                      ('book_adjustment', debt.id): debt.book_adjustment}, instrument=debt.id)
                        if accrued[debt.id]:
                            post(day, a, 'interest_settlement', cash=-accrued[debt.id],
                                 changes={('accrued_interest', debt.id): accrued[debt.id]}, instrument=debt.id)
                            accrued[debt.id] = 0.
                        debt.balance = debt.pledged_face = debt.book_adjustment = 0.
                    p.balance = p.allowance = marks[p.id] = 0.
                    p.book_adjustment = p.encumbered_fraction = 0.
                    p.commitment = 0.
                else:
                    rollover = min(1., max(0., p.rollover-scenario.rollover_loss*stress))
                    repay = p.balance*(1-rollover)
                    if p.collateral_position and p.balance:
                        released = p.pledged_face*(1-rollover)
                        collateral = pos_by_id[p.collateral_position]
                        collateral.encumbered_fraction = max(0., collateral.encumbered_fraction-released/collateral.balance) if collateral.balance else 0.
                        p.pledged_face -= released
                    p.balance -= repay
                    p.maturity_day = 0
                    p.rate += max(0., scenario.spread_shift*stress)
                    post(day, a, "funding_maturity", cash=-repay, changes={principal_key(p): repay}, instrument=p.id)
        for flow in flow_by_day[day]:
            p = pos_by_id[flow.position]
            sign = 1 if p.kind in {'loan', 'security', 'reverse_repo'} else -1
            # Expected schedules are conditional on surviving principal. Defaults,
            # withdrawals and sales reduce every remaining contractual flow pro rata.
            reference = schedule_balance[p.id]
            factor = p.balance/reference if reference > 1e-12 else 0.
            amount = min(flow.principal*factor, p.balance)
            schedule_balance[p.id] = max(0., reference-flow.principal)
            fraction = amount/p.balance if p.balance else 0.
            held = amount*p.encumbered_fraction
            restricted[p.account] += held
            amortization = flow.book_amortization*factor
            mark_release = -amount*(market[p.id]-1)-amortization if p.classification in {'afs', 'trading'} else 0.
            oci = mark_release if p.classification == 'afs' else 0.
            marks[p.id] += oci
            p.balance -= amount
            interest, accrual = flow.cash_interest*factor, flow.accrual_interest*factor
            p.book_adjustment += amortization
            accrued[p.id] += accrual-interest
            changes = {principal_key(p): -sign*amount, ('accrued_interest', p.id): sign*(accrual-interest),
                       ('book_adjustment', p.id): sign*amortization, ('restricted_cash', ''): held}
            if mark_release:
                changes['fair_value_adjustment', p.id] = mark_release
            post(day, p.account, 'contractual_cashflow', cash=sign*(amount+interest)-held,
                 earnings=sign*(accrual+amortization)+(mark_release if p.classification == 'trading' else 0.),
                 oci=oci, instrument=p.id, changes=changes)
            # The day's allowance was measured before contractual principal paid.
            # Release the repaid share now, including on the final reporting day.
            allowance_release = p.allowance*fraction if p.kind == 'loan' else 0.
            if allowance_release:
                p.allowance -= allowance_release
                post(day, p.account, 'repayment_allowance_release', earnings=allowance_release,
                     changes={('allowance', p.id): allowance_release}, instrument=p.id)
            for cid in claims_by_position[p.id]:
                claim = claims[cid]
                if not claim['balance'] or not fraction:
                    continue
                repay, proceeds = claim['balance']*fraction, claim['face']*fraction
                restricted[p.account] -= proceeds
                extra_debt[p.account] -= repay
                funding_interest[p.account] -= repay*claim['rate']
                claim['balance'] -= repay
                claim['face'] -= proceeds
                post(day, p.account, 'collateral_principal_repayment', cash=proceeds-repay,
                     changes={('restricted_cash', ''): -proceeds, ('secured_funding', cid): repay}, instrument=cid)
            for debt in opening_links[p.id]:
                if not fraction:
                    continue
                repay, proceeds = debt.balance*fraction, debt.pledged_face*fraction
                debt.balance -= repay
                debt.pledged_face -= proceeds
                restricted[p.account] -= proceeds
                post(day, p.account, 'opening_collateral_principal_repayment', cash=proceeds-repay,
                     changes={('restricted_cash', ''): -proceeds, principal_key(debt): repay}, instrument=debt.id)
        for n in netting:
            if day == scenario.start_day:
                loss = n.stress_loss*scenario.market_shock*severity
                n.fair_value -= loss
                if loss:
                    post(day, n.account, "derivative_mark", earnings=-loss, changes={('derivative_value', n.id): -loss}, instrument=n.id)
            # Exposure after enforceable netting and held collateral; no cross-set offset.
            exposure = max(n.fair_value-n.received_margin, 0.)
            pd = min(.999999, n.annual_pd*credit_mult*max(0., 1+(n.wrong_way_multiplier-1)*scenario.market_shock*stress))
            loss = exposure*n.lgd*(1-(1-max(0., pd))**(1/365))
            n.fair_value -= loss
            if loss:
                post(day, n.account, "counterparty_loss", earnings=-loss, changes={('derivative_value', n.id): -loss}, instrument=n.id)
            # Settle historical targets, never today's target with a nominal lag.
            margin_targets[day+n.margin_delay].append((n.id, max(-n.fair_value-n.margin_threshold, 0.)))
        for nid, target in margin_targets.pop(day, []):
            n = next(x for x in netting if x.id == nid)
            delta = target-n.posted_margin
            n.posted_margin = target
            if delta:
                post(day, n.account, "variation_margin", cash=-delta, changes={('posted_margin', n.id): delta}, instrument=n.id)
        for a in accounts.values():
            carry = funding_interest[a.id]/365
            if carry:
                post(day, a.id, "policy_funding_interest", cash=-carry, earnings=-carry)
            operating = (a.annual_fees-a.annual_costs)/365
            if operating:
                post(day, a.id, "fees_less_costs", cash=operating, earnings=operating)
            if a.annual_dividends:
                amount = a.annual_dividends/365
                post(day, a.id, "dividend", cash=-amount, earnings=-amount)
        # Policies act on observed balances; no access to a future state or scenario path.
        for p in spec["policies"]:
            target = accounts[p.destination] if p.kind == "transfer" else accounts[p.account]
            if target.cash < p.trigger_cash and used[p.id] < p.limit and p.id not in scheduled:
                pending[day+max(1, p.delay_days)].append(p)
                scheduled.add(p.id)
                log_action(day, p, "scheduled")
        for p in pending.pop(day, []):
            scheduled.remove(p.id)
            if active and severity > 0 and day < scenario.start_day+round(scenario.outage_days*severity):
                pending[day+1].append(p)
                scheduled.add(p.id)
                log_action(day, p, "delayed", reason="operational outage")
                continue
            source = accounts[p.account]
            target = accounts[p.destination] if p.kind == "transfer" else source
            need = max(0., p.trigger_cash-target.cash)
            amount = min(need, p.limit-used[p.id])
            if not amount:
                log_action(day, p, "skipped", reason="trigger cleared or limit exhausted")
                continue
            if p.kind == "cut_dividend":
                source.annual_dividends = 0.
                used[p.id] = p.limit
                log_action(day, p, "executed", reason="future dividends suspended")
                continue
            if p.kind == "transfer":
                amount = min(amount, max(0., source.cash-max(source.cash_floor, 0.)))
                extra_assets[source.id] += amount
                extra_debt[target.id] += amount
                cid = f'{p.id}:{day}'
                intercompany[source.id, 'intercompany_receivable', cid] += amount
                intercompany[target.id, 'intercompany_payable', cid] -= amount
                post(day, source.id, "intercompany_out", cash=-amount, changes={('intercompany_receivable', cid): amount}, instrument=cid)
                post(day, target.id, "intercompany_in", cash=amount, changes={('intercompany_payable', cid): -amount}, instrument=cid)
            else:
                pos = pos_by_id[p.position]
                free_face = pos.balance*(1-pos.encumbered_fraction)
                price = market[pos.id]
                if p.kind == "secured_funding":
                    haircut = min(.99, pos.haircut+scenario.haircut_add*stress)
                    proceeds = price*(1-haircut)
                    eligible_face = pos.balance*max(0., pos.eligible_fraction-pos.encumbered_fraction)
                    face = min(eligible_face, amount/proceeds)
                    amount = face*proceeds
                    pos.encumbered_fraction += face/pos.balance if pos.balance else 0.
                    extra_debt[source.id] += amount
                    funding_interest[source.id] += amount*p.funding_rate
                    cid = f'{p.id}:{day}'
                    claims[cid] = dict(account=source.id, position=pos.id, face=face, balance=amount,
                                       rate=p.funding_rate, due=day+p.funding_tenor_days)
                    claims_by_position[pos.id].append(cid)
                    claim_maturities[day+p.funding_tenor_days].append(cid)
                    post(day, source.id, "secured_funding", cash=amount, changes={('secured_funding', cid): -amount}, instrument=cid)
                else:
                    if pos.classification == "htm":
                        free_face = min(free_face, max(0., p.htm_sale_limit-htm_sold[pos.id])) if p.allow_htm_sale else 0.
                    face = min(free_face, amount/(price*(1-p.execution_cost)))
                    amount = face*price*(1-p.execution_cost)
                    adjustment = pos.book_adjustment*face/pos.balance if pos.balance else 0.
                    settled_interest = accrued[pos.id]*face/pos.balance if pos.balance else 0.
                    if settled_interest:
                        accrued[pos.id] -= settled_interest
                        post(day, source.id, 'sale_interest_settlement', cash=settled_interest,
                             changes={('accrued_interest', pos.id): -settled_interest}, instrument=pos.id)
                    carry = face*price if pos.classification in {"afs", "trading"} else face+adjustment
                    reclass = marks[pos.id]*face/pos.balance if pos.balance else 0.
                    # Preserve existing pledged face when reducing total face.
                    pledged = pos.balance*pos.encumbered_fraction
                    pos.balance -= face
                    pos.book_adjustment -= adjustment
                    pos.encumbered_fraction = pledged/pos.balance if pos.balance else 0.
                    marks[pos.id] -= reclass
                    htm_sold[pos.id] += face if pos.classification == "htm" else 0.
                    changes = {principal_key(pos): -face, ('book_adjustment', pos.id): -adjustment}
                    if pos.classification in {'afs', 'trading'}:
                        changes['fair_value_adjustment', pos.id] = -face*(price-1)+adjustment
                    post(day, source.id, "security_sale", cash=amount, earnings=amount-carry+reclass, oci=-reclass,
                         changes=changes, instrument=pos.id)
            used[p.id] += amount
            log_action(day, p, "executed" if amount else "blocked", amount, "" if amount else "cash, collateral or HTM limit")
        # Tax all daily earnings, including completed management actions. Dividends
        # are distributions, not deductions from taxable operating income.
        for a in accounts.values():
            income = sum(v[1] for (d, account, event), v in aggregate.items()
                         if d == day and account == a.id and event != 'dividend')
            tax = max(income, 0.)*a.tax_rate
            if tax:
                post(day, a.id, 'tax', cash=-tax, earnings=-tax)
        observe(day)
        # Flush each day: bounded transient state and no quadratic income scan.
        if detail:
            for (d, account, event), (cash, earnings, oci) in aggregate.items():
                if cash or earnings or oci:
                    ledger.append(dict(scenario=scenario.name, day=d, account=account, event=event,
                                       cash=cash, earnings=earnings, aoci=oci, memo_amount=0.))
        aggregate.clear()
    summary = [dict(scenario=scenario.name, account=a.id, entity=a.entity, currency=a.currency,
                    final_cash=a.cash, final_equity=a.equity, minimum_cash=minimum_cash[a.id],
                    peak_cash_shortfall=max(0., a.cash_floor-minimum_cash[a.id]),
                    first_cash_breach_day=first.get((a.id, "cash")), first_capital_breach_day=first.get((a.id, "cet1")),
                    first_leverage_breach_day=first.get((a.id, "leverage")),
                    first_lcr_breach_day=first.get((a.id, 'lcr')),
                    first_nsfr_breach_day=first.get((a.id, 'nsfr')),
                    first_htm_breach_day=first.get((a.id, 'htm')),
                    max_reconciliation_error=max_error) for a in accounts.values()]
    entries, trial = journal.close()
    claim_rows = [dict(scenario=scenario.name, claim_id=cid, **claim) for cid, claim in claims.items()]
    return dict(summary=summary, path=path, ledger=ledger, actions=actions, breaches=breaches,
                exposures=exposures, journal=entries if detail else [], trial_balance=trial if detail else [],
                funding_claims=claim_rows if detail else [])


def run_balance_stress(raw, progress=None, *, journal_backend='python'):
    from ..core.quant_native import enabled
    if enabled():
        if journal_backend != 'python':
            raise ValueError('Select the native state backend or a journal pilot, not both')
        import tempfile
        from .balance_stream import run_streamed_balance_stress, load_streamed_result
        with tempfile.TemporaryDirectory(prefix='native-state-') as directory:
            path = run_streamed_balance_stress(raw,directory,backend='rust',
                progress=(lambda stage: progress(stage,50)) if progress else None)
            result=load_streamed_result(path)
        manifest=result.pop('manifest')
        result['execution']={'financial_events':'rust','native_pilot':False,
            'journal_backend':'rust-state','journal_identity':manifest['binary_sha256'],
            'partition_validation':manifest['validation']}
        _result_metadata(result,raw)
        result['validation']=validation_status(result)
        if progress: progress('Balance-sheet stress complete',100)
        return result
    from .ledger_native import journal_factory
    make_journal, backend_identity = journal_factory(journal_backend)
    spec = validate(raw)
    outputs = defaultdict(list)
    journal_frames = []
    scenarios = [Scenario("baseline")] + spec["scenarios"]
    for i, scenario in enumerate(scenarios):
        if progress:
            progress(f"Balance-sheet stress: {scenario.name}", int(90*i/len(scenarios)))
        result = _simulate(spec, scenario, journal_factory=make_journal)
        for key, rows in result.items():
            if key == 'journal':
                journal_frames.append(rows if isinstance(rows, pl.DataFrame) else pl.DataFrame(rows, schema=JOURNAL_SCHEMA))
            else:
                outputs[key].extend(rows)
    reverse = []
    for scenario in spec["scenarios"]:
        for severity in spec["reverse_severities"]:
            result = _simulate(spec, scenario, severity=severity, detail=False, journal_factory=make_journal)
            for row in result["summary"]:
                reverse.append(dict(row, severity=severity, breached=any(v is not None for k, v in row.items() if k.startswith('first_'))))
    outputs["reverse_grid"] = reverse
    # Explicit schemas for empty result frames keep Arrow/Parquet/UI contracts stable.
    schemas = {"actions": {"scenario": pl.String, "day": pl.Int64, "policy": pl.String, "account": pl.String, "destination": pl.String, "kind": pl.String, "status": pl.String, "amount": pl.Float64, "reason": pl.String},
               "breaches": {"scenario": pl.String, "account": pl.String, "day": pl.Int64, "metric": pl.String},
               'journal': JOURNAL_SCHEMA,
               'trial_balance': {'scenario': pl.String, 'account': pl.String, 'gl_account': pl.String, 'instrument_id': pl.String, 'balance': pl.Float64},
               'funding_claims': {'scenario': pl.String, 'claim_id': pl.String, 'account': pl.String, 'position': pl.String,
                                  'face': pl.Float64, 'balance': pl.Float64, 'rate': pl.Float64, 'due': pl.Int64}}
    result = {k: pl.DataFrame(rows, schema=schemas.get(k), infer_schema_length=None) for k, rows in outputs.items()}
    result['journal'] = pl.concat(journal_frames)
    result['execution'] = {'journal_backend': journal_backend, 'journal_identity': backend_identity,
                           'financial_events': 'python', 'native_pilot': journal_backend == 'rust'}
    totals = defaultdict(lambda: [0., 0., 0.])
    for r in outputs["ledger"]:
        v = totals[r["scenario"], r["account"], r["event"]]
        for i, field in enumerate(("cash", "earnings", "aoci")):
            v[i] += r[field]
    attribution = []
    keys = {(account, event) for _, account, event in totals}
    for scenario in scenarios[1:]:
        for account, event in sorted(keys):
            base, stressed = totals["baseline", account, event], totals[scenario.name, account, event]
            attribution.append(dict(scenario=scenario.name, account=account, event=event,
                                    cash_delta=stressed[0]-base[0], earnings_delta=stressed[1]-base[1],
                                    aoci_delta=stressed[2]-base[2]))
    result["attribution"] = pl.DataFrame(attribution, schema={"scenario": pl.String, "account": pl.String,
        "event": pl.String, "cash_delta": pl.Float64, "earnings_delta": pl.Float64, "aoci_delta": pl.Float64})
    _result_metadata(result,raw)
    if progress:
        progress("Balance-sheet stress complete", 100)
    result['validation'] = validation_status(result)
    closing, consolidated = statements(result['trial_balance'].to_dicts(), {a.id: a for a in spec['accounts']}, [s.name for s in scenarios])
    result['closing_statements'], result['consolidated'] = pl.DataFrame(closing), pl.DataFrame(consolidated)
    return result


def _result_metadata(result,raw):
    result.update(model_version=MODEL_VERSION, specification=deepcopy(raw), warnings=[
        "Synthetic research model: explicit user assumptions; no regulatory certification or probability calibration.",
        "Independent stress specification, not an automatic mapping of the pricing books or optimizer allocations.",
        "ACT/365 daily accrual, user-defined regular payment intervals and 30-day reporting months; no business-day calendars or deferred tax assets.",
        "LCR uses shared 40% Level 2A and 75% inflow caps with explicit weights; NSFR and capital remain research proxies with 100% undrawn CCF.",
        "Credit uses deterministic expected defaults, two-state migration and a one-year expected-loss allowance, not CECL/IFRS9.",
        "Negative cash represents unfunded obligations. Paths continue diagnostically after failure; no automatic emergency borrowing.",
        "No currency aggregation or implicit FX transfers. Per-currency capital is a diagnostic allocation, not legal-entity regulatory capital.",
        "Securities use duration marks; derivatives use supplied netting-set stress losses and simplified posted VM. No full dealer/XVA pricer.",
        "Policies use observed state and at least one-day execution lag. Reverse stress is a sampled grid, not a minimum-failure proof.",
    ])


def example_specification():
    """Balanced synthetic USD-million bank/dealer example, intentionally not a filing."""
    return dict(version=MODEL_VERSION, horizon_days=360,
        accounts=[dict(id="bank_usd", entity="bank", currency="USD", cash=120., equity=100., cash_floor=25., annual_fees=8., annual_costs=12., annual_dividends=3., tax_rate=.21),
                  dict(id="dealer_usd", entity="dealer", currency="USD", cash=30., equity=30., cash_floor=10.)],
        positions=[dict(id="commercial", account="bank_usd", kind="loan", balance=600., rate=.06, annual_pd=.01, allowance=3., commitment=100., draw_fraction=.2, migration_rate=.1, risk_weight=1., rsf_weight=.85),
                   dict(id="afs", account="bank_usd", kind="security", balance=200., rate=.035, classification="afs", duration=4., risk_weight=.2, rsf_weight=.15, hqla_weight=.85, collateral_pool="agency", eligible_fraction=1., haircut=.1),
                   dict(id="htm", account="bank_usd", kind="security", balance=100., rate=.03, classification="htm", duration=6., risk_weight=.2, collateral_pool="agency", eligible_fraction=1., haircut=.15),
                   dict(id="deposits", account="bank_usd", kind="deposit", balance=817., rate=.02, floating_beta=.3, monthly_runoff=.005, uninsured_fraction=.55, concentration=.4, digital_fraction=.9, asf_weight=.9, rsf_weight=0., lcr_outflow_weight=.15),
                   dict(id="wholesale", account="bank_usd", kind="funding", balance=100., rate=.045, maturity_day=15, rollover=.8, asf_weight=.5, lcr_outflow_weight=1.),
                   dict(id="dealer_inventory", account="dealer_usd", kind="security", balance=100., rate=.04, classification="trading", duration=2., collateral_pool="dealer", eligible_fraction=1., encumbered_fraction=.7, haircut=.2),
                   dict(id="repo", account="dealer_usd", kind="repo", balance=110., rate=.04, maturity_day=7, rollover=.9, lcr_outflow_weight=1.)],
        netting_sets=[dict(id="csa_1", account="dealer_usd", counterparty="counterparty_a", fair_value=10., stress_loss=30., annual_pd=.01, wrong_way_multiplier=3., margin_delay=2)],
        policies=[dict(id="bank_funding", account="bank_usd", kind="secured_funding", position="afs", trigger_cash=35., limit=100., delay_days=2),
                  dict(id="bank_sale", account="bank_usd", kind="sell", position="afs", trigger_cash=25., limit=50., delay_days=3, execution_cost=.02),
                  dict(id="dealer_support", account="bank_usd", destination="dealer_usd", kind="transfer", trigger_cash=15., limit=15., delay_days=2),
                  dict(id="suspend_dividend", account="bank_usd", kind="cut_dividend", trigger_cash=30., limit=1.)],
        scenarios=[dict(name="rates_and_competition", rate_shift=.02, deposit_flight=.025, rollover_loss=.1),
                   dict(name="recession_credit", rate_shift=-.015, pd_multiplier=5., migration_multiplier=3., lgd_add=.15, draw_multiplier=2., spread_shift=.015),
                   dict(name="dealer_margin", spread_shift=.025, market_shock=1., haircut_add=.2, rollover_loss=.5),
                   dict(name="confidence_outage", deposit_flight=.25, rollover_loss=.8, draw_multiplier=3., haircut_add=.25, market_shock=.5, outage_days=5)],
        reverse_severities=[0., .5, 1., 1.5, 2.])
