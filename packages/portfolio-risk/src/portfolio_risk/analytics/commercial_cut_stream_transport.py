"""Strict transport-only decoding of the native sequential journal stream.

No prices, cashflows, balances or posting amounts are calculated here. Original
dictionary labels are shared, ordinary rows are moved, and numbers retain their
JSON categories. Financial acceptance still requires independent source replay.
"""
from .commercial_cut_capture_reference import strict_loads

MAX_STREAM_BYTES = 4 << 20
TABLES = frozenset('''commercial_source_lineage commercial_daily commercial_closing
commercial_fee_lineage commercial_fee_daily commercial_fee_closing journal ledger
exposures path breaches summary trial_balance closing_statements consolidated attribution'''.split())
LABELS = frozenset(('scenario', 'account', 'event', 'gl_account', 'instrument_id'))
COLUMNS = LABELS | {'day', 'transaction_number', 'value'}


def _require(ok, message):
    if not ok:
        raise ArithmeticError(message)


def decode(stream, check=lambda: False):
    """One strict parse per frame; no stream copy, deep row copy or native call.

    This is a bounded wire-format decoder, not an allocator/RSS admission proof.
    Publication and raw-source resource admission are separate caller gates.
    """
    check()
    _require(type(stream) is str and len(stream) <= MAX_STREAM_BYTES,
             'complete bounded commercial cut stream required')
    # Count UTF-8 without creating a second complete encoded stream. Check at
    # bounded intervals, including a huge single frame or missing final newline.
    encoded = 0
    for start in range(0, len(stream), 32768):
        check()
        try:
            encoded += len(stream[start:start + 32768].encode('utf8'))
        except UnicodeError as error:
            raise ArithmeticError('commercial stream Unicode scalar required') from error
        _require(encoded <= MAX_STREAM_BYTES, 'commercial cut stream exceeds4MiB')
    _require(stream.endswith('\n'), 'commercial cut stream truncated')
    tables, dictionary, seen = {}, [], set()
    completion = None
    offset = 0
    while offset < len(stream):
        check()
        end = stream.find('\n', offset)
        _require(end > offset, 'commercial cut empty/truncated frame')
        _require(completion is None, 'commercial cut frame follows completion')
        row = strict_loads(stream[offset:end])
        offset = end + 1
        _require(type(row) is dict, 'commercial cut frame must be an object')
        if 'table' not in row:
            _require(row.get('complete') is True and type(row.get('protocol')) is int
                     and row['protocol'] == 9, 'commercial cut completion differs')
            completion = row
            continue
        table = row['table']
        _require(type(table) is str and table in TABLES, 'commercial cut table differs')
        if table != 'journal':
            _require(set(row) == {'table', 'rows'} and type(row['rows']) is list,
                     'commercial cut ordinary frame differs')
            rows = row.pop('rows')
            for value in rows:
                check()
                _require(type(value) is dict, 'commercial cut ordinary row differs')
            tables.setdefault(table, []).extend(rows)
            continue
        _require(set(row) == {'table', 'columns', 'dictionary'}
                 and type(row['dictionary']) is list, 'commercial cut journal frame differs')
        for label in row.pop('dictionary'):
            check()
            _require(type(label) is str and label not in seen,
                     'commercial cut dictionary label differs or repeats')
            dictionary.append(label)
            seen.add(label)
        columns = row.pop('columns')
        _require(type(columns) is dict and set(columns) == COLUMNS
                 and all(type(values) is list for values in columns.values()),
                 'commercial cut journal columns differ')
        count = len(columns['value'])
        _require(all(len(values) == count for values in columns.values()),
                 'commercial cut journal shape differs')
        output = tables.setdefault('journal', [])
        for i in range(count):
            check()
            decoded = {}
            for key, values in columns.items():
                value = values[i]
                if key in LABELS:
                    _require(type(value) is int and 0 <= value < len(dictionary),
                             'commercial cut dictionary index differs')
                    decoded[key] = dictionary[value]
                elif key == 'value':
                    _require(type(value) is float, 'commercial cut journal amount type differs')
                    decoded[key] = value
                else:
                    _require(type(value) is int and value >= 0,
                             'commercial cut journal coordinate differs')
                    decoded[key] = value
            output.append(decoded)
    _require(completion is not None, 'commercial cut native journal did not complete')
    check()
    return tables, completion
