"""Golden-bank end-to-end evaluation through the public HTTP API.

    uv run --project apps/api python evals/golden_banks/suite.py            # compare with goldens
    uv run --project apps/api python evals/golden_banks/suite.py --update   # reviewed golden update

Each bank runs in a fresh durable workspace served by a real API and worker
process: tape import, cohort build, publication, trial-balance reconciliation,
risk, NII, stress, KPIs, a named market scenario, the optimizer and a strategy
evaluation. The runner only transports and compares; it performs no financial
calculation beyond summing published balances for the reconciliation.
"""
from __future__ import annotations

import argparse
import base64
import csv
import io
import json
import math
import os
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from decimal import Decimal
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
BANKS = HERE / 'banks'
TAPE_ID = 'retail'
BOOK_EXTRACTS = {'mbs': 'securities', 'loans': 'commercial_loans', 'debt': 'long_term_debt', 'deposits': 'wholesale_deposits',
                 'cds': 'time_deposits', 'mm': 'money_market'}
BALANCE = {'mbs': 'current_face', 'loans': 'face', 'debt': 'face', 'deposits': 'balance', 'cds': 'balance', 'mm': 'balance'}
# First matching prefix wins. Conservation metrics are exact to double rounding; model
# outputs allow cross-platform libm differences only. See README.md before changing these.
TOLERANCES = [('intake.', 0., 0.), ('cohorts.', 1e-12, 0.), ('publication.', 1e-12, 0.), ('reconciliation.', 1e-12, .005),
              ('optimize.', 1e-5, 1.), ('', 1e-7, 1e-6)]


class Failure(RuntimeError):
    pass


class Client:
    def __init__(self, base, token=None):
        self.base, self.token = base.rstrip('/'), token

    def call(self, method, path, body=None, raw=False, timeout=600):
        headers = {'content-type': 'application/json'} | ({'authorization': f'Bearer {self.token}'} if self.token else {})
        request = urllib.request.Request(self.base + path, method=method, headers=headers,
                                         data=None if body is None else json.dumps(body, allow_nan=False).encode())
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                data = response.read()
        except urllib.error.HTTPError as exc:
            raise Failure(f'{method} {path} -> {exc.code}: {exc.read().decode(errors="replace")[:800]}') from None
        return data if raw else json.loads(data)

    def job(self, method, path, body=None, timeout=1500):
        status = self.call(method, path, body)
        deadline = time.monotonic() + timeout
        while status['status'] not in ('done', 'error', 'failed', 'cancelled'):
            if time.monotonic() > deadline: raise Failure(f'{path} job {status["id"]} did not finish in {timeout}s')
            time.sleep(.25)
            status = self.call('GET', f'/jobs/{status["id"]}')
        if status['status'] != 'done': raise Failure(f'{path} job {status["id"]} {status["status"]}: {status.get("detail")}')
        return status['id']

    def result(self, jid):
        return envelope(self.call('GET', f'/jobs/{jid}/result', raw=True))

    def rows(self, book):
        value = envelope(self.call('GET', f'/books/{book}', raw=True))
        return value.to_dicts() if hasattr(value, 'to_dicts') else []


def envelope(data):
    """Decode an ARW1 envelope into its JSON skeleton with frames substituted in place."""
    import polars as pl
    if data[:4] != b'ARW1': raise Failure('result is not an ARW1 envelope')
    size = struct.unpack('<I', data[4:8])[0]
    skeleton, pos = json.loads(data[8:8 + size]), 8 + size
    count = struct.unpack('<I', data[pos:pos + 4])[0]
    lengths = struct.unpack(f'<{count}I', data[pos + 4:pos + 4 + 4 * count])
    pos += 4 + 4 * count
    frames = []
    for length in lengths:
        frames.append(pl.read_ipc(io.BytesIO(data[pos:pos + length])))
        pos += length

    def place(node):
        if isinstance(node, dict):
            if set(node) == {'__arrow__'}: return frames[node['__arrow__']]
            return {k: place(v) for k, v in node.items()}
        return [place(v) for v in node] if isinstance(node, list) else node
    return place(skeleton)


def flatten(prefix, node, out):
    """Reduce any result to named scalars: [value, scale]. Scale is the L1 norm behind a sum."""
    import polars as pl
    if isinstance(node, pl.DataFrame):
        out[f'{prefix}.rows'] = [len(node), None]
        for name, dtype in node.schema.items():
            column = node[name]
            if dtype.is_numeric():
                values = column.drop_nulls().cast(pl.Float64)
                out[f'{prefix}.{name}.sum'] = [float(values.sum()), float(values.abs().sum())]
            elif dtype == pl.String:
                out[f'{prefix}.{name}.distinct'] = [column.n_unique(), None]
    elif isinstance(node, dict):
        for key in sorted(node): flatten(f'{prefix}.{key}', node[key], out)
    elif isinstance(node, list):
        if node and all(isinstance(v, dict) for v in node):
            keys = sorted({k for v in node for k in v})
            out[f'{prefix}.rows'] = [len(node), None]
            for key in keys:
                values = [v[key] for v in node if isinstance(v.get(key), (int, float)) and not isinstance(v.get(key), bool)]
                if values: out[f'{prefix}.{key}.sum'] = [float(sum(values)), float(sum(abs(v) for v in values))]
        elif node and all(isinstance(v, (int, float)) and not isinstance(v, bool) for v in node):
            out[f'{prefix}.sum'] = [float(sum(node)), float(sum(abs(v) for v in node))]
            out[f'{prefix}.len'] = [len(node), None]
        elif node and all(isinstance(v, str) for v in node):
            import hashlib
            out[f'{prefix}.len'] = [len(node), None]
            out[f'{prefix}.sha256'] = [hashlib.sha256(json.dumps(node).encode()).hexdigest()[:16], None]
        else:
            for i, value in enumerate(node): flatten(f'{prefix}[{i}]', value, out)
    elif isinstance(node, bool) or (isinstance(node, str) and len(node) <= 64):
        out[prefix] = [node, None]
    elif isinstance(node, (int, float)):
        out[prefix] = [float(node), abs(float(node))]


def table(path):
    with open(path, newline='', encoding='utf-8') as handle:
        return list(csv.DictReader(handle))


def typed(rows):
    """CSV extract rows as JSON book rows; numeric text becomes numbers, blanks are omitted."""
    out = []
    for row in rows:
        item = {}
        for key, value in row.items():
            if value == '': continue
            try: item[key] = float(value) if key not in ('id', 'cusip', 'maturity') else value
            except ValueError: item[key] = value
        out.append(item)
    return out


def execute(client, bank):
    """Run one bank end to end and return its metrics."""
    folder, spec = BANKS / bank, json.loads((BANKS / bank / 'bank.json').read_text(encoding='utf-8'))
    metrics, log = {}, lambda text: print(f'[{bank}] {text}', flush=True)

    # 1. Assumptions and position extracts that are not tape-driven.
    client.call('PUT', '/settings', spec['settings'])
    client.call('PUT', '/market', spec['market'])
    client.call('PUT', f'/scenarios/{spec["scenario"]["name"]}', spec['scenario'])
    for book, extract in BOOK_EXTRACTS.items():
        client.call('PUT', f'/books/{book}', typed(table(folder / 'extracts' / f'{extract}.csv')))
    log('assumptions and extracts loaded')

    # 2. Tape intake, cohort build, publication.
    tape = (folder / 'extracts' / 'retail_tape.csv').read_bytes()
    imported = client.job('POST', '/cohorts/imports', dict(name='retail_tape.csv', format='csv', content_base64=base64.b64encode(tape).decode()))
    revision = client.call('GET', '/cohorts/presets')['revision']
    client.call('PUT', f'/cohorts/tapes/{TAPE_ID}', dict(job_id=imported, expected_revision=revision))
    presets = client.call('GET', '/cohorts/presets')
    held = presets['tapes'][TAPE_ID]
    metrics['intake.rows'], metrics['intake.columns'] = [held['rows'], None], [len(held['columns']), None]
    import hashlib
    metrics['intake.sha256_matches_extract'] = [held['sha256'] == hashlib.sha256(tape).hexdigest(), None]
    headers = spec['generator']['tape'].get('headers', {})
    config = presets['config'] | dict(column_map={k: v for k, v in headers.items() if k != v}, defaults=spec['cohort_defaults'])
    config['rules']['mortgage']['averages'].append('book_yield')  # the saved securities book carries historical yields
    build = client.job('POST', '/cohorts/builds', dict(tape_id=TAPE_ID, config=config, expected_revision=presets['revision']))
    summary = client.call('GET', f'/cohorts/builds/{build}/summary')
    flatten('cohorts.summary', {k: v for k, v in summary['summary'].items() if k != 'backend'}, metrics)
    metrics['cohorts.pricing_supported'] = [all(v['supported'] for v in summary.get('pricing_support', {}).values()), None]
    revision = client.call('GET', '/cohorts/presets')['revision']
    published = client.call('PUT', f'/cohorts/builds/{build}/publish', dict(products=['mortgage', 'deposit'], expected_revision=revision, mode='replace_tape'))
    flatten('publication.receipt', published.get('books', {}), metrics)
    listing = client.call('GET', '/books')
    for book, item in sorted(listing.items()):
        metrics[f'publication.books.{book}.positions'] = [item['positions'], None]
        metrics[f'publication.books.{book}.balance'] = [item['balance'], abs(item['balance'])]
    log(f'published {summary["summary"]["loans"]} tape rows as {summary["summary"]["cohorts"]} cohorts')

    # 3. Reconciliation: published books by GL account against the bank's trial balance.
    books = {book: client.rows(book) for book in BOOK_EXTRACTS}
    def total(book, keep=lambda r: True):
        return float(sum(Decimal(repr(float(r[BALANCE[book]]))) for r in books[book] if keep(r)))
    from_tape = lambda r: r.get('source_tape_id') == TAPE_ID
    booked = {'1010': total('mm', lambda r: r['side'] == 'asset'), '1200': total('mbs', lambda r: not from_tape(r)),
              '1300': total('mbs', from_tape), '1400': total('loans'), '2010': total('deposits', from_tape),
              '2020': total('deposits', lambda r: not from_tape(r)), '2100': total('cds'),
              '2200': total('mm', lambda r: r['side'] == 'liability'), '2300': total('debt')}
    rules, breaks = spec['reconciliation'], {}
    for row in table(folder / 'trial_balance.csv'):
        account, ledger = row['gl_account'], float(row['balance'])
        if account in rules['not_in_books']: continue
        difference = ledger - booked.get(account, 0.)
        metrics[f'reconciliation.{account}.ledger'] = [ledger, abs(ledger)]
        metrics[f'reconciliation.{account}.book'] = [booked.get(account, 0.), abs(booked.get(account, 0.))]
        metrics[f'reconciliation.{account}.break'] = [difference, max(abs(ledger), 1.)]
        if abs(difference) > rules['tolerance_abs_usd']: breaks[account] = difference
    expected = rules['expected_breaks']
    unexplained = sorted(a for a in set(breaks) | set(expected) if abs(breaks.get(a, 0.) - expected.get(a, 0.)) > rules['tolerance_abs_usd'])
    metrics['reconciliation.unexplained_accounts'] = [len(unexplained), None]
    if unexplained: log(f'UNEXPLAINED reconciliation breaks: {unexplained}')
    log(f'reconciled {len(booked)} accounts; {len(breaks)} documented break(s)')

    # 4. Risk, income, stress, capital; then the named scenario; then decisions.
    for name, body in (('risk', dict(kind='risk')), ('nii', dict(kind='nii')), ('stress', dict(kind='stress')), ('kpis', dict(kind='kpis')),
                       ('scenario_nii', dict(kind='nii', scenario=spec['scenario']['name'])),
                       ('scenario_kpis', dict(kind='kpis', scenario=spec['scenario']['name'])),
                       ('scenario_stress', dict(kind='stress', scenario=spec['scenario']['name']))):
        started = time.monotonic()
        flatten(name, client.result(client.job('POST', '/run', body)), metrics)
        log(f'{name} {time.monotonic() - started:.1f}s')
    started = time.monotonic()
    flatten('optimize', client.result(client.job('POST', '/optimize', spec['optimize'])), metrics)
    log(f'optimize {time.monotonic() - started:.1f}s')
    flatten('unitlib', client.result(client.job('POST', '/run', dict(kind='unitlib'))), metrics)
    flatten('strategy_eval', envelope(client.call('POST', '/strategy/eval', spec['strategy'], raw=True)), metrics)
    metrics = {k: v for k, v in metrics.items() if not ignored(k)}
    log(f'{len(metrics)} metrics')
    return metrics, dict(build_job=build, breaks=breaks, summary=summary['summary'], revision=client.call('GET', '/cohorts/presets')['revision'])


def tolerance(name):
    return next((r, a) for prefix, r, a in TOLERANCES if name.startswith(prefix))


def ignored(name):
    """Solver work counters and residuals depend on the platform's pivoting order, not on the economics."""
    return name.startswith(('optimize.solver.', 'optimize.native_max_violation'))


def compare(expected, actual):
    """Per-metric differences. Missing and unexpected metrics are differences too."""
    diffs = []
    for name in sorted(n for n in set(expected) | set(actual) if not ignored(n)):
        if name not in actual: diffs.append((name, expected[name][0], None, 'missing from this run')); continue
        if name not in expected: diffs.append((name, None, actual[name][0], 'not in golden')); continue
        (want, scale_e), (got, scale_a) = expected[name], actual[name]
        if isinstance(want, (bool, str)) or isinstance(got, (bool, str)) or scale_e is None:
            if want != got: diffs.append((name, want, got, 'exact'))
            continue
        rtol, atol = tolerance(name)
        allowed = rtol * max(scale_e, scale_a or 0.) + atol
        if not (math.isfinite(got) and abs(got - want) <= allowed):
            diffs.append((name, want, got, f'|diff|={abs(got - want):.6g} > {allowed:.6g}'))
    return diffs


class Stack:
    """A private API and worker on a temporary SQLite store and object folder."""
    def __enter__(self):
        self.folder = tempfile.mkdtemp(prefix='golden-bank-')
        ports = []
        for _ in range(2):
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0)); ports.append(sock.getsockname()[1])
        env = os.environ | dict(WORKBENCH_EXECUTION='external', WORKBENCH_ENV='development', WORKBENCH_SEED_DEMO='1',
            DATABASE_URL='sqlite:///' + os.path.join(self.folder, 'state.db').replace('\\', '/'), ARTIFACT_URL=os.path.join(self.folder, 'objects'),
            WORKBENCH_TENANT_ID='golden', WORKBENCH_WORKSPACE_ID='bank', WORKBENCH_WORKER_URL=f'http://127.0.0.1:{ports[1]}',
            WORKBENCH_WORKER_TOKEN='', WORKBENCH_API_TOKEN='', NUMBA_NUM_THREADS='2')
        self.processes = [subprocess.Popen([sys.executable, '-m', 'uvicorn', f'app.{module}:app', '--host', '127.0.0.1', '--port', str(port)],
            cwd=REPO / 'apps' / 'api', env=env, stdout=open(os.path.join(self.folder, f'{module}.log'), 'w'), stderr=subprocess.STDOUT)
            for module, port in zip(('main', 'worker'), ports)]
        self.url = f'http://127.0.0.1:{ports[0]}'
        deadline = time.monotonic() + 180
        while True:
            try:
                urllib.request.urlopen(self.url + '/health', timeout=2).read()
                urllib.request.urlopen(f'http://127.0.0.1:{ports[1]}/state', timeout=2).read()
                return self
            except OSError:
                if time.monotonic() > deadline or any(p.poll() is not None for p in self.processes):
                    self.__exit__(None, None, None)
                    raise Failure(f'API or worker did not start; logs in {self.folder}')
                time.sleep(.5)

    def __exit__(self, kind, *_):
        for process in self.processes: process.terminate()
        for process in self.processes:
            try: process.wait(20)
            except subprocess.TimeoutExpired: process.kill()
        if kind is not None: print(f'server logs kept in {self.folder}', file=sys.stderr)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('banks', nargs='*', help='bank ids (default: all)')
    parser.add_argument('--update', action='store_true', help='rewrite expected.json from this run; review the diff before committing')
    parser.add_argument('--report', type=Path, help='write the per-metric comparison as JSON')
    args = parser.parse_args(argv)
    names = args.banks or sorted(p.name for p in BANKS.iterdir() if (p / 'bank.json').exists())
    report, failed = {}, False
    for bank in names:
        started = time.monotonic()
        with Stack() as stack:
            metrics, _ = execute(Client(stack.url), bank)
        path = BANKS / bank / 'expected.json'
        if args.update:
            lines = [f' {json.dumps(k)}: {json.dumps(metrics[k], allow_nan=False)}' for k in sorted(metrics)]
            path.write_text('{\n' + ',\n'.join(lines) + '\n}\n', encoding='utf-8', newline='\n')
            print(f'[{bank}] wrote {len(metrics)} golden metrics to {path.relative_to(REPO)}')
            continue
        if not path.exists(): raise Failure(f'{bank} has no expected.json; run with --update and review it')
        diffs = compare(json.loads(path.read_text(encoding='utf-8')), metrics)
        report[bank] = dict(metrics=len(metrics), seconds=round(time.monotonic() - started, 1),
                            differences=[dict(metric=n, expected=w, actual=g, reason=r) for n, w, g, r in diffs])
        print(f'[{bank}] {len(metrics) - len(diffs)}/{len(metrics)} metrics match in {time.monotonic() - started:.0f}s')
        for name, want, got, reason in diffs[:200]:
            print(f'  DIFF {name}: expected {want!r}, got {got!r} ({reason})')
        failed |= bool(diffs)
    if args.report:
        args.report.write_text(json.dumps(report, indent=1), encoding='utf-8')
    return 1 if failed else 0


if __name__ == '__main__':
    try: sys.exit(main())
    except Failure as exc:
        print(f'golden-bank run failed: {exc}', file=sys.stderr)
        sys.exit(2)
