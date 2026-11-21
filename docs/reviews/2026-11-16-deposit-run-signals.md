# Deposit-run signals: held-out evaluation on 2023 outcomes

**Result: the year-end 2022 signals carried real but noisy information. On the 151 banks that every signal scores (4 events), deposit growth 2019 2021 ranked the banks that then ran best (AUC 0.95), followed by deposit decline 2022 (0.75) and uninsured share (0.73); securities loss to equity was close to uninformative (0.61). Every signal's top-decile alarm was mostly false alarms (at least 13 of 16), and the data before 2023 contain almost no run-sized outflows to estimate a run model from.**

This is an evidence study on public quarterly data. It fits nothing to the outcome, changes no model and cannot say how fast a run is: quarterly balances do not show days.

## Design

- **Banks.** Every FDIC-insured bank with at least $10bn of assets on 2022-12-31: 162 banks. 2 are excluded because they filed no first-quarter 2023 report and did not fail (they merged or closed), leaving 160.
- **Signals.** Computed from Call Reports dated 2022-12-31 or earlier; the script refuses a later row.
- **Outcome.** An event is an FDIC-listed failure by 2023-06-30, or total deposits falling at least 20% in the first quarter of 2023. 5 banks qualify.
- **Alarm.** Each signal flags its top 10% of banks. The event threshold, alarm share and windows are fixed in the script before any 2023 row is read; the sensitivity table shows other thresholds.
- **What is not held out.** The data are out of sample; the hypothesis is not. These signals were chosen with knowledge of what was discussed after March 2023.

## Event banks

| Bank | Assets ($bn) | Failed | Deposits, Q1 2023 | Uninsured share | Securities loss / equity | Deposit decline in 2022 |
|---|---:|---|---:|---:|---:|---:|
| First Republic Bank | 212.6 | 2023-05-01 | -40.8% | 67.7% | 30.0% | -12.9% |
| Silicon Valley Bank | 209.0 | 2023-03-10 | n/a | 93.9% | 114.4% | 8.4% |
| Signature Bank | 110.4 | 2023-03-12 | n/a | 89.7% | 40.1% | 16.5% |
| Bank Of America Ca Na | 18.2 | no | -27.5% | 2.6% | 0.0% | 47.1% |
| Silvergate Bank | 11.4 | no | -76.3% | 58.9% | 19.9% | 55.9% |

Medians across all 160 banks: uninsured share 41.9%, securities loss to equity 21.9%, deposit decline in 2022 0.2%. Banks that failed before filing have no first-quarter deposit figure.

## How the signals ranked them

Each signal is scored only on the banks it has data for, so its AUC, hits and misses are conditional on that set. The last AUC column uses the 151 banks every signal scores (4 events), which is the like-for-like comparison; it leaves out Silvergate Bank.

| Signal | Banks scored | Events scored | AUC | Alarms | Hits | Missed | False alarms | AUC, common banks | Ranks of the event banks among the banks scored (1 = highest) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| uninsured share | 160 | 5 | 0.77 | 16 | 3 | 2 | 13 | 0.73 | Silicon Valley Bank 4, Signature Bank 5, First Republic Bank 12, Silvergate Bank 21, Bank Of America Ca Na 155 |
| securities loss to equity | 156 | 5 | 0.57 | 16 | 1 | 4 | 15 | 0.61 | Silicon Valley Bank 7, Signature Bank 34, First Republic Bank 54, Silvergate Bank 89, Bank Of America Ca Na 156 |
| uninsured times loss | 156 | 5 | 0.68 | 16 | 2 | 3 | 14 | 0.70 | Silicon Valley Bank 1, Signature Bank 9, First Republic Bank 28, Silvergate Bank 60, Bank Of America Ca Na 155 |
| deposit decline 2022 | 160 | 5 | 0.80 | 16 | 3 | 2 | 13 | 0.75 | Silvergate Bank 1, Bank Of America Ca Na 2, Signature Bank 10, Silicon Valley Bank 28, First Republic Bank 126 |
| deposit growth 2019 2021 | 155 | 4 | 0.95 | 16 | 3 | 1 | 13 | 0.95 | Silicon Valley Bank 1, Signature Bank 5, Bank Of America Ca Na 8, First Republic Bank 27; not scored: Silvergate Bank |

AUC is the chance that a randomly chosen event bank scores above a randomly chosen non-event bank: 0.5 is no information, 1.0 is perfect ranking. With 5 events these figures are imprecise; one bank changing category moves them materially. A signal is unscored for a bank when an input is missing: banks with no equity capital (US branches of foreign banks) have no loss ratio, and banks below $5bn in 2019 are not in the panel then, so they have no 2019 to 2021 growth.

| Signal | AUC at 10% outflow | AUC at 20% | AUC at 30% |
|---|---:|---:|---:|
| uninsured share | 0.61 (14 events) | 0.77 (5 events) | 0.95 (4 events) |
| securities loss to equity | 0.58 (13 events) | 0.57 (5 events) | 0.71 (4 events) |
| uninsured times loss | 0.62 (13 events) | 0.68 (5 events) | 0.86 (4 events) |
| deposit decline 2022 | 0.84 (14 events) | 0.80 (5 events) | 0.75 (4 events) |
| deposit growth 2019 2021 | 0.79 (13 events) | 0.95 (4 events) | 0.94 (3 events) |

## The ordinary-runoff benchmark

Each bank's quarterly log deposit growth from 2019 to 2022 (8 to 16 quarters) gives a mean and a deviation. Its 1.0% lower bound for the next quarter is the Student-t predictive bound, 2.68 to 3.18 deviations below the mean, which allows for the mean and deviation being estimated from so few quarters. If quarterly growth were independent and normal with a constant mean and variance, and 2023 were ordinary, about 1.0% of banks would fall below it. 7 of 157 did (4.5%). Those assumptions are strong: 2020 deposit surges widen the deviations, growth is not independent across quarters or banks, and the bank count is small, so the comparison is indicative, not a test.

| Event bank | First-quarter outcome, in deviations from its own history |
|---|---:|
| First Republic Bank | -22.0 |
| Silvergate Bank | -4.1 |
| Bank Of America Ca Na | -2.0 |

Not scored, because they failed before reporting: Silicon Valley Bank, Signature Bank. The benchmark is a forecast of ordinary behaviour, so it has nothing to say before the fact; this shows only how far outside ordinary experience the outcomes were.

## What the pre-2023 data can support

From 2019 to 2022 these banks produced 2549 bank-quarters. 9 show a deposit decline of 20% or more (0.4%).

| Bank | Quarter | Deposit change |
|---|---|---:|
| Silvergate Bank | 2022-12 | -52.2% |
| Banco Bilbao Vizcaya Sa | 2021-03 | -33.9% |
| Banco Bilbao Vizcaya Sa | 2021-12 | -28.9% |
| Beal Bank Usa | 2021-03 | -28.3% |
| Bank Of America Ca Na | 2022-09 | -27.2% |
| Deutsche Bank Tr Co Americas | 2021-03 | -21.4% |
| Bank Of China | 2019-03 | -21.3% |
| State Street Bank&Trust Co | 2020-06 | -21.1% |
| Bancfirst | 2021-12 | -20.4% |

These are not screened for restructurings or transfers between affiliates, which is what several of them are likely to be.

**A confidence-driven jump component cannot be estimated from this history: there are too few run-sized outflows, and none is known to be a run.** Any such component fitted on pre-2023 large-bank data would be an assumption, and fitting it on 2023 would be tuning to the outcome.

## What this does and does not establish

- It supports using these signals as an ex-ante ranking of exposure, with the false-alarm rates shown. It does not support any one of them as a trigger.
- The event rule is mechanical. It counts an affiliate bank whose deposits moved within its group as an event and has no way to tell that from a run.
- Early quarters include a bank only once it had $5bn of assets, so growth signals are missing for banks that were smaller in 2019.
- It does not give a probability of a run, a date or a speed, and it is one episode with a handful of events.
- It does not calibrate the daily ledger's deposit model. That needs daily or weekly outflow data across several run episodes, which this source does not contain.
- Deposit changes are not adjusted for mergers, so large increases are ignored and large decreases at non-failed banks may include transfers.

## Reproduce

```bash
uv run --project apps/api python scripts/evaluate_deposit_run_signals.py
```

Data: `examples/deposit-runs-2023/` (FDIC BankFind Suite (financials and failures endpoints), retrieved 2026-11-16; USD thousands, as published). The script runs offline from that frozen panel.
