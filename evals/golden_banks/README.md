# Golden banks

One end-to-end signal that intake, reconciliation, risk and decisions still work
for a realistic bank. Two synthetic banks are taken through the public HTTP API
and every output is compared with reviewed golden values. The same banks are the
test bed for the `foundry/` onboarding skills.

| Bank | Size | Shape | Deliberate difficulty |
|---|---|---|---|
| `prairie-20b` | $20B | Deposit-funded, fixed-rate lending, 20% securities | Retail extract uses core-system column names and needs a column map |
| `harbor-120b` | $120B | 30% securities, floating commercial loans, wholesale and brokered funding | 62 rows with missing fields that need explicit defaults; two ledger balances with no position extract |

All data is synthetic and describes no real institution. Retail tape rows are
servicing-pool level, not individual borrowers.

## Run it

```bash
make build-native     # once, or after native changes
make golden           # both banks, about two minutes locally
uv run --project apps/api python evals/golden_banks/suite.py harbor-120b --report report.json
```

Each bank gets a private API process, worker process, SQLite store and object
folder, so a run never touches a developer workspace. The steps are:

1. Load settings, market, a named market scenario and the non-tape extracts
   (`PUT /settings`, `/market`, `/scenarios/{name}`, `/books/{name}`).
2. Import the retail tape, adopt it, build cohorts and publish mortgage and
   deposit positions (`/cohorts/imports`, `/tapes`, `/builds`, `/publish`).
3. Reconcile published books to `trial_balance.csv` by GL account.
4. Run risk, NII, stress and KPIs on the base market; NII, KPIs and stress under
   the scenario; the optimizer; the unit library and a strategy evaluation.

The runner transports and compares. Its only arithmetic is summing published
balances for the reconciliation.

## What is compared

Every result is reduced to named scalars: row counts, the sum of each numeric
column (with its L1 norm as the comparison scale), distinct counts of text
columns, and every scalar in the result. A bank has about 465 metrics. A metric
that disappears or appears is a difference, so output-contract changes show up.

| Metrics | Relative | Absolute | Why |
|---|---|---|---|
| `intake.*` | exact | exact | Row counts and source hash |
| `cohorts.*`, `publication.*` | 1e-12 | 0 | Balances must be conserved to double rounding |
| `reconciliation.*` | 1e-12 | $0.005 | Books tie to the ledger to the cent |
| `optimize.*` | 1e-5 | $1 | LP objective and allocation after solver tolerances |
| everything else | 1e-7 | 1e-6 | Model outputs; allows cross-platform math-library roundoff only |

Relative tolerance is applied to the larger L1 scale, so a sum of offsetting
values is not held to an impossible precision. Solver iteration counts,
residuals and the list of constraints reported as binding are not compared:
they depend on how the platform breaks ties. Shadow prices, allocations and
the objective are. These tolerances are for this suite's aggregates;
they do not replace or loosen any engine test tolerance.

## Updating goldens

An intended engine change that moves outputs fails this check. Update with the
reviewed command and commit the resulting diff with the change that caused it:

```bash
make golden-update
git diff --stat evals/golden_banks/banks/*/expected.json
```

The diff lists each moved metric, old and new. A reviewer should be able to
explain every line. Never edit `expected.json` by hand, and never update goldens
to make an unexplained difference pass.

To change a bank itself, edit its `bank.json` and regenerate the extracts and
trial balance, then update goldens:

```bash
python evals/golden_banks/generate.py
```

`scripts/tests/test_golden_banks.py` fails if committed extracts differ from the
generator's output.

## Files per bank

- `bank.json`: generator inputs, run settings, market, scenario, optimizer
  limits, cohort defaults, reconciliation rules and the gate answer key.
- `extracts/retail_tape.csv`: mortgage and retail deposit rows for tape intake.
- `extracts/{securities,commercial_loans,long_term_debt,wholesale_deposits,time_deposits,money_market}.csv`:
  rows for the `mbs`, `loans`, `debt`, `deposits`, `cds` and `mm` books.
- `trial_balance.csv`: the general ledger the books must tie to.
- `expected.json`: golden metrics as `[value, scale]`.

GL account map used by the reconciliation:

| Account | Book rows |
|---|---|
| 1010 money-market assets | `mm`, side `asset` |
| 1200 agency MBS | `mbs` rows not published from the tape |
| 1300 residential mortgages | `mbs` rows published from the tape |
| 1400 commercial loans | `loans` |
| 2010 retail deposits | `deposits` rows published from the tape |
| 2020 wholesale deposits | `deposits` rows not published from the tape |
| 2100 time deposits | `cds` |
| 2200 short-term borrowings | `mm`, side `liability` |
| 2300 long-term debt | `debt` |
| 3000 equity | not a book; excluded |

Harbor's accounts 1390 and 2090 have no extract. They are the expected breaks;
any other break, or a different amount, fails the run.

## Agent harness

`harness.py` evaluates the foundry skills on the same banks. It hands an agent a
pod and scores the gate records the agent drafts; it never calls a model itself.

```bash
python evals/golden_banks/harness.py run harbor-120b --agent "<your agent command>"
python evals/golden_banks/harness.py score harbor-120b path/to/pod
python evals/golden_banks/harness.py reference harbor-120b path/to/pod   # the answer key as drafts
```

`run` starts a private API, writes a pod (pod template, extracts, trial balance,
`TASK.md`), runs the command in the pod with `GOLDEN_POD_DIR` and
`WORKBENCH_MCP_API_URL` set, then scores `evidence/gates/G1..G3`.

A gate draft scores zero if it is malformed or if it sets `decision`, `approver`
or `decided_at`: agents draft, people approve. Otherwise the score is
50% recall of the bank's known exceptions, 35% precision (invented exceptions
cost points) and 15% for proposing a disposition instead of leaving a known
item `open`. A data exception matches when it names the field and row count; a
reconciliation break matches when it names the GL account and the amount to the
cent. Each gate needs 0.8 to pass. G4 is a narrative pack and is not scored.

## Limits

- Synthetic banks show that the path is intact and stable. They are not evidence
  of predictive accuracy, accounting acceptance or regulatory adequacy.
- The valuation date, hedge book and behavioural histories come from the seeded
  research workspace; no public route sets them, so they are the same for both
  banks.
- Only mortgage and deposit tape products are published; card, auto and personal
  cohorts have no dedicated behavioural pricer yet.
- Branch protection decides whether the `Golden banks` check is required on
  `main`; that is a repository setting, not part of this directory.
