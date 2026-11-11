"""Bounded temporary independent replay state; never a financial input owner."""
from collections.abc import Sequence
from contextlib import contextmanager
import json
import sqlite3
import tempfile
from pathlib import Path


class Store:
    def __init__(self, directory, check):
        self.check = check
        self.db = sqlite3.connect(Path(directory) / 'replay.sqlite')
        self.db.execute('PRAGMA journal_mode=OFF')
        self.db.execute('PRAGMA synchronous=OFF')
        self.db.execute('PRAGMA cache_size=-8192')
        self.db.execute('PRAGMA temp_store=FILE')
        self.db.execute('PRAGMA max_page_count=524288')
        self.db.execute('CREATE TABLE item (tag INTEGER, seq INTEGER, scenario TEXT, day INTEGER, clock REAL, binding INTEGER, native_group INTEGER, payload TEXT, PRIMARY KEY(tag,seq))')
        self.db.execute('CREATE INDEX event_order ON item(tag,scenario,day,clock,binding,native_group)')
        self.db.execute('CREATE TABLE keyed (tag INTEGER, key TEXT, payload TEXT, PRIMARY KEY(tag,key))')
        self.cursors = set()
        self.tags = 0
        self.bytes = self.rows = 0
        self.scenarios = {}

    def sequence(self, *, events=False):
        self.tags += 1
        return Items(self, self.tags, events)

    def put(self, tag, seq, value, events=False):
        self.check()
        encoded = json.dumps(value, allow_nan=False, separators=(',', ':'))
        self.bytes += len(encoded.encode('utf-8'))
        self.rows += 1
        if self.bytes > 2 << 30 or self.rows > 8_000_000 or len(encoded.encode('utf-8')) > 32 << 20:
            raise ArithmeticError('generated independent spool exceeds2GiB/8M/32MiB phase capacity')
        order = ((self.scenarios[value[0]], *value[1:5]) if events else (None,) * 5)
        self.db.execute('INSERT INTO item VALUES(?,?,?,?,?,?,?,?)', (tag, seq, *order, encoded))

    def cursor(self, query, parameters):
        cursor = self.db.execute(query, parameters)
        self.cursors.add(cursor)
        return cursor

    def release_cursor(self, cursor):
        if cursor in self.cursors:
            self.cursors.remove(cursor)
            cursor.close()

    def close(self):
        # Tracebacks may retain a suspended iterator after a financial refusal
        # or cancellation. Finalize its SQLite statement before closing the
        # connection and unlinking the owned temporary directory on Windows.
        try:
            for cursor in tuple(self.cursors):
                self.release_cursor(cursor)
        finally:
            self.db.close()

    def keyed(self):
        self.tags += 1
        return Keyed(self, self.tags)


class Keyed:
    def __init__(self, store, tag):
        self.store, self.tag = store, tag

    @staticmethod
    def key(value):
        return json.dumps(value, allow_nan=False, separators=(',', ':'))

    def __setitem__(self, key, value):
        self.store.check()
        encoded, name = self.key(value), self.key(key)
        self.store.bytes += len(encoded.encode()) + len(name.encode())
        self.store.rows += 1
        if self.store.bytes > 2 << 30 or self.store.rows > 8_000_000:
            raise ArithmeticError('generated keyed spool exceeds2GiB/8M phase capacity')
        self.store.db.execute('INSERT INTO keyed VALUES(?,?,?)', (self.tag, name, encoded))

    def __getitem__(self, key):
        self.store.check()
        row = self.store.db.execute('SELECT payload FROM keyed WHERE tag=? AND key=?', (self.tag, self.key(key))).fetchone()
        if row is None:
            raise KeyError(key)
        return json.loads(row[0])

    def __contains__(self, key):
        self.store.check()
        return self.store.db.execute('SELECT 1 FROM keyed WHERE tag=? AND key=?', (self.tag, self.key(key))).fetchone() is not None

    def __iter__(self):
        cursor = self.store.cursor('SELECT key FROM keyed WHERE tag=?', (self.tag,))
        try:
            for row in cursor:
                self.store.check()
                yield tuple(json.loads(row[0]))
        finally:
            self.store.release_cursor(cursor)

    def add(self, key):
        if key not in self:
            self[key] = True

    def same_keys(self, other):
        self.store.check()
        query = 'SELECT key FROM keyed WHERE tag=? EXCEPT SELECT key FROM keyed WHERE tag=?'
        return (self.store.db.execute(query, (self.tag, other.tag)).fetchone() is None
                and self.store.db.execute(query, (other.tag, self.tag)).fetchone() is None)


class Items(Sequence):
    def __init__(self, store, tag, events):
        self.store, self.tag, self.events = store, tag, events
        self.length = 0

    def append(self, value):
        self.store.put(self.tag, self.length, value, self.events)
        self.length += 1

    def __len__(self):
        return self.length

    def __getitem__(self, index):
        if index < 0:
            index += self.length
        if not 0 <= index < self.length:
            raise IndexError(index)
        self.store.check()
        row = self.store.db.execute('SELECT payload FROM item WHERE tag=? AND seq=?', (self.tag, index)).fetchone()
        if row is None:
            raise ArithmeticError('independent source spool lost a record')
        return json.loads(row[0])

    def __iter__(self):
        order = 'scenario,day,clock,binding,native_group' if self.events else 'seq'
        cursor = self.store.cursor(f'SELECT payload FROM item WHERE tag=? ORDER BY {order}', (self.tag,))
        try:
            for row in cursor:
                self.store.check()
                yield json.loads(row[0])
        finally:
            self.store.release_cursor(cursor)

    def sort(self, *, key):
        if not self.events:
            raise ArithmeticError('source spool has no event ordering capability')
        # scenario is encoded as its authoritative ordinal in event tuples.

    def for_day(self, scenario, day):
        cursor = self.store.cursor('SELECT payload FROM item WHERE tag=? AND scenario=? AND day=? ORDER BY clock,binding,native_group', (self.tag, self.store.scenarios[scenario], day))
        try:
            for row in cursor:
                self.store.check()
                yield json.loads(row[0])
        finally:
            self.store.release_cursor(cursor)

    def snapshots(self):
        return SnapshotMap(self.store)


class SnapshotMap:
    def __init__(self, store):
        self.items, self.index = store.sequence(), {}

    def __setitem__(self, key, value):
        self.items.append(value)
        self.index[key] = len(self.items) - 1

    def __contains__(self, key):
        return key in self.index

    def __getitem__(self, key):
        return self.items[self.index[key]]


@contextmanager
def replay_store(check):
    with tempfile.TemporaryDirectory(prefix='commercial-generated-replay-') as directory:
        store = Store(directory, check)
        try:
            yield store
        finally:
            store.close()
