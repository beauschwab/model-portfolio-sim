"""Opt-in full Rust daily simulation with immutable partitioned Parquet output.

The Python reference can use the same sink and independent persisted replay.
No manifest is published before every partition and closing GL are verified.
This local engine runner is separate from API worker admission/publication.
"""
from collections import defaultdict
from copy import deepcopy
from dataclasses import asdict
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import uuid

import polars as pl

from .balance_stress import validate, _simulate, Scenario, MODEL_VERSION
from .balance_rules import RULESET
from .journal import Journal, SCHEMA as JOURNAL_SCHEMA, statements


def _schema(strings='', floats='', integers='', booleans=''):
    return {**dict.fromkeys(strings.split(), pl.String), **dict.fromkeys(floats.split(), pl.Float64),
            **dict.fromkeys(integers.split(), pl.Int64), **dict.fromkeys(booleans.split(), pl.Boolean)}


SUMMARY = _schema('scenario account entity currency', 'final_cash final_equity minimum_cash peak_cash_shortfall max_reconciliation_error',
                  'first_cash_breach_day first_capital_breach_day first_leverage_breach_day first_lcr_breach_day first_nsfr_breach_day first_htm_breach_day')
SCHEMAS = {
    'journal': JOURNAL_SCHEMA,
    'summary': SUMMARY,
    'reverse_grid': dict(SUMMARY, severity=pl.Float64, breached=pl.Boolean),
    'path': _schema('scenario account entity currency', 'cash restricted_cash cash_floor assets liabilities equity aoci cet1 rwa cet1_ratio leverage_ratio usable_collateral undrawn hqla_proxy lcr_proxy nsfr_proxy reconciliation_error cash_headroom cet1_headroom leverage_headroom lcr_headroom nsfr_headroom htm_headroom', 'day'),
    'ledger': _schema('scenario account event', 'cash earnings aoci memo_amount', 'day'),
    'actions': _schema('scenario policy account destination kind status reason', 'amount', 'day'),
    'breaches': _schema('scenario account metric', '', 'day'),
    'exposures': _schema('scenario account position kind', 'balance allowance watch_fraction commitment market_factor encumbered_fraction accrued_interest', 'day'),
    'trial_balance': _schema('scenario account gl_account instrument_id', 'balance'),
    'funding_claims': _schema('scenario claim_id account position', 'face balance rate', 'due'),
    'attribution': _schema('scenario account event', 'cash_delta earnings_delta aoci_delta'),
    'closing_statements': _schema('scenario account currency', 'assets liabilities equity cash restricted_cash intercompany_assets intercompany_liabilities reconciliation_error'),
    'consolidated': _schema('scenario currency', 'assets liabilities equity cash restricted_cash'),
}


def binary_path():
    override = os.environ.get('PORTFOLIO_BALANCE_RUST_BIN')
    if override:
        return Path(override).resolve()
    name = 'portfolio-balance.exe' if sys.platform == 'win32' else 'portfolio-balance'
    return Path(__file__).resolve().parents[4]/'portfolio-ledger-native'/'target'/'release'/name


def _hash(path):
    digest=hashlib.sha256()
    with path.open('rb') as handle:
        for block in iter(lambda:handle.read(1024*1024),b''):
            digest.update(block)
    return digest.hexdigest()


def source_identity():
    root=Path(__file__).resolve().parents[1]
    digest=hashlib.sha256()
    for path in sorted(root.rglob('*.py')):
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(path.read_bytes().replace(b'\r\n',b'\n'))
    return digest.hexdigest()


class PartitionWriter:
    def __init__(self, root, partition_rows=65536):
        self.root = Path(root)
        self.partition_rows = partition_rows
        self.buffers = defaultdict(list)
        self.counts = defaultdict(int)
        self.parts = defaultdict(list)

    def add(self, table, rows):
        if table not in SCHEMAS:
            raise ValueError(f'unknown native output table: {table}')
        if isinstance(rows, pl.DataFrame):
            frame = rows.select(list(SCHEMAS[table])).cast(SCHEMAS[table]) if rows.height else pl.DataFrame(schema=SCHEMAS[table])
        else:
            if any(set(r) != set(SCHEMAS[table]) for r in rows):
                raise ValueError(f'invalid output columns: {table}')
            frame = pl.DataFrame(rows, schema=SCHEMAS[table], infer_schema_length=None)
        if frame.height:
            self.buffers[table].append(frame)
            self.counts[table] += frame.height
        if self.counts[table] >= self.partition_rows:
            self.flush(table)

    def flush(self, table):
        frames = self.buffers.pop(table, [])
        self.counts[table] = 0
        if not frames:
            return
        frame = pl.concat(frames)
        for start in range(0, frame.height, self.partition_rows):
            part = frame.slice(start, self.partition_rows)
            name = f'{table}-{len(self.parts[table]):06d}.parquet'
            path = self.root/name
            part.write_parquet(path)
            # Finish/flush each immutable object before any manifest can refer to it.
            with path.open('r+b') as handle:
                os.fsync(handle.fileno())
            self.parts[table].append(dict(path=name, rows=part.height, bytes=path.stat().st_size, sha256=_hash(path)))

    def finish(self):
        for table in list(self.buffers):
            self.flush(table)

    def frames(self, table):
        for part in self.parts[table]:
            path = self.root/part['path']
            if _hash(path) != part['sha256']:
                raise ValueError('partition checksum mismatch')
            frame = pl.read_parquet(path)
            if frame.height != part['rows'] or frame.schema != SCHEMAS[table]:
                raise ValueError('partition rows/schema mismatch')
            yield frame


class PartitionJournal(Journal):
    """Original Python event/posting implementation, with bounded row retention."""
    def __init__(self, scenario, writer, cancelled=None):
        super().__init__(scenario)
        self.writer = writer
        self.cancelled = cancelled

    def verify(self, expected):
        if self.cancelled and self.cancelled():
            raise InterruptedError('simulation cancelled')
        super().verify(expected)

    def post(self, day, account, event, changes):
        super().post(day, account, event, changes)
        if len(self.rows) >= (self.writer.partition_rows if self.writer else 65536):
            self.flush()

    def flush(self):
        if self.writer and self.rows:
            self.writer.add('journal', self.rows)
        self.rows = []

    def close(self):
        self.flush()
        # Independent reconstruction is from saved partitions, before publication.
        return [], [dict(scenario=self.scenario, account=a, gl_account=g, instrument_id=i, balance=v)
                    for (a,g,i),v in sorted(self.balances.items())]


def replay_partitions(frames):
    """Ordered, independent Python replay; memory is GL keys + one transaction.

    Transactions can cross partitions. No list of all rows or transactions is held.
    Sequence checks reject missing, reordered and duplicated transaction batches.
    """
    balances, sequences = defaultdict(float), defaultdict(int)
    current, amounts, scale, count = None, [], 0., 0
    last_day = defaultdict(int)
    def check():
        if not math.isfinite(scale) or abs(math.fsum(amounts)) > 1e-9*max(1., scale):
            raise ArithmeticError('unbalanced persisted journal transaction')
    columns = list(JOURNAL_SCHEMA)
    for frame in frames:
        for tx, scenario, day, account, event, gl, instrument, debit, credit in frame.select(columns).iter_rows():
            if not math.isfinite(debit) or not math.isfinite(credit) or debit < 0 or credit < 0 or (debit and credit):
                raise ValueError('invalid persisted journal debit/credit')
            key = (tx, scenario, account, day, event)
            if key != current:
                if current is not None:
                    check()
                sequences[scenario] += 1
                if tx != f'{scenario}:{sequences[scenario]}' or day < last_day[scenario]:
                    raise ValueError('journal transaction sequence mismatch')
                last_day[scenario] = day
                current, amounts, scale = key, [], 0.
            value = debit-credit
            balances[scenario, account, gl, instrument] += value
            if not math.isfinite(balances[scenario, account, gl, instrument]):
                raise ArithmeticError('persisted GL overflow')
            amounts.append(value)
            scale += abs(value)
            count += 1
    if current is not None:
        check()
    return dict(balances), count


def _python(spec, writer, progress, cancelled):
    scenarios = [Scenario('baseline')] + spec['scenarios']
    for scenario in scenarios:
        if cancelled and cancelled():
            raise InterruptedError('simulation cancelled')
        if progress:
            progress(scenario.name)
        def factory(name):
            return PartitionJournal(name, writer, cancelled)
        result = _simulate(spec, scenario, journal_factory=factory)
        for table, rows in result.items():
            writer.add(table, rows)
    for scenario in spec['scenarios']:
        for severity in spec['reverse_severities']:
            if cancelled and cancelled():
                raise InterruptedError('simulation cancelled')
            result = _simulate(spec, scenario, severity=severity, detail=False,
                               journal_factory=lambda name: PartitionJournal(name, None, cancelled))
            writer.add('reverse_grid', [dict(r, severity=severity, breached=any(v is not None for k,v in r.items() if k.startswith('first_'))) for r in result['summary']])


def _rust(spec, writer, progress, cancelled, timeout, *, executable=None, workflow=None, consume_input=False):
    binary = executable or binary_path()
    if not binary.is_file():
        raise RuntimeError('Rust state engine is not built; run scripts/build_ledger_native.py. No fallback was used.')
    normalized = {k: [asdict(x) if hasattr(x, '__dataclass_fields__') else x for x in v] if isinstance(v,list) else v for k,v in spec.items()}
    # File input avoids a pipe deadlock on large requests while the child emits output.
    request = writer.root/'request.json'
    import datetime
    def encode(value):
        if isinstance(value, datetime.date): return value.toordinal()
        raise TypeError(f'Unsupported native input {type(value).__name__}')
    request.write_text(json.dumps(normalized, allow_nan=False, default=encode, separators=(',', ':'))+'\n', encoding='utf-8')
    del normalized
    if consume_input:
        # Only private workflow request dictionaries opt in. The immutable file
        # is now the child's input; release duplicate transport rows before output.
        spec.clear()
    started = time.monotonic()
    dictionary = []
    with request.open('rb') as source, (writer.root/'native-stderr.log').open('wb') as errors:
        process = subprocess.Popen([str(binary)], stdin=source, stdout=subprocess.PIPE, stderr=errors,
                                   creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        complete = False
        stop = threading.Event()
        interruption = []
        def watchdog():
            while not stop.wait(.05):
                try:
                    if cancelled and cancelled():
                        interruption.append(InterruptedError('simulation cancelled'))
                    elif time.monotonic()-started > timeout:
                        interruption.append(TimeoutError('native simulation timed out'))
                except Exception as exc:
                    interruption.append(exc)
                if interruption:
                    if process.poll() is None:
                        process.kill()
                    return
        monitor=threading.Thread(target=watchdog,daemon=True)
        monitor.start()
        try:
            for line in process.stdout:
                if cancelled and cancelled():
                    raise InterruptedError('simulation cancelled')
                if time.monotonic()-started > timeout:
                    raise TimeoutError('native simulation timed out')
                message = json.loads(line)
                if 'complete' in message:
                    if complete or message != {'complete':True, 'protocol':2}:
                        raise ValueError('invalid completion message')
                    complete = True
                elif complete:
                    raise ValueError('native output follows completion')
                elif 'workflow' in message:
                    if workflow is None or workflow or dictionary or writer.counts:
                        raise ValueError('invalid workflow header')
                    workflow.update(message['workflow'])
                else:
                    if workflow is not None and not workflow: raise ValueError('missing workflow header')
                    if message['table']=='journal':
                        dictionary.extend(message['dictionary'])
                        columns=message['columns']
                        labels=pl.Series(dictionary,dtype=pl.String)
                        frame=pl.DataFrame({k: labels.gather(columns[k]) for k in
                                            ['scenario','account','event','gl_account','instrument_id']})
                        frame=frame.with_columns(pl.Series('day',columns['day'],dtype=pl.Int64),
                                                 pl.Series('transaction_number',columns['transaction_number'],dtype=pl.Int64),
                                                 pl.Series('value',columns['value'],dtype=pl.Float64))
                        frame=frame.with_columns(pl.concat_str('scenario',pl.lit(':'),pl.col('transaction_number').cast(pl.String)).alias('transaction_id'),
                                                 pl.col('value').clip(lower_bound=0.).alias('debit'),
                                                 (-pl.col('value')).clip(lower_bound=0.).alias('credit')).select(list(JOURNAL_SCHEMA))
                        writer.add('journal',frame)
                    else:
                        writer.add(message['table'], message['rows'])
                    if progress:
                        progress(message['table'])
            code = process.wait(timeout=max(1.,timeout-(time.monotonic()-started)))
            if interruption:
                raise interruption[0]
            if code or not complete:
                raise RuntimeError(f'native state engine failed (exit {code}): '+(writer.root/'native-stderr.log').read_text(errors='replace')[-2000:])
        finally:
            stop.set();monitor.join()
            if process.poll() is None:
                process.kill()
            process.wait()
            process.stdout.close()
    if workflow is None: request.unlink()
    else: request.rename(writer.root/'workflow-input.json')
    (writer.root/'native-stderr.log').unlink()


def _finalize(spec, writer, cancelled=None, *, native_reports=False):
    if cancelled and cancelled(): raise InterruptedError('simulation cancelled')
    writer.finish()
    def checked_frames(table):
        for frame in writer.frames(table):
            if cancelled and cancelled():
                raise InterruptedError('simulation cancelled')
            yield frame
    reconstructed, journal_rows = replay_partitions(checked_frames('journal'))
    trial = []
    for frame in checked_frames('trial_balance'):
        trial.extend(frame.to_dicts())
    if len(trial) != len(reconstructed):
        raise ArithmeticError('persisted GL key set mismatch')
    seen=set()
    for r in trial:
        key = tuple(r[k] for k in ('scenario','account','gl_account','instrument_id'))
        if key in seen or key not in reconstructed or not math.isclose(reconstructed[key], r['balance'], rel_tol=1e-12, abs_tol=1e-9):
            raise ArithmeticError('persisted closing GL mismatch')
        seen.add(key)
    if native_reports:
        expected = {(name, a.id) for name in ['baseline']+[s.name for s in spec['scenarios']] for a in spec['accounts']}
        actual = set()
        for frame in checked_frames('closing_statements'):
            for row in frame.iter_rows(named=True):
                key = row['scenario'], row['account']
                if key in actual or key not in expected:
                    raise ArithmeticError('native closing statement key mismatch')
                actual.add(key)
                values = [row[k] for k in ('assets','liabilities','equity','cash','restricted_cash','intercompany_assets','intercompany_liabilities','reconciliation_error')]
                if not all(v is not None and math.isfinite(v) for v in values):
                    raise ArithmeticError('nonfinite native closing statement')
                if abs(values[0]-values[1]-values[2]) > 1e-8*max(1.,abs(values[0]),abs(values[1])):
                    raise ArithmeticError('unbalanced native closing statement')
        if actual != expected:
            raise ArithmeticError('missing native closing statement')
    else:
        names = ['baseline']+[s.name for s in spec['scenarios']]
        closing, consolidated = statements(trial, {a.id:a for a in spec['accounts']}, names)
        writer.add('closing_statements', closing)
        writer.add('consolidated', consolidated)
        totals = defaultdict(lambda: [0.,0.,0.])
        for frame in checked_frames('ledger'):
            for scenario, account, event, cash, earnings, oci in frame.select('scenario','account','event','cash','earnings','aoci').iter_rows():
                v=totals[scenario,account,event];v[0]+=cash;v[1]+=earnings;v[2]+=oci
        keys = sorted({(account,event) for _,account,event in totals})
        attribution=[]
        for scenario in names[1:]:
            for account,event in keys:
                base,stress=totals['baseline',account,event],totals[scenario,account,event]
                attribution.append(dict(scenario=scenario,account=account,event=event,cash_delta=stress[0]-base[0],earnings_delta=stress[1]-base[1],aoci_delta=stress[2]-base[2]))
        writer.add('attribution',attribution)
    writer.finish()
    # Verify even non-journal partitions before publication, without retaining them.
    for table in list(writer.parts):
        for _ in checked_frames(table):
            pass
    return dict(journal_replayed=True,journal_rows=journal_rows,gl_keys=len(trial),
                dynamic_validated=not writer.parts['breaches'],ruleset=RULESET,
                regulatory_validated=False,calibration_validated=False)


def run_streamed_balance_stress(raw, directory, *, backend='rust', large_book=False,
                                partition_rows=65536, progress=None, cancelled=None, timeout=1800.):
    """Run in a fresh local attempt directory; atomically publish its manifest.

    large_book is an explicit engine-only capacity tier: <=60k positions and
    <=90m work units. Public API admission still uses the original limits.
    """
    if backend not in {'python','rust'}:
        raise ValueError('unknown state engine backend')
    if type(partition_rows) is not int or not 1 <= partition_rows <= 65536:
        raise ValueError('partition_rows must be in [1,65536]')
    if not math.isfinite(timeout) or timeout <= 0:
        raise ValueError('timeout must be finite and positive')
    if cancelled and cancelled():
        raise InterruptedError('simulation cancelled')
    raw=deepcopy(raw)
    initial_source=source_identity()
    started=time.perf_counter()
    deadline=started+timeout
    def check_runtime():
        if cancelled and cancelled(): raise InterruptedError('simulation cancelled')
        if time.perf_counter() >= deadline: raise TimeoutError('simulation deadline exceeded')
        return False
    from .ledger_native import validate_spec
    validator=validate_spec if backend=="rust" else validate
    spec = validator(raw, max_positions=60000 if large_book else 2000, max_work=90_000_000 if large_book else 3_000_000)
    root = Path(directory).resolve()
    root.mkdir(parents=True, exist_ok=True)
    identity = _hash(binary_path()) if backend == 'rust' and binary_path().is_file() else 'python-reference'
    with tempfile.TemporaryDirectory(prefix='.partial-',dir=root) as attempt:
        staging = Path(attempt)
        writer = PartitionWriter(staging,partition_rows)
        check_runtime()
        if backend == 'rust':
            _rust(spec,writer,progress,check_runtime,deadline-time.perf_counter())
        else:
            _python(spec,writer,progress,check_runtime)
        compute_end=time.perf_counter()
        if cancelled and cancelled():
            raise InterruptedError('simulation cancelled')
        validation = _finalize(spec,writer,check_runtime,native_reports=backend == 'rust')
        check_runtime()
        if backend == 'rust' and _hash(binary_path()) != identity:
            raise RuntimeError('native binary changed during run')
        if source_identity()!=initial_source:
            raise RuntimeError('engine source changed during run')
        manifest = dict(version='balance-partitions-1',model_version=MODEL_VERSION,backend=backend,
                        binary_sha256=identity,large_book=large_book,partition_rows=partition_rows,
                        source_sha256=initial_source,
                        timings=dict(compute_and_partition_seconds=compute_end-started,
                                     replay_and_finalize_seconds=time.perf_counter()-compute_end),
                        specification=raw,validation=validation,
                        tables={table:dict(schema={k:str(v) for k,v in schema.items()},parts=writer.parts[table]) for table,schema in SCHEMAS.items()})
        path=staging/'manifest.json'
        with path.open('w',encoding='utf-8') as handle:
            json.dump(manifest,handle,allow_nan=False,indent=2);handle.flush();os.fsync(handle.fileno())
        final=root/('run-'+uuid.uuid4().hex)
        check_runtime()
        os.replace(staging,final)
    return final/'manifest.json'


def load_streamed_result(manifest_path):
    """Materialize small runs for analysis/parity; large consumers scan partitions."""
    path=Path(manifest_path).resolve()
    manifest=json.loads(path.read_text(encoding='utf-8'))
    if manifest['version']!='balance-partitions-1':
        raise ValueError('unsupported partition manifest')
    result={}
    for name,schema in SCHEMAS.items():
        frames=[]
        for part in manifest['tables'][name]['parts']:
            file=(path.parent/part['path']).resolve()
            if file.parent != path.parent or file.suffix!='.parquet' or _hash(file)!=part['sha256']:
                raise ValueError('invalid or corrupt partition')
            frame=pl.read_parquet(file)
            if frame.height!=part['rows'] or frame.schema!=schema:
                raise ValueError('partition rows/schema mismatch')
            frames.append(frame)
        result[name]=pl.concat(frames) if frames else pl.DataFrame(schema=schema)
    result['manifest']=manifest
    return result
