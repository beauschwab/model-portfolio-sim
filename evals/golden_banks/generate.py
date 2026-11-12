"""Deterministic raw extracts and trial balance for one golden bank.

Reviewed command: `python evals/golden_banks/generate.py [bank ...]`.
Standard library only. Balances are whole cents allocated exactly to the totals in
`bank.json`, so the trial balance ties to the extracts by construction. Every
value is synthetic; none describes a real institution.
"""
from __future__ import annotations

import csv
import datetime as dt
import io
import json
import random
import sys
from decimal import Decimal
from pathlib import Path

ROOT = Path(__file__).resolve().parent / 'banks'
ASOF = dt.date(2026, 6, 10)  # valuation date of the seeded workspace
CANONICAL = ['loan_id', 'product', 'balance', 'currency', 'entity', 'accounting_category', 'assumption_set', 'fico',
             'age_months', 'remaining_term_months', 'oltv', 'factor', 'wac', 'net_coupon', 'price', 'hpi_orig_ratio',
             'pay_delay_days', 'state', 'channel', 'delinquency', 'rate_type', 'book_yield', 'segment', 'insured_status',
             'relationship', 'rate_paid', 'svc_cost']
ACCOUNTS = [  # GL account, description, side, source
    ('1010', 'Interest-earning deposits and money-market assets', 'asset', 'money_market:asset'),
    ('1200', 'Agency mortgage-backed securities', 'asset', 'securities'),
    ('1300', 'Residential mortgage loans', 'asset', 'retail_tape:mortgage'),
    ('1400', 'Commercial loans', 'asset', 'commercial_loans'),
    ('2010', 'Retail non-maturity deposits', 'liability', 'retail_tape:deposit'),
    ('2020', 'Commercial and wholesale non-maturity deposits', 'liability', 'wholesale_deposits'),
    ('2100', 'Time deposits', 'liability', 'time_deposits'),
    ('2200', 'Short-term borrowings', 'liability', 'money_market:liability'),
    ('2300', 'Long-term debt', 'liability', 'long_term_debt'),
]


def cents(total, count, rng):
    """Split a dollar total into `count` positive cent amounts that sum to it exactly."""
    weights = [rng.uniform(.5, 1.5) for _ in range(count)]
    scale, whole = sum(weights), int(round(total * 100))
    parts = [int(whole * w / scale) for w in weights]
    parts[-1] += whole - sum(parts)
    return [Decimal(p) / 100 for p in parts]


def month(offset):
    y, m = divmod(ASOF.month - 1 + offset, 12)
    return dt.date(ASOF.year + y, m + 1, 10).isoformat()


def pick(rng, weighted):
    names, weights = zip(*weighted.items())
    return rng.choices(names, weights)[0]


def text(rows, columns):
    out = io.StringIO()
    writer = csv.writer(out, lineterminator='\n')
    writer.writerow(columns)
    for row in rows:
        writer.writerow(['' if row.get(c) is None else row[c] for c in columns])
    return out.getvalue()


def tape(spec, rng, total):
    rows, mix, t = [], spec['mix'], spec['tape']
    common = dict(currency='USD', entity=spec['entity'], accounting_category='amortized_cost', assumption_set='base')
    for i, bal in enumerate(cents(total * mix['resi_mortgage'], t['mortgages'], rng)):
        age = rng.randint(3, 140)
        wac = round(rng.uniform(.0325, .0725), 4)
        rows.append(common | dict(loan_id=f'RM{i:06d}', product='mortgage', balance=bal, fico=rng.randint(640, 810), age_months=age,
            remaining_term_months=rng.choice([180, 240, 360]) - min(age, 150), oltv=round(rng.uniform(.5, .95), 3),
            factor=round(rng.uniform(.55, 1.), 3), wac=wac, net_coupon=round(wac - .0025, 4), price=round(rng.uniform(88., 103.), 3),
            hpi_orig_ratio=round(rng.uniform(.95, 1.6), 3), pay_delay_days=0, state=pick(rng, t['states']),
            channel=pick(rng, {'retail': .7, 'broker': .3}), delinquency='current', rate_type='fixed',
            book_yield=round(wac - rng.uniform(.002, .006), 4)))
    for i, bal in enumerate(cents(total * mix['retail_deposits'], t['deposits'], rng)):
        segment = pick(rng, t['segments'])
        paid = 0. if segment == 'DDA' else round(rng.uniform(*{'NOW': (.001, .01), 'SAV': (.002, .025), 'MMDA': (.015, .045)}[segment]), 4)
        rows.append(common | dict(loan_id=f'DP{i:06d}', product='deposit', balance=bal, age_months=rng.randint(1, 180),
            price=round(rng.uniform(94., 99.5), 2), segment=segment, insured_status=pick(rng, {'insured': .6, 'uninsured': .4}),
            relationship=pick(rng, {'primary': .65, 'single': .35}), rate_paid=paid, svc_cost=round(rng.uniform(.0005, .002), 4)))
    for field, count in t.get('missing', {}).items():  # documented data-quality exceptions
        for row in rng.sample([r for r in rows if r.get(field) is not None], count):
            row[field] = None
    rng.shuffle(rows)
    headers = t.get('headers') or {c: c for c in CANONICAL}
    return text([{headers[k]: v for k, v in r.items()} for r in rows], [headers[c] for c in CANONICAL]), rows


def books(spec, rng, total):
    mix, n = spec['mix'], spec['counts']
    out = {}
    out['securities'] = (['cusip', 'current_face', 'net_coupon', 'wac', 'wam', 'age', 'oltv', 'factor', 'fico', 'avg_loan_size',
                          'state', 'channel', 'price', 'book_yield'], [])
    for i, bal in enumerate(cents(total * mix['securities'], n['securities'], rng)):
        coupon, age = round(rng.uniform(.02, .06), 4), rng.randint(6, 150)
        out['securities'][1].append(dict(cusip=f'{spec["prefix"]}MBS{i:04d}', current_face=bal, net_coupon=coupon, wac=round(coupon + .006, 4),
            wam=360 - age, age=age, oltv=round(rng.uniform(.6, .8), 3), factor=round(rng.uniform(.5, 1.), 3), fico=rng.randint(700, 780),
            avg_loan_size=rng.randint(180, 420) * 1000, state=pick(rng, spec['tape']['states']), channel='retail',
            price=round(rng.uniform(86., 102.), 3), book_yield=round(rng.uniform(.02, .05), 4)))
    corp = ['id', 'face', 'maturity', 'freq_months', 'daycount', 'is_float', 'coupon_or_spread', 'amort_type', 'price', 'book_yield']
    for name, key, tag, floating, months in (('commercial_loans', 'commercial', 'CL', spec['commercial_float_share'], (6, 84)),
                                             ('long_term_debt', 'debt', 'LD', .4, (18, 120))):
        rows = []
        for i, bal in enumerate(cents(total * mix[key], n[name], rng)):
            is_float = int(rng.random() < floating)
            rows.append(dict(id=f'{spec["prefix"]}{tag}{i:04d}', face=bal, maturity=month(rng.randint(*months)), freq_months=rng.choice([3, 6]),
                daycount='ACT/360', is_float=is_float, coupon_or_spread=round(rng.uniform(.012, .03) if is_float else rng.uniform(.04, .075), 5),
                amort_type='bullet', price=round(rng.uniform(97., 101.), 3), book_yield=round(rng.uniform(.045, .07), 4)))
        out[name] = (corp, rows)
    out['wholesale_deposits'] = (['id', 'balance', 'segment', 'age_months', 'avg_account_size', 'rate_paid', 'svc_cost', 'price'], [])
    for i, bal in enumerate(cents(total * mix['wholesale_deposits'], n['wholesale_deposits'], rng)):
        segment = rng.choice(['DDA', 'MMDA'])
        out['wholesale_deposits'][1].append(dict(id=f'{spec["prefix"]}WD{i:04d}', balance=bal, segment=segment, age_months=rng.randint(6, 120),
            avg_account_size=rng.randint(400, 5000) * 1000, rate_paid=0. if segment == 'DDA' else round(rng.uniform(.025, .045), 4),
            svc_cost=.0008, price=round(rng.uniform(96., 99.8), 2)))
    out['time_deposits'] = (['id', 'balance', 'rate', 'maturity', 'freq_months', 'daycount', 'channel', 'penalty_months', 'price'], [])
    for i, bal in enumerate(cents(total * mix['time_deposits'], n['time_deposits'], rng)):
        out['time_deposits'][1].append(dict(id=f'{spec["prefix"]}TD{i:04d}', balance=bal, rate=round(rng.uniform(.03, .052), 4),
            maturity=month(rng.randint(2, 48)), freq_months=rng.choice([0, 6]), daycount='ACT/365F',
            channel=pick(rng, {'retail': 1 - spec['brokered_cd_share'], 'brokered': spec['brokered_cd_share']}), penalty_months=6.,
            price=round(rng.uniform(99.5, 101.), 3)))
    mm = []
    for side, share, lines in (('asset', mix['money_market_assets'], (('IEDB', 0., .7), ('RESALE', 29., .3))),
                               ('liability', mix['short_term_borrowings'], (('REPO', 36., .6), ('ST_BORROW', 66., .4)))):
        amount = int(round(total * share * 100))
        first = int(amount * lines[0][2])
        for (name, spread, _), part in zip(lines, (first, amount - first)):
            mm.append(dict(id=name, balance=Decimal(part) / 100, side=side, spread_bp=spread, category=name))
    out['money_market'] = (['id', 'balance', 'side', 'spread_bp', 'category'], mm)
    return out


def trial_balance(spec, tape_rows, extracts):
    def total(rows, field, keep=lambda r: True):
        return sum((r[field] for r in rows if keep(r)), Decimal(0))
    amounts = {
        'money_market:asset': total(extracts['money_market'][1], 'balance', lambda r: r['side'] == 'asset'),
        'money_market:liability': total(extracts['money_market'][1], 'balance', lambda r: r['side'] == 'liability'),
        'securities': total(extracts['securities'][1], 'current_face'),
        'retail_tape:mortgage': total(tape_rows, 'balance', lambda r: r['product'] == 'mortgage'),
        'retail_tape:deposit': total(tape_rows, 'balance', lambda r: r['product'] == 'deposit'),
        'commercial_loans': total(extracts['commercial_loans'][1], 'face'),
        'wholesale_deposits': total(extracts['wholesale_deposits'][1], 'balance'),
        'time_deposits': total(extracts['time_deposits'][1], 'balance'),
        'long_term_debt': total(extracts['long_term_debt'][1], 'face'),
    }
    rows = [dict(gl_account=a, description=d, side=s, source_extract=src, balance=amounts[src]) for a, d, s, src in ACCOUNTS]
    for item in spec.get('gl_only', []):  # ledger balances with no position extract: expected reconciliation breaks
        rows.append(dict(gl_account=item['gl_account'], description=item['description'], side=item['side'], source_extract='',
                         balance=Decimal(str(item['balance']))))
    sides = {s: sum(r['balance'] for r in rows if r['side'] == s) for s in ('asset', 'liability')}
    rows.append(dict(gl_account='3000', description='Total equity', side='equity', source_extract='', balance=sides['asset'] - sides['liability']))
    rows.sort(key=lambda r: r['gl_account'])
    return text(rows, ['gl_account', 'description', 'side', 'source_extract', 'balance'])


def files(bank):
    spec = json.loads((ROOT / bank / 'bank.json').read_text(encoding='utf-8'))['generator']
    rng = random.Random(spec['seed'])
    total = float(spec['total_assets'])
    csv_text, tape_rows = tape(spec, rng, total)
    extracts = books(spec, rng, total)
    out = {'extracts/retail_tape.csv': csv_text}
    out.update({f'extracts/{name}.csv': text(rows, columns) for name, (columns, rows) in extracts.items()})
    out['trial_balance.csv'] = trial_balance(spec, tape_rows, extracts)
    return out


def main(argv):
    for bank in argv or sorted(p.name for p in ROOT.iterdir() if (p / 'bank.json').exists()):
        for name, content in files(bank).items():
            path = ROOT / bank / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content.encode('utf-8'))
            print(f'{bank}/{name}: {len(content.splitlines()) - 1} rows')


if __name__ == '__main__':
    main(sys.argv[1:])
