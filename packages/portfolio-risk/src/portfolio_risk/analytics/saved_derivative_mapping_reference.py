"""Independent saved mapping and raw derivative verification; never a backend."""
import hashlib
import json
import math
import struct

from .saved_derivative_financial_reference import (
    DerivativeVerificationError, verify_derivative_accounting,
)

SCHEMA = 'saved-derivative-source-1'
CAP = 32 * 1024 * 1024
FLOAT_FIELDS = frozenset(('notional', 'fixed_rate', 'float_spread', 'discount_factor',
    'rate', 'annual_rate', 'margin', 'fee', 'floor', 'cap', 'probability', 'discount_to_expiry'))


def require(condition, message):
    if not condition:
        raise DerivativeVerificationError(message)


def bits(value):
    require(type(value) in (int, float) and math.isfinite(value), 'invalid saved binary64 anchor')
    return struct.pack('>d', float(value))


def exact(actual, expected):
    """Exact source identity including binary64 sign; no financial tolerance here."""
    require(type(actual) is type(expected), 'saved source primitive type differs')
    if isinstance(expected, dict):
        require(set(actual) == set(expected), 'saved source fields differ')
        for name, value in expected.items():
            exact(actual[name], value)
    elif isinstance(expected, list):
        require(len(actual) == len(expected), 'saved source length differs')
        for left, right in zip(actual, expected):
            exact(left, right)
    elif isinstance(expected, float):
        require(bits(actual) == bits(expected), 'saved source binary64 differs')
    else:
        require(actual == expected, 'saved source identity/order/value differs')


def typed(value, field=None):
    """Normalize only the declared financial f64 cells in saved trade DTOs."""
    if field in FLOAT_FIELDS and value is not None:
        bits(value)
        return float(value)
    if isinstance(value, list):
        return [typed(v) for v in value]
    if isinstance(value, dict):
        result = {k: typed(v, k) for k, v in value.items()}
        if result.get('kind') == 'expired_european':
            result.setdefault('delivered_underlying', None)
            result.setdefault('settlement_discount', None)
        if result.get('kind') == 'indexed':
            result.setdefault('reset_cutoff_unix_seconds', None)
        if field in {'index_bounds', 'all_in_bounds'}:
            result.setdefault('floor', None)
            result.setdefault('cap', None)
        if {'index_id', 'kind'} <= set(result):
            result.setdefault('tenor_months', None)
            result.setdefault('average_calendar_days', None)
        return result
    return value


def identity(value):
    require(isinstance(value, dict) and set(value) == {'id', 'revision'}, 'invalid saved identity')
    require(all(type(v) is str and v.strip() and len(v.encode()) <= 4096
                for v in value.values()), 'invalid saved identity text')


def trade_request(trade):
    return trade['terminal'] if trade['kind'] == 'expired_european' else trade['request']


def validate_mapping(raw):
    required = {'schema', 'model_version', 'book', 'legacy_bindings', 'physical'}
    require(required <= set(raw) <= required | {'contractual_upgrade_review'}, 'saved request fields differ')
    require(raw['schema'] == raw['model_version'] == SCHEMA, 'saved schema/model differs')
    require(len(json.dumps(raw, ensure_ascii=False, allow_nan=False, separators=(',', ':')).encode()) <= CAP,
            'saved raw byte admission exceeded')
    book, physical = raw['book'], raw['physical']
    require(set(book) == {'identity', 'asof_ordinal', 'entity', 'currency', 'monetary_unit',
                          'legacy_hedges', 'trades'}, 'saved book fields differ')
    identity(book['identity'])
    review = raw.get('contractual_upgrade_review')
    if review is not None:
        identity(review)
    operation = physical['derivative']['operation']
    require(operation['kind'] == 'netting', 'saved source needs raw legal netting lifecycle')
    source = operation['request']
    require(len(source['paths']) == 1 and 0 < len(book['trades']) <= 256
            and len(book['legacy_hedges']) <= 256, 'saved mapping admission differs')
    require(all(book[k] == physical[k] for k in ('entity', 'currency', 'monetary_unit'))
            and type(book['asof_ordinal']) is int and book['asof_ordinal'] == source['asof_ordinal'],
            'saved header differs')
    opening = source['paths'][0]['nodes'][0]
    require(opening['ordinal'] == book['asof_ordinal'] and
            len(book['trades']) == len(opening['trades']) == len(source['mapping']), 'saved opening coverage differs')
    seen = {}
    for saved, initial, mapping in zip(book['trades'], opening['trades'], source['mapping']):
        request = trade_request(saved)
        position = request['position_id']
        require(position not in seen and type(position) is str and position.strip(), 'duplicate saved trade owner')
        exact(typed(saved), typed(initial))
        require(mapping['position_id'] == position, 'saved legal position differs')
        exact(mapping['contract'], request['contract'])
        seen[position] = saved
    legacy, bindings = book['legacy_hedges'], raw['legacy_bindings']
    require(len(legacy) == len(bindings) and (not legacy or review is not None), 'reviewed legacy coverage missing')
    rows, owners = set(), set()
    for binding in bindings:
        require(set(binding) == {'row', 'position_id'}, 'saved binding fields differ')
        row, position = binding['row'], binding['position_id']
        require(type(row) is int and 0 <= row < len(legacy) and row not in rows and position not in owners,
                'legacy binding duplicate or outside row')
        rows.add(row)
        owners.add(position)
        old, trade = legacy[row], seen[position]
        require(set(old) <= {'notional', 'fixed_rate', 'side', 'maturity', 'float_spread'} and
                {'notional', 'fixed_rate', 'side', 'maturity'} <= set(old) and trade['kind'] == 'swap',
                'legacy swap needs complete explicit swap upgrade')
        current = trade['request']
        require(current['side'] == old['side'] and
                bits(current['notional']) == bits(old['notional']) and
                bits(current['fixed_rate']) == bits(old['fixed_rate']), 'legacy swap economics changed')
        require(type(old['maturity']) is int and current['fixed_periods'][-1]['end_ordinal'] ==
                current['floating_periods'][-1]['end_ordinal'] == old['maturity'], 'legacy maturity changed')
        for period in current['floating_periods']:
            require(period['coupon']['kind'] == 'indexed' and bits(period['coupon']['margin']) ==
                    bits(old.get('float_spread', 0.0)), 'legacy floating spread changed')
    return book


def verify(raw, envelope):
    try:
        book = validate_mapping(raw)
        require(set(envelope) == {'original_request', 'result'}, 'saved envelope fields differ')
        exact(envelope['original_request'], raw)
        result = envelope['result']
        require(set(result) == {'schema', 'book', 'contractual_upgrade_review', 'legacy_bindings',
                'source', 'exact_saved_mapping_verified', 'mixed_daily_journal_adoption',
                'pricing_graph_adoption', 'empirical_validation'}, 'saved result fields differ')
        require(result['schema'] == SCHEMA and result['exact_saved_mapping_verified'] is True and
                all(result[k] is False for k in ('mixed_daily_journal_adoption',
                    'pricing_graph_adoption', 'empirical_validation')), 'unverified adoption permission')
        expected_book = typed(book)
        for legacy in expected_book['legacy_hedges']:
            legacy.setdefault('float_spread', 0.0)
        exact(result['book'], expected_book)
        exact(result['contractual_upgrade_review'], raw.get('contractual_upgrade_review'))
        exact(result['legacy_bindings'], raw['legacy_bindings'])
        source_receipt = verify_derivative_accounting(raw['physical'], result['source'])
        encoded = json.dumps(raw, sort_keys=True, allow_nan=False, separators=(',', ':')).encode()
        return dict(schema='saved-derivative-verification-1', verified=True,
            raw_request_sha256=hashlib.sha256(encoded).hexdigest(),
            checked_saved_trades=len(book['trades']), checked_legacy_bindings=len(raw['legacy_bindings']),
            source_verification=source_receipt, mixed_daily_journal_adoption=False)
    except DerivativeVerificationError:
        raise
    except (KeyError, TypeError, ValueError, IndexError, OverflowError) as error:
        raise DerivativeVerificationError(f'incomplete saved derivative source: {error}') from error


def verify_with_check(raw,envelope,check):
    from .saved_derivative_financial_reference import _CONTROL,_checkpoint
    token=_CONTROL.set(check)
    try:
        _checkpoint();result=verify(raw,envelope);_checkpoint();return result
    finally:_CONTROL.reset(token)
