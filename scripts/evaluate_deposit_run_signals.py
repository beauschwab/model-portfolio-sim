"""Held-out evaluation of year-end 2022 deposit-run signals against 2023 outcomes.

An evidence study, not a model: it fits nothing to the outcome and adds nothing to the
engine. Signals use Call Report data up to 2022-12-31 only; outcomes use 2023 data only.
Run: uv run --project apps/api python scripts/evaluate_deposit_run_signals.py
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from datetime import date
from pathlib import Path

import polars as pl

ROOT = Path(__file__).resolve().parents[1]
CUTOFF = '20221231'
# Fixed before any 2023 row is read. The sensitivity table shows the result under the others.
CONFIG = dict(
    outflow_event=0.20,                 # first-quarter deposit decline that counts as a run
    outflow_sensitivity=[0.10, 0.20, 0.30],
    failure_window_end='2023-06-30',    # FDIC failures up to here count as events
    alarm_share=0.10,                   # a signal "alarms" on its top decile
    training_start='20190331',
    minimum_training_quarters=8,
    ordinary_lower_tail=0.01,           # one-sided tail of the ordinary-runoff forecast
)
Z_ONE_PERCENT = 2.3263478740408408      # standard normal quantile for the 1% tail


def signals(pre):
    """Signals from data on or before the cutoff. `pre` must contain no later row."""
    assert pre['REPDTE'].max() <= CUTOFF, 'a post-cutoff row reached the signal calculation'
    at = lambda q, name: pre.filter(pl.col('REPDTE') == q).select('CERT', pl.col('DEP').alias(name))
    now = pre.filter(pl.col('REPDTE') == CUTOFF)
    out = now.join(at('20211231', 'dep_2021'), on='CERT', how='left').join(at('20191231', 'dep_2019'), on='CERT', how='left')
    return out.select(
        'CERT', 'NAME', 'ASSET', 'DEP',
        uninsured_share=pl.col('DEPUNINS')/pl.col('DEPDOM'),
        securities_loss_to_equity=((pl.col('SCHA')-pl.col('SCHF'))+(pl.col('SCAA')-pl.col('SCAF')))/pl.col('EQ'),
        deposit_decline_2022=-(pl.col('DEP')/pl.col('dep_2021')-1),
        deposit_growth_2019_2021=pl.col('dep_2021')/pl.col('dep_2019')-1,
    ).with_columns(
        uninsured_times_loss=pl.col('uninsured_share')*pl.max_horizontal(pl.col('securities_loss_to_equity'), pl.lit(0.)))


def ordinary_forecast(pre):
    """Each bank's own quarterly log deposit growth over the training window: mean and deviation."""
    assert pre['REPDTE'].max() <= CUTOFF
    growth = (pre.sort(['CERT', 'REPDTE'])
              .with_columns(g=(pl.col('DEP')/pl.col('DEP').shift(1)).log().over('CERT'))
              .filter((pl.col('REPDTE') >= CONFIG['training_start']) & pl.col('g').is_finite()))
    return growth.group_by('CERT').agg(mean=pl.col('g').mean(), deviation=pl.col('g').std(), quarters=pl.len())


def auc(scores, events):
    """Probability that a random event bank outranks a random non-event bank; ties count half."""
    positive = [s for s, e in zip(scores, events) if e]
    negative = [s for s, e in zip(scores, events) if not e]
    if not positive or not negative:
        return None
    wins = sum((p > n) + .5*(p == n) for p in positive for n in negative)
    return wins/(len(positive)*len(negative))


def evaluate(panel, failures):
    pre, post = panel.filter(pl.col('REPDTE') <= CUTOFF), panel.filter(pl.col('REPDTE') > CUTOFF)
    table = signals(pre)
    window_end = date.fromisoformat(CONFIG['failure_window_end'])
    failed = {}
    for row in failures:
        month, day, year = (int(x) for x in row['FAILDATE'].split('/'))
        if date(year, month, day) <= window_end:
            failed[row['CERT']] = f'{year:04d}-{month:02d}-{day:02d}'
    first = post.filter(pl.col('REPDTE') == '20230331').select('CERT', pl.col('DEP').alias('dep_2023q1'))
    table = table.join(first, on='CERT', how='left').with_columns(
        deposit_change_2023q1=pl.col('dep_2023q1')/pl.col('DEP')-1,
        failed_on=pl.col('CERT').replace_strict(failed, default=None, return_dtype=pl.String))
    # A bank with no first-quarter report that is not on the failures list merged or closed:
    # its outcome is unobserved, so it is excluded, never counted as a non-event.
    excluded = table.filter(pl.col('dep_2023q1').is_null() & pl.col('failed_on').is_null())
    table = table.filter(~pl.col('CERT').is_in(excluded['CERT'].to_list()))

    def events_at(threshold):
        return (table['failed_on'].is_not_null() | (table['deposit_change_2023q1'].fill_null(0.) <= -threshold)).to_list()

    events = events_at(CONFIG['outflow_event'])
    table = table.with_columns(event=pl.Series(events))
    n, k = table.height, math.ceil(CONFIG['alarm_share']*table.height)
    names = ['uninsured_share', 'securities_loss_to_equity', 'uninsured_times_loss', 'deposit_decline_2022', 'deposit_growth_2019_2021']
    results = []
    for name in names:
        usable = table.filter(pl.col(name).is_finite())
        scores, flags = usable[name].to_list(), usable['event'].to_list()
        order = sorted(range(len(scores)), key=lambda i: -scores[i])
        alarms = set(order[:k])
        hits = sum(flags[i] for i in alarms)
        ranks = {usable['NAME'][i]: 1 + sum(s > scores[i] for s in scores) for i in range(len(scores)) if flags[i]}
        results.append(dict(signal=name, banks=len(scores), events=sum(flags), auc=auc(scores, flags), alarms=k, hits=hits,
            missed=sum(flags)-hits, false_alarms=k-hits, precision=hits/k, recall=hits/sum(flags) if sum(flags) else None,
            event_ranks=ranks,
            sensitivity=[dict(outflow=t, events=sum(e for e, ok in zip(events_at(t), table[name].is_finite().to_list()) if ok),
                              auc=auc(scores, [e for e, ok in zip(events_at(t), table[name].is_finite().to_list()) if ok]))
                         for t in CONFIG['outflow_sensitivity']]))
    # Ordinary-runoff benchmark: was the 2023 outcome inside each bank's own pre-event range?
    forecast = ordinary_forecast(pre).filter(pl.col('quarters') >= CONFIG['minimum_training_quarters'])
    scored = table.join(forecast, on='CERT', how='inner').filter(pl.col('dep_2023q1').is_not_null()).with_columns(
        z=((pl.col('dep_2023q1')/pl.col('DEP')).log()-pl.col('mean'))/pl.col('deviation'))
    below = scored.filter(pl.col('z') < -Z_ONE_PERCENT)
    ordinary = dict(banks=scored.height, expected_breach_share=CONFIG['ordinary_lower_tail'],
        observed_breach_share=below.height/scored.height if scored.height else None, breaches=below.height,
        event_z={r['NAME']: r['z'] for r in scored.filter(pl.col('event')).iter_rows(named=True)},
        unscored_events=[r['NAME'] for r in table.filter(pl.col('event') & pl.col('dep_2023q1').is_null()).iter_rows(named=True)])
    # How many run-sized quarterly declines exist before the cutoff to learn from?
    history = (pre.sort(['CERT', 'REPDTE']).with_columns(change=(pl.col('DEP')/pl.col('DEP').shift(1)-1).over('CERT'))
               .filter((pl.col('REPDTE') >= CONFIG['training_start']) & pl.col('change').is_finite()))
    prior = history.filter(pl.col('change') <= -CONFIG['outflow_event'])
    return dict(
        config=CONFIG, cutoff=CUTOFF, universe=n+excluded.height, evaluated=n, events=sum(events),
        excluded=[dict(name=r['NAME'], reason='no 2023Q1 Call Report and not on the FDIC failures list (merged or closed)')
                  for r in excluded.iter_rows(named=True)],
        event_banks=[dict(name=r['NAME'], assets=r['ASSET'], failed_on=r['failed_on'], deposit_change_2023q1=r['deposit_change_2023q1'],
                          **{s: r[s] for s in names}) for r in table.filter(pl.col('event')).sort('ASSET', descending=True).iter_rows(named=True)],
        medians={s: table[s].median() for s in names}, signals=results, ordinary_runoff=ordinary,
        pre_event_history=dict(bank_quarters=history.height, run_sized_declines=prior.height,
            share=prior.height/history.height if history.height else None,
            cases=[dict(name=r['NAME'], quarter=r['REPDTE'], change=r['change']) for r in prior.sort('change').iter_rows(named=True)]),
        top_alarms={s: [dict(name=r['NAME'], value=r[s], event=r['event'])
                        for r in table.filter(pl.col(s).is_finite()).sort(s, descending=True).head(k).iter_rows(named=True)] for s in names})


def report(r, source):
    pct = lambda v: 'n/a' if v is None else f'{100*v:.1f}%'
    ranked = sorted(r['signals'], key=lambda x: -x['auc'])
    label = lambda x: x['signal'].replace('_', ' ')
    lines = ['# Deposit-run signals: held-out evaluation on 2023 outcomes', '',
        f"**Result: the year-end 2022 signals carried real but noisy information. {label(ranked[0]).capitalize()} ranked the banks that then ran best (AUC {ranked[0]['auc']:.2f}), "
        f"followed by {label(ranked[1])} ({ranked[1]['auc']:.2f}) and {label(ranked[2])} ({ranked[2]['auc']:.2f}); {label(ranked[-1])} was close to uninformative ({ranked[-1]['auc']:.2f}). "
        f"Every signal's top-decile alarm was mostly false alarms (at least {min(x['false_alarms'] for x in r['signals'])} of {ranked[0]['alarms']}), "
        f"and the data before 2023 contain almost no run-sized outflows to estimate a run model from.**", '',
        'This is an evidence study on public quarterly data. It fits nothing to the outcome, changes no model and cannot say how fast a run is: quarterly balances do not show days.', '',
        '## Design', '',
        f"- **Banks.** Every FDIC-insured bank with at least $10bn of assets on 2022-12-31: {r['universe']} banks. "
        f"{len(r['excluded'])} are excluded because they filed no first-quarter 2023 report and did not fail (they merged or closed), leaving {r['evaluated']}.",
        '- **Signals.** Computed from Call Reports dated 2022-12-31 or earlier; the script refuses a later row.',
        f"- **Outcome.** An event is an FDIC-listed failure by {r['config']['failure_window_end']}, or total deposits falling at least {100*r['config']['outflow_event']:.0f}% in the first quarter of 2023. "
        f"{r['events']} banks qualify.",
        f"- **Alarm.** Each signal flags its top {100*r['config']['alarm_share']:.0f}% of banks. The event threshold, alarm share and windows are fixed in the script before any 2023 row is read; the sensitivity table shows other thresholds.",
        '- **What is not held out.** The data are out of sample; the hypothesis is not. These signals were chosen with knowledge of what was discussed after March 2023.', '',
        '## Event banks', '',
        '| Bank | Assets ($bn) | Failed | Deposits, Q1 2023 | Uninsured share | Securities loss / equity | Deposit decline in 2022 |',
        '|---|---:|---|---:|---:|---:|---:|']
    for b in r['event_banks']:
        lines.append(f"| {b['name'].title()} | {b['assets']/1e6:.1f} | {b['failed_on'] or 'no'} | {pct(b['deposit_change_2023q1'])} | "
                     f"{pct(b['uninsured_share'])} | {pct(b['securities_loss_to_equity'])} | {pct(b['deposit_decline_2022'])} |")
    m = r['medians']
    lines += ['', f"Medians across all {r['evaluated']} banks: uninsured share {pct(m['uninsured_share'])}, securities loss to equity {pct(m['securities_loss_to_equity'])}, "
        f"deposit decline in 2022 {pct(m['deposit_decline_2022'])}. Banks that failed before filing have no first-quarter deposit figure.", '',
        '## How the signals ranked them', '',
        f"| Signal | Ranking quality (AUC) | Alarms | Hits | Missed | False alarms | Ranks of the event banks (1 = highest of {r['evaluated']}) |",
        '|---|---:|---:|---:|---:|---:|---|']
    for s in r['signals']:
        ranks = ', '.join(f"{name.title()} {rank}" for name, rank in sorted(s['event_ranks'].items(), key=lambda x: x[1]))
        lines.append(f"| {s['signal'].replace('_', ' ')} | {s['auc']:.2f} | {s['alarms']} | {s['hits']} | {s['missed']} | {s['false_alarms']} | {ranks} |")
    lines += ['', 'AUC is the chance that a randomly chosen event bank scores above a randomly chosen non-event bank: 0.5 is no information, 1.0 is perfect ranking. '
        f"With {r['events']} events these figures are imprecise; one bank changing category moves them materially.", '',
        '| Signal | AUC at 10% outflow | AUC at 20% | AUC at 30% |', '|---|---:|---:|---:|']
    for s in r['signals']:
        cells = ' | '.join(f"{x['auc']:.2f} ({x['events']} events)" if x['auc'] is not None else 'n/a' for x in s['sensitivity'])
        lines.append(f"| {s['signal'].replace('_', ' ')} | {cells} |")
    o, h = r['ordinary_runoff'], r['pre_event_history']
    lines += ['', '## The ordinary-runoff benchmark', '',
        f"Each bank's quarterly deposit growth from {r['config']['training_start'][:4]} to 2022 gives a mean and a deviation. If 2023 were ordinary, about {pct(o['expected_breach_share'])} of banks would fall below their own 1% lower bound. "
        f"{o['breaches']} of {o['banks']} did ({pct(o['observed_breach_share'])}).", '',
        '| Event bank | First-quarter outcome, in deviations from its own history |', '|---|---:|']
    for name, z in sorted(o['event_z'].items(), key=lambda x: x[1]):
        lines.append(f"| {name.title()} | {z:.1f} |")
    lines += ['', f"Not scored, because they failed before reporting: {', '.join(x.title() for x in o['unscored_events']) or 'none'}. "
        'The benchmark is a forecast of ordinary behaviour, so it has nothing to say before the fact; this shows only how far outside ordinary experience the outcomes were.', '',
        '## What the pre-2023 data can support', '',
        f"From {r['config']['training_start'][:4]} to 2022 these banks produced {h['bank_quarters']} bank-quarters. {h['run_sized_declines']} show a deposit decline of {100*r['config']['outflow_event']:.0f}% or more ({pct(h['share'])}).", '']
    if h['cases']:
        lines += ['| Bank | Quarter | Deposit change |', '|---|---|---:|']
        lines += [f"| {c['name'].title()} | {c['quarter'][:4]}-{c['quarter'][4:6]} | {pct(c['change'])} |" for c in h['cases'][:12]]
        lines += ['', 'These are not screened for restructurings or transfers between affiliates, which is what several of them are likely to be.', '']
    lines += ['**A confidence-driven jump component cannot be estimated from this history: there are too few run-sized outflows, and none is known to be a run.** '
        'Any such component fitted on pre-2023 large-bank data would be an assumption, and fitting it on 2023 would be tuning to the outcome.', '',
        '## What this does and does not establish', '',
        '- It supports using these signals as an ex-ante ranking of exposure, with the false-alarm rates shown. It does not support any one of them as a trigger.',
        '- The event rule is mechanical. It counts an affiliate bank whose deposits moved within its group as an event and has no way to tell that from a run.',
        '- Early quarters include a bank only once it had $5bn of assets, so growth signals are missing for banks that were smaller in 2019.',
        '- It does not give a probability of a run, a date or a speed, and it is one episode with a handful of events.',
        '- It does not calibrate the daily ledger\'s deposit model. That needs daily or weekly outflow data across several run episodes, which this source does not contain.',
        '- Deposit changes are not adjusted for mergers, so large increases are ignored and large decreases at non-failed banks may include transfers.', '',
        '## Reproduce', '', '```bash', 'uv run --project apps/api python scripts/evaluate_deposit_run_signals.py', '```', '',
        f"Data: `examples/deposit-runs-2023/` ({source['publisher']}, retrieved {source['retrieved']}; {source['amount_unit']}). "
        'The script runs offline from that frozen panel.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--data', type=Path, default=ROOT/'examples/deposit-runs-2023')
    parser.add_argument('--output', type=Path, default=ROOT/'docs/reviews/2026-11-16-deposit-run-signals.json')
    args = parser.parse_args()
    source = json.loads((args.data/'source.json').read_text(encoding='utf-8'))
    panel = pl.read_csv(args.data/'fdic_large_bank_panel.csv', schema_overrides={'REPDTE': pl.String})
    result = evaluate(panel, source['failures']['rows'])
    result['panel_sha256'] = hashlib.sha256((args.data/'fdic_large_bank_panel.csv').read_bytes()).hexdigest()
    result['source_sha256'] = hashlib.sha256((args.data/'source.json').read_bytes()).hexdigest()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    args.output.with_suffix('.md').write_text(report(result, source), encoding='utf-8')
    print(json.dumps(dict(report=str(args.output.with_suffix('.md')), evaluated=result['evaluated'], events=result['events'],
                          signals={s['signal']: round(s['auc'], 3) for s in result['signals']}), indent=2))


if __name__ == '__main__':
    main()
