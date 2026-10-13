"""Independent prospective stream/publication checks; never a finance backend.

The financial control is raw-source SciPy/accounting replay. Verified producer
line numbers determine only exact-zero topology after complete source checking.
Every retained amount is independently compared at the original tolerance.
"""
from dataclasses import dataclass
from copy import deepcopy
import hashlib
import json
import time
import math
import polars as pl
from portfolio_risk.analytics.dated_ledger import DATED_SCHEMAS

from . import _mortgage_mixed_financial_reference as financial

MAX_PARTITION_BYTES = 32 * 1024 * 1024
MAX_PARTITION_ROWS = 65536
MAX_VERIFY_WORK = 64_000_000
TABLES = frozenset(('journal', 'mortgage_lineage', 'mortgage_daily',
    'mortgage_closing', 'trial_balance', 'path', 'summary', 'exposures',
    'ledger', 'event_lineage', 'dated_rights', 'closing_statements',
    'consolidated', 'attribution', 'breaches'))
STR_COLUMNS = frozenset(('scenario', 'account', 'event', 'gl_account', 'instrument_id'))
IMMUTABLE_ROW_FIELDS = frozenset(('source_weight','allocation_factor','original_yield_anchor','review',
    'scenario','run_id','position_id','path_id','registry_source_id','native_source_id',
    'source_action_indices','native_group_index','source_groups_sole_postings',
    'memo_checkpoint_source','native_daily_index','native_daily_ordinal',
    'capital_ftp_eligible','closing_capital_eligible','full_tail_complete'))
JOURNAL_COLUMNS = frozenset(('transaction_number', 'scenario', 'day', 'account',
                            'event', 'gl_account', 'instrument_id', 'value'))
PERMISSIONS = ('production_adoption', 'mixed_ledger_adoption',
              'accounting_standard_approval', 'capital_ftp_eligible', 'closing_capital_eligible')
FENCE_FIELDS = frozenset(('lease', 'attempt_token', 'input_revision',
    'executable_sha256', 'source_sha256', 'raw_request_sha256'))
_SEAL = object()


def encoded(value):
    return json.dumps(value, ensure_ascii=False, allow_nan=False,
                      separators=(',', ':'), sort_keys=True).encode('utf8')


def digest(value):
    return hashlib.sha256(encoded(value)).hexdigest()


class Guard:
    """Trusted operational callbacks only; none may supply financial values."""
    def __init__(self, check=lambda: False, *, deadline=None, fence=None):
        self.check = check
        self.deadline = deadline
        if deadline is not None and (type(deadline) not in (int,float) or not math.isfinite(deadline)):
            raise ArithmeticError('trusted finite original deadline required')
        self.fence = fence
        self.identity = deepcopy(fence()) if fence is not None else None
        if self.identity is not None:
            financial.keys(self.identity, FENCE_FIELDS)
            for key, value in self.identity.items():
                if not isinstance(value, str) or not value:
                    raise ArithmeticError('trusted fence string identity required')
                if key.endswith('sha256') and (len(value) != 64 or
                        any(c not in '0123456789abcdef' for c in value)):
                    raise ArithmeticError('trusted SHA256 identity required')
        self.work = 0
        self.configuration = digest([self.deadline,self.identity])
        self.tick()

    def tick(self, units=1, extra=lambda: False):
        if type(units) is not int or units < 0:
            raise ArithmeticError('verification work domain')
        self.work += units
        if digest([self.deadline,self.identity]) != self.configuration:
            raise ArithmeticError('original deadline/source fence configuration changed')
        if self.work > MAX_VERIFY_WORK:
            raise ArithmeticError('bounded independent verification work')
        if self.deadline is not None and time.perf_counter() >= self.deadline:
            raise TimeoutError('original verification deadline expired')
        financial.check_cancel(self.check)
        financial.check_cancel(extra)
        if self.fence is not None:
            financial.equal(self.fence(), self.identity, 'current publication ownership/source fence')
        return False


@dataclass(frozen=True)
class CheckedControl:
    """Private bounded source/control, not a serializable financial authority."""
    raw: dict
    specification: dict
    events: list
    bindings: list
    sources: dict
    topology: dict
    admission: dict
    guard: Guard
    content_digest: str
    input_digest: str
    preparation_byte_charge: int
    seal: object


def _control_digest(raw, specification, events, bindings, sources, topology, admission):
    return digest([raw, specification, events, bindings, sources, topology, admission])


def verify_retained_request_identity(raw, retained, check=lambda: False):
    """Check the declared native DTO separately from unchanged queued JSON.

    Existing independent admission retains the raw specification Value and
    normalizes only declared binary64 fields. No approximate numeric equality,
    producer flags or supplied digest can authorize a changed source.
    """
    normalized, *_ = financial.admit(raw, check)
    financial.exact(retained, normalized, 'immutable typed preparation request')


def verify_preparation(raw, prepared_dated, receipt, check=lambda: False, *,
                       deadline=None, fence=None):
    """Rebuild sources before allowing native exact-zero topology in replay.

    Native preparation header shape is deliberately strict. Executable/lease
    identities come from trusted transport fences, never caller input flags.
    """
    guard = Guard(check, deadline=deadline, fence=fence)
    financial.output_size_guard(receipt, guard.tick)
    financial.keys(receipt, 'schema raw_request admission sources source_groups_sole_postings memo_checkpoint_policy ' + ' '.join(PERMISSIONS))
    financial.equal(receipt['schema'], 'mortgage-mixed-preparation-1', 'preparation schema')
    financial.equal(receipt['source_groups_sole_postings'], True, 'sole monetary posting ownership')
    financial.equal(receipt['memo_checkpoint_policy'], 'native_complete_daily_boundary', 'memo-only boundary ownership')
    for key in PERMISSIONS:
        financial.equal(receipt[key], False, key)
    r, spec, events, bindings, admission = financial.admit(raw, guard.tick)
    financial.exact(receipt['raw_request'], r, 'immutable raw preparation')
    financial.exact(prepared_dated, r['dated'], 'prepared dated source')
    financial.exact(receipt['admission'], admission, 'joined admission')
    if len(receipt['sources']) != len(r['sources']):
        raise ArithmeticError('complete immutable source inventory')
    if guard.identity is not None and guard.identity['raw_request_sha256'] != digest(raw):
        raise ArithmeticError('queued raw input digest differs')
    sources, topology = {}, {}
    for item, actual in zip(r['sources'], receipt['sources'], strict=True):
        guard.tick()
        financial.keys(actual, 'source_id native')
        financial.equal(actual['source_id'], item['source_id'], 'registry exact order')
        independently = financial.pending.reconstruct(item['request'], guard.tick)
        for group in independently['groups']:
            financial.money_book(group['committed'], guard.tick,3*independently['admission']['interval_count'])
        for book in independently['daily']:
            financial.money_book(book, guard.tick,3*independently['admission']['interval_count'])
        financial.money_book(independently['closing'], guard.tick,3*independently['admission']['interval_count'])
        financial.closed(independently['closing'])
        # Actual wire topology is copied ONLY after raw economics, amounts,
        # states, actions, reviews and complete source identities all pass.
        financial.pending.verify(item['request'], actual['native'], guard.tick)
        sources[item['source_id']] = independently
        topology[item['source_id']] = deepcopy([g['lines'] for g in actual['native']['groups']])
    guard.tick()
    content = _control_digest(r, spec, events, bindings, sources, topology, admission)
    header=dict(workflow=dict(schema='model-ledger-preparation-1',model_schema=financial.SCHEMA,
        prepared_request=prepared_dated,source_receipt=receipt))
    charge=len(encoded(header))+1
    if charge>min(financial.MAX_BYTES,admission['reserved_bytes']):
        raise ArithmeticError('complete preparation serialized bound')
    return CheckedControl(r, spec, events, bindings, sources, topology,
                          admission, guard, content, digest(raw), charge, _SEAL)


def verify_workflow(raw,header,check=lambda:False,**operational):
    financial.keys(header,'workflow'); workflow=header['workflow']
    financial.keys(workflow,'schema model_schema prepared_request source_receipt')
    financial.equal(workflow['schema'],'model-ledger-preparation-1','workflow schema')
    financial.equal(workflow['model_schema'],financial.SCHEMA,'workflow model identity')
    return verify_preparation(raw,workflow['prepared_request'],workflow['source_receipt'],check,**operational)


def _rows(control, table, check):
    """A lazy table cursor: one current day, GL and independent source state.

    Different native table buffer boundaries need not match replay day order.
    Each bounded cursor independently traverses chronological replay; no full
    expected journal or combined table dictionary is retained.
    """
    for name, row in financial.replay_rows(control.raw, control.specification,
            control.events, control.bindings, control.sources, check, control.topology):
        control.guard.tick(extra=check)
        if name == table:
            yield row


def _decoded_frame(frame, dictionary):
    if not isinstance(frame, dict) or frame.get('table') not in TABLES:
        raise ArithmeticError('strict admitted stream table')
    table = frame['table']
    if table == 'journal':
        financial.keys(frame, 'table columns dictionary')
        cols = frame['columns']
        financial.keys(cols, JOURNAL_COLUMNS)
        additions = frame['dictionary']
        if (not isinstance(additions, list) or any(not isinstance(x, str) for x in additions)
                or len(dictionary) + len(additions) > 250_000):
            raise ArithmeticError('bounded exact journal dictionary')
        dictionary.extend(additions)
        count = len(cols['value'])
        if count > MAX_PARTITION_ROWS or any(not isinstance(c, list) or len(c) != count for c in cols.values()):
            raise ArithmeticError('bounded journal column shape')
        for i in range(count):
            row = {}
            for name, column in cols.items():
                value = column[i]
                if name in STR_COLUMNS:
                    if type(value) is not int or not 0 <= value < len(dictionary):
                        raise ArithmeticError('exact journal dictionary index')
                    value = dictionary[value]
                row[name] = value
            yield row
    else:
        financial.keys(frame, 'table rows')
        if not isinstance(frame['rows'], list) or len(frame['rows']) > MAX_PARTITION_ROWS:
            raise ArithmeticError('bounded table partition rows')
        yield from frame['rows']


JSON_CELLS = {'mortgage_lineage': frozenset(('source_action_indices','review','original_yield_anchor')),
              'mortgage_daily': frozenset(('unweighted_book',)),
              'mortgage_closing': frozenset(('unweighted_book',))}
TRANSPORT_SCHEMAS=dict(DATED_SCHEMAS,
    mortgage_lineage={**dict.fromkeys('scenario run_id position_id path_id registry_source_id native_source_id source_action_indices_json review_json original_yield_anchor_json'.split(),pl.String),
        **dict.fromkeys('ordinal native_group_index transaction_number'.split(),pl.Int64),
        **dict.fromkeys('source_weight allocation_factor'.split(),pl.Float64),'source_groups_sole_postings':pl.Boolean},
    mortgage_daily={**dict.fromkeys('scenario run_id position_id path_id registry_source_id memo_checkpoint_source unweighted_book_json'.split(),pl.String),
        **dict.fromkeys('ordinal native_daily_index native_daily_ordinal'.split(),pl.Int64),
        **dict.fromkeys('source_weight allocation_factor'.split(),pl.Float64),
        **dict.fromkeys('acquired capital_ftp_eligible closing_capital_eligible'.split(),pl.Boolean)},
    mortgage_closing={**dict.fromkeys('scenario position_id path_id registry_source_id unweighted_book_json'.split(),pl.String),
        **dict.fromkeys('full_tail_complete capital_ftp_eligible closing_capital_eligible'.split(),pl.Boolean)})


def facade_frames(facade, guard):
    """Bounded partition facade; no frame/table concatenation or native reader.

    New transport must retain actual producer completion through persistence.
    Merely finding all tables must never synthesize completion authority.
    """
    if facade.schemas != TRANSPORT_SCHEMAS:
        raise ArithmeticError('exact prospective persisted table schema inventory')
    dictionary_size=0
    for table in sorted(TRANSPORT_SCHEMAS):
        for frame in facade.frames(table):
            guard.tick()
            if not hasattr(frame,'iter_rows') or type(frame.height) is not int or frame.height>MAX_PARTITION_ROWS:
                raise ArithmeticError('bounded persisted dataframe partition')
            if frame.schema!=TRANSPORT_SCHEMAS[table]:
                raise ArithmeticError('persisted typed column schema differs')
            if table not in TABLES and frame.height:
                raise ArithmeticError('unsupported generic financial table must remain empty')
            chunk=[]
            for row in frame.iter_rows(named=True):
                guard.tick()
                for name in JSON_CELLS.get(table,()):
                    column=name+'_json'
                    if column not in row: raise ArithmeticError('complete nested financial column')
                    cell=row.pop(column)
                    row[name]=None
                    if cell is not None:
                        if not isinstance(cell,str): raise ArithmeticError('lossless JSON cell required')
                        row[name]=financial.parse_raw(cell)
                        if row[name] is None: raise ArithmeticError('null financial cell must retain typed null')
                chunk.append(row)
                if len(chunk)==4096:
                    message,dictionary_size=_row_frame(table,chunk,dictionary_size)
                    yield message;chunk=[]
            if chunk:
                message,dictionary_size=_row_frame(table,chunk,dictionary_size)
                yield message
    financial.equal(facade.completion,dict(complete=True,protocol=3),'persisted actual completion')
    yield facade.completion


def _row_frame(table,rows,dictionary_size):
    if table!='journal': return dict(table=table,rows=rows),dictionary_size
    columns={key:[] for key in JOURNAL_COLUMNS}; dictionary=[]
    for row in rows:
        financial.keys(row,TRANSPORT_SCHEMAS['journal'])
        debit=row['debit'];credit=row['credit']
        if any(type(v) not in (int,float) or not math.isfinite(v) or v<0. for v in (debit,credit)) or debit and credit:
            raise ArithmeticError('persisted signed debit/credit domain')
        prefix=row['scenario']+':'
        identifier=row['transaction_id']
        if not isinstance(identifier,str) or not identifier.startswith(prefix):
            raise ArithmeticError('persisted transaction identity')
        suffix=identifier[len(prefix):]
        if not suffix.isascii() or not suffix.isdigit() or len(suffix)>8:
            raise ArithmeticError('canonical bounded transaction number')
        number=int(suffix)
        if not 1<=number<=financial.MAX_WORK or suffix!=str(number):
            raise ArithmeticError('canonical bounded transaction number')
        row={k:v for k,v in row.items() if k not in ('debit','credit','transaction_id')}
        row.update(transaction_number=number,value=debit-credit)
        for key,value in row.items():
            if key in STR_COLUMNS:
                if not isinstance(value,str): raise ArithmeticError('persisted journal label domain')
                columns[key].append(dictionary_size+len(dictionary));dictionary.append(value)
            else: columns[key].append(value)
    return dict(table=table,columns=columns,dictionary=dictionary),dictionary_size+len(dictionary)


def verify_stream(raw, control, frames, check=lambda: False):
    """Incrementally verify every partition and final complete stream.

    The private guard retains the original deadline and lease/source fence.
    Callers cannot extend a deadline between generation, persistence and replay.
    Returned booleans are review evidence only, never deployment/adoption flags.
    """
    if not isinstance(control, CheckedControl) or control.seal is not _SEAL:
        raise ArithmeticError('private independently checked control required')
    guard = control.guard
    guard.tick(extra=check)
    financial.equal(raw, control.raw, 'persisted immutable raw input')
    if digest(raw) != control.input_digest:
        raise ArithmeticError('persisted raw input byte identity differs')
    if _control_digest(control.raw, control.specification, control.events, control.bindings,
                       control.sources, control.topology, control.admission) != control.content_digest:
        raise ArithmeticError('checked source/control mutated')
    cursors = {table: _rows(control, table, check) for table in TABLES}
    dictionary, counts = [], {table: 0 for table in TABLES}
    complete, total_bytes, frame_count = False, 0, 0
    stream_hash = hashlib.sha256()
    if hasattr(frames,'frames'):
        frames=facade_frames(frames,guard)
    try:
        for frame in frames:
            guard.tick(extra=check)
            if complete:
                raise ArithmeticError('partition after complete fence')
            wire = encoded(frame)
            if len(wire) > MAX_PARTITION_BYTES:
                raise ArithmeticError('bounded individual stream partition')
            total_bytes += len(wire)
            frame_count += 1
            if total_bytes+frame_count+control.preparation_byte_charge > min(financial.MAX_BYTES, control.admission['reserved_bytes']) or frame_count > 250_000:
                raise ArithmeticError('bounded joined serialized stream')
            stream_hash.update(len(wire).to_bytes(8, 'little'))
            stream_hash.update(wire)
            if isinstance(frame, dict) and 'table' not in frame:
                financial.equal(frame, dict(complete=True, protocol=3), 'strict final stream fence')
                complete = True
                continue
            table = frame.get('table') if isinstance(frame, dict) else None
            if table not in TABLES:
                raise ArithmeticError('unknown streamed table')
            for row in _decoded_frame(frame, dictionary):
                guard.tick(extra=check)
                try:
                    expected = next(cursors[table])
                except StopIteration as error:
                    raise ArithmeticError('added ordered row in ' + table) from error
                for field in IMMUTABLE_ROW_FIELDS & expected.keys():
                    if field not in row:raise ArithmeticError('missing immutable streamed field '+field)
                    financial.exact(row[field],expected[field],table+'/'+field)
                financial.equal(row, expected, table + '/' + str(counts[table]))
                counts[table] += 1
        if not complete:
            raise ArithmeticError('missing complete stream fence')
        for table, cursor in cursors.items():
            guard.tick(extra=check)
            if next(cursor, None) is not None:
                raise ArithmeticError('omitted ordered row in ' + table)
        if _control_digest(control.raw, control.specification, control.events, control.bindings,
                           control.sources, control.topology, control.admission) != control.content_digest:
            raise ArithmeticError('source/control changed during persisted replay')
        guard.tick(extra=check)
        return dict(verified=True, native_mortgage_source_recomputed=True,
            single_daily_gl_verified=True, pending_memo_phase_verified=True,
            complete_unweighted_tail_verified=True, ordered_mixed_journal_recomputed=True,
            raw_request_sha256=digest(raw), stream_sha256=stream_hash.hexdigest(),
            serialized_stream_bytes=total_bytes, frame_count=frame_count,
            preparation_byte_charge=control.preparation_byte_charge,
            table_rows={k: v for k, v in sorted(counts.items()) if v},
            production_adoption=False, mixed_ledger_adoption=False,
            accounting_standard_approval=False,capital_ftp_eligible=False, closing_capital_eligible=False,
            empirical_validation=False)
    finally:
        for cursor in cursors.values():
            cursor.close()


def persisted_frames(partitions, guard):
    """Trusted transport supplies bytes; verify each descriptor and raw digest.

    This does not read arbitrary paths or execute a native reader. Codec/Parquet
    transport may provide bounded equivalent JSON frames after its own decode.
    """
    for index, part in enumerate(partitions):
        guard.tick()
        financial.keys(part, 'index bytes sha256 payload')
        if type(part['index']) is not int or part['index'] != index:
            raise ArithmeticError('persisted partition sequence')
        data = part['payload']
        if not isinstance(data, bytes) or not 0 < len(data) <= MAX_PARTITION_BYTES:
            raise ArithmeticError('bounded persisted partition payload')
        if type(part['bytes']) is not int or part['bytes'] != len(data):
            raise ArithmeticError('persisted partition exact bytes')
        if part['sha256'] != hashlib.sha256(data).hexdigest():
            raise ArithmeticError('persisted partition SHA256 differs')
        yield financial.parse_raw(data.decode('utf8'),max_bytes=MAX_PARTITION_BYTES)
        guard.tick()
