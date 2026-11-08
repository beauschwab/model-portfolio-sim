"""Independent comparison of persisted modeled cuts at a later reporting date.

This constructs verification expectations only. Native execution owns the actual
opening, completed replay and resumed Sim. Monetary tolerances are unchanged.
"""
from .commercial_cut_controls_reference import _window
from .commercial_source_reference import compare
from .modeled_json_identity import exact_json_identity


def prefix(native, target_ordinal=None, check=lambda: False):
    if native.get('transport') != 'fixed-sequential-cut-1':
        raise ValueError('verified fixed sequential cut source required')
    raw = native['raw_source']
    origin = raw['completed_candidate']['as_of_ordinal']
    target = raw['cut_ordinal'] if target_ordinal is None else target_ordinal
    if type(target) is not int or target < raw['cut_ordinal']:
        raise ValueError('cut cannot precede its published predecessor')
    cut = target - origin
    tables = native['cut_tables']
    if target != raw['cut_ordinal']:
        days = {r['day'] for r in tables.get('path', [])}
        if cut not in days or cut >= max(days):
            raise ValueError('advanced cut must precede the complete published tail')
        specification = native['native_packet']['revised_phase']['preparation']['resolved_specification']
        # Columnar output omits empty tables, including zero-participation cash
        # summaries. Restore only those empty collection slots for the oracle.
        complete = dict(tables)
        complete.setdefault('ledger', [])
        complete.setdefault('attribution', [])
        tables = _window(complete, specification, cut, check)
    result = {}
    for name, rows in tables.items():
        check()
        selected = [r for r in rows if type(r.get('day')) is int and r['day'] <= cut]
        if selected:
            result[name] = selected
    return dict(origin_asof_ordinal=origin, cut_ordinal=target, cut_day=cut, tables=result)


def match_advanced(expected, actual, check=lambda: False):
    # Only accumulated journal money is a newly computed expectation. Snapshot
    # money, identities, ordering and scalar types must still match exactly.
    for key in ('origin_asof_ordinal', 'cut_ordinal', 'cut_day'):
        if not exact_json_identity(expected[key], actual[key]):
            raise ArithmeticError('advanced predecessor calendar differs')
    a, b = expected['tables'], actual['tables']
    if a.keys() != b.keys():
        raise ArithmeticError('advanced predecessor table coverage differs')
    for name in a:
        check()
        if name != 'journal':
            if not exact_json_identity(a[name], b[name]):
                raise ArithmeticError('advanced predecessor snapshot differs/' + name)
            continue
        if len(a[name]) != len(b[name]):
            raise ArithmeticError('advanced predecessor opening coverage differs')
        for old, new in zip(a[name], b[name], strict=True):
            if not exact_json_identity({k:v for k,v in old.items() if k != 'value'},
                    {k:v for k,v in new.items() if k != 'value'}):
                raise ArithmeticError('advanced predecessor opening lineage differs')
            if type(old['value']) is not float or type(new['value']) is not float:
                raise ArithmeticError('advanced predecessor opening scalar differs')
            compare(old['value'], new['value'], 'advanced_cut.opening.value', check)
